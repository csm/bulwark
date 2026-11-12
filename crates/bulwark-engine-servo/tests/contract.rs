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
    engine.set_window_state(2.0, false);
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
    assert_eq!(
        engine
            .active_webview()
            .unwrap()
            .1
            .hidpi_scale_factor()
            .get(),
        2.0
    );
    assert!(!engine.active_webview().unwrap().1.focused());
    assert!(
        engine.needs_repaint(),
        "A new native view needs an initial paint"
    );
    let extra = admit(
        &client,
        &mut endpoint,
        &mut engine,
        Command::Create {
            url: "about:blank".into(),
        },
    );
    let Data::Handle(extra) = executor.block_on(extra.wait()).unwrap() else {
        panic!("Expected retained view handle")
    };
    assert_eq!(engine.active_webview().unwrap().0, view);
    let selected = admit(
        &client,
        &mut endpoint,
        &mut engine,
        Command::Select(Some(extra)),
    );
    assert_eq!(executor.block_on(selected.wait()), Ok(Data::Nil));
    assert_eq!(engine.active_webview().unwrap().0, extra);
    assert!(!engine.active_webview().unwrap().1.focused());
    engine.set_window_state(1.5, true);
    assert!(engine.active_webview().unwrap().1.focused());
    let selected = admit(
        &client,
        &mut endpoint,
        &mut engine,
        Command::Select(Some(view)),
    );
    assert_eq!(executor.block_on(selected.wait()), Ok(Data::Nil));
    assert_eq!(engine.active_webview().unwrap().0, view);
    assert_eq!(
        engine
            .active_webview()
            .unwrap()
            .1
            .hidpi_scale_factor()
            .get(),
        1.5
    );
    let hidden = admit(&client, &mut endpoint, &mut engine, Command::Select(None));
    assert_eq!(executor.block_on(hidden.wait()), Ok(Data::Nil));
    assert!(engine.active_webview().is_none());
    assert!(!engine.needs_repaint());
    assert!(!engine.paint_frame(true));
    let background = admit(
        &client,
        &mut endpoint,
        &mut engine,
        Command::Create {
            url: "about:blank".into(),
        },
    );
    let Data::Handle(background) = executor.block_on(background.wait()).unwrap() else {
        panic!("Expected background handle")
    };
    assert!(
        engine.active_webview().is_none(),
        "Creation must preserve a hidden layout"
    );
    let closed = admit(
        &client,
        &mut endpoint,
        &mut engine,
        Command::Close(background),
    );
    assert_eq!(executor.block_on(closed.wait()), Ok(Data::Nil));
    let selected = admit(
        &client,
        &mut endpoint,
        &mut engine,
        Command::Select(Some(view)),
    );
    assert_eq!(executor.block_on(selected.wait()), Ok(Data::Nil));
    let closed = admit(&client, &mut endpoint, &mut engine, Command::Close(extra));
    assert_eq!(executor.block_on(closed.wait()), Ok(Data::Nil));
    assert_eq!(engine.active_webview().unwrap().0, view);
    let stale = admit(
        &client,
        &mut endpoint,
        &mut engine,
        Command::Select(Some(extra)),
    );
    assert_eq!(executor.block_on(stale.wait()), Err(Fault::StaleHandle));
    assert_eq!(engine.active_webview().unwrap().0, view);
    engine.set_window_state(2.0, false);

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
    assert_eq!(
        engine
            .active_webview()
            .unwrap()
            .1
            .hidpi_scale_factor()
            .get(),
        2.0
    );
    engine.set_window_state(1.0, true);
    let (_, native_view) = engine.active_webview().unwrap();
    assert!(native_view.focused());
    assert_eq!(native_view.hidpi_scale_factor().get(), 1.0);
    native_view.resize((800, 500).into());
    assert_eq!(native_view.size(), euclid::Size2D::new(800.0, 500.0));
    drop(native_view);

    // A described frame owns visibility. Rejected frames leave the current one intact.
    let description = |revision, handle: Option<bulwark_engine::Handle>| {
        use bulwark_engine::ui::{Element, Rect, Ui, Widget};
        Ui::new(
            revision,
            Some(1),
            vec![Element {
                id: Some(1),
                rect: Rect {
                    x: 400,
                    y: 0,
                    width: 400,
                    height: 500,
                },
                widget: handle.map_or(Widget::Text("notes".into()), Widget::Web),
            }],
        )
        .unwrap()
    };
    for (ui, expected, visible, revision) in [
        (
            description(2, Some(replacement)),
            Ok(Data::Nil),
            Some(replacement),
            2,
        ),
        (
            description(1, None),
            Err(Fault::StaleRevision),
            Some(replacement),
            2,
        ),
        (
            description(2, None),
            Err(Fault::StaleRevision),
            Some(replacement),
            2,
        ),
        (
            description(3, Some(view)),
            Err(Fault::StaleHandle),
            Some(replacement),
            2,
        ),
        (description(3, None), Ok(Data::Nil), None, 3),
        (
            description(4, Some(replacement)),
            Ok(Data::Nil),
            Some(replacement),
            4,
        ),
    ] {
        let reply = admit(&client, &mut endpoint, &mut engine, Command::SetUi(ui));
        assert_eq!(executor.block_on(reply.wait()), expected);
        assert_eq!(engine.active_webview().map(|(id, _)| id), visible);
        assert_eq!(engine.ui().unwrap().revision(), revision);
        if visible.is_none() {
            assert!(!engine.paint_frame(true));
        }
    }

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
    assert!(engine.active_webview().is_none());
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
        "Servo contract passed: retained views, selection, UI revisions, visibility, close, stale handles, cancellation, deadline cleanup, shutdown"
    );
}
