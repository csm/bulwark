# D2: Data and host callbacks

Status: accepted for Phase 0, 2026-10-27.

Rust delivers plain event data to one Clojurust dispatcher.
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
