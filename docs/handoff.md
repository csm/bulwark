# Development handoff

Snapshot: Bulwark `main` at `744acac`, followed by upstream adoption and Phase 0 exit on 2026-11-07.
The requested pull reported that this detached worktree was already up to date.
Checkpoint branch: `codex/phase-0-exit`. This checkpoint includes upstream adoption and the completed Phase 0 review.
No pull request or remote push is part of this checkpoint.

## Current state

Phase 0 is complete. The [exit review](phase-0-exit.md) accepts the architecture and records all gate dispositions.
Phase 1 product implementation is next. The default executable still prints the embedded namespace status and exits.
The product shell does not open a browser window yet.
The proven integration mechanisms remain in standalone experiments.

Clojurust fixes are now on upstream `main` at `b847a29b46e2b6b3fbe624f6d2394cb656ea5882`.
The workspace dependency pin and lockfile now use that revision for all ten Clojurust packages.
No dependency patch or local checkout supplies the fixes.
Servo remains `=0.7.0`, single process, with `bundled` and `js_jit`.
The local platform remains macOS 26.5.2 arm64 with Rust 1.97.1.

## Completed in this continuation

CLJ-ASYNC-004 now passes the deterministic idle acceptance assertion.
Both baseline and initialized controls produce zero self-wakes across sixteen sampled polls.
The initialized one-second control used 0.035 CPU-seconds, compared with the earlier 1.054 CPU-seconds.
The baseline used 0.026 CPU-seconds. The Clojure timer used 0.039 CPU-seconds.
These are whole-process measurements, including startup and the mock engine driver.

CLJ-ASYNC-003 now has upstream mechanisms and E7 host integration.
The driver uses `spawn_pending_budget` with four jobs per turn.
Its server accepts at most 32 unfinished jobs globally and 24 per session, including parked and queued work.
It limits request frames to 64 KiB and queued output to 256 KiB per connection.
The driver records batch size, sampled pending count, budget exhaustion, and admission duration.
A later one-millisecond timer tick supplies each subsequent admission turn.

The expanded nREPL probe fills session capacity and global capacity with parked and queued evaluations.
It requires overload refusal, protocol interrupts under overload, capacity recovery, and no side effects from cancelled queued forms.
Mock, software Servo, and native Servo pass these assertions and the existing concurrency and shutdown checks.
The native close case still joins the runtime with an evaluation parked, closes the listener, and removes the port file.

The focused upstream suites pass: seven `gc_service` tests and five nREPL `limits` tests.
GC coverage includes suspension, collection requests, parked roots, configuration cancellation, repeated initialization, and shutdown.
The nREPL suite includes request size and output pressure.
These suites ran from the fetched upstream source with its own lockfile and no default features, with `regex-full` enabled.
Their build output is in `/private/tmp/bulwark-upstream-tests`.
The complete upstream suite did not run.

The [D1 ADR](adr/0001-shell-thread.md) now records the ownership review and the initial product queue contract.
It defines queue capacities, payload limits, event overload, admission turns, cancellation, and shutdown.
These product policies require Phase 1 implementation. The small E7 transport remains an experiment.
The exit review records E5 deferral with one visible webview per native window and the final E6 scheduling policy.
All six ADRs now exist, including D4. D3 selects monotonic process-wide identifiers with an explicit exhaustion error.
The [development plan](development-plan.md) and [gap register](clojurust-async-gaps.md) now reflect upstream adoption.
The previous [upstream comparison](clojurust-upstream-review.md) remains historical evidence.

## Validation

| Check | Result |
|---|---|
| Workspace formatting, Clippy, and unit tests | Passed, eight unit tests |
| E7 Clippy with Servo and all targets | Passed |
| E1 five-second concurrent nREPL smoke | Passed progress assertions, historical frame skips remain |
| E2 tree-walk and tiered callbacks | Passed 10,000 ordered callbacks per mode, 200 collections, errors, and redefinition |
| E6 fiber probe and protocol probe | Passed, including parked interruption and session recovery |
| E7 mock and nested-await matrix | Passed |
| E7 deterministic idle controls and CPU controls | Passed |
| Focused upstream GC and nREPL limit suites | Passed, twelve tests |
| E4 Servo DOM channel | Passed, trivial p50 56.125 microseconds, subtree p50 27.058 ms |
| E7 software Servo bridge | Passed, 119 completed requests and 18 painted frames during the raw runtime stall |
| E7 native lifecycle suite | All six cases passed, 19 presentations during the raw runtime stall |
| E7 nREPL with mock, software Servo, and native Servo | Passed overload, interruption, recovery, concurrency, order, redefinition, and shutdown |
| E7 native close with parked nREPL evaluation | Passed worker join, listener close, and port-file removal |

