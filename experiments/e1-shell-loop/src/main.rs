use anyhow::{Result, ensure};
use bulwark_runtime::RuntimeHost;
use cljrs_runtime::{ExecutionMode, tiered::eval};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

const FRAME: Duration = Duration::from_nanos(1_000_000_000 / 60);

struct FrameClock {
    started: Instant,
    last_frame: Instant,
    next_frame: Instant,
    frames: u64,
    late_frames: u64,
    max_gap: Duration,
    max_gap_at_frame: u64,
    skipped_deadlines: u64,
    duration: Duration,
}
impl FrameClock {
    fn new(duration: Duration) -> Self {
        let now = Instant::now();
        Self {
            started: now,
            last_frame: now,
            next_frame: now + FRAME,
            frames: 0,
            late_frames: 0,
            max_gap: Duration::ZERO,
            max_gap_at_frame: 0,
            skipped_deadlines: 0,
            duration,
        }
    }
    fn tick(&mut self) -> bool {
        self.tick_at(Instant::now())
    }
    fn tick_at(&mut self, now: Instant) -> bool {
        let gap = now.duration_since(self.last_frame);
        if gap > self.max_gap {
            self.max_gap = gap;
            self.max_gap_at_frame = self.frames + 1;
        }
        if gap > FRAME + FRAME / 2 {
            self.late_frames += 1;
        }
        self.last_frame = now;
        self.next_frame += FRAME;
        while self.next_frame <= now {
            self.next_frame += FRAME;
            self.skipped_deadlines += 1;
        }
        self.frames += 1;
        now.duration_since(self.started) >= self.duration
    }
    fn report(&self, label: &str) {
        println!(
            "{label}: elapsed={:?}, frames={}, gaps_over_25ms={}, max_gap={:?}, max_gap_at_frame={}, skipped_deadlines={}",
            self.started.elapsed(),
            self.frames,
            self.late_frames,
            self.max_gap,
            self.max_gap_at_frame,
            self.skipped_deadlines,
        );
    }
}
struct Driver {
    // Construct and drop these values on their owning thread, in either mode.
    poller: cljrs_nrepl::Poller,
    local: tokio::task::LocalSet,
    executor: tokio::runtime::Runtime,
    host: RuntimeHost,
    clock: FrameClock,
    max_poll: Duration,
    max_slice: Duration,
}
impl Driver {
    fn new(duration: Duration) -> Result<Self> {
        let mut host = RuntimeHost::new(ExecutionMode::Tiered)?;
        let executor = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()?;
        let local = tokio::task::LocalSet::new();
        executor.block_on(local.run_until(async {
            cljrs_async::init(&host.env().globals);
            host.eval(include_str!("../loop.cljrs"))
        }))?;
        let poller = cljrs_nrepl::start(cljrs_nrepl::Config::default(), host.env().globals.clone())
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .into_poller();
        println!("nREPL: 127.0.0.1:{}", poller.port());
        Ok(Self {
            poller,
            local,
            executor,
            host,
            clock: FrameClock::new(duration),
            max_poll: Duration::ZERO,
            max_slice: Duration::ZERO,
        })
    }
    fn tick(&mut self) -> Result<bool> {
        let poll_started = Instant::now();
        // Enter both runtimes while polling: nREPL eval can spawn local tasks.
        let executor_guard = self.executor.enter();
        let local_guard = self.local.enter();
        self.poller.poll_timeout(&mut eval, Duration::ZERO);
        drop(local_guard);
        drop(executor_guard);
        self.max_poll = self.max_poll.max(poll_started.elapsed());
        // This is a cooperative slice, not preemption. A task can overrun it.
        let slice_started = Instant::now();
        self.executor.block_on(self.local.run_until(async {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }));
        self.max_slice = self.max_slice.max(slice_started.elapsed());
        Ok(self.clock.tick())
    }
    fn report(&mut self) -> Result<()> {
        let ticks = self.host.eval("@bulwark.experiment/async-ticks")?;
        let evals = self.host.eval("@bulwark.experiment/nrepl-evals")?;
        println!(
            "E1: max_poll={:?}, max_slice={:?}, async_ticks={ticks}, nrepl_evals={evals}",
            self.max_poll, self.max_slice,
        );
        self.clock.report("E1 runtime");
        ensure!(
            ticks.to_string().parse::<u64>()? > 0,
            "Async loop did not advance"
        );
        // The full E1 criterion also requires a concurrent nREPL client and UI.
        Ok(())
    }
}
struct App {
    driver: Option<Driver>,
    clock: FrameClock,
    window: Option<Window>,
    first_redraw: Option<Duration>,
    error: Option<anyhow::Error>,
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        self.window = Some(
            event_loop
                .create_window(Window::default_attributes().with_title("Bulwark E1"))
                .expect("window"),
        );
        // Record the delay from window creation to the initial redraw separately.
        self.clock = FrameClock::new(self.clock.duration);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                if self.first_redraw.is_none() {
                    self.first_redraw = Some(self.clock.started.elapsed());
                    self.clock = FrameClock::new(self.clock.duration);
                    if let Some(driver) = &mut self.driver {
                        driver.clock = FrameClock::new(driver.clock.duration);
                    }
                    return;
                }
                // Creation, exposure, and resizing can request extra redraws.
                // Only scheduled frames may drive the runtime or advance its clock.
                if Instant::now() < self.clock.next_frame {
                    return;
                }
                if let Some(driver) = &mut self.driver
                    && let Err(error) = driver.tick()
                {
                    self.error = Some(error);
                    event_loop.exit();
                    return;
                }
                let done = self.clock.tick();
                if let Some(window) = &self.window {
                    window.set_title(&format!("Bulwark E1 — frame {}", self.clock.frames));
                }
                if done {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.clock.started.elapsed() >= self.clock.duration {
            event_loop.exit();
            return;
        }
        let next = self.clock.next_frame;
        if Instant::now() >= next
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(next));
    }
}

