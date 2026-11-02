// Upstream fiber and eval callbacks return EvalError by value.
#![allow(clippy::result_large_err)]

use anyhow::{Result, ensure};
use bulwark_runtime::RuntimeHost;
use cljrs_async::fiber;
use cljrs_gc::mutator::MutatorState;
use cljrs_runtime::{ExecutionMode, tiered::eval};
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

const FRAME: Duration = Duration::from_nanos(1_000_000_000 / 60);

struct Driver {
    // Keep executor state and runtime values on the shell thread.
    poller: cljrs_nrepl::Poller,
    local: Option<tokio::task::LocalSet>,
    /// Executor task roots retained between root-fiber drives.
    carrier: MutatorState,
    executor: tokio::runtime::Runtime,
    host: RuntimeHost,
    started: Instant,
    last_frame: Instant,
    next_frame: Instant,
    frames: u64,
    late_frames: u64,
    max_gap: Duration,
    duration: Duration,
}
impl Driver {
    fn new(duration: Duration) -> Result<Self> {
        let mut host = RuntimeHost::new(ExecutionMode::Tiered)?;
        let executor = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()?;
        let local = tokio::task::LocalSet::new();
        let mut carrier = MutatorState::fresh();
        local.block_on(&executor, async { cljrs_async::init(&host.env().globals) });
        // Top-level forms run on a root fiber, so they may park.
        host.eval_with(include_str!("../loop.cljrs"), |form, env| {
            fiber::run_root(&executor, &local, &mut carrier, || eval(form, env))
        })?;
        let poller = cljrs_nrepl::start(cljrs_nrepl::Config::default(), host.env().globals.clone())
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .into_poller();
        println!("nREPL: 127.0.0.1:{}", poller.port());
        let now = Instant::now();
        Ok(Self {
            poller,
            local: Some(local),
            carrier,
            executor,
            host,
            started: now,
            last_frame: now,
            next_frame: now + FRAME,
            frames: 0,
            late_frames: 0,
            max_gap: Duration::ZERO,
            duration,
        })
    }
    fn tick(&mut self) -> Result<bool> {
        let local = self.local.as_ref().unwrap();
        let (executor, carrier) = (&self.executor, &mut self.carrier);
        // Each nREPL form runs on a root fiber. A parked form lets other
        // fibers run, but this call returns only when the form completes.
        self.poller
            .poll(&mut |form: &cljrs_reader::Form, env: &mut _| {
                fiber::run_root(executor, local, carrier, || eval(form, env))
            });
        // The root parks on the slice timer while ready fibers run. Fibers
        // yield at safepoints when their time slice expires.
        fiber::run_root(executor, local, carrier, || {
            fiber::park_on(tokio::time::sleep(Duration::from_millis(1)));
            Ok(cljrs_value::Value::Nil)
        })
        .map_err(|error| anyhow::anyhow!("{error}"))?;
        let now = Instant::now();
        let gap = now.duration_since(self.last_frame);
        self.max_gap = self.max_gap.max(gap);
        if gap > FRAME + FRAME / 2 {
            self.late_frames += 1;
        }
        self.last_frame = now;
        self.next_frame += FRAME;
        while self.next_frame <= now {
            self.next_frame += FRAME;
        }
        self.frames += 1;
        Ok(now.duration_since(self.started) >= self.duration)
    }
    fn report(&mut self) -> Result<()> {
        let ticks = self.host.eval("@bulwark.experiment/async-ticks")?;
        let evals = self.host.eval("@bulwark.experiment/nrepl-evals")?;
        println!(
            "E1: elapsed={:?}, frames={}, gaps_over_25ms={}, max_gap={:?}, async_ticks={ticks}, nrepl_evals={evals}",
            self.started.elapsed(),
            self.frames,
            self.late_frames,
            self.max_gap
        );
        ensure!(
            ticks.to_string().parse::<u64>()? > 0,
            "Async loop did not advance"
        );
        // The full E1 criterion also requires a concurrent nREPL client and UI.
        Ok(())
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        if let Some(local) = self.local.take() {
            fiber::drop_local_with_carrier(local, &mut self.carrier);
        }
    }
}
struct App {
    driver: Driver,
    window: Option<Window>,
    error: Option<anyhow::Error>,
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.window = Some(
            event_loop
                .create_window(Window::default_attributes().with_title("Bulwark E1"))
                .expect("window"),
        );
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => match self.driver.tick() {
                Ok(done) => {
                    if let Some(window) = &self.window {
                        window.set_title(&format!("Bulwark E1 — frame {}", self.driver.frames));
                    }
                    if done {
                        event_loop.exit();
                    }
                }
                Err(error) => {
                    self.error = Some(error);
                    event_loop.exit();
                }
            },
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.driver.started.elapsed() >= self.driver.duration {
            event_loop.exit();
            return;
        }
        let next = self.driver.next_frame;
        if Instant::now() >= next
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(next));
    }
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let headless = args.iter().any(|a| a == "--headless");
    let duration = match args.iter().position(|a| a == "--seconds") {
        Some(index) => args
            .get(index + 1)
            .ok_or_else(|| anyhow::anyhow!("Missing --seconds value"))?
            .parse()?,
        None => 60,
    };
    let mut driver = Driver::new(Duration::from_secs(duration))?;
    if headless {
        loop {
            std::thread::sleep(driver.next_frame.saturating_duration_since(Instant::now()));
            if driver.tick()? {
                break;
            }
        }
        driver.report()
    } else {
        let mut app = App {
            driver,
            window: None,
            error: None,
        };
        EventLoop::new()?.run_app(&mut app)?;
        if let Some(error) = app.error {
            return Err(error);
        }
        app.driver.report()
    }
}
