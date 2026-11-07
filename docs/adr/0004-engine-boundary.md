# D4: Engine abstraction boundary

Status: accepted for Phase 1, 2026-11-07.

## Decision

`bulwark-engine` defines the engine-facing data contract and the main-thread engine interface.
`bulwark-engine-servo` implements that interface through Servo's published embedding API.
The interface uses owned Rust data, explicit resource handles, and structured errors.
Servo types remain inside the adapter. Clojurust values remain inside `bulwark-runtime`.

The shell owns the adapter and calls it on the main thread, as required by D1.
The runtime sends requests through the bounded bridge.
The shell admits those requests and converts adapter results into replies and events.
An operation can start synchronously and finish through a later engine callback.
The bridge correlates that callback with the request identifier and its one-shot reply.
The adapter does not run an executor or wait for the runtime to consume a reply.

The initial interface covers view creation and close, navigation, reload, history, view information, input, geometry, focus, and JavaScript evaluation.
It also exposes owned lifecycle events, engine wakeups, frame readiness, and explicit shutdown.
The adapter owns native resources and invalidates handles before their replacement or destruction.
The shell owns native windows, chrome, and presentation scheduling.
The adapter translates engine input and rendering operations without defining commands, keymaps, hooks, or browser behavior.

The interface can use a trait with an associated adapter type.
Phase 1 does not require dynamic loading, a backend registry, or a second production engine.
Exact method signatures belong to the implementation review, subject to this data and ownership contract.
A mock implementation supplies deterministic tests for request correlation, deadlines, overload, and stale handles.

## Evidence

E3 embeds Servo 0.7.0 through its public crate and forwards native input.
E4 uses public JavaScript callbacks and resource interception.
E7 supplies mock and Servo implementations behind the same owned request/reply bridge.
Its native driver preserves main-thread ownership while Clojurust runs independently.
The [Phase 0 exit review](../phase-0-exit.md) records the acceptance evidence and remaining platform limits.

## Initial scope

The first product uses single-process Servo and one visible webview per native window.
Multiple visible webviews require E5 before web content splits.
The initial chrome uses egui under D6.
DOM node handles, privileged page bridges, and capability enforcement remain later integration work under D5.
The [D1 contract](0001-shell-thread.md) supplies queue, payload, overload, cancellation, and shutdown requirements.
