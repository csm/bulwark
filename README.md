# Bulwark

Bulwark is a programmable browser built with Rust, Servo, and Clojurust.
The project is in Phase 0: experiments for the native host and runtime integration.
macOS is the first development platform. The initial browser chrome will use egui.

## Start development

The initial builds use Rust 1.97.1 on macOS arm64.
The Cargo workspace pins Servo and Clojurust in [docs/upstream.md](docs/upstream.md).

1. Run the embedded namespace bootstrap:

   ```sh
   cargo run --locked
   ```

   This command prints `{:phase 0, :status :experiments}` and exits.
   The product shell does not open a browser window yet.

2. Run the callback and GC experiment:

   ```sh
   cargo run --locked -p bulwark-experiment-e2
   ```

3. Run the async and nREPL experiment:

   ```sh
   cargo build --locked -p bulwark-experiment-e1
   python3 experiments/run_e1.py
   ```

4. Run the Servo window experiment:

   ```sh
   cargo run --locked -p bulwark-experiment-e3 --features servo -- https://servo.org
   ```

5. Run the async channel bridge experiment:

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

   The native command needs the Servo build. The [nREPL report](docs/nrepl-runtime.md) records coverage and remaining admission limits.

6. Run the DOM channel experiment:

   ```sh
   cargo build --locked -p bulwark-experiment-e4 --features servo
   python3 experiments/run_e4.py
   ```

   The [DOM channel report](docs/dom-channel.md) records the latency and interception results.

Servo requires a native display and platform build dependencies.
The first Servo build is much larger than the runtime experiments.
The default workspace build excludes Servo compilation.

## Project references

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
cargo run --locked -p bulwark-experiment-e2
python3 experiments/run_e1.py --seconds 5
```

The E1 command needs the experiment binary from step 3.
The checks without `--features servo` do not compile Servo.
