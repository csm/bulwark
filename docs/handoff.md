# Development handoff

Snapshot: Bulwark `main` at `fc68765`, followed by the command and browser continuation on 2026-11-08.
The fast-forward pull advanced this worktree from `678fcf5` to `fc68765`.
The product nREPL continuation starts from `7781d87` in an isolated development worktree.
Working branch: `codex/phase-1-product-nrepl`.

## Phase 1 product nREPL continuation

The [product nREPL checkpoint](phase-1-nrepl.md) adds `--nrepl` and `--profile DIR` to the native shell.
The listener binds only to `127.0.0.1` and uses an operating-system-selected port.
The profile defaults to `$HOME/.bulwark`. Its `.nrepl-port` file identifies the listener.
Exclusive file creation prevents a second process from replacing an active profile's discovery file.
Normal shutdown and runtime failures close the listener and remove the port file.

The runtime admits at most four nREPL jobs per one-millisecond turn.
Limits remain 32 unfinished jobs globally, 24 per session, 64 KiB request frames, and 256 KiB queued output per connection.
The product process suite covers overload, interruption, capacity recovery, ordered forms, live commands, key bindings, and title hooks.
It also covers profile collisions and shutdown with parked evaluations.
All three mock cases and all four native cases passed locally.
Every native case joined the runtime worker. The normal native case painted eight frames and made eleven presentations.
Formatting, workspace Clippy, all 33 workspace tests, native/Servo Clippy, and all five native unit tests passed.
The Servo contract, seven existing native process cases, and both product smokes passed.
The software Servo smoke painted thirteen frames and joined the worker.

