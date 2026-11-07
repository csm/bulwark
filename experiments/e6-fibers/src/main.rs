//! E6 probe: Clojure work on the shell thread with Clojurust fibers.
//!
//! One shell thread owns the runtime, a current-thread Tokio runtime, and its
//! LocalSet. A headless 60 Hz loop gives the LocalSet a bounded slice each
//! frame through a root fiber that parks on a timer. Each scenario records
//! frame gaps next to Clojure-side progress counters.

// Upstream fiber and eval callbacks return EvalError by value.
#![allow(clippy::result_large_err)]
use anyhow::{Context, Result, anyhow, ensure};
use bulwark_runtime::RuntimeHost;
use cljrs_async::fiber::{self, FiberResult};
use cljrs_gc::GcPtr;
use cljrs_gc::mutator::MutatorState;
use cljrs_runtime::ExecutionMode;
use cljrs_runtime::env::apply::apply_value;
use cljrs_runtime::tiered::{Env, eval};
use cljrs_value::{Arity, Keyword, MapValue, NativeFn, Value, ValueError, ValueResult};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

mod bencode;

const FRAME: Duration = Duration::from_nanos(1_000_000_000 / 60);
const PROBE: &str = include_str!("../probe.cljrs");

/// The shell thread's executor and runtime state.
struct Shell {
    rt: tokio::runtime::Runtime,
    local: Option<tokio::task::LocalSet>,
    /// Executor task roots retained between root-fiber drives.
    carrier: MutatorState,
    host: RuntimeHost,
    /// LocalSet slice per frame.
    budget: Duration,
}

impl Shell {
    fn new(mode: ExecutionMode, budget: Duration) -> Result<Self> {
        let host = RuntimeHost::new(mode)?;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()?;
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async { cljrs_async::init(&host.env().globals) });
        let mut shell = Self {
            rt,
            local: Some(local),
            carrier: MutatorState::fresh(),
            host,
            budget,
        };
        install_natives(&shell.host);
        shell.root_eval(PROBE)?;
        Ok(shell)
    }

    /// Evaluates top-level forms on a root fiber. The forms may park, but the
    /// shell thread does not return until the last form completes.
    fn root_eval(&mut self, source: &str) -> Result<Value> {
        let Self {
            rt,
            local,
            carrier,
            host,
            ..
        } = self;
        let local = local.as_ref().unwrap();
        host.eval_with(source, |form, env| {
            fiber::run_root(rt, local, carrier, || eval(form, env))
        })
    }

    /// Evaluates directly on the carrier inside the LocalSet context. Spawning
    /// works, but waits that would park are rejected or block the thread.
    fn carrier_eval(&mut self, source: &str) -> Result<Value> {
        let _rt = self.rt.enter();
        let _local = self.local.as_ref().unwrap().enter();
        self.host.eval(source)
    }

    fn long(&mut self, source: &str) -> Result<i64> {
        match self.carrier_eval(source)? {
            Value::Long(n) => Ok(n),
            other => Err(anyhow!("{source} returned {other}")),
        }
    }

    /// Runs ready fibers and tasks until the slice timer fires.
    fn drive(&mut self) -> Result<()> {
        let budget = self.budget;
        let local = self.local.as_ref().unwrap();
        fiber::run_root(&self.rt, local, &mut self.carrier, || {
            fiber::park_on(tokio::time::sleep(budget));
            Ok(Value::Nil)
        })
        .map_err(|error| anyhow!("{error}"))?;
        Ok(())
    }

    /// Spawns one Clojure call on its own fiber. The callee and argument stay
    /// rooted until the fiber starts and applies them.
    fn spawn_call(&mut self, function: &Value, arg: Value) -> Result<JoinHandle<FiberResult>> {
        let _rt = self.rt.enter();
        let _local = self.local.as_ref().unwrap().enter();
        let globals = self.host.env().globals.clone();
        let (function, captured) = (function.clone(), arg.clone());
        fiber::spawn_rooted(vec![function.clone(), arg], move || {
            let mut env = Env::new(globals, "user");
            apply_value(&function, vec![captured], &mut env)
        })
        .map_err(|error| anyhow!("{error}"))
    }

    /// Collects a finished fiber's result without driving other work.
    fn join(&mut self, handle: JoinHandle<FiberResult>) -> Result<Value> {
        let local = self.local.as_ref().unwrap();
        let result = local.block_on(&self.rt, handle)?;
        result.into_result().map_err(|error| anyhow!("{error}"))
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        if let Some(local) = self.local.take() {
            fiber::drop_local_with_carrier(local, &mut self.carrier);
        }
    }
}

