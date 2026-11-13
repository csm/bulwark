//! Main-thread native host. Runtime requests and Servo wakes share a winit proxy.
mod frame;
mod input;

use anyhow::{Result, anyhow, ensure};
use bulwark_engine::{Command, Data, Engine, EventKind, Handle, bridge, ui::Widget};
use bulwark_engine_servo::ServoEngine;
use bulwark_runtime::worker::Worker;
use servo::{OffscreenRenderingContext, RenderingContext, WindowRenderingContext};
use std::{
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::{Window, WindowId},
};

const MAINTENANCE: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug)]
enum Wake {
    Work,
    Repaint(Duration),
}

struct Surface {
    // Drop Servo and offscreen GL resources before their parent context and window.
    engine: ServoEngine,
    page: Rc<OffscreenRenderingContext>,
    chrome: egui_glow::EguiGlow,
    context: Rc<WindowRenderingContext>,
    window: Rc<Window>,
    input: input::Input,
    edits: frame::Edits,
    events: bridge::EventSink,
    control: bridge::Control,
    echo: String,
    // The page's place in the window. None hides the page.
    page_rect: Option<input::PageRect>,
    focused: bool,
    presentations: u64,
    echo_presented: String,
}
impl Surface {
    /// Whether the page owns the keyboard: always without a description, else by its focus.
    fn page_has_focus(&self) -> bool {
        self.engine
            .ui()
            .is_none_or(|ui| matches!(ui.focused(), Some(Widget::Web(_))))
    }

    /// Whether an editable control owns the keyboard.
    fn input_has_focus(&self) -> bool {
        self.engine
            .ui()
            .is_some_and(|ui| matches!(ui.focused(), Some(Widget::Input { .. })))
    }

    /// The pointer in logical pixels, when it is inside the window.
    fn pointer(&self) -> Option<(f64, f64)> {
        let scale = self.window.scale_factor();
        self.input.position().map(|(x, y)| (x / scale, y / scale))
    }

    /// Whether chrome covers the pointer. A description answers with its own hit test.
    fn chrome_covers_pointer(&self, egui_consumed: bool) -> bool {
        match (self.engine.ui(), self.pointer()) {
            (Some(ui), Some((x, y))) => {
                !matches!(ui.hit(x, y).map(|e| &e.widget), Some(Widget::Web(_)))
            }
            (Some(_), None) => true,
            (None, _) => egui_consumed,
        }
    }

    fn emit(&self, view: Handle, kind: EventKind) {
        // EventSink stores fatal overflow outside the queue and wakes the host.
        let _ = self.events.emit(view, kind);
    }

    /// Applies the window size, the presented description, and the focus to the page.
    fn layout(&mut self) {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        let scale = self.window.scale_factor();
        self.context.resize(size);
        self.page_rect = frame::page_rect(size, scale, self.engine.ui());
        if let Some(rect) = self.page_rect {
            let page_size = winit::dpi::PhysicalSize::new(rect.width, rect.height);
            if self.page.size() != page_size {
                if let Some((_, view)) = self.engine.active_webview() {
                    view.resize(page_size);
                } else {
                    self.page.resize(page_size);
                }
            }
        }
        self.input.place(self.page_rect);
        self.edits.sync(self.engine.ui());
        self.engine
            .set_window_state(scale as f32, self.focused && self.page_has_focus());
        if self.control.set_viewport(frame::viewport(size, scale)) {
            self.emit(Handle::NONE, EventKind::ViewportChanged);
        }
        self.window.request_redraw();
    }

