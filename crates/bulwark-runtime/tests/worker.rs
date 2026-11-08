use bulwark_engine::{Engine, bridge, mock::MockEngine};
use bulwark_runtime::worker::Worker;
use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

fn wait(worker: &mut Worker) -> anyhow::Result<String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !worker.finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    worker
        .join_finished()
        .expect("Worker did not stop within two seconds")
}

#[test]
fn runtime_errors_wake_the_host_and_remain_visible_at_join() {
    let wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = wakes.clone();
    let (runtime, endpoint) = bridge::channel(Arc::new(move || {
        count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));
    let mut worker =
        Worker::spawn(runtime, "(throw (ex-info \"startup failure\" {}))".into()).unwrap();
    assert!(
        wait(&mut worker)
            .unwrap_err()
            .to_string()
            .contains("startup failure")
    );
    assert!(endpoint.control.is_stopped());
    assert!(wakes.load(std::sync::atomic::Ordering::SeqCst) > 0);
}

#[test]
fn parked_program_cancels_after_engine_cleanup() {
    let (runtime, mut endpoint) = bridge::channel(Arc::new(|| {}));
    let mut engine = MockEngine::new(endpoint.events.clone());
    let mut worker = Worker::spawn(
        runtime,
        "(require '[clojure.core.async :refer [timeout]])
        (defn ^:async parked! []
          (await (bulwark.native/webview-create \"about:blank\"))
          (await (timeout 60000))) (parked!)"
            .into(),
    )
    .unwrap();
    // Wait for the program to park on a browser operation before shutting down.
    let mut active = None;
    let deadline = Instant::now() + Duration::from_secs(2);
    while active.is_none() && !worker.finished() && Instant::now() < deadline {
        endpoint.admit(|request| active = Some(request));
        thread::sleep(Duration::from_millis(1));
    }
    assert!(
        active.is_some(),
        "Program never submitted its browser operation"
    );
    endpoint.stop_admission();
    engine.shutdown();
    active
        .unwrap()
        .complete(Err(bulwark_engine::Fault::ShuttingDown));
    worker.stop();
    assert!(wait(&mut worker).is_err());
}

#[test]
fn crash_and_key_payloads_reach_clojure_in_order() {
    use bulwark_engine::{EventKind, Handle, KeyInput};
    let (runtime, endpoint) = bridge::channel(Arc::new(|| {}));
    endpoint
        .events
        .emit(Handle(7), EventKind::Crashed("fixture failure".into()))
        .unwrap();
    endpoint
        .events
        .emit(
            Handle(7),
            EventKind::KeyInput(KeyInput {
                key: "r".into(),
                control: true,
                alt: false,
                shift: false,
                meta: false,
                pressed: true,
            }),
        )
        .unwrap();
    let mut worker = Worker::spawn(
        runtime,
        "(require 'clojure.core.async)
        (in-ns 'bulwark.core)
        (def events (atom []))
        (defn dispatch! [event] (swap! events conj event))
        (defn ^:async verify! []
          (loop []
            (when (< (count @events) 2)
              (await (clojure.core.async/timeout 1)) (recur)))
          (let [crash (first @events) key (second @events)]
            (if (and (= :page/crashed (:event/type crash))
                     (= \"fixture failure\" (:message crash))
                     (= 1 (:sequence crash)) (= 7 (:webview crash))
                     (= :input/key (:event/type key)) (= \"r\" (:key key))
                     (= 2 (:sequence key)) (= 7 (:webview key))
                     (:control key) (:pressed key) (not (:alt key))
                     (not (:shift key)) (not (:meta key)))
              :payloads-passed
              (throw (ex-info \"Wrong event payload\" {})))))
        (verify!)"
            .into(),
    )
    .unwrap();
    assert_eq!(wait(&mut worker).unwrap(), ":payloads-passed");
}

#[test]
fn dispatcher_error_stops_the_program_and_wakes_the_host() {
    let (runtime, mut endpoint) = bridge::channel(Arc::new(|| {}));
    let mut engine = MockEngine::new(endpoint.events.clone());
    let mut worker = Worker::spawn(
        runtime,
        "(require 'clojure.core.async)
        (in-ns 'bulwark.core)
        (defn dispatch! [event] (throw (ex-info \"handler failure\" {})))
        (defn ^:async run! []
          (await (bulwark.native/webview-create \"about:blank\"))
          (await (clojure.core.async/timeout 60000)))
        (run!)"
            .into(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !worker.finished() && Instant::now() < deadline {
        endpoint.admit(|r| engine.handle(r));
        engine.tick();
        thread::sleep(Duration::from_millis(1));
    }
    endpoint.stop_admission();
    engine.shutdown();
    worker.stop();
    assert!(
        wait(&mut worker)
            .unwrap_err()
            .to_string()
            .contains("handler failure")
    );
}

#[test]
fn embedded_shell_opens_a_view_and_displays_native_key_events() {
    use bulwark_engine::{Command, Data, EventKind, KeyInput};
    let (runtime, mut endpoint) = bridge::channel(Arc::new(|| {}));
    let mut engine = MockEngine::new(endpoint.events.clone());
    let mut worker = Worker::spawn(
        runtime,
        "(require 'bulwark.shell) (bulwark.shell/run! \"about:blank\")".into(),
    )
    .unwrap();
    let mut sent_key = false;
    let mut opened = false;
    let mut received_echo = false;
    let deadline = Instant::now() + Duration::from_secs(2);
    while !received_echo && !worker.finished() && Instant::now() < deadline {
        endpoint.admit(|request| match &request.command {
            Command::SetEcho(text) => {
                received_echo |= text == "Key: z";
                request.complete(Ok(Data::Nil));
            }
            Command::Create { .. } => {
                engine.handle(request);
                opened = true;
            }
            _ => engine.handle(request),
        });
        engine.tick();
        if opened && !sent_key {
            endpoint
                .events
                .emit(
                    bulwark_engine::Handle(7),
                    EventKind::KeyInput(KeyInput {
                        key: "z".into(),
                        control: false,
                        alt: false,
                        shift: false,
                        meta: false,
                        pressed: true,
                    }),
                )
                .unwrap();
            sent_key = true;
        }
        thread::sleep(Duration::from_millis(1));
    }
    endpoint.stop_admission();
    engine.shutdown();
    worker.stop();
    let result = wait(&mut worker);
    assert!(opened, "Shell did not create a view: {result:?}");
    assert!(received_echo, "Shell did not echo the key: {result:?}");
}