// The native callback contract returns ValueError by value.
#[allow(clippy::result_large_err)]
fn millis(args: &[Value]) -> ValueResult<Duration> {
    match args {
        [Value::Long(ms)] if *ms >= 0 => Ok(Duration::from_millis(*ms as u64)),
        _ => Err(ValueError::Other("expected milliseconds".into())),
    }
}

/// Native functions that model host calls which block in Rust.
fn install_natives(host: &RuntimeHost) {
    let globals = &host.env().globals;
    // Blocks the OS thread, which pins every fiber on the isolate.
    globals.intern(
        "bulwark.native",
        "block-ms!".into(),
        Value::NativeFunction(GcPtr::new(NativeFn::new(
            "bulwark.native/block-ms!",
            Arity::Fixed(1),
            |args| {
                std::thread::sleep(millis(args)?);
                Ok(Value::Nil)
            },
        ))),
    );
    // Moves the same wait to the blocking worker pool and parks the caller.
    globals.intern(
        "bulwark.native",
        "offload-ms!".into(),
        Value::NativeFunction(GcPtr::new(NativeFn::new(
            "bulwark.native/offload-ms!",
            Arity::Fixed(1),
            |args| {
                let duration = millis(args)?;
                cljrs_value::parking::blocking(move || std::thread::sleep(duration));
                Ok(Value::Nil)
            },
        ))),
    );
    globals.mark_loaded("bulwark.native");
}

#[derive(Default)]
struct Frames {
    /// (frame end, gap since the previous frame end)
    samples: Vec<(Instant, Duration)>,
    max_drive: Duration,
}

impl Frames {
    fn max_gap(&self) -> Duration {
        self.samples.iter().map(|s| s.1).max().unwrap_or_default()
    }
    fn over(&self, limit: Duration) -> usize {
        self.samples.iter().filter(|s| s.1 > limit).count()
    }
    fn percentile(&self, p: f64) -> Duration {
        let mut gaps: Vec<_> = self.samples.iter().map(|s| s.1).collect();
        gaps.sort();
        gaps.get(((gaps.len() as f64 - 1.0) * p).round() as usize)
            .copied()
            .unwrap_or_default()
    }
    /// Largest gap of a frame that began inside a window (one frame of slack).
    fn max_in(&self, start: Instant, end: Instant) -> Duration {
        self.samples
            .iter()
            .filter(|(at, gap)| *at > start && *at - *gap + FRAME >= start && *at - *gap < end)
            .map(|s| s.1)
            .max()
            .unwrap_or_default()
    }
    fn summary(&self) -> String {
        format!(
            "frames={}, p50={:.1?}, p99={:.1?}, max_gap={:.1?}, gaps_over_25ms={}, max_drive={:.1?}",
            self.samples.len(),
            self.percentile(0.5),
            self.percentile(0.99),
            self.max_gap(),
            self.over(Duration::from_millis(25)),
            self.max_drive
        )
    }
}