    fn redraw(&mut self, capture: Option<&std::path::Path>) -> Result<()> {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        self.context
            .make_current()
            .map_err(|error| anyhow!("GL context: {error:?}"))?;
        self.engine.paint_frame(true);
        self.context.prepare_for_rendering();
        self.chrome
            .painter
            .clear(size.into(), [0.08, 0.09, 0.11, 1.0]);
        if let Some(rect) = self.page_rect
            && self.engine.active_webview().is_some()
            && let Some(blit) = self.page.render_to_parent_callback()
        {
            // GL's origin is at the bottom; input and content layout start at the top.
            let target = euclid::default::Rect::new(
                euclid::point2(
                    rect.x as i32,
                    size.height as i32 - (rect.y + rect.height) as i32,
                ),
                euclid::size2(rect.width as i32, rect.height as i32),
            );
            blit(&self.context.glow_gl_api(), target);
        }
        let input = self.chrome.egui_winit.take_egui_input(&self.window);
        let mut interactions = Vec::new();
        let output = match self.engine.ui() {
            Some(ui) => frame::draw(
                &self.chrome.egui_ctx,
                input,
                &self.echo,
                ui,
                &mut self.edits,
                &mut interactions,
            ),
            None => frame::draw_echo(&self.chrome.egui_ctx, input, &self.echo),
        };
        for interaction in interactions {
            self.emit(Handle::NONE, interaction);
        }
        self.chrome
            .egui_winit
            .handle_platform_output(&self.window, output.platform_output);
        let primitives = self
            .chrome
            .egui_ctx
            .tessellate(output.shapes, output.pixels_per_point);
        self.context.prepare_for_rendering();
        self.chrome.painter.paint_and_update_textures(
            size.into(),
            output.pixels_per_point,
            &primitives,
            &output.textures_delta,
        );
        if let Some(path) = capture {
            let rect = euclid::Box2D::from_origin_and_size(
                euclid::point2(0, 0),
                euclid::size2(size.width as i32, size.height as i32),
            );
            self.context
                .read_to_image(rect)
                .ok_or_else(|| anyhow!("Cannot read native frame"))?
                .save(path)?;
        }
        self.window.pre_present_notify();
        self.context.present();
        self.presentations += 1;
        self.echo_presented.clone_from(&self.echo);
        Ok(())
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        let _ = self.context.make_current();
        self.chrome.destroy();
    }
}

