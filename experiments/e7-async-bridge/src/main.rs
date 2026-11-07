//! E7: independent engine and Clojurust executors with an asynchronous data boundary.
mod bridge;
mod engine;
#[cfg(feature = "servo")]
mod native;
mod nrepl;

use anyhow::{Result, ensure};
use bridge::{Command, Data, EventSink};
use bulwark_runtime::RuntimeHost;
use cljrs_runtime::{
    ExecutionMode,
    env::{gas::InterruptFlag, gc_roots},
};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};

struct Report {
    samples: Vec<Duration>,
    stall_turns: String,
    elapsed: Duration,
}

#[derive(Clone, Copy, PartialEq)]
enum Program {
    Example,
    Idle,
    IdleExecutor,
    IdleBaseline,
    AwaitKeyword,
    Interactive,
    Fail,
    Panic,
    Unresponsive,
    Nrepl,
}

struct RuntimeConfig {
    program: Program,
    real_servo: bool,
    port_file: Option<PathBuf>,
}

fn port_file(args: &[String]) -> Result<Option<PathBuf>> {
    args.iter()
        .position(|arg| arg == "--nrepl-port-file")
        .map(|index| {
            ensure!(
                args.iter().any(|arg| arg == "--nrepl"),
                "Port file requires --nrepl"
            );
            let path = args
                .get(index + 1)
                .ok_or_else(|| anyhow::anyhow!("Missing nREPL port file path"))?;
            ensure!(
                !path.is_empty() && !path.starts_with("--"),
                "Missing nREPL port file path"
            );
            Ok(PathBuf::from(path))
        })
        .transpose()
}

fn runtime_thread(
    sender: mpsc::Sender<bridge::Request>,
    events: mpsc::Receiver<bridge::Event>,
    wake: bridge::Wake,
    cancel: oneshot::Receiver<()>,
    interrupted: Arc<InterruptFlag>,
    config: RuntimeConfig,
) -> Result<Report> {
    let RuntimeConfig {
        program: program_mode,
        real_servo,
        port_file,
    } = config;
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let local = tokio::task::LocalSet::new();
    let local_ref = &local;
    executor.block_on(local.run_until(async move {
        let started = Instant::now();
        let mut host = RuntimeHost::new(ExecutionMode::Tiered)?;
        if program_mode != Program::IdleBaseline {
            cljrs_async::init(&host.env().globals);
        }
        if matches!(program_mode, Program::IdleExecutor | Program::IdleBaseline) {
            // No Clojure future or fiber: isolate init's background service.
            tokio::time::sleep(Duration::from_secs(1)).await;
            return Ok(Report {
                samples: Vec::new(),
                stall_turns: "not run".into(),
                elapsed: started.elapsed(),
            });
        }
        let samples = Rc::new(RefCell::new(Vec::new()));
        bridge::install(
            &host.env().globals,
            sender,
            events,
            wake,
            samples.clone(),
            real_servo,
        );
        let _interrupt = cljrs_runtime::env::gas::InterruptGuard::install(interrupted);
        let source = if program_mode == Program::Idle {
            "(require '[clojure.core.async :refer [timeout]])
             (defn ^:async idle! [] (await (timeout 1000)) :async-bridge-passed) (idle!)"
        } else if program_mode == Program::AwaitKeyword {
            include_str!("../await-keyword.cljrs")
        } else if program_mode == Program::Interactive {
            include_str!("../native.cljrs")
        } else if program_mode == Program::Nrepl {
            include_str!("../nrepl.cljrs")
        } else if program_mode == Program::Fail {
            "(throw (ex-info \"Injected runtime failure\" {}))"
        } else if program_mode == Program::Panic {
            panic!("Injected runtime panic");
        } else if program_mode == Program::Unresponsive {
            // Probe an uncooperative native extension, not a fiber-aware sleep.
            std::thread::sleep(Duration::from_secs(5));
            return Err(anyhow::anyhow!("Unresponsive probe returned"));
        } else {
            include_str!("../user.cljrs")
        };
        let program = host.eval(source)?;
        let root = gc_roots::root_value(&program);
        host.collect();
        let mut nrepl = if program_mode == Program::Nrepl {
            Some(nrepl::Driver::start(host.env().globals.clone(), port_file)?)
        } else {
            None
        };
        let mut program = Box::pin(cljrs_async::await_value(program));
        let mut cancel = cancel;
        let result = loop {
            tokio::select! {
                result = &mut program => break result.map_err(|error| anyhow::anyhow!("{error}"))?,
                _ = &mut cancel => anyhow::bail!("Engine stopped before the program completed"),
                _ = nrepl::next_turn(&mut nrepl) => {
                    if let Some(driver) = &mut nrepl { driver.admit(local_ref)?; }
                }
            }
        };
        ensure!(
            result.to_string() == ":async-bridge-passed",
            "Unexpected Clojure result: {result}"
        );
        drop(program);
        drop(root);
        let stall_turns = if program_mode != Program::Example {
            "not run".into()
        } else {
            let program = host.eval(include_str!("../stall.cljrs"))?;
            let _root = gc_roots::root_value(&program);
            cljrs_async::await_value(program)
                .await
                .map_err(|error| anyhow::anyhow!("{error}"))?
                .to_string()
        };
        let samples = samples.borrow().clone();
        Ok(Report {
            samples,
            stall_turns,
            elapsed: started.elapsed(),
        })
    }))
}