Native nREPL presented 16 frames during CPU-bound Clojure evaluation.
The native overload run reached a sampled pending count of 32, with at most four jobs per admission turn.
Its maximum admission call was 16.666 microseconds.
Measured durations are observations, not latency or display frame-rate guarantees.
The E6 timing run overlapped Servo Clippy compilation and does not supply an isolated performance comparison.
The exit continuation rebuilt E3, E4, and E7 with the final locked dependency graph.
Workspace formatting, Clippy, all eight unit tests, and the default bootstrap passed again.
All six native lifecycle cases and both native nREPL modes passed again.
The final lifecycle run presented 18 frames during the raw stall.
The final native nREPL run presented 16 frames, admitted at most four jobs per turn, and reported twelve budget exhaustions.
Its maximum admission duration was 25.084 microseconds. Both idle controls again reported zero self-wakes.
All local Markdown links in the README, experiment guide, reports, and ADRs resolve.
Remote CI results remain uninspected. CI now requires both deterministic idle acceptance controls.

The existing GL texture warning remains.
The cold Servo build also emitted a linker warning about a large `__eh_frame` section.
The lifecycle suite intentionally emits errors and a panic message in its expected-failure cases.

## Next work, in order

1. Promote the proven mechanisms into the product crates and begin Phase 1 deliverables 1–3.
2. Implement D1's product queue and payload limits with overload and cancellation coverage.
3. Add the remaining Phase 1 command, keymap, hooks, chrome, and browser behavior.
4. Preserve budgeted nREPL admission and overload regressions in the product integration.
5. Run the Phase 1 acceptance checks against the product executable.

D1 accepts separate ownership. It does not claim that product queue policies already exist in the skeleton.
The upstream output limit permits one oversized reply when its connection queue is empty.
Connection count, session count, retained values, active language tasks, and the language heap remain outside these bounds.
A disconnected client can leave its evaluation active until completion or interruption.
Neither upstream limits nor product payload limits imply a bound on total process memory.

E1 and E6 preserve the historical shared-thread comparison. Their old frame deadlines do not gate the separate-thread design.
Raw blocking Rust calls can still stop the runtime thread. Evaluator safepoints cannot preempt arbitrary native code.
Forced process exit does not establish runtime cleanup or rollback of browser side effects.
IME, clipboard, display-scale transitions, Linux native behavior, and multiprocess Servo remain follow-up work.
The initial product uses one visible webview per window until E5 passes.

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
cargo build --locked -p bulwark-experiment-e1 -p bulwark-experiment-e2 -p bulwark-experiment-e6 -p bulwark-experiment-e7
cargo run --locked -p bulwark-experiment-e2
cargo run --locked -p bulwark-experiment-e6
python3 experiments/run_e1.py --seconds 5
python3 experiments/run_e6.py
python3 experiments/run_e7.py
python3 experiments/run_e7.py --require-keyword-await
python3 experiments/run_e7_nrepl.py
cargo run --locked -p bulwark-experiment-e7 --example gc_idle -- --baseline --require-idle
cargo run --locked -p bulwark-experiment-e7 --example gc_idle -- --require-idle
```

The nREPL probes need loopback listener access.

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
The Python runners impose independent process deadlines.

## Code entry points

- `experiments/e7-async-bridge/src/main.rs`: runtime ownership, worker lifetime, program selection, and headless driver.
- `experiments/e7-async-bridge/src/bridge.rs`: bounded browser requests, one-shot replies, native functions, and event queue.
- `experiments/e7-async-bridge/src/engine.rs`: mock and Servo backends.
- `experiments/e7-async-bridge/src/native.rs`: winit host, proxy wakeups, and shutdown.
- `experiments/e7-async-bridge/src/nrepl.rs`: budgeted admission, explicit limits, and measurements.
- `experiments/run_e7_nrepl.py`: overload, protocol, concurrency, and shutdown assertions.
- `experiments/e7-async-bridge/examples/gc_idle.rs`: deterministic idle suspension diagnostic.
- `crates/bulwark-runtime/src/lib.rs`: existing embedded runtime host.
- `crates/bulwark-shell/src/main.rs`: product bootstrap, which still exits without a window.
