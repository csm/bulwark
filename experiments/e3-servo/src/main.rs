/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! E3 uses only Servo's public embedding API. Adapted from its MPL-2.0
//! `examples/winit_minimal.rs`; see NOTICE.md for attribution.
use servo::{
    DevicePoint, EventLoopWaker, InputEvent, RenderingContext, Servo, ServoBuilder, WebView,
    WebViewBuilder, WindowRenderingContext,
};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::{Window, WindowId},
};

#[derive(Clone)]
struct Waker(EventLoopProxy<()>);
impl EventLoopWaker for Waker {
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(self.clone())
    }
    fn wake(&self) {
        let _ = self.0.send_event(());
    }
}

struct State {
    // Servo and its surfaces must drop before the native window.
    view: RefCell<Option<WebView>>,
    servo: Servo,
    context: Rc<WindowRenderingContext>,
    window: Window,
    cursor: Cell<DevicePoint>,
    modifiers: Cell<servo::Modifiers>,
}
impl servo::WebViewDelegate for State {
    fn notify_new_frame_ready(&self, _: WebView) {
        self.window.request_redraw();
    }
    fn notify_page_title_changed(&self, _: WebView, title: Option<String>) {
        println!("title: {title:?}");
        self.window
            .set_title(title.as_deref().unwrap_or("Bulwark E3"));
    }
    fn notify_url_changed(&self, _: WebView, url: url::Url) {
        println!("url: {url}");
    }
    fn notify_load_status_changed(&self, _: WebView, status: servo::LoadStatus) {
        println!("load: {status:?}");
    }
}
struct App {
    state: Option<Rc<State>>,
    waker: Waker,
    url: url::Url,
    screenshot: Option<PathBuf>,
    capture_result: Rc<RefCell<Option<Result<(), String>>>>,
    deadline: Option<Instant>,
}
impl ApplicationHandler<()> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let window = event_loop
            .create_window(Window::default_attributes().with_title("Bulwark E3"))
            .expect("window");
        let context = Rc::new(
            WindowRenderingContext::new(
                event_loop.display_handle().expect("display"),
                window.window_handle().expect("window handle"),
                window.inner_size(),
            )
            .expect("rendering context"),
        );
        context.make_current().expect("current GL context");
        let servo = ServoBuilder::default()
            .event_loop_waker(Box::new(self.waker.clone()))
            .build();
        let state = Rc::new(State {
            view: RefCell::new(None),
            servo,
            context,
            window,
            cursor: Cell::new(DevicePoint::zero()),
            modifiers: Cell::new(servo::Modifiers::empty()),
        });
        let view = WebViewBuilder::new(&state.servo, state.context.clone())
            .url(self.url.clone())
            .delegate(state.clone())
            .hidpi_scale_factor(euclid::Scale::new(state.window.scale_factor() as f32))
            .build();
        view.show();
        view.set_focused(true);
        if let Some(path) = self.screenshot.take() {
            let result = self.capture_result.clone();
            view.take_screenshot(None, move |image| {
                let captured = image
                    .map_err(|error| format!("{error:?}"))
                    .and_then(|image| image.save(&path).map_err(|error| error.to_string()));
                if captured.is_ok() {
                    println!("screenshot: {}", path.display());
                }
                *result.borrow_mut() = Some(captured);
            });
        }
        *state.view.borrow_mut() = Some(view);
        state.servo.spin_event_loop();
        self.state = Some(state);
    }
    fn user_event(&mut self, _: &ActiveEventLoop, _: ()) {
        if let Some(state) = &self.state {
            state.servo.spin_event_loop();
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(state) = &self.state else {
            return;
        };
        let views = state.view.borrow();
        let Some(view) = views.as_ref() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                view.paint();
                state.context.present();
            }
            WindowEvent::Resized(size) => view.resize(size),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                view.set_hidpi_scale_factor(euclid::Scale::new(scale_factor as f32));
            }
            WindowEvent::Focused(focused) => view.set_focused(focused),
            WindowEvent::CursorMoved { position, .. } => {
                let point = DevicePoint::new(position.x as f32, position.y as f32);
                state.cursor.set(point);
                view.notify_input_event(InputEvent::MouseMove(servo::MouseMoveEvent::new(
                    point.into(),
                )));
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => {
                let button = match button {
                    winit::event::MouseButton::Left => servo::MouseButton::Primary,
                    winit::event::MouseButton::Right => servo::MouseButton::Secondary,
                    winit::event::MouseButton::Middle => servo::MouseButton::Auxiliary,
                    winit::event::MouseButton::Back => servo::MouseButton::Back,
                    winit::event::MouseButton::Forward => servo::MouseButton::Forward,
                    winit::event::MouseButton::Other(n) => servo::MouseButton::Other(n),
                };
                let action = if button_state == ElementState::Pressed {
                    servo::MouseButtonAction::Down
                } else {
                    servo::MouseButtonAction::Up
                };
                view.notify_input_event(InputEvent::MouseButton(servo::MouseButtonEvent::new(
                    action,
                    button,
                    state.cursor.get().into(),
                )));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // Keep line deltas in line units. Servo converts them to pixels.
                let (x, y, mode) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        (x as f64, y as f64, servo::WheelMode::DeltaLine)
                    }
                    MouseScrollDelta::PixelDelta(p) => (p.x, p.y, servo::WheelMode::DeltaPixel),
                };
                view.notify_input_event(InputEvent::Wheel(servo::WheelEvent::new(
                    servo::WheelDelta { x, y, z: 0., mode },
                    state.cursor.get().into(),
                )));
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let keys = modifiers.state();
                let mut flags = servo::Modifiers::empty();
                flags.set(servo::Modifiers::SHIFT, keys.shift_key());
                flags.set(servo::Modifiers::CONTROL, keys.control_key());
                flags.set(servo::Modifiers::ALT, keys.alt_key());
                flags.set(servo::Modifiers::META, keys.super_key());
                state.modifiers.set(flags);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let key = match event.logical_key {
                    winit::keyboard::Key::Character(text) => {
                        servo::Key::Character(text.to_string())
                    }
                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::Space) => {
                        servo::Key::Character(" ".into())
                    }
                    winit::keyboard::Key::Named(named) => format!("{named:?}")
                        .parse()
                        .unwrap_or(servo::Key::Named(servo::NamedKey::Unidentified)),
                    _ => servo::Key::Named(servo::NamedKey::Unidentified),
                };
                let code = match event.physical_key {
                    winit::keyboard::PhysicalKey::Code(code) => format!("{code:?}")
                        .parse()
                        .unwrap_or(servo::Code::Unidentified),
                    _ => servo::Code::Unidentified,
                };
                let key_state = if event.state == ElementState::Pressed {
                    servo::KeyState::Down
                } else {
                    servo::KeyState::Up
                };
                let location = match event.location {
                    winit::keyboard::KeyLocation::Standard => servo::Location::Standard,
                    winit::keyboard::KeyLocation::Left => servo::Location::Left,
                    winit::keyboard::KeyLocation::Right => servo::Location::Right,
                    winit::keyboard::KeyLocation::Numpad => servo::Location::Numpad,
                };
                view.notify_input_event(InputEvent::Keyboard(
                    servo::KeyboardEvent::new_without_event(
                        key_state,
                        key,
                        code,
                        location,
                        state.modifiers.get(),
                        event.repeat,
                        false,
                    ),
                ));
            }
            _ => {}
        }
        // Every WebView request must be followed by a spin, even without a wake.
        drop(views);
        state.servo.spin_event_loop();
    }
    fn exiting(&mut self, _: &ActiveEventLoop) {
        if let Some(state) = self.state.take() {
            // Break the view -> delegate -> state ownership cycle.
            state.view.borrow_mut().take();
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.capture_result.borrow().is_some() {
            event_loop.exit();
        }
        if let Some(deadline) = self.deadline {
            if Instant::now() >= deadline {
                event_loop.exit();
            }
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(deadline));
        }
    }
}
fn main() -> anyhow::Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let mut args = std::env::args().skip(1);
    let first = args.next();
    let screenshot = if first.as_deref() == Some("--screenshot") {
        Some(PathBuf::from(
            args.next()
                .ok_or_else(|| anyhow::anyhow!("Missing screenshot path"))?,
        ))
    } else {
        None
    };
    let url_text = if screenshot.is_some() {
        args.next()
    } else {
        first
    };
    let url = url::Url::parse(&url_text.unwrap_or_else(|| "https://servo.org".into()))?;
    let capture_result = Rc::new(RefCell::new(None));
    let deadline = screenshot
        .as_ref()
        .map(|_| Instant::now() + Duration::from_secs(30));
    let event_loop = EventLoop::with_user_event().build()?;
    let mut app = App {
        state: None,
        waker: Waker(event_loop.create_proxy()),
        url,
        screenshot,
        capture_result: capture_result.clone(),
        deadline,
    };
    event_loop.run_app(&mut app)?;
    if deadline.is_some() {
        let result = capture_result
            .borrow_mut()
            .take()
            .ok_or_else(|| anyhow::anyhow!("Screenshot timed out"))?;
        result.map_err(anyhow::Error::msg)?;
    }
    Ok(())
}
