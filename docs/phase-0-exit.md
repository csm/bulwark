# Phase 0 exit review

Status: complete, 2026-11-07. Phase 1 can begin.
The product remains a bootstrap executable. This review accepts architecture and experiment evidence, not Phase 1 product behavior.

## Review scope

The [development plan](development-plan.md) requires completed D1 ownership review, E2, and E3 for Phase 0 exit.
E4, E5, and E6 must pass or have a recorded fallback.
ADRs D1–D6 must record final choices for the initial product.
E7 supplies the ownership evidence for the owner's requested separate-thread direction.
E1 remains the comparison with the earlier shared-thread design.

The review uses Servo `=0.7.0` and Clojurust `b847a29b46e2b6b3fbe624f6d2394cb656ea5882`.
All Clojurust packages share this revision. No permanent dependency patch or language restriction supplies an adoption requirement.
The initial platform is macOS arm64, with single-process Servo and egui chrome.
Linux runtime CI remains configured. Native Linux and multiprocess comparisons remain follow-up work under Q2 and Q5.

## Exit criteria

| Gate | Disposition | Evidence and consequence |
|---|---|---|
| D1 ownership review | Complete | Main thread owns Servo, windows, and chrome. Dedicated runtime thread owns Clojurust, GC values, LocalSet, and nREPL. Only owned messages cross channels. |
| E1 shared-thread scheduling | Replaced by E7 for D1 | Native runs missed the original frame target. The failure remains recorded and supports the separate-thread choice. |
| E2 host callback | Passed | 10,000 ordered callbacks per mode, forced collections, stable live objects, errors, and live redefinition in tree-walk and tiered modes. |
| E3 macOS embedding | Passed | Native rendering and link navigation on Servo.org and Wikipedia. Fixture input, editing, scroll, resize, and close passed. |
| E4 DOM channel | Passed | Public JavaScript callbacks, a 5,000-element JSON subtree, and an intercepted page message passed. Trivial-call p50 remained less than 5 ms. |
| E5 multiple visible webviews | Fallback accepted | One visible webview per native window. Native chrome and non-web panels use egui. Web content splits remain unavailable until E5 passes. |
| E6 runtime policy | Complete for separate ownership | Fiber-aware waits, native offload, cooperative interrupts, and bounded nREPL admission define the policy. Product event dispatch must implement D1 limits in Phase 1. |
| E7 bridge and lifecycle | Passed | Mock and Servo bridges, reply correlation, overload, deadlines, invalidation, native wakeups, continued presentation during runtime stalls, and bounded worker shutdown passed. |
| E7 nREPL | Passed | Session and global overload refusal, interruption under overload, capacity recovery, ordered sessions, browser calls, redefinition, and parked-evaluation shutdown passed. |
| CLJ-ASYNC-001 and 002 | Native adoption complete | Notification waits and the callable/promise matrix pass with concurrent heartbeat progress. |
| CLJ-ASYNC-003 | Upstream mechanisms adopted | Four jobs per host turn, 32 global accepted jobs, and 24 accepted jobs per session. Queued, running, and parked work count. |
| CLJ-ASYNC-004 | Native adoption complete | Both idle controls report zero self-wakes across sixteen polls. Focused upstream tests cover collection requests, parked roots, cancellation, and shutdown. |
| D1–D6 ADRs | Complete | The decision table links all six accepted contracts. Implementation remains Phase 1 work. |

## Accepted decisions

| ADR | Initial product choice |
|---|---|
| [D1: ownership](adr/0001-shell-thread.md) | Separate engine and runtime threads, bounded queues and payloads, explicit overload, correlated replies, and bounded shutdown. |
| [D2: runtime boundary](adr/0002-runtime-boundary.md) | Owned data across threads. The runtime creates GC values and calls the current dispatcher var. |
| [D3: resource handles](adr/0003-revocable-handles.md) | Monotonic identifiers that never repeat within a process. Explicit close invalidates resources. Exhaustion returns an error instead of wrapping. |
| [D4: engine boundary](adr/0004-engine-boundary.md) | Engine data and interface in `bulwark-engine`, public Servo adapter in `bulwark-engine-servo`, main-thread execution. |
| [D5: DOM channel](adr/0005-dom-channel.md) | Injected JavaScript agent and evaluation callbacks. Page data remains untrusted. |
| [D6: chrome](adr/0006-chrome-rendering.md) | egui for initial chrome and non-web panels. Clojure defines behavior and layout data. |