/// Runs the frame loop. `work` is the host's per-frame work before the
/// LocalSet slice; it returns true to stop early.
fn run_frames(
    shell: &mut Shell,
    duration: Duration,
    mut work: impl FnMut(&mut Shell, u64) -> Result<bool>,
) -> Result<Frames> {
    let started = Instant::now();
    let mut frames = Frames::default();
    let mut last = started;
    let mut next = started + FRAME;
    let mut n = 0;
    while started.elapsed() < duration {
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
        let done = work(shell, n)?;
        let drive = Instant::now();
        shell.drive()?;
        frames.max_drive = frames.max_drive.max(drive.elapsed());
        let now = Instant::now();
        frames.samples.push((now, now - last));
        last = now;
        next += FRAME;
        while next <= now {
            next += FRAME;
        }
        n += 1;
        if done {
            break;
        }
    }
    Ok(frames)
}

fn idle(shell: &mut Shell, seconds: u64) -> Result<()> {
    let before = shell.long("@bulwark.experiment/async-ticks")?;
    let frames = run_frames(shell, Duration::from_secs(seconds), |_, _| Ok(false))?;
    let ticks = shell.long("@bulwark.experiment/async-ticks")? - before;
    println!("idle: go-loop ticks={ticks}; {}", frames.summary());
    ensure!(ticks > 0, "go-loop did not advance");
    Ok(())
}

/// A CPU-bound future runs beside the frame loop. Only safepoint time-slice
/// yields return control to the LocalSet and then to the host.
fn cpu_fiber(shell: &mut Shell, seconds: u64) -> Result<()> {
    shell.root_eval(
        "(in-ns 'bulwark.experiment)
         (reset! stop? false) (reset! cpu-progress 0)
         (def spinner (future (spin!)))",
    )?;
    let ticks = shell.long("@bulwark.experiment/async-ticks")?;
    let frames = run_frames(shell, Duration::from_secs(seconds), |_, _| Ok(false))?;
    let ticks = shell.long("@bulwark.experiment/async-ticks")? - ticks;
    shell.carrier_eval("(reset! bulwark.experiment/stop? true)")?;
    shell.root_eval("@bulwark.experiment/spinner")?;
    let progress = shell.long("@bulwark.experiment/cpu-progress")?;
    println!(
        "cpu-fiber: spin progress={progress}k iterations, go-loop ticks={ticks}; {}",
        frames.summary()
    );
    ensure!(progress > 0, "CPU fiber did not advance");
    Ok(())
}

/// The same one-second CPU loop on the carrier and then on a root fiber.
fn cpu_inline(shell: &mut Shell) -> Result<()> {
    const BUSY: &str = "(let [end (+ (System/currentTimeMillis) 1000)]
                          (loop [] (when (< (System/currentTimeMillis) end) (recur))))";
    for (label, on_root) in [("carrier", false), ("root-fiber", true)] {
        let ticks = shell.long("@bulwark.experiment/async-ticks")?;
        let frames = run_frames(shell, Duration::from_millis(1500), |shell, n| {
            if n == 10 {
                if on_root {
                    shell.root_eval(BUSY)?;
                } else {
                    shell.carrier_eval(BUSY)?;
                }
            }
            Ok(false)
        })?;
        let ticks = shell.long("@bulwark.experiment/async-ticks")? - ticks;
        println!(
            "cpu-inline/{label}: go-loop ticks over 1.5 s={ticks}; {}",
            frames.summary()
        );
    }
    Ok(())
}

/// Sleeping futures and a go block that dereferences a future both park.
fn parked_waits(shell: &mut Shell) -> Result<()> {
    let started = Instant::now();
    shell.root_eval(
        "(in-ns 'bulwark.experiment)
         (def sleepers (mapv (fn [i] (future (Thread/sleep 1000) i)) (range 100)))
         (def go-result (go (+ 1 @(future (Thread/sleep 300) 41))))",
    )?;
    let frames = run_frames(shell, Duration::from_millis(1500), |_, _| Ok(false))?;
    let sum = shell.root_eval("(reduce + (map deref bulwark.experiment/sleepers))")?;
    let from_go = shell.root_eval("(clojure.core.async/<!! bulwark.experiment/go-result)")?;
    println!(
        "parked-waits: 100 x (Thread/sleep 1000) futures sum={sum}, go+deref={from_go}, wall={:.2?}; {}",
        started.elapsed(),
        frames.summary()
    );
    ensure!(
        matches!(sum, Value::Long(4950)),
        "sleeping futures lost results"
    );
    ensure!(matches!(from_go, Value::Long(42)), "go deref result lost");
    Ok(())
}

