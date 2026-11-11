#[cfg(feature = "native")]
mod native;
mod options;

fn main() -> anyhow::Result<()> {
    let options::Options { args, nrepl, mode } = options::parse(std::env::args().skip(1))?;
    if args == ["--nrepl-smoke"] {
        anyhow::ensure!(nrepl.is_some(), "--nrepl-smoke requires --nrepl");
        return nrepl_smoke(nrepl, mode);
    }
    if args == ["--headless-smoke"] {
        anyhow::ensure!(nrepl.is_none(), "Use --nrepl-smoke for nREPL checks");
        return headless_smoke(mode);
    }
    if args == ["--servo-smoke"] {
        anyhow::ensure!(nrepl.is_none(), "Use the native shell for Servo nREPL");
        #[cfg(feature = "servo")]
        return servo_smoke(mode);
        #[cfg(not(feature = "servo"))]
        anyhow::bail!("--servo-smoke requires --features servo");
    }
    #[cfg(feature = "native")]
    return native::run(&args, nrepl, mode);
    #[cfg(not(feature = "native"))]
    {
        anyhow::ensure!(
            nrepl.is_none(),
            "--nrepl requires --features native or --nrepl-smoke"
        );
        bootstrap(&args, mode)
    }
}

fn nrepl_smoke(
    nrepl: Option<bulwark_runtime::nrepl::Config>,
    mode: cljrs_runtime::ExecutionMode,
) -> anyhow::Result<()> {
    let report = run_headless(
        include_str!("../../../clj/tests/nrepl.cljrs").into(),
        |events, _| Ok(bulwark_engine::mock::MockEngine::new(events)),
        |_| {},
        nrepl,
        mode,
    )?;
    anyhow::ensure!(
        report.result == ":nrepl-passed",
        "Unexpected nREPL smoke result: {}",
        report.result
    );
    println!("Bulwark nREPL smoke: shutdown=joined");
    Ok(())
}

#[cfg(not(feature = "native"))]
fn bootstrap(args: &[String], mode: cljrs_runtime::ExecutionMode) -> anyhow::Result<()> {
    anyhow::ensure!(
        args.is_empty(),
        "Usage: bulwark-shell [--headless-smoke | --servo-smoke]; use --features native to open a window"
    );
    let mut host = bulwark_runtime::RuntimeHost::new(mode)?;
    println!("{}", host.eval("(bulwark.core/bootstrap-status)")?);
    Ok(())
}

/// Exercises product transport and runtime ownership without a native display.
fn headless_smoke(mode: cljrs_runtime::ExecutionMode) -> anyhow::Result<()> {
    let report = run_headless(
        include_str!("../../../clj/tests/foundation.cljrs").into(),
        |events, _| Ok(bulwark_engine::mock::MockEngine::new(events)),
        |_| {},
        None,
        mode,
    )?;
    anyhow::ensure!(
        report.result == ":foundation-passed",
        "Unexpected runtime result: {}",
        report.result
    );
    anyhow::ensure!(
        report.echo == "redefined dispatcher",
        "Event handler did not update echo: {}",
        report.echo
    );
    println!(
        "Phase 1 foundation passed: native API, ordered events, live redefinition, and worker shutdown"
    );
    Ok(())
}

#[cfg(feature = "servo")]
fn servo_smoke(mode: cljrs_runtime::ExecutionMode) -> anyhow::Result<()> {
    // File paths and fixture assertions belong to the smoke program, not browser policy.
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../clj/tests/fixtures/servo.html")
        .canonicalize()?;
    let url =
        url::Url::from_file_path(fixture).map_err(|_| anyhow::anyhow!("Invalid fixture path"))?;
    let source = format!(
        "(def servo-fixture-url \"{url}\")\n{}",
        include_str!("../../../clj/tests/servo.cljrs")
    );
    let mut frames = 0;
    let report = run_headless(
        source,
        |events, wake| Ok(bulwark_engine_servo::ServoEngine::software(events, wake)?),
        |engine| {
            engine.paint();
            frames = engine.frame_count();
        },
        None,
        mode,
    )?;
    anyhow::ensure!(
        report.result == ":servo-passed",
        "Unexpected runtime result: {}",
        report.result
    );
    anyhow::ensure!(
        report.echo == "Servo title reached Clojure",
        "Title handler did not update echo: {}",
        report.echo
    );
    anyhow::ensure!(frames > 0, "Servo did not paint a frame");
    println!(
        "Phase 1 Servo passed: local page, native API, Clojure title event, {frames} painted frames, and worker shutdown"
    );
    Ok(())
}

struct HeadlessReport {
    result: String,
    echo: String,
}

fn run_headless<E: bulwark_engine::Engine>(
    source: String,
    create: impl FnOnce(
        bulwark_engine::bridge::EventSink,
        bulwark_engine::bridge::Wake,
    ) -> anyhow::Result<E>,
    mut after_tick: impl FnMut(&mut E),
    nrepl: Option<bulwark_runtime::nrepl::Config>,
    mode: cljrs_runtime::ExecutionMode,
) -> anyhow::Result<HeadlessReport> {
    use bulwark_engine::{Command, Data, Fault, bridge};
    use std::{
        sync::Arc,
        thread,
        time::{Duration, Instant},
    };
    let main_thread = thread::current();
    let wake: bridge::Wake = Arc::new(move || main_thread.unpark());
    let (runtime, mut endpoint) = bridge::channel(wake.clone());
    let mut engine = create(endpoint.events.clone(), wake)?;
    let mut worker =
        bulwark_runtime::worker::Worker::spawn_configured(runtime, source, mode, nrepl)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut echo = String::new();
    let host_control = endpoint.control.clone();
    while !worker.finished() && !endpoint.control.is_stopped() && Instant::now() < deadline {
        endpoint.admit(|request| match &request.command {
            Command::Log(text) => {
                println!("{text}");
                request.complete(Ok(Data::Nil));
            }
            Command::SetEcho(text) => {
                echo.clone_from(text);
                request.complete(Ok(Data::Nil));
            }
            Command::Quit => {
                request.complete(Ok(Data::Nil));
                // Signals the host through a channel independent of events.
                // Handles and replies are cleaned up before worker cancellation.
                host_control.stop();
            }
            _ => engine.handle(request),
        });
        engine.tick();
        after_tick(&mut engine);
        thread::park_timeout(Duration::from_millis(1));
    }
    let timed_out = Instant::now() >= deadline;
    let fault = endpoint.control.error();
    endpoint.stop_admission();
    engine.shutdown();
    worker.stop();
    let shutdown_deadline = Instant::now() + Duration::from_secs(2);
    while !worker.finished() && Instant::now() < shutdown_deadline {
        thread::park_timeout(Duration::from_millis(1));
    }
    if !worker.finished() {
        eprintln!("Runtime failed to stop within two seconds");
        std::process::exit(1);
    }
    let result = worker
        .join_finished()
        .expect("Finished worker must be joinable");
    anyhow::ensure!(!timed_out, "Headless product smoke timed out");
    if let Some(fault) = fault {
        return Err(Fault::Engine(format!("Bridge failed: {fault}")).into());
    }
    Ok(HeadlessReport {
        result: result?,
        echo,
    })
}
