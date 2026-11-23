# Bulwark

Bulwark is a programmable browser built with Rust, Servo, and Clojurust.
Phase 0 is complete. The native host and runtime experiments now have an accepted architecture.
Phase 1 acceptance is complete. The product has a bounded engine/runtime bridge, a native Servo window, and an egui echo area.
The [acceptance review](docs/phase-1-acceptance.md) records passing CI and physical macOS checks.
Phase 2 is complete and accepted by the owner. The [mechanism checkpoint](docs/phase-2-mechanisms.md) adds retained webviews and standalone AOT tests.
The [buffer checkpoint](docs/phase-2-buffers.md) adds the Clojure buffer registry, lifecycle hooks, and buffer commands.
The [window checkpoint](docs/phase-2-windows.md) adds mixed panes, modelines, and bounded UI descriptions.
The [mode checkpoint](docs/phase-2-modes.md) adds mode definitions and contextual keymaps.
The [minibuffer checkpoint](docs/phase-2-minibuffer.md) adds prompts, fuzzy completion, and the `M-x` command palette.
The [introspection checkpoint](docs/phase-2-introspection.md) adds help buffers, Messages, and live init reload.
The [browsing checkpoint](docs/phase-2-browsing.md) adds default configuration modules, find, zoom, popup support, and an internal REPL.
The [P2.5/P2.6 validation report](docs/phase-2-shell-acceptance.md) records passing Linux CI and macOS physical checks.
The [P2.8 acceptance review](docs/phase-2-acceptance.md) records physical browsing and REPL checks, release startup samples, and the native policy review.
The owner accepted Phase 2 on 2026-11-22 with its documented scope and input limits.
The small Linux investigation remains deferred. Phase 3 planning is next.
The [Option setting](docs/phase-2-option-mode.md) preserves Mac character input by default and offers live switching to Meta.
The [physical follow-up](docs/phase-2-physical-follow-up.md) records passing active-window GitHub navigation and the remaining compatibility limits.
The [preference correction](docs/phase-2-preference-evaluation.md) passes the consented BBC article task and clears GitHub placeholders in the release comparison.
Its [CI run](https://github.com/csm/bulwark/actions/runs/44986893459) passed all Linux jobs and the macOS runtime job.
The [repeat checks](docs/phase-2-repeat-checks.md) passed all five physical site tasks, live configuration, introspection, REPL checks, and local automated suites.
See the [foundation checkpoint](docs/phase-1-foundation.md) and [Phase 0 exit review](docs/phase-0-exit.md).
macOS is the first development platform. The initial browser chrome will use egui.

## Start development

The initial builds use Rust 1.97.1 on macOS arm64.
The Cargo workspace pins Servo and Clojurust in [docs/upstream.md](docs/upstream.md).

1. Open the browser:

   ```sh
   cargo run --locked
   ```

   This command opens a native Servo window at `about:blank` with a modeline and an egui echo area.
   To omit Servo and run the namespace bootstrap, use `cargo run --locked --no-default-features`.

   Run the product foundation check without a native display:

   ```sh
   cargo run --locked --no-default-features -- --headless-smoke
   ```

   This runs Clojure assertions against the product bridge and a mock engine.
   It checks native calls, ordered events, live dispatcher redefinition, and worker shutdown.
   Run the product Servo check against a local HTML file:

   ```sh
   cargo run --locked -- --servo-smoke
   ```

   This uses `SoftwareRenderingContext` and the same product bridge and runtime worker.
   Clojure assertions cover real JavaScript, title events, navigation history, stale handles, and retained page state.
   The host also requires a painted frame and a joined worker.
   The adapter retains multiple live webviews with one visible view per context.
   Open a native browser window:

   ```sh
   cargo run --locked -- https://servo.org
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
   Default keys include `C-c r` for reload, `C-x h` for recent page titles, `M-x` or `C-c x` for the command palette, `C-x b` for buffers, and `C-l` for a URL.
   `C-x 3` splits a page beside text. `C-x o` selects the next window, and `C-x 1` keeps only that window.
   The native shell reserves command sequences before Servo receives them.
   Unbound keys reach Servo without a runtime wait.
   The [key interception checkpoint](docs/phase-1-keys.md) records ownership rules and coverage.

   The shell loads `~/.bulwark/init.cljrs` after the default configuration.
   `--profile DIR` selects `DIR/init.cljrs` without an nREPL requirement.
   `M-x reload-init` reloads the file. Errors appear in `M-x show-messages`.
   `M-x describe-command`, `describe-key`, `describe-variable`, and `apropos` open read-only help buffers.
   [examples/init.cljrs](examples/init.cljrs) supplies a buffer binding and a Wikipedia reader mode.

   On macOS, Option preserves native character input by default. Other platforms retain Alt as Meta.
   Set `(bulwark.keymap/set-option-mode! :meta)` in your init file or REPL to use Option for `M-` bindings.
   Use `:characters` for native character input or `:platform-default` to restore the platform default.
   Changes apply without restarting. `C-c x` opens the command palette in either mode.
   Outside prompts, `Escape x` also invokes `M-x`. `Escape Escape` cancels the pending Meta prefix.
   Inside prompts, Escape cancels immediately, including during a pending command prefix.

   `C-s` finds page text. `C-c s` selects the next match.
   `C-c +`, `C-c -`, and `C-c 0` change or reset page zoom.
   `C-c l` selects a page link for a retained background buffer.
   Popup policy lives in `bulwark.default.pages/popup-policy` and defaults to background buffers.
   `C-c e` opens the internal REPL without a network listener.
   Enter submits source. `C-p` and `C-n` recall input, and `C-c C-c` interrupts an evaluation.
   The [P2.7 checkpoint](docs/phase-2-browsing.md) records limits, lifecycle hooks, and remaining acceptance work.

   Record startup from process creation to the first page/chrome presentation:

   ```sh
   cargo build --locked --release -p bulwark-shell
   python3 experiments/measure_product_startup.py
   ```

   Each sample uses a new empty profile at `about:blank`, without nREPL.
   The [acceptance review](docs/phase-2-acceptance.md#release-startup-measurements) records the timing definition, hardware, and cache conditions.

   Enable the product nREPL listener:

   ```sh
   cargo run --locked -- --nrepl --profile /tmp/bulwark-profile about:blank
   ```

   Connect to `127.0.0.1` at the port in `/tmp/bulwark-profile/.nrepl-port`.
   Without `--profile`, the directory defaults to `$HOME/.bulwark`.
   The [product nREPL checkpoint](docs/phase-1-nrepl.md) describes admission limits, cleanup, and live command examples.
   Add `--interpreter` after `--` to select TreeWalk execution for the shell, scripts, and nREPL.
   Without this flag, the runtime uses Tiered execution.
   The [acceptance review](docs/phase-1-acceptance.md) records automated coverage and the completed physical checks.

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
The default workspace build includes the native shell and Servo.
Use `--no-default-features` for the portable workspace checks without Servo.

## Project references

- [Phase 2 implementation plan](docs/phase-2-plan.md)
- [Phase 2 acceptance review and open site tasks](docs/phase-2-acceptance.md)
- [Phase 2 mechanism checkpoint](docs/phase-2-mechanisms.md)
- [Phase 2 buffer checkpoint](docs/phase-2-buffers.md)
- [Phase 2 window and UI checkpoint](docs/phase-2-windows.md)
- [P2.3 physical and Linux validation](docs/phase-2-window-acceptance.md)
- [Headless Linux test host and runner](docs/linux-testing.md)
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
cargo clippy --locked --workspace --all-targets --no-default-features -- -D warnings
cargo test --locked --workspace --no-default-features
cargo run --locked --no-default-features -- --headless-smoke
cargo run --locked -p bulwark-experiment-e2
python3 experiments/run_e1.py --seconds 5
cargo run --locked -p bulwark-experiment-e6
```

The E1 command needs the experiment binary from step 3.
The portable workspace checks use `--no-default-features` to omit Servo.
