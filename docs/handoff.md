# Development handoff

Snapshot: 2026-11-06, implementation through `50f6540`.
Branch: `codex/native-async-bridge`.

## Current state

Bulwark remains in Phase 0. The default executable prints the embedded namespace status and exits.
The product shell does not yet open a browser window.
The completed integration work lives in standalone experiments.

Two feature commits were pushed to origin during this continuation:

- `649c654`: DOM channel probe, native E7 window, bounded shutdown, and the Clojurust dependency update.
- `50f6540`: runtime-thread nREPL integration, protocol probes, and CI coverage for the portable nREPL probe.

The [development plan](development-plan.md) remains the phase checklist.
The [async gap register](clojurust-async-gaps.md) is the canonical source for remaining upstream requirements.

## Architecture and constraints

The main thread owns winit, Servo, webviews, rendering contexts, and native windows.
A dedicated thread owns Clojurust, its GC values, Tokio current-thread executor, and LocalSet.
Only owned Rust requests, replies, and events cross this boundary.

Native Clojurust `go` and `^:async` bodies now run on stackful fibers through `corosensei`.
Ordinary helpers inside those fibers can await or dereference browser futures.
`Thread/sleep` parks a fiber. Raw blocking Rust code can still stall the runtime thread.
The Rust bridge continues to use ordinary async futures and channels.
`go` returns a result channel, while `^:async` returns a future.

The native bridge uses `EventLoopProxy` for requests, Servo wakeups, worker completion, and worker panic.
Both drivers allow two seconds for worker shutdown before they abandon a join and fail the process.
A forced process exit does not establish successful runtime cleanup or cancellation of browser side effects.
The Python runners add independent process deadlines.

The native probe supports one live webview at a time.
Initial product development can keep one visible webview per window and defer web content splits until E5.
The initial chrome choice remains egui. Browser behavior belongs in Clojure.
Permanent local dependency patches and language restrictions do not satisfy the upstream adoption gates.

Pins and local platform:

| Item | Value |
|---|---|
| Clojurust | `c6199faea544e029db7222aa3e9f5c9f64952773` |
| Servo | `=0.7.0`, single process, `bundled` and `js_jit` |
| Rust | 1.97.1 |
| Local platform | macOS 26.5.2 arm64 |

## Evidence already established

| Area | Result | Details |
|---|---|---|
| E2 host callbacks | 10,000 ordered callbacks per mode, stable live objects, errors and redefinition passed | [Experiment guide](../experiments/README.md) |
| E3 macOS baseline | Fixture input, scroll, resize, close, and navigation on Servo.org and Wikipedia passed | [Native input report](native-macos.md) |
| E4 DOM channel | Injected agent, JSON subtree, and intercepted page message passed | [DOM report](dom-channel.md) |
| E7 native bridge | 119 requests completed and 20 presentations during a raw 200 ms runtime stall | [Native bridge report](native-async-bridge.md) |
| E7 lifecycle | Six cases passed, including panic and an unresponsive native call | [Native bridge report](native-async-bridge.md) |
| E7 nested awaits | Eleven callable and promise cases passed with concurrent heartbeat progress | [Gap register](clojurust-async-gaps.md) |
| E7 nREPL | Session order, concurrent browser calls, interruption, redefinition, and shutdown passed across mock and Servo backends | [nREPL report](nrepl-runtime.md) |

The native nREPL run presented 17 frames during CPU-bound Clojure evaluation in another session.
Native close with a parked evaluation joined the runtime, closed the listener, and removed the port file.
These are finite-load results. They do not establish admission bounds, bounded memory use, or display frame-rate guarantees.

Seven workspace unit tests, formatting, workspace Clippy, and E7 Servo Clippy passed locally.
The portable nREPL probe now appears in the macOS/Linux CI workflow.
Remote CI results were not inspected during these continuations.
The complete upstream Clojurust test suite was not run here.

## Next work, in order

