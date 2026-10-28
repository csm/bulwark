# D1: Engine and runtime thread ownership

Status: provisional, 2026-10-27. Separate-thread direction under investigation in E7.

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
A blocking user call can still stop the Clojurust thread, but it does not stop Servo.
The pinned runtime repeatedly polls idle awaits and blocks on some nested await expressions.
These limits require upstream fixes before product adoption. Permanent language restrictions are not the resolution.
The [async gap register](../clojurust-async-gaps.md) defines the required changes and acceptance criteria.
The prototype does not yet integrate native window wakeups, nREPL, or product event overload policies.

E1 and E6 remain evidence for the earlier shared-thread proposal.
That proposal allowed synchronous user code to stop rendering.
The [historical scheduling report](../scheduling.md) preserves its measurements.
The next Phase 0 review must record the revised D1 acceptance gate and queue policies.