## E6 scheduling policy

The runtime drives its LocalSet continuously on its dedicated thread.
User commands and nREPL evaluations run on native fibers.
Supported waits park those fibers. CPU-bound Clojure evaluation yields at evaluator safepoints.
Long native work uses fiber-aware offload or a worker outside the runtime thread.
Raw blocking Rust calls can stop runtime commands and hooks until they return.
Neither evaluator safepoints nor a host admission budget can preempt one arbitrary native call.

The embedded nREPL admits at most four jobs per one-millisecond host turn.
The server counts accepted work through completion or cancellation cleanup, including session queues and parked evaluations.
The network thread answers interrupts before ordinary admission, so overload does not block the interrupt path.
Phase 1 must apply D1's engine admission limits and bounded event delivery in the product crates.
A single synchronous handler can still exceed a nominal time budget.
Handlers that wait must run inside fibers rather than on the runtime carrier.

These choices isolate rendering from runtime stalls. They do not guarantee display frame rate, command latency, or a total memory bound.
The old per-frame root-fiber driver and unbudgeted poll remain comparison tools in E1 and E6.
The product must use E7's continuously driven runtime and budgeted nREPL path.

## Evidence retained

The completed local validation includes workspace formatting, Clippy, and all eight workspace unit tests.
E1 progress checks, E2, both E6 probes, E7 mock, nested awaits, and deterministic idle controls passed at the new pin.
The focused upstream suites passed seven GC-service tests and five nREPL-limit tests.
E4, the real Servo bridge, all six native lifecycle cases, and all nREPL backends also passed.
The exit continuation repeated workspace checks, rebuilt all three Servo probes, and repeated native lifecycle, nREPL, and idle gates.
The [handoff](handoff.md) records commands, measurements, platform details, and the final validation summary.

The [native macOS report](native-macos.md) supplies direct E3 input observations.
The [DOM report](dom-channel.md) supplies payload and timing details.
The [native bridge report](native-async-bridge.md) supplies worker failure and shutdown behavior.
The [nREPL report](nrepl-runtime.md) supplies overload and concurrency assertions.
The [gap register](clojurust-async-gaps.md) supplies upstream adoption details.
The [scheduling report](scheduling.md) preserves the earlier shared-thread results.
Remote CI results and the complete upstream suite remain outside this review.

## Deferred work and limits

E5 remains mandatory before simultaneous web content splits. The fallback does not imply that E5 passed.
Linux native behavior, multiprocess Servo, IME, clipboard, alternate keyboard layouts, and display-scale transitions remain follow-up work.
Wikipedia still shows incomplete icons, and the probes emit an existing GL texture warning.
E3 observations do not establish full site compatibility or native page accessibility.

The upstream nREPL output policy permits one oversized reply when its output queue is empty.
Connection count, session count, retained values, active language tasks, and the language heap remain outside the accepted-work limits.
An interrupted or timed-out browser operation can retain side effects that already occurred.
Forced process exit after an unresponsive worker does not establish cleanup or rollback.
These limits remain explicit in D1 and the integration reports.

## Phase 1 entry

Phase 0 ends with proven standalone mechanisms and accepted contracts.
At Phase 0 exit, the `cargo run --locked` bootstrap printed `{:phase 0, :status :experiments}` and exited.
Promotion into product crates is the next implementation step.

Phase 1 starts with deliverables 1–3: product crate boundaries, native API v0, and event delivery to `bulwark.core/dispatch!`.
That implementation must preserve D1 ownership, queue limits, payload limits, and failure behavior.
Subsequent work adds commands, keymaps, hooks, opt-in loopback nREPL, and one browser behavior written entirely in Clojure.
The Phase 1 acceptance criteria remain open until those product behaviors exist and pass their checks.

The subsequent [Phase 1 foundation checkpoint](phase-1-foundation.md) records implementation progress after this review.