1. Compare the remaining findings with current upstream Clojurust before a patch or issue.
2. Prepare the CLJ-ASYNC-004 upstream fix for idle GC scheduling, with deterministic suspension and GC-root coverage.
3. Prepare the CLJ-ASYNC-003 upstream mechanisms for bounded nREPL admission and accepted pending work.
4. After upstream merge, update the dependency pin and lockfile deliberately and repeat the integration probes.
5. Complete D1's ownership review and product queue policies, including event overload, payload limits, and shutdown.
6. After the Phase 0 review, promote the proven mechanisms into the product crates and begin Phase 1 deliverables 1–3.

CLJ-ASYNC-004 remains a measured idle CPU defect.
`cljrs_async::init` starts a GC service that repeatedly yields and collects without an idle wait.
The initialized executor control used 1.054 CPU-seconds during a one-second timer.
The control without initialization used 0.098 CPU-seconds.
Future and promise waiters themselves now use notifications.
The previous nested-await defect no longer reproduces on native targets.

CLJ-ASYNC-003 remains a missing admission mechanism.
`spawn_pending` admits every queued job in one call, and the upstream queues remain unbounded.
E7's one-millisecond admission timer does not bound the work inside that call.
An upstream batch API needs a job budget and a result that identifies budget exhaustion.
Pending-work limits must include session queues and running or parked evaluations.
Interrupts must remain available under overload. Request size and output buffering need separate limits.

These continuations did not prepare upstream patches or pull requests for those remaining findings.

## Resume commands

For Git operations, load the configured SSH agent environment:

```sh
source /Users/csm/.ssh/agent-env
```

Run the portable checks:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --locked -p bulwark-experiment-e1 -p bulwark-experiment-e2 -p bulwark-experiment-e7
cargo run --locked -p bulwark-experiment-e2
python3 experiments/run_e7.py
python3 experiments/run_e7.py --require-keyword-await
python3 experiments/run_e7_nrepl.py
```

The nREPL probe needs permission to bind a loopback listener.

Reproduce the idle CPU defect:

```sh
python3 experiments/run_e7.py --idle-baseline
python3 experiments/run_e7.py --idle-executor
python3 experiments/run_e7.py --idle
```

Run the Servo and native checks:

```sh
cargo build --locked -p bulwark-experiment-e4 -p bulwark-experiment-e7 --features servo
cargo clippy --locked -p bulwark-experiment-e7 --features servo --all-targets -- -D warnings
python3 experiments/run_e4.py
python3 experiments/run_e7.py --servo
python3 experiments/run_e7_native.py
python3 experiments/run_e7_nrepl.py --servo
python3 experiments/run_e7_nrepl.py --native
python3 experiments/run_e7_nrepl.py --native --close-window
```

On macOS, Servo needs graphics access outside the Codex filesystem sandbox.
The first executable start can take several seconds with a cold cache.
The native lifecycle suite deliberately emits errors and a panic message in its expected-failure cases.

## Code entry points and remaining limits

- `experiments/e7-async-bridge/src/main.rs`: runtime ownership, worker lifetime, program selection, and headless driver.
- `experiments/e7-async-bridge/src/bridge.rs`: bounded browser requests, one-shot replies, native functions, and event queue.
- `experiments/e7-async-bridge/src/engine.rs`: mock and real Servo backends.
- `experiments/e7-async-bridge/src/native.rs`: winit host, proxy wakeups, and shutdown.
- `experiments/e7-async-bridge/src/nrepl.rs`: runtime-thread nREPL admission and duration measurements.
- `experiments/e7-async-bridge/nrepl.cljrs`: browser fixture and synchronous helpers that park their caller's fiber.
- `experiments/run_e7_nrepl.py`: two-session protocol assertions and shutdown checks.
- `crates/bulwark-runtime/src/lib.rs`: the existing embedded runtime host.
- `crates/bulwark-shell/src/main.rs`: the product bootstrap, which still exits without a window.

E1 and E6 preserve the earlier shared-thread scheduling comparison. Their old frame deadlines do not gate the separate-thread design.
Manual E7 input inspection timed out. E3's earlier native input observations remain separate evidence.
Wikipedia still shows black squares for some icons, and the probes emit an existing GL texture warning.
IME, clipboard integration, display-scale transitions, Linux native behavior, and multiprocess behavior remain follow-up checks.
