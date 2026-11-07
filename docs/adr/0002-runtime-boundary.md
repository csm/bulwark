# D2: Data and host callbacks

Status: accepted for Phase 0, 2026-10-27.

The engine sends owned Rust messages to the Clojurust thread.
Only that thread creates GC values from replies and events.
Browser operations return futures, and an async Clojure event loop routes events to one dispatcher.
The engine never waits for a Clojure callback on its own thread.

E7 implements this direction as an experiment. Its contract remains provisional.
The [async bridge report](../async-bridge.md) records the channel and future semantics.

E2 established a separate, local host callback path:
The host resolves the current var value for each call and calls `env::apply::apply_value`.
This path accepts an explicit environment and roots the callee and arguments before collection.
Returned values require a host root if they survive another evaluation or collection.
Allocation frames end after each event.

E2 passed 10,000 calls in both tree-walk and tiered modes.
Each mode forced 100 collections inside active callbacks and another 100 between calls.
Live-object counts stayed constant after warmup.
Exception propagation and live dispatcher redefinition also passed.
No upstream change is necessary for this path.

The native registry and event schema remain Phase 1 work.
This decision does not extend the result to JIT or AOT execution.