struct Worker {
    thread: Option<thread::JoinHandle<Result<Report>>>,
    interrupt: Arc<InterruptFlag>,
    cancel: Option<oneshot::Sender<()>>,
}

#[derive(Debug)]
struct WorkerPanic;
impl std::fmt::Display for WorkerPanic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Runtime thread panicked")
    }
}
impl std::error::Error for WorkerPanic {}

impl Worker {
    fn spawn(
        sender: mpsc::Sender<bridge::Request>,
        events: mpsc::Receiver<bridge::Event>,
        wake: bridge::Wake,
        config: RuntimeConfig,
    ) -> Result<Self> {
        let (cancel, cancelled) = oneshot::channel();
        let interrupt = Arc::new(InterruptFlag::new(false));
        let worker_interrupt = interrupt.clone();
        let stopped = wake.clone();
        let thread = thread::Builder::new()
            .name("bulwark-clojurust".into())
            .spawn(move || {
                // A panic must wake the native loop just as a returned error does.
                struct WakeOnDrop(bridge::Wake);
                impl Drop for WakeOnDrop {
                    fn drop(&mut self) {
                        (self.0)();
                    }
                }
                let _wake = WakeOnDrop(stopped);
                runtime_thread(sender, events, wake, cancelled, worker_interrupt, config)
            })?;
        Ok(Self {
            thread: Some(thread),
            interrupt,
            cancel: Some(cancel),
        })
    }
    fn stop(&mut self) {
        self.interrupt.request();
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
    }
    fn finished(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(|worker| worker.is_finished())
    }
    fn join_finished(&mut self) -> Option<Result<Report>> {
        if !self.finished() {
            return None;
        }
        self.thread
            .take()
            .map(|worker| worker.join().unwrap_or_else(|_| Err(WorkerPanic.into())))
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--native") {
        #[cfg(feature = "servo")]
        return native::run(&args);
        #[cfg(not(feature = "servo"))]
        anyhow::bail!("--native requires --features servo");
    }
    let real_servo = args.iter().any(|arg| arg == "--servo");
    let idle = args.iter().any(|arg| arg == "--idle");
    let idle_executor = args.iter().any(|arg| arg == "--idle-executor");
    let idle_baseline = args.iter().any(|arg| arg == "--idle-baseline");
    let keyword_probe = args.iter().any(|arg| arg == "--await-keyword-probe");
    let nrepl = args.iter().any(|arg| arg == "--nrepl");
    ensure!(
        [idle, idle_executor, idle_baseline, keyword_probe, nrepl]
            .iter()
            .filter(|&&enabled| enabled)
            .count()
            <= 1,
        "Choose only one program"
    );
    let program = if idle {
        Program::Idle
    } else if idle_executor {
        Program::IdleExecutor
    } else if idle_baseline {
        Program::IdleBaseline
    } else if keyword_probe {
        Program::AwaitKeyword
    } else if nrepl {
        Program::Nrepl
    } else {
        Program::Example
    };
    let trace = args.iter().any(|arg| arg == "--trace");
    let (sender, mut requests) = mpsc::channel(bridge::CAPACITY);
    let (events, receiver) = mpsc::channel(bridge::CAPACITY);
    let events = EventSink::new(events);
    let mut engine = engine::create(real_servo, events.clone())?;
    let engine_thread = thread::current();
    let mut worker = Worker::spawn(
        sender,
        receiver,
        Arc::new(move || engine_thread.unpark()),
        RuntimeConfig {
            program,
            real_servo,
            port_file: port_file(&args)?,
        },
    )?;
    let started = Instant::now();
    let mut turns = 0;
    let mut stall_started = None;
    let mut error = None;
    let mut last_request = 0;
    let mut handled = 0;
    let mut frames_during_stall = 0;
    while !worker.finished() {
        turns += 1;
        // A bounded amount of command work precedes every Servo turn.
        for _ in 0..4 {
            let Ok(request) = requests.try_recv() else {
                break;
            };
            if request.reply.is_closed() {
                continue;
            }
            if trace {
                eprintln!("request {}: {:?}", request.id, request.command);
            }
            last_request = last_request.max(request.id);
            handled += 1;
            match request.command {
                Command::StallBegin => {
                    stall_started = Some((turns, engine.frame_count()));
                    let _ = request.reply.send(Ok(Data::Nil));
                }
                Command::StallEnd => {
                    let (begin_turn, begin_frame) = stall_started
                        .take()
                        .unwrap_or((turns, engine.frame_count()));
                    frames_during_stall = engine.frame_count() - begin_frame;
                    let _ = request.reply.send(Ok(Data::Count(turns - begin_turn)));
                }
                _ => engine.handle(request),
            }
        }
        engine.tick();
        if let Some(failure) = events.take_error() {
            error = Some(failure);
            break;
        }
        if started.elapsed() > Duration::from_secs(30) {
            error = Some("Experiment timed out".into());
            break;
        }
        thread::park_timeout(Duration::from_millis(1));
    }
    worker.stop();
    drop(requests);
    drop(engine);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !worker.finished() && Instant::now() < deadline {
        thread::park_timeout(Duration::from_millis(1));
    }
    let report = worker.join_finished().ok_or_else(|| {
        anyhow::anyhow!("Runtime did not stop within two seconds; process must exit")
    })?;
    if let Some(error) = error {
        anyhow::bail!(error);
    }
    let mut report = report?;
    ensure!(
        !real_servo || program != Program::Example || frames_during_stall > 0,
        "Servo did not paint during the runtime stall"
    );
    report.samples.sort();
    let median = report
        .samples
        .get(report.samples.len() / 2)
        .copied()
        .unwrap_or_default();
    let max = report.samples.last().copied().unwrap_or_default();
    println!(
        "E7: backend={}, elapsed={:?}, engine_turns={turns}, handled={handled}, last_request_id={last_request}, completed={}, request_p50={median:?}, request_max={max:?}, engine_turns_during_200ms_runtime_stall={}, painted_frames_during_stall={frames_during_stall}",
        if real_servo { "servo" } else { "mock" },
        report.elapsed,
        report.samples.len(),
        report.stall_turns
    );
    Ok(())
}
