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
