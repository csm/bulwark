//! E7's native winit driver. No runtime or GC value crosses onto this thread.
#[path = "input.rs"]
mod input;

use crate::{
    Program, Report, RuntimeConfig, Worker, WorkerPanic,
    bridge::{self, Command, Data, EventSink},
    engine::{Backend, real::ServoBackend},
};
use anyhow::{Result, ensure};
use servo::{EventLoopWaker, WindowRenderingContext};
use std::{
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
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

struct Surface {
    // The engine drops views and GL surfaces before the native window.
    engine: ServoBackend,
    window: Rc<Window>,
    input: input::Input,
}

#[derive(Clone, Copy, Debug)]
enum Stop {
    Completed,
    WindowClosed,
    TimedClose,
    Failed,
}

struct App {
    surface: Option<Surface>,
    worker: Option<Worker>,
    requests: Option<mpsc::Receiver<bridge::Request>>,
    events: Option<EventSink>,
    proxy: EventLoopProxy<()>,
    program: Program,
    close_after: Option<Duration>,
    close_at: Option<Instant>,
    deadline: Instant,
    stop: Option<Stop>,
    stop_deadline: Option<Instant>,
    error: Option<String>,
    report: Option<Report>,
    joined: bool,
    turns: u64,
    wakeups: u64,
    handled: u64,
    stall_started: Option<(u64, u64)>,
    stall_frames: u64,
    trace: bool,
    port_file: Option<std::path::PathBuf>,
}
impl App {
    fn start(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let window = Rc::new(
            event_loop.create_window(Window::default_attributes().with_title("Bulwark E7"))?,
        );
        let context = Rc::new(
            WindowRenderingContext::new(
                event_loop.display_handle()?,
                window.window_handle()?,
                window.inner_size(),
            )
            .map_err(|error| anyhow::anyhow!("Window context: {error:?}"))?,
        );
        let (sender, requests) = mpsc::channel(bridge::CAPACITY);
        let (events, receiver) = mpsc::channel(bridge::CAPACITY);
        let events = EventSink::new(events);
        let engine = ServoBackend::with_context(
            context,
            events.clone(),
            Box::new(Waker(self.proxy.clone())),
            Some(window.clone()),
        )?;
        self.surface = Some(Surface {
            engine,
            window,
            input: input::Input::default(),
        });
        self.requests = Some(requests);
        self.events = Some(events);
        let proxy = self.proxy.clone();
        self.worker = Some(Worker::spawn(
            sender,
            receiver,
            Arc::new(move || {
                let _ = proxy.send_event(());
            }),
            RuntimeConfig {
                program: self.program,
                real_servo: true,
                port_file: self.port_file.take(),
            },
        )?);
        self.close_at = self.close_after.map(|after| Instant::now() + after);
        println!("E7 native: window-ready");
        Ok(())
    }

    fn begin_stop(&mut self, reason: Stop) {
        if self.stop.is_some() {
            return;
        }
        self.stop = Some(reason);
        self.stop_deadline = Some(Instant::now() + Duration::from_secs(2));
        if let Some(worker) = &mut self.worker {
            worker.stop();
        }
        // Reject queued work and drop all pending engine replies before joining.
        self.requests.take();
        self.surface.take();
        self.events.take();
    }

    fn fail(&mut self, message: String) {
        if self.error.is_none() {
            self.error = Some(message);
        }
        self.begin_stop(Stop::Failed);
    }

    fn poll_stop(&mut self, event_loop: &ActiveEventLoop) {
        if self.worker.as_ref().is_none_or(Worker::finished) {
            if let Some(result) = self.worker.as_mut().and_then(Worker::join_finished) {
                match result {
                    Ok(report) => self.report = Some(report),
                    Err(error)
                        if matches!(self.stop, Some(Stop::WindowClosed | Stop::TimedClose))
                            && !error.is::<WorkerPanic>() =>
                    {
                        // Once close starts, cancellation and disconnected
                        // replies can race. Preserve the result for diagnostics;
                        // an orderly join satisfies this intentional stop.
                        println!("E7 native runtime stopped: {error}");
                    }
                    Err(error) => {
                        self.error.get_or_insert(error.to_string());
                    }
                }
            }
            self.joined = true;
            event_loop.exit();
        } else if self
            .stop_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.error.get_or_insert(
                "Runtime did not stop within two seconds; exiting the process without joining it"
                    .into(),
            );
            event_loop.exit();
        }
    }

    fn pump(&mut self, event_loop: &ActiveEventLoop) {
        if self.stop.is_some() {
            self.poll_stop(event_loop);
            return;
        }
        if let Some(result) = self.worker.as_mut().and_then(Worker::join_finished) {
            match result {
                Ok(report) => {
                    self.report = Some(report);
                    if self.program == Program::Example && self.stall_frames == 0 {
                        self.fail(
                            "Native window did not present a frame during the runtime stall".into(),
                        );
                    } else {
                        self.begin_stop(Stop::Completed);
                    }
                }
                Err(error) => self.fail(error.to_string()),
            }
            self.poll_stop(event_loop);
            return;
        }
        let Some(surface) = &mut self.surface else {
            return;
        };
        self.turns += 1;
        for _ in 0..4 {
            let Some(request) = self
                .requests
                .as_mut()
                .and_then(|queue| queue.try_recv().ok())
            else {
                break;
            };
            if request.reply.is_closed() {
                continue;
            }
            self.handled += 1;
            if self.trace {
                eprintln!("native request {}: {:?}", request.id, request.command);
            }
            match request.command {
                Command::StallBegin => {
                    self.stall_started = Some((self.turns, surface.engine.frame_count()));
                    let _ = request.reply.send(Ok(Data::Nil));
                }
                Command::StallEnd => {
                    let (turn, frame) = self
                        .stall_started
                        .take()
                        .unwrap_or((self.turns, surface.engine.frame_count()));
                    self.stall_frames = surface.engine.frame_count() - frame;
                    let _ = request.reply.send(Ok(Data::Count(self.turns - turn)));
                }
                _ => surface.engine.handle(request),
            }
        }
        surface.engine.tick();
        if let Some(error) = self.events.as_ref().and_then(EventSink::take_error) {
            self.fail(error);
        }
        if self
            .requests
            .as_ref()
            .is_some_and(|queue| !queue.is_empty())
        {
            let _ = self.proxy.send_event(());
        }
    }
}
impl ApplicationHandler<()> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.surface.is_some() || self.stop.is_some() {
            return;
        }
        if let Err(error) = self.start(event_loop) {
            self.fail(error.to_string());
        }
        self.pump(event_loop);
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, _: ()) {
        self.wakeups += 1;
        self.pump(event_loop);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(surface) = &mut self.surface else {
            return;
        };
        if id != surface.window.id() {
            return;
        }
        if matches!(event, WindowEvent::CloseRequested) {
            self.begin_stop(Stop::WindowClosed);
            self.poll_stop(event_loop);
            return;
        }
        if matches!(event, WindowEvent::RedrawRequested) {
            surface.engine.paint();
        } else if let Some(view) = surface.engine.view() {
            surface.input.forward(&view, event);
        }
        self.pump(event_loop);
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self.stop.is_none() {
            if now >= self.deadline {
                self.fail("Native experiment exceeded its 30-second deadline".into());
            } else if self.close_at.is_some_and(|close| now >= close) {
                self.begin_stop(Stop::TimedClose);
            }
        }
        if self.stop.is_some() {
            self.poll_stop(event_loop);
            // Poll only during teardown; normal command/Servo work uses proxy wakes.
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(5)));
        } else {
            self.pump(event_loop);
            let mut deadline = self
                .close_at
                .map_or(self.deadline, |close| close.min(self.deadline));
            // Completion can wake just before JoinHandle becomes finished.
            if self
                .requests
                .as_ref()
                .is_some_and(|queue| queue.is_closed())
            {
                deadline = deadline.min(now + Duration::from_millis(1));
            }
            if self.stop.is_some() {
                deadline = now;
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
    }
    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.begin_stop(Stop::Failed);
    }
}