/// A Rust call that blocks the thread pins the isolate. The offloaded variant parks.
fn native_blocking(shell: &mut Shell) -> Result<()> {
    for name in ["block-ms!", "offload-ms!"] {
        let started = Instant::now();
        shell.root_eval(&format!(
            "(def bulwark.experiment/native-wait (future (bulwark.native/{name} 300)))"
        ))?;
        let frames = run_frames(shell, Duration::from_millis(800), |_, _| Ok(false))?;
        shell.root_eval("@bulwark.experiment/native-wait")?;
        println!(
            "native/{name}: 300 ms Rust wait in a future, wall={:.2?}; {}",
            started.elapsed(),
            frames.summary()
        );
    }
    // The watchdog reports only when another fiber is ready. A timer-parked
    // go-loop never becomes ready while the carrier is blocked, so add a
    // CPU-bound fiber that is always runnable.
    shell.root_eval(
        "(in-ns 'bulwark.experiment)
         (reset! stop? false)
         (def spinner (future (spin!)))
         (def native-wait (future (bulwark.native/block-ms! 300)))",
    )?;
    let frames = run_frames(shell, Duration::from_millis(800), |_, _| Ok(false))?;
    shell.carrier_eval("(reset! bulwark.experiment/stop? true)")?;
    shell.root_eval("@bulwark.experiment/native-wait @bulwark.experiment/spinner")?;
    println!(
        "native/block-ms! beside a runnable fiber: {}",
        frames.summary()
    );
    Ok(())
}

fn event(sequence: i64) -> Value {
    Value::Map(MapValue::from_pairs(vec![
        (
            Value::keyword(Keyword::qualified("event", "type")),
            Value::keyword(Keyword::qualified("page", "title-changed")),
        ),
        (
            Value::keyword(Keyword::simple("sequence")),
            Value::Long(sequence),
        ),
    ]))
}

/// Host events dispatched one fiber per event, 100 per frame, with collection
/// every tenth frame while some handlers are parked.
fn dispatch(shell: &mut Shell, total: i64) -> Result<()> {
    shell.carrier_eval(
        "(reset! bulwark.experiment/delivered 0) (reset! bulwark.experiment/parked-deliveries 0)",
    )?;
    let function = shell
        .host
        .env()
        .globals
        .lookup_in_ns("bulwark.experiment", "dispatch!")
        .context("dispatch! is unbound")?;
    let started = Instant::now();
    let mut pending: Vec<(i64, JoinHandle<FiberResult>)> = Vec::new();
    let mut next = 1;
    let mut checked = 0;
    let mut live = Vec::new();
    let mut peak_fibers = 0;
    let frames = run_frames(shell, Duration::from_secs(60), |shell, n| {
        // The callee lookup result stays reachable through the var; each event
        // is rooted by spawn_rooted until its fiber runs.
        for _ in 0..100 {
            if next > total {
                break;
            }
            let _frame = cljrs_gc::push_alloc_frame();
            pending.push((next, shell.spawn_call(&function, event(next))?));
            next += 1;
        }
        let (finished, still): (Vec<_>, Vec<_>) = pending
            .drain(..)
            .partition(|(_, handle)| handle.is_finished());
        pending = still;
        for (sequence, handle) in finished {
            let value = shell.join(handle)?;
            ensure!(
                matches!(value, Value::Long(v) if v == sequence),
                "event {sequence} returned {value}"
            );
            checked += 1;
        }
        peak_fibers = peak_fibers.max(cljrs_gc::governor::snapshot().fiber_count);
        if n % 10 == 9 {
            let before = cljrs_gc::HEAP.count();
            let at = Instant::now();
            shell.host.collect();
            let count = cljrs_gc::HEAP.count();
            if std::env::var_os("E6_TRACE").is_some() {
                eprintln!(
                    "dispatch trace: frame={n} spawned={} pending={} fibers={} heap before={before} after={count} collect={:.1?}",
                    next - 1,
                    pending.len(),
                    cljrs_gc::governor::snapshot().fiber_count,
                    at.elapsed()
                );
            }
            if next > 1_000 {
                live.push(count);
            }
        }
        Ok(next > total && pending.is_empty())
    })?;
    let elapsed = started.elapsed();
    shell.host.collect();
    let drained = cljrs_gc::HEAP.count();
    let delivered = shell.long("@bulwark.experiment/delivered")?;
    let parked = shell.long("@bulwark.experiment/parked-deliveries")?;
    let (min_live, max_live) = (
        live.iter().min().copied().unwrap_or_default(),
        live.iter().max().copied().unwrap_or_default(),
    );
    println!(
        "dispatch: {checked} fibers checked, delivered={delivered}, parked={parked}, peak live fibers={peak_fibers}, live objects with backlog={min_live}..{max_live}, after drain={drained}, wall={elapsed:.2?}; {}",
        frames.summary()
    );
    ensure!(checked == total && delivered == total, "lost events");
    Ok(())
}

