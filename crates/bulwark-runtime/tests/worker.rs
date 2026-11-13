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
fn stopping_during_bootstrap_joins_without_a_terminal_panic() {
    for mode in [
        cljrs_runtime::ExecutionMode::TreeWalk,
        cljrs_runtime::ExecutionMode::Tiered,
    ] {
        for delay_ms in [0, 1, 5, 25, 100] {
            let (runtime, mut endpoint) = bridge::channel(Arc::new(|| {}));
            let mut worker = Worker::spawn_with_mode(
                runtime,
                "(require 'clojure.core.async)
                 (defn ^:async parked! [] (await (clojure.core.async/timeout 60000)))
                 (parked!)"
                    .into(),
                mode,
            )
            .unwrap();
            thread::sleep(Duration::from_millis(delay_ms));
            endpoint.stop_admission();
            worker.stop();
            assert!(wait(&mut worker).is_err());
            assert!(
                endpoint.control.error().is_none(),
                "Cancellation became a terminal panic in {mode:?} after {delay_ms} ms: {:?}",
                endpoint.control.error()
            );
        }
    }
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

fn run_product_clojure(source: &str, expected: &str, mode: cljrs_runtime::ExecutionMode) {
    use bulwark_engine::{Command, Data};
    let (runtime, mut endpoint) = bridge::channel(Arc::new(|| {}));
    let mut engine = MockEngine::new(endpoint.events.clone());
    let mut worker = Worker::spawn_with_mode(runtime, source.into(), mode).unwrap();
    let timeout_seconds = std::env::var("BULWARK_TEST_PROGRAM_TIMEOUT_SECS")
        .map(|value| {
            value
                .parse::<u64>()
                .expect("BULWARK_TEST_PROGRAM_TIMEOUT_SECS must be an integer")
        })
        .unwrap_or(30);
    assert!(
        timeout_seconds > 0,
        "The Clojure assertion timeout must be positive"
    );
    let started = Instant::now();
    let deadline = started + Duration::from_secs(timeout_seconds);
    while !worker.finished() && Instant::now() < deadline {
        endpoint.admit(|request| match request.command {
            Command::SetEcho(_) | Command::Log(_) => request.complete(Ok(Data::Nil)),
            _ => engine.handle(request),
        });
        engine.tick();
        thread::sleep(Duration::from_millis(1));
    }
    let timed_out = !worker.finished();
    endpoint.stop_admission();
    engine.shutdown();
    worker.stop();
    let result = wait(&mut worker);
    eprintln!(
        "{expected} in {mode:?}: {:?}, budget={timeout_seconds}s",
        started.elapsed()
    );
    assert!(
        !timed_out,
        "Clojure program {expected} timed out in {mode:?} after {timeout_seconds}s: {result:?}"
    );
    assert_eq!(result.unwrap(), expected, "Evaluator: {mode:?}");
}

#[test]
fn portable_shell_model_passes_in_interpreter_and_tiered_modes() {
    for mode in [
        cljrs_runtime::ExecutionMode::TreeWalk,
        cljrs_runtime::ExecutionMode::Tiered,
    ] {
        run_product_clojure(
            include_str!("../../../clj/tests/shell-model.cljrs"),
            ":shell-model-passed",
            mode,
        );
    }
}

#[test]
fn foundation_passes_in_interpreter_and_tiered_modes() {
    for mode in [
        cljrs_runtime::ExecutionMode::TreeWalk,
        cljrs_runtime::ExecutionMode::Tiered,
    ] {
        run_product_clojure(
            include_str!("../../../clj/tests/foundation.cljrs"),
            ":foundation-passed",
            mode,
        );
    }
}

#[test]
fn command_keymap_and_hooks_pass_in_interpreter_and_tiered_modes() {
    for mode in [
        cljrs_runtime::ExecutionMode::TreeWalk,
        cljrs_runtime::ExecutionMode::Tiered,
    ] {
        run_product_clojure(
            include_str!("../../../clj/tests/commands.cljrs"),
            ":commands-passed",
            mode,
        );
    }
}

#[test]
fn clojure_browser_commands_and_history_use_the_product_bridge() {
    for mode in [
        cljrs_runtime::ExecutionMode::TreeWalk,
        cljrs_runtime::ExecutionMode::Tiered,
    ] {
        run_product_clojure(
            include_str!("../../../clj/tests/browser.cljrs"),
            ":browser-passed",
            mode,
        );
    }
}

#[test]
fn buffer_lifecycles_pass_in_interpreter_and_tiered_modes() {
    for mode in [
        cljrs_runtime::ExecutionMode::TreeWalk,
        cljrs_runtime::ExecutionMode::Tiered,
    ] {
        run_product_clojure(
            include_str!("../../../clj/tests/buffers.cljrs"),
            ":buffers-passed",
            mode,
        );
    }
}

#[test]
fn modes_and_contextual_keymaps_pass_in_interpreter_and_tiered_modes() {
    for mode in [
        cljrs_runtime::ExecutionMode::TreeWalk,
        cljrs_runtime::ExecutionMode::Tiered,
    ] {
        run_product_clojure(
            include_str!("../../../clj/tests/modes.cljrs"),
            ":modes-passed",
            mode,
        );
    }
}

#[test]
fn minibuffer_and_command_palette_pass_in_interpreter_and_tiered_modes() {
    for mode in [
        cljrs_runtime::ExecutionMode::TreeWalk,
        cljrs_runtime::ExecutionMode::Tiered,
    ] {
        run_product_clojure(
            include_str!("../../../clj/tests/minibuffer.cljrs"),
            ":minibuffer-passed",
            mode,
        );
    }
}

#[test]
fn windows_and_ui_descriptions_pass_in_interpreter_and_tiered_modes() {
    for mode in [
        cljrs_runtime::ExecutionMode::TreeWalk,
        cljrs_runtime::ExecutionMode::Tiered,
    ] {
        run_product_clojure(
            include_str!("../../../clj/tests/windows.cljrs"),
            ":windows-passed",
            mode,
        );
    }
}

#[test]
fn frame_events_and_the_viewport_snapshot_reach_clojure() {
    use bulwark_engine::{EventKind, Handle, ui::Viewport};
    let (runtime, endpoint) = bridge::channel(Arc::new(|| {}));
    let viewport = Viewport {
        width: 1000,
        height: 760,
        scale: 2.0,
    };
    assert!(endpoint.control.set_viewport(viewport));
    for kind in [
        EventKind::ViewportChanged,
        EventKind::UiPressed {
            element: 12,
            revision: 3,
        },
        EventKind::UiInputChanged {
            element: 13,
            revision: 3,
            text: "ab".into(),
        },
        EventKind::UiInputSubmitted {
            element: 13,
            revision: 4,
            text: "abc".into(),
        },
    ] {
        endpoint.events.emit(Handle::NONE, kind).unwrap();
    }
    let mut worker = Worker::spawn(
        runtime,
        "(require 'clojure.core.async)
        (in-ns 'bulwark.core)
        (def events (atom []))
        (defn dispatch! [event] (swap! events conj event))
        (defn ^:async verify! []
          (loop []
            (when (< (count @events) 4)
              (await (clojure.core.async/timeout 1)) (recur)))
          (if (and (= [:ui/viewport-changed :ui/pressed :ui/input-changed :ui/input-submitted]
                      (mapv :event/type @events))
                   (not-any? (fn [event] (contains? event :webview)) @events)
                   (= {:width 1000 :height 760 :scale 2.0} (bulwark.native/ui-viewport))
                   (= [12 3] (mapv (nth @events 1) [:ui/id :revision]))
                   (= [13 3 \"ab\"] (mapv (nth @events 2) [:ui/id :revision :text]))
                   (= [13 4 \"abc\"] (mapv (nth @events 3) [:ui/id :revision :text])))
            :frame-events-passed
            (throw (ex-info \"Wrong frame event payload\" {:events @events}))))
        (verify!)"
            .into(),
    )
    .unwrap();
    assert_eq!(wait(&mut worker).unwrap(), ":frame-events-passed");
    // Reading the snapshot rearmed the notification.
    assert!(endpoint.control.set_viewport(Viewport {
        width: 900,
        ..viewport
    }));
}

#[test]
fn key_reservations_publish_before_a_runtime_stall_and_unbind_live() {
    use bulwark_engine::{Command, Data, KeyInput};
    for mode in [
        cljrs_runtime::ExecutionMode::TreeWalk,
        cljrs_runtime::ExecutionMode::Tiered,
    ] {
        let (runtime, mut endpoint) = bridge::channel(Arc::new(|| {}));
        let source = r#"
          (require '[bulwark.keymap :as keymap]
                   '[bulwark.commands :refer [defcommand]]
                   '[bulwark.native :as native])
          (defcommand sample "Reservation probe." [] nil)
          (defn ^:async probe! []
            (keymap/bind-key! "C-x C-f" 'sample)
            (native/log "stall")
            ;; Keep the runtime thread occupied after synchronous publication.
            (let [end (+ (System/nanoTime) 200000000)]
              (loop [] (when (< (System/nanoTime) end) (recur))))
            (await (native/log "recovered"))
            (keymap/unbind-key! "C-x C-f")
            (await (native/log "unbound"))
            :reservations-passed)
          (probe!)"#;
        let mut worker = Worker::spawn_with_mode(runtime, source.into(), mode).unwrap();
        let mut stages = Vec::new();
        let control = endpoint.control.clone();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !worker.finished() && Instant::now() < deadline {
            endpoint.admit(|request| {
                let Command::Log(stage) = &request.command else {
                    panic!("Unexpected request: {:?}", request.command);
                };
                let routes = control.key_routes();
                let mut prefix = Vec::new();
                let key = |s: &str, control| KeyInput {
                    key: s.into(),
                    control,
                    alt: false,
                    shift: false,
                    meta: false,
                    pressed: true,
                };
                if stage == "unbound" {
                    assert!(!routes.capture(&mut prefix, &key("x", true)));
                } else {
                    assert!(routes.capture(&mut prefix, &key("x", true)));
                    assert!(routes.capture(&mut prefix, &key("f", true)));
                    assert!(prefix.is_empty());
                }
                assert!(!routes.capture(&mut prefix, &key("a", false)));
                stages.push(stage.clone());
                request.complete(Ok(Data::Nil));
            });
            thread::sleep(Duration::from_millis(1));
        }
        if !worker.finished() {
            worker.stop();
        }
        assert_eq!(wait(&mut worker).unwrap(), ":reservations-passed");
        assert_eq!(stages, ["stall", "recovered", "unbound"]);
    }
}

#[test]
fn key_reservations_follow_the_selected_buffer_before_a_runtime_stall() {
    use bulwark_engine::{Command, Data, KeyInput};
    for mode in [
        cljrs_runtime::ExecutionMode::TreeWalk,
        cljrs_runtime::ExecutionMode::Tiered,
    ] {
        let (runtime, mut endpoint) = bridge::channel(Arc::new(|| {}));
        let mut engine = MockEngine::new(endpoint.events.clone());
        let source = r#"
          (require '[bulwark.buffer :as buffer]
                   '[bulwark.commands :refer [defcommand]]
                   '[bulwark.mode :as mode]
                   '[bulwark.native :as native])
          (defcommand sample "Reservation probe." [] nil)
          (mode/register! :major :notes-mode "Reserves C-c m." {:keymap {"C-c m" 'sample}})
          (mode/register! :major :plain-mode "Reserves nothing." {})
          (mode/register! :minor :zen-mode "Reserves C-q z." {:keymap {"C-q z" 'sample}})
          (defn stall! [stage]
            (native/log stage)
            ;; Keep the runtime thread occupied after synchronous publication.
            (let [end (+ (System/nanoTime) 100000000)]
              (loop [] (when (< (System/nanoTime) end) (recur)))))
          (defn ^:async probe! []
            (let [notes (await (buffer/create! {:kind :text :major-mode :notes-mode}))
                  plain (await (buffer/create! {:kind :text :major-mode :plain-mode :select false}))]
              (stall! "notes")
              (await (buffer/select! plain))
              (stall! "plain")
              (await (mode/enable! plain :zen-mode))
              (stall! "zen")
              (await (buffer/kill! plain))
              (stall! "notes")
              (await (buffer/kill! notes))
              (await (native/log "none")))
            :contexts-passed)
          (probe!)"#;
        let mut worker = Worker::spawn_with_mode(runtime, source.into(), mode).unwrap();
        let mut stages = Vec::new();
        let control = endpoint.control.clone();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !worker.finished() && Instant::now() < deadline {
            endpoint.admit(|request| {
                let Command::Log(stage) = &request.command else {
                    return engine.handle(request);
                };
                // The stall keeps this request queued, so the snapshot is the only source.
                let routes = control.key_routes();
                let captures = |first: &str, last: &str| {
                    let mut prefix = Vec::new();
                    let key = |text: &str, control| KeyInput {
                        key: text.into(),
                        control,
                        alt: false,
                        shift: false,
                        meta: false,
                        pressed: true,
                    };
                    routes.capture(&mut prefix, &key(first, true))
                        && routes.capture(&mut prefix, &key(last, false))
                        && prefix.is_empty()
                };
                let expected = match stage.as_str() {
                    "notes" => (true, false),
                    "plain" | "none" => (false, false),
                    "zen" => (false, true),
                    other => panic!("Unexpected stage: {other}"),
                };
                assert_eq!(
                    (captures("c", "m"), captures("q", "z")),
                    expected,
                    "Stage: {stage}"
                );
                stages.push(stage.clone());
                request.complete(Ok(Data::Nil));
            });
            engine.tick();
            thread::sleep(Duration::from_millis(1));
        }
        endpoint.stop_admission();
        engine.shutdown();
        if !worker.finished() {
            worker.stop();
        }
        assert_eq!(wait(&mut worker).unwrap(), ":contexts-passed");
        assert_eq!(stages, ["notes", "plain", "zen", "notes", "none"]);
    }
}

#[test]
fn native_stroke_spelling_matches_the_clojure_keymap() {
    use bulwark_engine::KeyInput;
    let mut source = String::new();
    for text in [
        "x",
        "X",
        "é",
        "É",
        "<",
        "-",
        " ",
        "Enter",
        "Tab",
        "Escape",
        "ArrowLeft",
    ] {
        for flags in 0..16 {
            let key = KeyInput {
                key: text.into(),
                control: flags & 1 != 0,
                alt: flags & 2 != 0,
                shift: flags & 4 != 0,
                meta: flags & 8 != 0,
                pressed: true,
            };
            source.push_str(&format!(
                "(when-not (= {:?} (bulwark.keymap/canonical-stroke {:?} {} {} {} {})) (throw (ex-info \"Stroke mismatch\" {{}})))\n",
                key.stroke(), text, key.control, key.alt, key.shift, key.meta,
            ));
        }
    }
    source.push_str(":spelling-passed");
    for mode in [
        cljrs_runtime::ExecutionMode::TreeWalk,
        cljrs_runtime::ExecutionMode::Tiered,
    ] {
        run_product_clojure(&source, ":spelling-passed", mode);
    }
}