struct App {
    surface: Option<Surface>,
    endpoint: Option<bridge::EngineEndpoint>,
    worker: Option<Worker>,
    proxy: EventLoopProxy<Wake>,
    source: String,
    nrepl: Option<bulwark_runtime::nrepl::Config>,
    mode: cljrs_runtime::ExecutionMode,
    smoke: bool,
    deadline: Option<Instant>,
    close_after: Option<Duration>,
    close_at: Option<Instant>,
    repaint_at: Option<Instant>,
    stopping: Option<Instant>,
    intentional_stop: bool,
    error: Option<String>,
    result: Option<String>,
    joined: bool,
    capture: Option<std::path::PathBuf>,
}
impl App {
    fn start(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let window = Rc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Bulwark")
                    .with_inner_size(winit::dpi::LogicalSize::new(1000, 760)),
            )?,
        );
        let context = Rc::new(
            WindowRenderingContext::new(
                event_loop.display_handle()?,
                window.window_handle()?,
                window.inner_size(),
            )
            .map_err(|error| anyhow!("Window context: {error:?}"))?,
        );
        let page_rect = frame::page_rect(window.inner_size(), window.scale_factor(), None);
        let page = Rc::new(context.offscreen_context(
            page_rect.map_or((1, 1).into(), |rect| (rect.width, rect.height).into()),
        ));
        let proxy = self.proxy.clone();
        let wake: bridge::Wake = Arc::new(move || {
            let _ = proxy.send_event(Wake::Work);
        });
        let (runtime, endpoint) = bridge::channel(wake.clone());
        let mut engine = ServoEngine::with_context(page.clone(), endpoint.events.clone(), wake)?;
        engine.set_window_state(window.scale_factor() as f32, true);
        let chrome = egui_glow::EguiGlow::new(
            event_loop,
            context.glow_gl_api(),
            None,
            Some(window.scale_factor() as f32),
            false,
        );
        // The frame paints its own dark panels, so the text cannot follow a light system theme.
        chrome.egui_ctx.set_theme(egui::Theme::Dark);
        // Browser keys belong to Clojure/Servo. Keep chrome geometry at the
        // native scale instead of egui's default Ctrl/Cmd-plus zoom behavior.
        chrome
            .egui_ctx
            .options_mut(|options| options.zoom_with_keyboard = false);
        let proxy = self.proxy.clone();
        chrome
            .egui_ctx
            .set_request_repaint_callback(move |request| {
                let _ = proxy.send_event(Wake::Repaint(request.delay));
            });
        self.surface = Some(Surface {
            engine,
            page,
            chrome,
            context,
            window,
            input: input::Input::default(),
            edits: frame::Edits::default(),
            events: endpoint.events.clone(),
            control: endpoint.control.clone(),
            echo: String::new(),
            page_rect,
            focused: true,
            presentations: 0,
            echo_presented: String::new(),
        });
        self.endpoint = Some(endpoint);
        self.worker = Some(Worker::spawn_configured(
            runtime,
            self.source.clone(),
            self.mode,
            self.nrepl.take(),
        )?);
        self.close_at = self.close_after.map(|duration| Instant::now() + duration);
        println!("Bulwark native: window-ready");
        self.surface.as_mut().unwrap().layout();
        Ok(())
    }

    fn fail(&mut self, error: impl ToString) {
        self.error.get_or_insert(error.to_string());
        self.stop(false);
    }

    fn stop(&mut self, intentional: bool) {
        if self.stopping.is_some() {
            return;
        }
        self.intentional_stop = intentional;
        self.stopping = Some(Instant::now() + Duration::from_secs(2));
        if let Some(endpoint) = &mut self.endpoint {
            if let Some(error) = endpoint.control.error() {
                self.error.get_or_insert(error.to_string());
            }
            endpoint.stop_admission();
        }
        if let Some(surface) = &mut self.surface {
            // Flush the final diagnostic frame before destroying resources. macOS
            // may suppress RedrawRequested when the desktop is locked or occluded.
            if (self.smoke || self.capture.is_some())
                && let Err(error) = surface.redraw(self.capture.as_deref())
            {
                self.error.get_or_insert(error.to_string());
            }
            surface.engine.shutdown();
        }
        if let Some(worker) = &mut self.worker {
            worker.stop();
        }
    }

    fn poll_stop(&mut self, event_loop: &ActiveEventLoop) {
        if self.worker.as_ref().is_none_or(Worker::finished) {
            if let Some(result) = self.worker.as_mut().and_then(Worker::join_finished) {
                match result {
                    Ok(result) => self.result = Some(result),
                    Err(error) if self.intentional_stop => {
                        eprintln!("Native runtime stopped: {error}")
                    }
                    Err(error) => {
                        self.error.get_or_insert(error.to_string());
                    }
                }
            }
            if let Some(error) = self.endpoint.as_ref().and_then(|e| e.control.error()) {
                self.error.get_or_insert(error.to_string());
            }
            self.joined = true;
            event_loop.exit();
        } else if self
            .stopping
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.error
                .get_or_insert("Runtime did not stop within two seconds".into());
            event_loop.exit();
        }
    }

    fn pump(&mut self, event_loop: &ActiveEventLoop) {
        // Enforce diagnostic deadlines even under continuous proxy traffic.
        if self.stopping.is_none() {
            let now = Instant::now();
            if self.deadline.is_some_and(|deadline| now >= deadline) {
                self.fail("Native smoke timed out");
            } else if self.close_at.is_some_and(|deadline| now >= deadline) {
                self.stop(true);
            }
        }
        if self.stopping.is_some() {
            self.poll_stop(event_loop);
            return;
        }
        if self.worker.as_ref().is_some_and(Worker::finished) {
            if let Some(result) = self.worker.as_mut().and_then(Worker::join_finished) {
                match result {
                    Ok(result) => self.result = Some(result),
                    Err(error) => {
                        self.error.get_or_insert(error.to_string());
                    }
                }
            }
            self.stop(false);
            self.poll_stop(event_loop);
            return;
        }
        let (Some(surface), Some(endpoint)) = (&mut self.surface, &mut self.endpoint) else {
            return;
        };
        let control = endpoint.control.clone();
        let mut quit = false;
        // The key owner is the visible page and the described focus.
        let owner = |surface: &Surface| {
            (
                surface.engine.active_webview().map(|(handle, _)| handle),
                surface.engine.ui().and_then(|ui| ui.focus()),
            )
        };
        let previous_owner = owner(surface);
        let previous_ui = surface.engine.ui_generation();
        endpoint.admit(|request| match &request.command {
            Command::Log(text) => {
                println!("{text}");
                request.complete(Ok(Data::Nil));
            }
            Command::SetEcho(text) => {
                surface.echo.clone_from(text);
                surface.window.request_redraw();
                request.complete(Ok(Data::Nil));
            }
            Command::Quit => {
                request.complete(Ok(Data::Nil));
                control.stop();
                quit = true;
            }
            _ => surface.engine.handle(request),
        });
        surface.engine.tick();
        let current_owner = owner(surface);
        if previous_owner != current_owner {
            if previous_owner.0 != current_owner.0 {
                surface.input.change_view();
            } else {
                surface.input.change_focus();
            }
            // A frame without a page still cancels a pending prefix.
            if current_owner.0.is_some() || surface.engine.ui().is_some() {
                surface.emit(
                    current_owner.0.unwrap_or(Handle::NONE),
                    EventKind::InputReset,
                );
            }
        }
        if previous_owner != current_owner || previous_ui != surface.engine.ui_generation() {
            surface.layout();
        }
        if surface.engine.needs_repaint() {
            surface.window.request_redraw();
        }
        if quit {
            self.stop(true);
        } else if let Some(error) = endpoint.control.error() {
            self.fail(error);
        } else if endpoint.control.is_stopped() {
            // Worker completion can wake before JoinHandle reports finished.
            // Preserve errors at join even if the stop originated on that thread.
            self.stop(false);
        }
    }
}
impl ApplicationHandler<Wake> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.surface.is_some() || self.stopping.is_some() {
            return;
        }
        if let Err(error) = self.start(event_loop) {
            self.fail(error);
        }
        self.pump(event_loop);
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, wake: Wake) {
        if let Wake::Repaint(delay) = wake {
            if let Some(deadline) = Instant::now().checked_add(delay) {
                self.repaint_at = Some(self.repaint_at.map_or(deadline, |old| old.min(deadline)));
            }
        } else {
            self.pump(event_loop);
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.stopping.is_some() {
            return;
        }
        let Some(surface) = &mut self.surface else {
            return;
        };
        if id != surface.window.id() {
            return;
        }
        if matches!(event, WindowEvent::CloseRequested) {
            self.stop(true);
            self.poll_stop(event_loop);
            return;
        }
        if matches!(event, WindowEvent::RedrawRequested) {
            if let Err(error) = surface.redraw(None) {
                self.fail(error);
            }
        } else {
            // egui receives a key only when it is free and an editable control owns the focus.
            let keyboard = matches!(event, WindowEvent::KeyboardInput { .. });
            let response = if keyboard {
                Default::default()
            } else {
                surface.chrome.on_window_event(&surface.window, &event)
            };
            if response.repaint {
                surface.window.request_redraw();
            }
            let view = surface.engine.active_webview();
            let handle = view.as_ref().map_or(Handle::NONE, |(handle, _)| *handle);
            match event {
                WindowEvent::ModifiersChanged(modifiers) => {
                    surface.input.update_modifiers(modifiers.state());
                }
                WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                    surface.layout()
                }
                WindowEvent::Focused(focused) => {
                    surface.focused = focused;
                    surface.engine.set_window_state(
                        surface.window.scale_factor() as f32,
                        focused && surface.page_has_focus(),
                    );
                    surface.input.reset();
                    if view.is_some() || surface.engine.ui().is_some() {
                        surface.emit(handle, EventKind::InputReset);
                    }
                }
                _ => {
                    if let Some(endpoint) = &self.endpoint
                        && surface
                            .input
                            .sync_routes(endpoint.control.key_routes(), handle)
                        && (view.is_some() || surface.engine.ui().is_some())
                    {
                        surface.emit(handle, EventKind::InputReset);
                    }
                    let pressed = matches!(
                        event,
                        WindowEvent::MouseInput {
                            state: winit::event::ElementState::Pressed,
                            ..
                        }
                    );
                    // Only the focused page receives keys. Any visible page receives the pointer.
                    let target = view
                        .as_ref()
                        .filter(|_| !keyboard || surface.page_has_focus())
                        .map(|(_, view)| view);
                    let covered = surface.chrome_covers_pointer(response.consumed);
                    let routed =
                        surface
                            .input
                            .forward(target, event.clone(), surface.page_rect, covered);
                    if keyboard
                        && routed.free
                        && surface.input_has_focus()
                        && surface
                            .chrome
                            .on_window_event(&surface.window, &event)
                            .repaint
                    {
                        surface.window.request_redraw();
                    }
                    if let Some(key) = routed.key {
                        surface.emit(handle, EventKind::KeyInput(key));
                    }
                    // A press names its element, so Clojure can move the focus.
                    if pressed
                        && let (Some(ui), Some((x, y))) = (surface.engine.ui(), surface.pointer())
                        && let Some(element) = ui.hit(x, y).and_then(|element| element.id)
                    {
                        surface.emit(
                            Handle::NONE,
                            EventKind::UiPressed {
                                element,
                                revision: ui.revision(),
                            },
                        );
                    }
                }
            }
        }
        self.pump(event_loop);
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self.stopping.is_none() && self.repaint_at.is_some_and(|deadline| now >= deadline) {
            self.repaint_at = None;
            if let Some(surface) = &self.surface {
                surface.window.request_redraw();
            }
        }
        self.pump(event_loop);
        let mut next = now
            + if self.stopping.is_some() {
                Duration::from_millis(5)
            } else {
                MAINTENANCE
            };
        if self.stopping.is_none() {
            for deadline in [self.deadline, self.close_at, self.repaint_at]
                .into_iter()
                .flatten()
            {
                next = next.min(deadline);
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(next));
    }
    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.stop(true);
    }
}

