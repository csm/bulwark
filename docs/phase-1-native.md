# Phase 1 native shell checkpoint

Status: implemented locally, 2026-11-07. Phase 1 remains in progress.
Base: `main` at `7660535`. The fast-forward pull reported that the worktree was up to date.

## Run the native shell

```sh
cargo run --locked --features native -- https://servo.org
```

Without a URL, the native shell opens `about:blank`.
The existing default build still prints the foundation status and exits.
The `native` feature includes the `servo` feature and the egui dependencies.
Servo and Clojurust retain their previous pins. Egui and egui_glow use `0.34.1`.
This egui version shares Servo's Glow `0.17` and the workspace's winit `0.30`.

The shell accepts a trusted Clojure program through `--script PATH`.
The program can use `user/initial-url` and the embedded `bulwark.shell` namespace.
The diagnostic flags `--close-after-ms N` and `--screenshot PATH` support bounded local checks.
The screenshot contains the final composed frame before shutdown.

## Ownership and rendering

Winit, Servo, GL resources, egui, and the native window stay on the main thread.
The dedicated runtime worker retains its Tokio LocalSet, GC values, and ordered event dispatcher.
The host uses the existing bounded product bridge for all runtime requests and events.

The native window owns a `WindowRenderingContext`.
One Servo view uses its `OffscreenRenderingContext` above the echo area.
The shell copies that surface into the window, draws egui, and presents the combined frame once.
The echo area occupies 28 logical pixels at the bottom of the window.
It displays the text from `bulwark.native/set-echo!`.

The adapter paints each new view at least once.
Native exposure events can force another paint even without a new Servo frame notification.
Repeated Servo frame notifications cause one proxy wake until the next paint.
This prevents a wake loop while the window is occluded.

The host handles resize, focus, and scale events.
New views inherit the current window scale and focus state.
Zero-sized native surfaces do not cause a resize or presentation.
Display transitions still require interactive validation.

## Input and Clojure policy

The shell forwards page pointer movement, mouse buttons, wheel deltas, and keyboard events to Servo.
Line wheel deltas retain line units. Pixel wheel deltas retain pixel units.
The echo area does not receive page clicks or wheel events.
A button release follows its original page press across the chrome boundary.
Consumed egui events do not start new page input.

Forwarded keys also enter the ordered runtime event stream as `:input/key` maps.
The embedded `bulwark.shell` namespace opens the initial view and installs the default dispatcher.
Its dispatcher displays page titles, crash messages, and pressed keys in the echo area.
This policy lives in Clojure.
Keymaps and command interception remain later work. Current key events also reach the page.

IME, clipboard, native form controls, and simultaneous web splits remain outside this checkpoint.
The input module retains the MPL notice from Servo's example.
The [shell notice](../crates/bulwark-shell/NOTICE.md) records its source.

## Shutdown

Close, timed close, and `bulwark.native/quit` stop request admission.
The host rejects queued requests and invalidates engine resources before it cancels the worker.
Worker completion, runtime errors, dispatcher errors, and bridge errors also reach host shutdown.
The host joins only a finished worker and enforces a two-second shutdown deadline.
An unfinished worker causes an error exit.

Proxy events drive ordinary work.
A 50-millisecond maintenance deadline handles request cleanup and completion races.
Shutdown uses a five-millisecond poll interval.
These intervals do not bound individual Servo calls or arbitrary blocking runtime code.

## Local checks

```sh
cargo fmt --all -- --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked -p bulwark-shell --features native
cargo clippy --locked -p bulwark-shell -p bulwark-engine-servo --features native,servo --all-targets -- -D warnings
cargo test --locked -p bulwark-engine-servo --features servo
cargo run --locked --features servo -- --servo-smoke
cargo build --locked --features native
python3 experiments/run_product_native.py
```

The native runner uses independent process deadlines.
It covers the local page, a composed PNG, Clojure echo updates, parked close, runtime failure, dispatcher failure, and native quit.
The shell tests cover viewport geometry and mouse ownership across the chrome boundary.
The adapter tests cover repeated frame notifications, initial paint, scale inheritance, resize, and the existing request lifetime contract.
A runtime integration test loads the embedded shell and exercises its key dispatcher through the product bridge.

Formatting, both Clippy configurations, workspace tests, native unit tests, and Servo contract tests passed locally on macOS arm64.
The mock smoke and software Servo smoke also passed.
All five native runner cases passed.
The captured frame showed the Servo fixture above the egui text `Native chrome smoke ready`.
The native smoke flushes its final diagnostic frame before teardown because macOS can suppress redraw callbacks on a locked desktop.
This proves composition and presentation calls, not interactive input or visible display behavior.
The Mac was locked during this continuation, so interactive typing, clicking, scrolling, resize, and close checks remain pending.

CI now compiles the native feature and runs its unit tests and process runner under Xvfb on Linux.
Remote CI results remain uninspected.
The existing GL texture warning and large `__eh_frame` linker warning remain.

## Next work

1. Complete the native interactive checks on an unlocked Mac.
2. Add the Phase 1 Clojure command registry, keymap, and hooks.
3. Add opt-in loopback nREPL with the accepted admission limits.
4. Add the Clojure browser behavior and run the remaining Phase 1 acceptance checks.

The default executable still requires the `native` feature to open a window.
This checkpoint does not complete Phase 1 or its full acceptance criteria.
