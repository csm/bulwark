//! P3.1 probes through the unchanged product bridge and runtime worker.
use anyhow::{Result, ensure};
use bulwark_engine::{Command, Data, Engine, bridge};
use bulwark_runtime::{source_string, worker::Worker};
use cljrs_runtime::ExecutionMode;
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

fn run<E: Engine>(
    mode: ExecutionMode,
    source: String,
    create: impl FnOnce(bridge::EventSink, bridge::Wake) -> Result<E>,
    mut paint: impl FnMut(&mut E),
) -> Result<String> {
    let current = thread::current();
    let wake: bridge::Wake = Arc::new(move || current.unpark());
    let (runtime, mut endpoint) = bridge::channel(wake.clone());
    let mut engine = create(endpoint.events.clone(), wake)?;
    let mut worker = Worker::spawn_with_mode(runtime, source, mode)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut report = String::new();
    while !worker.finished() && !endpoint.control.is_stopped() && Instant::now() < deadline {
        endpoint.admit(|request| match &request.command {
            Command::SetEcho(text) => {
                report.clone_from(text);
                request.complete(Ok(Data::Nil));
            }
            Command::Log(text) => {
                println!("{text}");
                request.complete(Ok(Data::Nil));
            }
            _ => engine.handle(request),
        });
        engine.tick();
        paint(&mut engine);
        thread::park_timeout(Duration::from_millis(1));
    }
    let timed_out = Instant::now() >= deadline;
    let fault = endpoint.control.error();
    endpoint.stop_admission();
    engine.shutdown();
    if !worker.finished() {
        worker.stop();
    }
    let stopped = Instant::now() + Duration::from_secs(2);
    while !worker.finished() && Instant::now() < stopped {
        thread::park_timeout(Duration::from_millis(1));
    }
    // A process-level runner supplies a second, independent timeout.
    ensure!(
        worker.finished(),
        "P3.1 worker failed to stop within two seconds"
    );
    let result = worker.join_finished().expect("Finished worker")?;
    ensure!(!timed_out, "P3.1 probe exceeded its deadline");
    ensure!(fault.is_none(), "P3.1 bridge fault: {fault:?}");
    ensure!(
        result == ":phase3-passed",
        "Unexpected P3.1 result: {result}"
    );
    println!("P3.1 product worker shutdown=joined");
    Ok(report)
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.iter()
            .all(|a| ["--runtime", "--interpreter"].contains(&a.as_str())),
        "Usage: phase3 [--runtime] [--interpreter]"
    );
    let mode = if args.iter().any(|a| a == "--interpreter") {
        ExecutionMode::TreeWalk
    } else {
        ExecutionMode::Tiered
    };
    if args.iter().any(|a| a == "--runtime") {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../experiments/phase3/authority-data.txt")
            .canonicalize()?;
        let source = format!(
            "(def authority-fixture {})\n{}",
            source_string(fixture.to_str().expect("UTF-8 fixture path")),
            include_str!("../../../experiments/phase3/authority.cljrs")
        );
        let report = run(
            mode,
            source,
            |events, _| Ok(bulwark_engine::mock::MockEngine::new(events)),
            |_| {},
        )?;
        ensure!(
            report == "authority-gaps-reproduced",
            "Missing authority evidence"
        );
        return Ok(());
    }
    #[cfg(feature = "servo")]
    {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../clj/tests/fixtures/phase3-dom.html")
            .canonicalize()?;
        let url = url::Url::from_file_path(fixture).map_err(|_| anyhow::anyhow!("Fixture URL"))?;
        let source = format!(
            "(def phase3-fixture {})\n{}",
            source_string(url.as_str()),
            include_str!("../../../experiments/phase3/dom.cljrs")
        );
        let mut frames = 0;
        let report = run(
            mode,
            source,
            |events, wake| Ok(bulwark_engine_servo::ServoEngine::software(events, wake)?),
            |engine| {
                engine.paint();
                frames = engine.frame_count();
            },
        )?;
        ensure!(frames > 0, "No composed software frame");
        let mut samples: Vec<u64> = report
            .trim_matches(['[', ']'])
            .split_whitespace()
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()?;
        ensure!(samples.len() == 200, "Wrong number of timing samples");
        println!("P3.1 DOM raw nanoseconds: {samples:?}");
        samples.sort_unstable();
        println!(
            "P3.1 DOM prototype: mode={mode:?}, warmup=20, samples=200, p50_ns={}, p95_ns={}, max_ns={}, frames={frames}",
            samples[100], samples[190], samples[199]
        );
        println!(
            "P3.1 timing excludes future DOM handle registry and JSON schema conversion; C6 remains pending"
        );
        Ok(())
    }
    #[cfg(not(feature = "servo"))]
    anyhow::bail!("The DOM probe requires --features servo")
}