pub fn run(args: &[String]) -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let modes = [
        ("--interactive", Program::Interactive),
        ("--idle", Program::Idle),
        ("--fail-runtime", Program::Fail),
        ("--panic-runtime", Program::Panic),
        ("--unresponsive-runtime", Program::Unresponsive),
        ("--nrepl", Program::Nrepl),
    ];
    ensure!(
        modes
            .iter()
            .filter(|(flag, _)| args.iter().any(|arg| arg == flag))
            .count()
            <= 1,
        "Choose only one native program"
    );
    ensure!(
        !args.iter().any(|arg| matches!(
            arg.as_str(),
            "--idle-executor" | "--idle-baseline" | "--await-keyword-probe"
        )),
        "This diagnostic requires the headless driver"
    );
    let program = modes
        .iter()
        .find(|(flag, _)| args.iter().any(|arg| arg == flag))
        .map_or(Program::Example, |(_, program)| *program);
    let close_after = args
        .iter()
        .position(|arg| arg == "--close-after-ms")
        .map(|index| -> Result<Duration> {
            let milliseconds: u64 = args
                .get(index + 1)
                .ok_or_else(|| anyhow::anyhow!("Missing close delay"))?
                .parse()?;
            ensure!(milliseconds > 0, "Close delay must be positive");
            Ok(Duration::from_millis(milliseconds))
        })
        .transpose()?;
    let event_loop = EventLoop::with_user_event().build()?;
    let mut app = App {
        surface: None,
        worker: None,
        requests: None,
        events: None,
        proxy: event_loop.create_proxy(),
        program,
        close_after,
        close_at: None,
        deadline: Instant::now() + Duration::from_secs(30),
        stop: None,
        stop_deadline: None,
        error: None,
        report: None,
        joined: false,
        turns: 0,
        wakeups: 0,
        handled: 0,
        stall_started: None,
        stall_frames: 0,
        trace: args.iter().any(|arg| arg == "--trace"),
        port_file: crate::port_file(args)?,
    };
    event_loop.run_app(&mut app)?;
    println!(
        "E7 native: reason={:?}, shutdown={}, engine_turns={}, proxy_wakeups={}, handled={}, presented_frames_during_stall={}",
        app.stop,
        if app.joined { "joined" } else { "unjoined" },
        app.turns,
        app.wakeups,
        app.handled,
        app.stall_frames
    );
    if let Some(report) = &app.report {
        println!(
            "E7 native runtime: elapsed={:?}, replies={}",
            report.elapsed,
            report.samples.len()
        );
    }
    if let Some(error) = app.error {
        anyhow::bail!(error);
    }
    ensure!(
        app.joined,
        "Native experiment ended without joining the runtime"
    );
    Ok(())
}
