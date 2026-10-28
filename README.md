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

Servo requires a native display and platform build dependencies.
The first Servo build is much larger than the runtime experiments.
The default workspace build excludes Servo compilation.

## Project references

- [Vision](docs/bulwark.md)
- [Development plan and current progress](docs/development-plan.md)
- [Experiment instructions and results](experiments/README.md)
- [Dependency pins and upstream gaps](docs/upstream.md)
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
