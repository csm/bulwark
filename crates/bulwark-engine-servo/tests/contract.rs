//! A main-thread test executable, because native graphics initialization can require it.
use bulwark_engine::{
    Command, Data, Engine, Fault,
    bridge::{self, Client, DEFAULT_TIMEOUT, EngineEndpoint, Pending},
};
use bulwark_engine_servo::ServoEngine;
use std::{sync::Arc, time::Duration};

fn admit(
    client: &Client,
    endpoint: &mut EngineEndpoint,
    engine: &mut ServoEngine,
    command: Command,
) -> Pending {
    let pending = client.submit(command, DEFAULT_TIMEOUT).unwrap();
    endpoint.admit(|request| engine.handle(request));
    pending
}

fn main() {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let (runtime, mut endpoint) = bridge::channel(Arc::new(|| {}));
    let mut engine = ServoEngine::software(endpoint.events.clone(), Arc::new(|| {})).unwrap();
    let client = runtime.client;
    let created = admit(
        &client,
        &mut endpoint,
        &mut engine,
        Command::Create {
            url: "about:blank".into(),
        },
    );
    let Data::Handle(view) = executor.block_on(created.wait()).unwrap() else {
        panic!("Expected handle")
    };
    let extra = admit(
        &client,
        &mut endpoint,
        &mut engine,
        Command::Create {
            url: "about:blank".into(),
        },
    );
    assert!(matches!(
        executor.block_on(extra.wait()),
        Err(Fault::Engine(_))
    ));

    // Close before the next Servo turn. Deferred replies must be invalidated explicitly.
    let evaluated = admit(
        &client,
        &mut endpoint,
        &mut engine,
        Command::Evaluate {
            view,
            script: "'late'".into(),
        },
    );
    let closed = admit(&client, &mut endpoint, &mut engine, Command::Close(view));
    assert_eq!(executor.block_on(closed.wait()), Ok(Data::Nil));
    assert_eq!(executor.block_on(evaluated.wait()), Err(Fault::StaleHandle));
    assert_eq!(client.unfinished(), 0);
    let stale = admit(&client, &mut endpoint, &mut engine, Command::Info(view));
    assert_eq!(executor.block_on(stale.wait()), Err(Fault::StaleHandle));
    let created = admit(
        &client,
        &mut endpoint,
        &mut engine,
        Command::Create {
            url: "about:blank".into(),
        },
    );
    let Data::Handle(replacement) = executor.block_on(created.wait()).unwrap() else {
        panic!("Expected handle")
    };
    assert!(replacement > view);

    let cancelled = admit(
        &client,
        &mut endpoint,
        &mut engine,
        Command::Evaluate {
            view: replacement,
            script: "'cancelled'".into(),
        },
    );
    drop(cancelled);
    assert_eq!(client.unfinished(), 1);
    engine.tick();
    assert_eq!(client.unfinished(), 0);

    let expired = client
        .submit(
            Command::Evaluate {
                view: replacement,
                script: "'expired'".into(),
            },
            Duration::from_millis(10),
        )
        .unwrap();
    endpoint.admit(|request| engine.handle(request));
    std::thread::sleep(Duration::from_millis(15));
    engine.tick();
    assert_eq!(executor.block_on(expired.wait()), Err(Fault::Timeout));
    assert_eq!(client.unfinished(), 0);

    let pending = admit(
        &client,
        &mut endpoint,
        &mut engine,
        Command::Evaluate {
            view: replacement,
            script: "'shutdown'".into(),
        },
    );
    engine.shutdown();
    assert_eq!(executor.block_on(pending.wait()), Err(Fault::ShuttingDown));
    assert_eq!(client.unfinished(), 0);
    let stopped = admit(
        &client,
        &mut endpoint,
        &mut engine,
        Command::Info(replacement),
    );
    assert_eq!(executor.block_on(stopped.wait()), Err(Fault::ShuttingDown));
    engine.shutdown();
    println!(
        "Servo contract passed: close, stale handles, cancellation, deadline cleanup, shutdown"
    );
}