fn run_headless(mut driver: Driver, stop: &AtomicBool) -> Result<()> {
    while !stop.load(Ordering::Relaxed) {
        thread::sleep(
            driver
                .clock
                .next_frame
                .saturating_duration_since(Instant::now()),
        );
        if driver.tick()? {
            break;
        }
    }
    driver.report()
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let headless = args.iter().any(|a| a == "--headless");
    let worker = args.iter().any(|a| a == "--worker");
    ensure!(!worker || !headless, "--worker requires a native window");
    let duration = match args.iter().position(|a| a == "--seconds") {
        Some(index) => args
            .get(index + 1)
            .ok_or_else(|| anyhow::anyhow!("Missing --seconds value"))?
            .parse()?,
        None => 60,
    };
    let duration = Duration::from_secs(duration);
    let stop = Arc::new(AtomicBool::new(false));
    if headless {
        run_headless(Driver::new(duration)?, &stop)
    } else {
        // The fallback moves ownership, never a non-Send runtime value. This
        // measures scheduling isolation only; a browser command queue is future work.
        let worker_thread: Option<JoinHandle<Result<()>>> = worker.then(|| {
            let stop = stop.clone();
            thread::spawn(move || run_headless(Driver::new(duration)?, &stop))
        });
        let mut app = App {
            driver: if worker {
                None
            } else {
                Some(Driver::new(duration)?)
            },
            clock: FrameClock::new(duration),
            window: None,
            first_redraw: None,
            error: None,
        };
        let result = EventLoop::new().and_then(|event_loop| event_loop.run_app(&mut app));
        stop.store(true, Ordering::Relaxed);
        if let Some(worker_thread) = worker_thread {
            worker_thread
                .join()
                .map_err(|_| anyhow::anyhow!("Runtime worker panicked"))??;
        }
        result?;
        if let Some(error) = app.error {
            return Err(error);
        }
        if let Some(driver) = &mut app.driver {
            driver.report()?;
        }
        app.clock.report("E1 UI");
        println!("E1 startup: window_to_first_redraw={:?}", app.first_redraw);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_time_frames_keep_the_original_schedule() {
        let mut clock = FrameClock::new(Duration::from_secs(1));
        let start = clock.started;
        for n in 1..=60 {
            clock.tick_at(start + FRAME * n);
        }
        assert_eq!(clock.frames, 60);
        assert_eq!(clock.skipped_deadlines, 0);
        assert_eq!(clock.late_frames, 0);
        assert_eq!(clock.next_frame, start + FRAME * 61);
    }

    #[test]
    fn a_stall_counts_skipped_slots_without_catch_up_frames() {
        let mut clock = FrameClock::new(Duration::from_secs(1));
        let start = clock.started;
        clock.tick_at(start + FRAME * 4);
        assert_eq!(clock.frames, 1);
        assert_eq!(clock.skipped_deadlines, 3);
        assert_eq!(clock.late_frames, 1);
        assert_eq!(clock.max_gap, FRAME * 4);
        assert_eq!(clock.max_gap_at_frame, 1);
        assert_eq!(clock.next_frame, start + FRAME * 5);
        clock.tick_at(start + FRAME * 5);
        assert_eq!(clock.frames, 2);
        assert_eq!(clock.skipped_deadlines, 3);
    }
}