The [commands CI run](https://github.com/csm/bulwark/actions/runs/42580601137) passed both runtime jobs, Servo Clippy, and native unit tests.
The Servo job reached its 30-minute deadline during another SpiderMonkey compilation.
The workflow now keeps a single native feature configuration for product Servo checks and permits 60 minutes.
The workflow also runs the mock and native product nREPL suites.
Linux CI results for this continuation remain pending.

## Phase 1 commands and browser continuation

The [command checkpoint](phase-1-commands.md) adds the Clojure command registry, Emacs-style key sequences, event hooks, and browser commands.
Commands retain Vars, so later definitions affect existing bindings.
Hooks run in registration order, await returned futures, and use a snapshot for each event.
The default dispatcher routes keys to commands and events to hooks.
The native shell installs its echo policy as hooks instead of replacing the dispatcher.

The Clojure browser layer tracks the active view and supplies navigation, reload, back, and forward helpers.
Its title hooks retain the latest 256 visits and omit consecutive identical URL/title pairs.
`C-x h` displays recent titles. `bulwark.browser/visit-history!` revisits an entry by index.
The new command and browser suites pass against the product mock bridge in both TreeWalk and Tiered modes.
All 30 workspace tests, workspace Clippy, and the headless smoke passed locally.
All three native unit tests and native/Servo Clippy passed.
All seven native process cases passed, including browser commands/history and hook-error shutdown.
Every native case joined the runtime worker. The final browser case painted 21 frames and made 24 presentations.
The Servo contract tests and software smoke passed. The software smoke painted 14 frames and joined the worker.

Native input routing and echo geometry remain unchanged.
Command keys still reach Servo before Clojure receives them. Key interception remains open.
The product still requires `--features native` for a window. The later continuation adds opt-in nREPL.

The [post-merge main CI run](https://github.com/csm/bulwark/actions/runs/42578603502) passed both runtime jobs but failed the Servo job.
SpiderMonkey attempted a source build after its prebuilt archive failed, then stopped because `llvm-objdump` was absent.
The workflow now installs `llvm`. That dependency change still requires Linux CI confirmation.

## Phase 1 Linux CI continuation

The [main CI run](https://github.com/csm/bulwark/actions/runs/42541318802) failed before the product Servo tests and software smoke.
Rust 1.99.1 deprecated `AtomicU64::fetch_update`, and both Linux Clippy jobs rejected the warning.
The macOS runtime job was cancelled after the Linux runtime failure.

The allocator now uses a compare-and-exchange loop with the same relaxed ordering and exhaustion boundary.
This implementation also supports the local Rust 1.97.1 toolchain.
A concurrent allocation test requires every remaining identifier exactly once at exhaustion.
Local formatting, workspace Clippy, all 28 workspace tests, and the headless product smoke passed.
The [first branch CI run](https://github.com/csm/bulwark/actions/runs/42541766008) passed both runtime jobs, Servo Clippy, and native unit tests.
The Linux Servo contract tests and software smoke also passed. The smoke painted 13 frames and joined the runtime worker.
The native process runner then failed because `libxkbcommon-x11.so` was absent.
The Linux dependency installation now includes `libxkbcommon-x11-dev`.
The [final CI run](https://github.com/csm/bulwark/actions/runs/42542721292) passed all three jobs at code commit `93a0724`.
The Linux software smoke painted 14 frames and joined the runtime worker.
All five native process cases passed: page and chrome, parked close, runtime failure, dispatcher failure, and native quit.
Every native case reported `shutdown=joined`. The page and chrome case painted eight frames and made nine presentations.
These Xvfb checks establish automated Linux coverage. Interactive Linux behavior remains untested.

## Current state

Phase 0 is complete. The [exit review](phase-0-exit.md) accepts the architecture and records all gate dispositions.
Phase 1 foundation is implemented. The default executable prints `{:phase 1, :status :foundation}` and exits.
The `native` feature opens a browser window with Servo content and an egui echo area.
The product bridge and dedicated runtime worker now run with a mock engine through `cargo run --locked -- --headless-smoke`.
The [foundation report](phase-1-foundation.md) records the native API, coverage, and remaining integration work.
The product Servo adapter now supports software rendering through `cargo run --locked --features servo -- --servo-smoke`.
The product shell now includes the native window and input mechanisms.
The [native checkpoint](phase-1-native.md) records the implementation, commands, and interactive results.

## Phase 1 native continuation

Run `cargo run --locked --features native -- https://servo.org` to open the product window.
The native shell uses one offscreen Servo surface above a 28-pixel logical echo area.
It composites that surface and egui before one native presentation.
The existing product bridge carries Clojure echo updates and ordered key events.
The embedded `bulwark.shell` namespace owns initial view creation and echo policy.

The shell handles pointer, button, wheel, key, resize, focus, and scale events.
It preserves button ownership across the page/chrome boundary and coalesces repeated frame wakes.
New views inherit scale and focus. Native exposure events can force a repaint.
Shutdown retains the product admission, invalidation, cancellation, and two-second join contract.

The five cases in `experiments/run_product_native.py` passed locally.
The captured native frame showed the local page and the Clojure echo text together.
The runner also covers parked close, runtime failure, dispatcher failure, and native quit.
The interactive macOS checks passed on 2026-11-08 after a fix to egui input routing.
The egui root previously consumed page clicks. Its input rectangle now covers only the echo strip.
The regression test covers page and echo hit testing before and after resize.
Native button clicks, text editing, scrolling, echo isolation, resize, key delivery, and close passed.
The smoke flushes a final diagnostic frame before teardown to handle suppressed macOS redraw callbacks.
Its result does not establish interactive display behavior.

CI compiles the native feature and runs its unit tests and process runner under Xvfb.
The Linux CI continuation above records the current remote results.

Clojurust fixes are now on upstream `main` at `b847a29b46e2b6b3fbe624f6d2394cb656ea5882`.
The workspace dependency pin and lockfile now use that revision for all ten Clojurust packages.
No dependency patch or local checkout supplies the fixes.
Servo remains `=0.7.0`, single process, with `bundled` and `js_jit`.
The local platform remains macOS 26.5.2 arm64 with Rust 1.97.1.

## Phase 1 foundation continuation

Product crates now define owned requests, replies, lifecycle/key events, and the engine interface.
The bridge enforces 32 queued requests, 64 unfinished requests, 256 events, payload limits, and per-turn admission.
Completed replies retain capacity until consumption. Cancellation retains capacity until engine cleanup.
Event overflow records the first fatal error outside the queue and wakes host shutdown.
Resource registries use process-wide identifiers that never repeat or wrap.

The runtime installs the v0 `bulwark.native` functions and drives a Clojure event loop on its dedicated LocalSet thread.
Dispatchers can await browser calls and can be redefined without restarting the worker.
The product smoke runs Clojure assertions for native calls, history, ordered events, redefinition, and stale handles.
CI now runs that smoke on its Linux and macOS runtime jobs.
The current local validation and reproduction commands are in the foundation report.

## Phase 1 Servo continuation

The product adapter implements view lifecycle, navigation, history, information, and deferred JavaScript evaluation.
It accepts one live webview per context and sends lifecycle events through the existing bounded queue.
Closing a view invalidates pending evaluations. Each tick removes cancelled or expired requests.
Shutdown invalidates resources before the host cancels the runtime worker.

The software smoke loads a local HTML file and requires a title event in Clojure and a painted frame.
Local macOS validation passed with 11 painted frames and a joined worker.
Workspace tests, workspace Clippy, Servo Clippy, adapter tests, and the mock smoke passed.
CI includes the software smoke and adapter tests in a Linux job.
Both passed in the Linux CI continuation above.

## Completed during Phase 0 exit

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
Remote CI passed both runtime jobs in the Linux CI continuation above.
CI requires both deterministic idle acceptance controls.

The existing GL texture warning remains.
The cold Servo build also emitted a linker warning about a large `__eh_frame` section.
The lifecycle suite intentionally emits errors and a panic message in its expected-failure cases.

## Next work, in order

1. Finish Linux CI with the shared native feature configuration, longer deadline, and product nREPL suites.
2. Add key interception while preserving ordinary Servo input during runtime stalls.
3. Run the Phase 1 acceptance checks against the product executable, including remaining interpreter coverage.

D1 accepts separate ownership. The foundation implements product transport policies, and the Servo adapter uses those policies.
The native shell and local fixture checks are complete. Broader platform and compatibility checks remain open.
The upstream output limit permits one oversized reply when its connection queue is empty.
Connection count, session count, retained values, active language tasks, and the language heap remain outside these bounds.
A disconnected client can leave its evaluation active until completion or interruption.
Neither upstream limits nor product payload limits imply a bound on total process memory.

E1 and E6 preserve the historical shared-thread comparison. Their old frame deadlines do not gate the separate-thread design.
Raw blocking Rust calls can still stop the runtime thread. Evaluator safepoints cannot preempt arbitrary native code.
Forced process exit does not establish runtime cleanup or rollback of browser side effects.
IME, clipboard, interactive display-scale transitions, Linux native behavior, and multiprocess Servo remain follow-up work.
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
- `crates/bulwark-engine/src/bridge.rs`: product queue, payload, admission, reply, and shutdown contracts.
- `crates/bulwark-engine/src/mock.rs`: deterministic product test engine.
- `crates/bulwark-runtime/src/native.rs`: native API registry and GC conversion.
- `crates/bulwark-runtime/src/worker.rs`: dedicated runtime thread and event fiber lifecycle.
- `crates/bulwark-runtime/src/nrepl.rs`: opt-in listener, profile discovery, and budgeted admission.
- `experiments/run_product_nrepl.py`: product protocol, overload, live-code, and cleanup assertions.
- `clj/bulwark/bridge.cljrs`: ordered event loop calling the current dispatcher var.
- `clj/tests/foundation.cljrs`: product integration assertions.
- `crates/bulwark-engine-servo/src/engine.rs`: product Servo adapter, deferred evaluations, lifecycle events, and frame painting.
- `clj/tests/servo.cljrs`: software Servo integration assertions against a local HTML fixture.
- `crates/bulwark-shell/src/main.rs`: bootstrap, mock smoke, software Servo driver, and native feature entry point.
- `crates/bulwark-shell/src/native/`: winit host, input routing, Servo/egui composition, and bounded shutdown.
- `clj/bulwark/shell.cljrs`: initial view creation and default echo dispatcher.
- `clj/bulwark/commands.cljrs`: documented commands and live Var invocation.
- `clj/bulwark/keymap.cljrs`: Emacs-style sequence parsing and command dispatch.
- `clj/bulwark/hooks.cljrs`: ordered, awaited event hooks.
- `clj/bulwark/browser.cljrs`: browser commands and retained title history.
- `clj/tests/commands.cljrs` and `clj/tests/browser.cljrs`: interpreter and tiered integration suites.
- `clj/tests/native.cljrs`: native smoke program.
- `experiments/run_product_native.py`: process-bounded product rendering and lifecycle checks.
