# D1: Engine and runtime thread ownership

Status: provisional, updated 2026-11-06. Separate-thread direction under investigation in E7.

The owner requested async Clojurust user code with Servo and Clojurust on separate threads.
The main thread owns `winit`, Servo, webviews, rendering contexts, and native chrome.
The Clojurust thread owns the runtime, GC values, Tokio LocalSet, and future nREPL poller integration.
Owned Rust requests, replies, and events cross channels. Runtime and Servo objects never cross threads.
The LocalSet runs continuously, independently of the engine's frame schedule.

E7 runs pure async Clojure helpers against a bounded channel bridge and real Servo webviews.
During a deliberate 200 ms runtime block, Servo completed 57 turns and painted six frames.
The page timer also advanced.
The probe covers request correlation, overload, ordered lifecycle events, deadlines, and explicit handle invalidation.
The [async bridge report](../async-bridge.md) records the mechanism and its limits.

Browser operations return futures. User entry points use `^:async` and `await`.
At the new pin, native Clojure bodies run on stackful fibers within the LocalSet.
Waits and `Thread/sleep` park those fibers. The Rust bridge still uses ordinary async futures.
A raw blocking native call can still stop the Clojurust thread without stopping Servo.
The previous nested-await defect no longer reproduces.
The background GC service still spins while idle, and nREPL admission remains unbounded.
These limits need upstream work and host integration before product adoption.
The [async gap register](../clojurust-async-gaps.md) defines the required changes and acceptance criteria.
The native window probe now uses proxy wakeups and bounds shutdown before a worker join.
Its six lifecycle cases cover completion, intentional close, runtime error, panic, and an unresponsive native call.
The [native bridge report](../native-async-bridge.md) records the results and process-exit fallback.
E7 now admits nREPL evaluations onto runtime fibers and passes finite-load concurrency and shutdown probes.
The [nREPL integration report](../nrepl-runtime.md) records those results.
Admission limits, pending-work limits, and product event overload policies remain open.

E1 and E6 remain evidence for the earlier shared-thread proposal.
That proposal allowed synchronous user code to stop rendering.
The [historical scheduling report](../scheduling.md) preserves its measurements.
The next Phase 0 review must record the revised D1 acceptance gate and queue policies.

## Historical fiber update at release 0.2.34, 2026-11-02

Clojurust 0.2.34 runs `go`, `future`, `thread`, and `io-thread` bodies on fibers on the shell thread.
E6 measured them in a 60 Hz loop. The provisional policy for Clojure work on the thread that owns the runtime
(the runtime thread under the separate-thread direction, or the shell thread under the earlier proposal):

- The host drives the LocalSet once per frame for a fixed budget, through a root fiber parked on a timer.
- Run Bulwark with `CLJRS_FIBER_SLICE_US` at 1–2 ms. A CPU-bound fiber can overrun the budget by one slice.
- Host callbacks call handlers directly. A handler that waits spawns a fiber or sends to a channel.
- Long or waiting work belongs in a fiber, never on the carrier or in a root-fiber evaluation.
- Native functions that block call `cljrs_value::parking::blocking`. A thread-blocking call freezes every fiber.
- nREPL forms run on root fibers. Until csm/clojurust#447, a parked nREPL form blocks the loop that polls it.
  Until csm/clojurust#446, interrupt cannot stop it.

Stop-the-world collection remains a frame risk for whichever thread owns the runtime.
On 0.2.34, E7's keyword-await probe completes, so CLJ-ASYNC-002 no longer reproduces. Idle await polling still costs about one CPU-second per idle second.

This section preserves the earlier shared-thread experiment and its findings at that release.
At the current `c6199fae` pin, E7 uses `spawn_pending` and demonstrates interruption of parked evaluations.
The current design drives the runtime LocalSet continuously on its dedicated thread.
The admission and idle GC requirements at the start of this ADR remain open.