/// Per-event cost: direct host call versus one fiber per call.
fn dispatch_cost(shell: &mut Shell) -> Result<()> {
    const N: i64 = 10_000;
    shell.carrier_eval(
        "(in-ns 'bulwark.experiment) (defn record! [event] (swap! delivered inc) (:sequence event))",
    )?;
    let function = shell
        .host
        .env()
        .globals
        .lookup_in_ns("bulwark.experiment", "record!")
        .context("record! is unbound")?;
    let started = Instant::now();
    for sequence in 1..=N {
        let _frame = cljrs_gc::push_alloc_frame();
        shell
            .host
            .call("bulwark.experiment", "record!", vec![event(sequence)])?;
    }
    let direct = started.elapsed();
    let started = Instant::now();
    for chunk in (1..=N).collect::<Vec<_>>().chunks(100) {
        let mut handles = Vec::new();
        for &sequence in chunk {
            let _frame = cljrs_gc::push_alloc_frame();
            handles.push(shell.spawn_call(&function, event(sequence))?);
        }
        for handle in handles {
            shell.join(handle)?;
        }
    }
    let fibers = started.elapsed();
    println!(
        "dispatch-cost: direct={:.1} us/event, fiber={:.1} us/event (record!, no parks)",
        direct.as_secs_f64() * 1e6 / N as f64,
        fibers.as_secs_f64() * 1e6 / N as f64
    );
    Ok(())
}

/// A blocking nREPL client that records request windows.
struct Client {
    stream: TcpStream,
    buffer: Vec<u8>,
}

impl Client {
    fn connect(port: u16) -> Result<Self> {
        let stream = TcpStream::connect(("127.0.0.1", port))?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        Ok(Self {
            stream,
            buffer: Vec::new(),
        })
    }
    fn send(&mut self, fields: &[(&str, &str)]) -> Result<()> {
        self.stream.write_all(&bencode::encode_dict(fields))?;
        Ok(())
    }
    /// Reads responses for `id` until its status contains "done".
    fn until_done(&mut self, id: &str) -> Result<Vec<bencode::Dict>> {
        let mut responses = Vec::new();
        loop {
            while let Some((message, used)) = bencode::decode_dict(&self.buffer)? {
                self.buffer.drain(..used);
                if message.get("id").map(String::as_str) == Some(id) {
                    let done = message.statuses().iter().any(|s| s == "done");
                    responses.push(message);
                    if done {
                        return Ok(responses);
                    }
                }
            }
            let mut chunk = [0; 4096];
            let read = self.stream.read(&mut chunk)?;
            ensure!(read > 0, "nREPL closed the connection");
            self.buffer.extend_from_slice(&chunk[..read]);
        }
    }
    fn request(&mut self, id: &str, fields: &[(&str, &str)]) -> Result<Vec<bencode::Dict>> {
        let mut all = vec![("id", id)];
        all.extend_from_slice(fields);
        self.send(&all)?;
        self.until_done(id)
    }
}

