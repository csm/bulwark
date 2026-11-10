# Bulwark

Bulwark is a programmable browser built with Rust, Servo, and Clojurust.
Phase 0 is complete. The native host and runtime experiments now have an accepted architecture.
Phase 1 is in progress. The product now has a bounded engine/runtime bridge, a native Servo window, and an egui echo area.
See the [foundation checkpoint](docs/phase-1-foundation.md) and [Phase 0 exit review](docs/phase-0-exit.md).
macOS is the first development platform. The initial browser chrome will use egui.

## Start development

The initial builds use Rust 1.97.1 on macOS arm64.
The Cargo workspace pins Servo and Clojurust in [docs/upstream.md](docs/upstream.md).

1. Run the embedded namespace bootstrap:

   ```sh
   cargo run --locked
   ```

   This command prints `{:phase 1, :status :foundation}` and exits.
   The `native` feature opens a browser window, as described below.

   Run the product foundation check without a native display:

   ```sh
   cargo run --locked -- --headless-smoke
   ```

   This runs Clojure assertions against the product bridge and a mock engine.
   It checks native calls, ordered events, live dispatcher redefinition, and worker shutdown.
   Run the product Servo check against a local HTML file:

   ```sh
   cargo run --locked --features servo -- --servo-smoke
   ```

   This uses `SoftwareRenderingContext` and the same product bridge and runtime worker.
   Clojure assertions cover real JavaScript, title events, navigation history, and stale handles.
   The host also requires a painted frame and a joined worker.
   The adapter currently accepts one live webview per context.
   Open a native browser window:

   ```sh
   cargo run --locked --features native -- https://servo.org
   ```

   Without a URL, the native shell opens `about:blank`.
   Clojure displays page titles and key events in the egui echo area.
   The native shell forwards pointer, wheel, and keyboard input to Servo.
   It reserves the echo area outside the page viewport.

   Run the native product checks:

   ```sh
   cargo build --locked --features native
   python3 experiments/run_product_native.py
   ```

   The checks cover page rendering, echo updates, parked shutdown, runtime errors, dispatcher errors, and native quit.
   The [native shell checkpoint](docs/phase-1-native.md) records the implementation and validation limits.
   The [command checkpoint](docs/phase-1-commands.md) describes Clojure commands, keymaps, hooks, and title history.
   Default keys include `C-c r` for reload and `C-x h` for recent page titles.
   The native shell reserves command sequences before Servo receives them.
   Unbound keys reach Servo without a runtime wait.
   The [key interception checkpoint](docs/phase-1-keys.md) records ownership rules and coverage.

   Enable the product nREPL listener:

   ```sh
   cargo run --locked --features native -- --nrepl --profile /tmp/bulwark-profile about:blank
   ```

   Connect to `127.0.0.1` at the port in `/tmp/bulwark-profile/.nrepl-port`.
   Without `--profile`, the directory defaults to `$HOME/.bulwark`.
   The [product nREPL checkpoint](docs/phase-1-nrepl.md) describes admission limits, cleanup, and live command examples.

2. Run the callback and GC experiment:

   ```sh
   cargo run --locked -p bulwark-experiment-e2
   ```

3. Run the async and nREPL experiment:

   ```sh
   cargo build --locked -p bulwark-experiment-e1
   python3 experiments/run_e1.py
   ```

4. Run the fiber time-budget experiment:

   ```sh
   cargo run --locked -p bulwark-experiment-e6
   ```

5. Run the Servo window experiment:

   ```sh
   cargo run --locked -p bulwark-experiment-e3 --features servo -- https://servo.org
   ```

6. Run the async channel bridge experiment:

   ```sh
   cargo build --locked -p bulwark-experiment-e7
   python3 experiments/run_e7.py
   ```

   This command uses a mock engine. The [async bridge report](docs/async-bridge.md) includes the real Servo command and measured limits.

   For the native window and lifecycle tests, run:

   ```sh
   cargo build --locked -p bulwark-experiment-e7 --features servo
   python3 experiments/run_e7_native.py
   ```

   The [native bridge report](docs/native-async-bridge.md) includes interactive commands and the shutdown contract.

   To exercise runtime-thread nREPL sessions, run:

   ```sh
   python3 experiments/run_e7_nrepl.py
   python3 experiments/run_e7_nrepl.py --native
   ```

   The native command needs the Servo build. The [nREPL report](docs/nrepl-runtime.md) records bounded admission, overload coverage, and remaining resource limits.

7. Run the DOM channel experiment:

   ```sh
   cargo build --locked -p bulwark-experiment-e4 --features servo
   python3 experiments/run_e4.py
   ```

   The [DOM channel report](docs/dom-channel.md) records the latency and interception results.

Servo requires a native display and platform build dependencies.
The first Servo build is much larger than the runtime experiments.
The default workspace build excludes Servo compilation.

## Project references

- [Phase 0 exit review](docs/phase-0-exit.md)
- [Phase 1 foundation checkpoint and native API](docs/phase-1-foundation.md)
- [Phase 1 native window and egui checkpoint](docs/phase-1-native.md)
- [Phase 1 commands, hooks, and browser behavior](docs/phase-1-commands.md)
- [Phase 1 product nREPL](docs/phase-1-nrepl.md)
- [Development handoff and next steps](docs/handoff.md)
- [Vision](docs/bulwark.md)
- [Development plan and current progress](docs/development-plan.md)
- [Experiment instructions and results](experiments/README.md)
- [Separate engine and async runtime threads](docs/async-bridge.md)
- [Native macOS input and navigation results](docs/native-macos.md)
- [DOM channel measurements and choice](docs/dom-channel.md)
- [Dependency pins and upstream gaps](docs/upstream.md)
- [Clojurust async status and remaining fixes](docs/clojurust-async-gaps.md)
- [Architecture decisions](docs/adr/)

## Local checks

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo run --locked -- --headless-smoke
cargo run --locked -p bulwark-experiment-e2
python3 experiments/run_e1.py --seconds 5
cargo run --locked -p bulwark-experiment-e6
```

The E1 command needs the experiment binary from step 3.
The checks without `--features servo` do not compile Servo.
