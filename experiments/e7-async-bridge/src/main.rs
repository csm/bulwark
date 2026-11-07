//! E7: independent engine and Clojurust executors with an asynchronous data boundary.
mod bridge;
mod engine;

use anyhow::{Result, ensure};
use bridge::{Command, Data, EventSink};
use bulwark_runtime::RuntimeHost;
use cljrs_runtime::{ExecutionMode, env::gc_roots};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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
    AwaitKeyword,
}

fn runtime_thread(
    sender: mpsc::Sender<bridge::Request>,
    events: mpsc::Receiver<bridge::Event>,
    engine_thread: thread::Thread,
    cancel: oneshot::Receiver<()>,
    interrupted: Arc<AtomicBool>,
    program_mode: Program,
    real_servo: bool,
) -> Result<Report> {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let local = tokio::task::LocalSet::new();
    executor.block_on(local.run_until(async move {
        let started = Instant::now();
        let mut host = RuntimeHost::new(ExecutionMode::Tiered)?;
        cljrs_async::init(&host.env().globals);
        let samples = Rc::new(RefCell::new(Vec::new()));
        bridge::install(&host.env().globals, sender, events, engine_thread, samples.clone(), real_servo);
        let _interrupt = cljrs_runtime::env::gas::InterruptGuard::install(interrupted);
        let source = if program_mode == Program::Idle {
            "(require '[clojure.core.async :refer [timeout]])
             (defn ^:async idle! [] (await (timeout 1000)) :async-bridge-passed) (idle!)"
        } else if program_mode == Program::AwaitKeyword { include_str!("../await-keyword.cljrs") }
        else { include_str!("../user.cljrs") };
        let program = host.eval(source)?;
        let root = gc_roots::root_value(&program);
        host.collect();
        let result = tokio::select! {
            result = cljrs_async::await_value(program) => result.map_err(|error| anyhow::anyhow!("{error}"))?,
            _ = cancel => anyhow::bail!("Engine stopped before the program completed"),
        };
        ensure!(result.to_string() == ":async-bridge-passed", "Unexpected Clojure result: {result}");
        drop(root);
        let stall_turns = if program_mode != Program::Example { "not run".into() } else {
            let program = host.eval(include_str!("../stall.cljrs"))?;
            let _root = gc_roots::root_value(&program);
            cljrs_async::await_value(program).await.map_err(|error| anyhow::anyhow!("{error}"))?.to_string()
        };
        let samples = samples.borrow().clone();
        Ok(Report { samples, stall_turns, elapsed: started.elapsed() })
    }))
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let real_servo = args.iter().any(|arg| arg == "--servo");
    let idle = args.iter().any(|arg| arg == "--idle");
    let keyword_probe = args.iter().any(|arg| arg == "--await-keyword-probe");
    ensure!(!idle || !keyword_probe, "Choose only one program");
    let program = if idle {
        Program::Idle
    } else if keyword_probe {
        Program::AwaitKeyword
    } else {
        Program::Example
    };
    let trace = args.iter().any(|arg| arg == "--trace");
    let (sender, mut requests) = mpsc::channel(bridge::CAPACITY);
    let (events, receiver) = mpsc::channel(bridge::CAPACITY);
    let events = EventSink::new(events);
    let mut engine = engine::create(real_servo, events.clone())?;
    let (cancel, cancelled) = oneshot::channel();
    let interrupted = Arc::new(AtomicBool::new(false));
    let worker_interrupt = interrupted.clone();
    let engine_thread = thread::current();
    let worker = thread::Builder::new()
        .name("bulwark-clojurust".into())
        .spawn(move || {
            runtime_thread(
                sender,
                receiver,
                engine_thread,
                cancelled,
                worker_interrupt,
                program,
                real_servo,
            )
        })?;
    let started = Instant::now();
    let mut turns = 0;
    let mut stall_started = None;
    let mut error = None;
    let mut last_request = 0;
    let mut handled = 0;
    let mut frames_during_stall = 0;
    while !worker.is_finished() {
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
    interrupted.store(true, Ordering::SeqCst);
    let _ = cancel.send(());
    drop(requests);
    drop(engine);
    let report = worker
        .join()
        .map_err(|_| anyhow::anyhow!("Runtime thread panicked"))?;
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
