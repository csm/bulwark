# Phase 1 key interception

The native shell reserves Clojure command sequences before it forwards keyboard events to Servo.
Unbound keys reach Servo immediately, without a request to the runtime thread.
Clojure still defines commands, bindings, cancellation keys, and command behavior.

## Reservation snapshot

`bulwark.keymap/bind-key!` and `unbind-key!` publish the complete reservation snapshot before they replace the Clojure binding map.
A failed publication leaves the previous bindings and snapshot intact.
Commands still retain Vars, so command redefinition does not require another publication.
Direct changes to the `bindings` atom bypass publication. Callers must use the binding functions.

`bulwark.native/set-key-routes!` accepts two vectors: canonical key sequences and cancellation strokes.
This function synchronously returns `nil` or throws an error.
Unlike browser operations, publication does not create a request or a future.
The synchronous result guarantees publication before a caller starts a synchronous Clojure loop.
A host without the native bridge retains Clojure keymap behavior without native reservations.

The bridge retains one immutable snapshot under a mutex.
The native thread holds the mutex only while it clones the snapshot pointer.
The runtime constructs and validates each snapshot before it acquires that mutex.
Publication replaces the previous snapshot without a queue.
No GC value or command function crosses the thread boundary.

The snapshot permits at most 256 sequences, 16 strokes per sequence, and 128 UTF-8 bytes per stroke.
It also permits at most 16 cancellation strokes.
The native decoder checks each vector length and string length before it copies that data.
These structural limits bound the snapshot separately from browser request capacity.

## Input ownership

The native matcher reserves a complete binding, a prefix, and any invalid continuation of a prefix.
The Clojure keymap publishes `C-g` and `Escape` as cancellation strokes.
Rust contains no binding strings or command names.
Modifier keys do not advance a pending sequence.

Each physical key retains its initial owner until release.
A captured press and its release never reach Servo.
A page press retains its page release, even after a binding change or chrome consumption.
Repeated presses retain their owner and do not advance the Clojure keymap again.
Ordinary page repeats still reach Servo.

Focus changes clear native ownership and pending prefixes.
A view change also clears ownership.
A new snapshot clears the native prefix but preserves held-key ownership within the same view.
The shell emits an ordered `:input/reset` event before keys from the new snapshot or view.
Focus changes also emit that event. The Clojure dispatcher cancels its pending prefix on receipt.

The existing bounded event queue carries key events during a runtime stall.
Commands run after the runtime resumes, against the current command Vars and binding map.
The queue retains its 256-event capacity and fatal overflow behavior.
The shell does not wait for a Clojure decision, replay keys, or release a reserved key to Servo after a timeout.

## Checks

Run the portable checks:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo run --locked -- --headless-smoke
```

The command suite runs under TreeWalk and Tiered modes.
A cross-language test compares native stroke spelling with Clojure for letters, named keys, punctuation, and all modifier combinations.
It covers invalid native snapshots, failed binding publication, and prefix reset events.
A worker integration test checks reservation visibility during a 200-millisecond synchronous loop, recovery, and live unbinding in both modes.
Native unit tests cover capture, repeats, release ownership, chrome consumption, focus, view changes, and snapshot replacement.

Run the native checks:

```sh
cargo clippy --locked -p bulwark-shell -p bulwark-engine-servo --features native,servo --all-targets -- -D warnings
cargo test --locked -p bulwark-shell -p bulwark-engine-servo --features native,servo
cargo build --locked -p bulwark-shell --features native
python3 experiments/run_product_native.py
python3 experiments/run_product_nrepl.py --native
```

The existing CI jobs include the new portable and native unit tests.

Local macOS checks passed: formatting, workspace Clippy, all 38 workspace tests, and the foundation smoke.
Native/Servo Clippy, all eight native shell unit tests, both Servo unit tests, and the Servo contract passed.
All seven native process cases, all three mock nREPL cases, and all four native nREPL cases passed.
Every native process joined its runtime worker. The software Servo smoke painted 13 frames and joined its worker.
The native browser case painted 33 frames and made 36 presentations.

The graphics contract first failed inside the filesystem sandbox because macOS denied surface creation.
The same contract passed with graphics access outside that sandbox.
The existing GL texture and large `__eh_frame` linker warnings remain.
The Mac was locked, so computer-use tools could not run physical keyboard checks.
These local changes still require Linux CI and the interactive Phase 1 acceptance checks.

Physical keyboard input and native rendering during a two-second runtime stall still require the Phase 1 acceptance check.
The matcher and worker tests do not establish interactive display behavior.