struct Step {
    name: &'static str,
    start: Instant,
    end: Instant,
    outcome: String,
}

fn summarize(responses: &[bencode::Dict]) -> String {
    let mut parts = Vec::new();
    for message in responses {
        for key in ["value", "ex"] {
            if let Some(value) = message.get(key) {
                parts.push(format!("{key}={value}"));
            }
        }
        let statuses: Vec<_> = message
            .statuses()
            .into_iter()
            .filter(|s| s != "done")
            .collect();
        if !statuses.is_empty() {
            parts.push(format!("status={}", statuses.join(",")));
        }
    }
    parts.join(" ")
}

fn nrepl_script(port: u16) -> Result<Vec<Step>> {
    let mut steps = Vec::new();
    let mut a = Client::connect(port)?;
    let clone = a.request("c", &[("op", "clone")])?;
    let session = clone
        .iter()
        .find_map(|m| m.get("new-session").cloned())
        .context("clone returned no session")?;
    let step = |name, client: &mut Client, code: &str| -> Result<Step> {
        let start = Instant::now();
        let responses = client.request(
            name,
            &[("op", "eval"), ("session", &session), ("code", code)],
        )?;
        Ok(Step {
            name,
            start,
            end: Instant::now(),
            outcome: summarize(&responses),
        })
    };
    steps.push(step(
        "sleep-in-eval",
        &mut a,
        "(let [t @bulwark.experiment/async-ticks] (Thread/sleep 500) (- @bulwark.experiment/async-ticks t))",
    )?);
    steps.push(step(
        "sleep-in-future",
        &mut a,
        "(do (future (Thread/sleep 500)) :spawned)",
    )?);
    // Interrupt a busy eval from a second connection.
    let start = Instant::now();
    a.send(&[
        ("id", "busy"),
        ("op", "eval"),
        ("session", &session),
        ("code", "(loop [i 0] (recur (inc i)))"),
    ])?;
    std::thread::sleep(Duration::from_millis(300));
    let mut b = Client::connect(port)?;
    let sent = Instant::now();
    let reply = b.request(
        "i",
        &[
            ("op", "interrupt"),
            ("session", &session),
            ("interrupt-id", "busy"),
        ],
    )?;
    let busy = a.until_done("busy")?;
    let end = Instant::now();
    steps.push(Step {
        name: "interrupt-busy",
        start,
        end,
        outcome: format!(
            "interrupt reply: {}; eval: {}; interrupt-to-done={:.1?}",
            summarize(&reply),
            summarize(&busy),
            end - sent
        ),
    });
    // The same interrupt while the eval is parked rather than running.
    let start = Instant::now();
    a.send(&[
        ("id", "parked"),
        ("op", "eval"),
        ("session", &session),
        ("code", "(do (Thread/sleep 3000) :slept)"),
    ])?;
    std::thread::sleep(Duration::from_millis(300));
    let sent = Instant::now();
    let reply = b.request(
        "i2",
        &[
            ("op", "interrupt"),
            ("session", &session),
            ("interrupt-id", "parked"),
        ],
    )?;
    let parked = a.until_done("parked")?;
    let end = Instant::now();
    steps.push(Step {
        name: "interrupt-parked",
        start,
        end,
        outcome: format!(
            "interrupt reply: {}; eval: {}; interrupt-to-done={:.1?}",
            summarize(&reply),
            summarize(&parked),
            end - sent
        ),
    });
    steps.push(step("recovery", &mut a, "(+ 1 2)")?);
    Ok(steps)
}

