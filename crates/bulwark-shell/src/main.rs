fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--headless-smoke"] {
        return headless_smoke();
    }
    anyhow::ensure!(args.is_empty(), "Usage: bulwark-shell [--headless-smoke]");
    let mut host = bulwark_runtime::RuntimeHost::new(cljrs_runtime::ExecutionMode::Tiered)?;
    println!("{}", host.eval("(bulwark.core/bootstrap-status)")?);
    Ok(())
}

/// Exercises product transport and runtime ownership without a native display.
fn headless_smoke() -> anyhow::Result<()> {
    use bulwark_engine::{Command, Data, Engine, Fault, bridge, mock::MockEngine};
    use std::{
        sync::Arc,
        thread,
        time::{Duration, Instant},
    };
    let main_thread = thread::current();
    let (runtime, mut endpoint) = bridge::channel(Arc::new(move || main_thread.unpark()));
    let mut engine = MockEngine::new(endpoint.events.clone());
    let mut worker = bulwark_runtime::worker::Worker::spawn(
        runtime,
        include_str!("../../../clj/tests/foundation.cljrs").into(),
    )?;
    let deadline = Instant::now() + Duration::from_secs(15);
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
    let result = result?;
    anyhow::ensure!(
        result == ":foundation-passed",
        "Unexpected runtime result: {result}"
    );
    anyhow::ensure!(
        echo == "redefined dispatcher",
        "Event handler did not update echo: {echo}"
    );
    println!(
        "Phase 1 foundation passed: native API, ordered events, live redefinition, and worker shutdown"
    );
    Ok(())
}