pub fn run(
    args: &[String],
    nrepl: Option<bulwark_runtime::nrepl::Config>,
    mode: cljrs_runtime::ExecutionMode,
) -> Result<()> {
    let smoke = args.iter().any(|arg| arg == "--native-smoke");
    let mut initial = None;
    let mut script = None;
    let mut close_after = None;
    let mut capture = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--native-smoke" => {}
            "--screenshot" => {
                capture = Some(std::path::PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow!("Missing screenshot path"))?,
                ))
            }
            "--script" => {
                script = Some(std::fs::read_to_string(
                    args.next().ok_or_else(|| anyhow!("Missing script path"))?,
                )?)
            }
            "--close-after-ms" => {
                let ms: u64 = args
                    .next()
                    .ok_or_else(|| anyhow!("Missing close delay"))?
                    .parse()?;
                ensure!(ms > 0, "Close delay must be positive");
                close_after = Some(Duration::from_millis(ms));
            }
            _ if !arg.starts_with('-') && initial.is_none() => {
                initial = Some(url::Url::parse(arg)?)
            }
            _ => anyhow::bail!(
                "Usage: bulwark-shell [URL] [--native-smoke | --script PATH] [--close-after-ms N] [--screenshot PATH]"
            ),
        }
    }
    ensure!(
        !(smoke && script.is_some()),
        "Choose native smoke or a script"
    );
    let initial = match initial {
        Some(url) => url,
        None if smoke => url::Url::from_file_path(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../clj/tests/fixtures/servo.html")
                .canonicalize()?,
        )
        .map_err(|_| anyhow!("Invalid fixture URL"))?,
        None => url::Url::parse("about:blank")?,
    };
    let program = script.unwrap_or_else(|| {
        if smoke {
            include_str!("../../../../clj/tests/native.cljrs").into()
        } else {
            "(bulwark.shell/run! user/initial-url)".into()
        }
    });
    let source = format!(
        "(def initial-url \"{}\")\n(require 'bulwark.shell)\n{program}",
        initial.as_str().replace('\\', "\\\\").replace('"', "\\\"")
    );
    let event_loop = EventLoop::with_user_event().build()?;
    let mut app = App {
        surface: None,
        endpoint: None,
        worker: None,
        proxy: event_loop.create_proxy(),
        source,
        nrepl,
        mode,
        smoke,
        deadline: smoke.then(|| Instant::now() + Duration::from_secs(30)),
        close_after,
        close_at: None,
        repaint_at: None,
        stopping: None,
        intentional_stop: false,
        error: None,
        result: None,
        joined: false,
        capture,
    };
    event_loop.run_app(&mut app)?;
    let (frames, presentations, echo) = app.surface.as_ref().map_or((0, 0, ""), |s| {
        (
            s.engine.frame_count(),
            s.presentations,
            s.echo_presented.as_str(),
        )
    });
    println!(
        "Bulwark native: frames={frames}, presentations={presentations}, shutdown={}",
        if app.joined { "joined" } else { "unjoined" }
    );
    if let Some(error) = app.error {
        anyhow::bail!(error);
    }
    ensure!(app.joined, "Native host ended without joining the runtime");
    if app.smoke {
        ensure!(
            app.result.as_deref() == Some(":native-passed"),
            "Native Clojure assertions did not complete"
        );
        ensure!(
            frames > 0 && presentations > 0,
            "Native host did not paint and present"
        );
        ensure!(
            echo == "Native chrome smoke ready",
            "Clojure echo was not presented: {echo}"
        );
        println!("Phase 1 native passed: local page, egui echo, and joined worker");
    }
    Ok(())
}