/// nREPL jobs evaluate on root fibers inside the frame loop.
fn nrepl(shell: &mut Shell) -> Result<()> {
    let mut poller = cljrs_nrepl::start(
        cljrs_nrepl::Config::default(),
        shell.host.env().globals.clone(),
    )
    .map_err(|error| anyhow!("{error}"))?
    .into_poller();
    let port = poller.port();
    let (tx, rx) = mpsc::channel();
    let client = std::thread::spawn(move || tx.send(nrepl_script(port)));
    let mut steps = None;
    let frames = run_frames(shell, Duration::from_secs(30), |shell, _| {
        let Shell {
            rt, local, carrier, ..
        } = shell;
        let local = local.as_ref().unwrap();
        poller.poll(&mut |form: &cljrs_reader::Form, env: &mut Env| {
            fiber::run_root(rt, local, carrier, || eval(form, env))
        });
        if let Ok(result) = rx.try_recv() {
            steps = Some(result?);
            return Ok(true);
        }
        Ok(false)
    })?;
    client
        .join()
        .map_err(|_| anyhow!("nREPL client panicked"))??;
    for step in steps.context("nREPL client did not finish")? {
        println!(
            "nrepl/{}: request={:.1?}, max frame gap during request={:.1?}; {}",
            step.name,
            step.end - step.start,
            frames.max_in(step.start, step.end),
            step.outcome
        );
    }
    println!("nrepl: {}", frames.summary());
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let budget = Duration::from_millis(value("--budget-ms").map_or(Ok(4), |v| v.parse())?);
    let only = value("--only");
    let mode = match value("--mode").as_deref() {
        None | Some("tiered") => ExecutionMode::Tiered,
        Some("tree-walk") => ExecutionMode::TreeWalk,
        Some(other) => return Err(anyhow!("unknown mode {other}")),
    };
    println!(
        "E6 fibers: mode={mode:?}, LocalSet budget={budget:?}, CLJRS_FIBER_SLICE_US={}",
        std::env::var("CLJRS_FIBER_SLICE_US").unwrap_or_else(|_| "default (10000)".into())
    );
    let mut shell = Shell::new(mode, budget)?;
    // Warm up the first timer, go-loop iteration, and root-fiber stack.
    run_frames(&mut shell, Duration::from_millis(300), |_, _| Ok(false))?;
    let run = |name: &str| {
        let selected = only.as_deref().is_none_or(|o| o == name);
        if selected && std::env::var_os("E6_TRACE").is_some() {
            eprintln!("heap before {name}: {}", cljrs_gc::HEAP.count());
        }
        selected
    };
    if run("idle") {
        idle(&mut shell, 3)?;
    }
    if run("cpu-fiber") {
        cpu_fiber(&mut shell, 3)?;
    }
    if run("cpu-inline") {
        cpu_inline(&mut shell)?;
    }
    if run("parked-waits") {
        parked_waits(&mut shell)?;
    }
    if run("native") {
        native_blocking(&mut shell)?;
    }
    if run("dispatch") {
        dispatch(&mut shell, 10_000)?;
    }
    if run("dispatch-cost") {
        dispatch_cost(&mut shell)?;
    }
    if run("nrepl") {
        nrepl(&mut shell)?;
    }
    if std::env::var_os("E6_TRACE").is_some() {
        for pass in 1..=3 {
            let at = Instant::now();
            let before = cljrs_gc::HEAP.count();
            shell.host.collect();
            eprintln!(
                "final collect {pass}: {before} -> {} in {:.1?}",
                cljrs_gc::HEAP.count(),
                at.elapsed()
            );
        }
    }
    let memory = cljrs_gc::governor::snapshot();
    println!(
        "fibers: live={}, peak={}, pooled stacks={}, reserved stack bytes={}",
        memory.fiber_count,
        memory.peak_fiber_count,
        memory.pooled_fiber_stacks,
        memory.reserved_fiber_stack_bytes
    );
    Ok(())
}
