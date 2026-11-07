# Clojurust async status

Status: upstream fixes adopted at `b847a29b46e2b6b3fbe624f6d2394cb656ea5882`, 2026-11-07.
Bulwark pins this revision without dependency patches.
The previous pin was `c6199faea544e029db7222aa3e9f5c9f64952773`.

## Current findings

| ID | Finding | Current status |
|---|---|---|
| CLJ-ASYNC-001 | Future and promise awaits repeatedly polled | Closed for native integration. Notification waits and idle controls pass. |
| CLJ-ASYNC-002 | Await under a non-symbol call blocked | Closed for native integration. The callable and promise matrix passes with heartbeat progress. |
| CLJ-ASYNC-003 | Embedded nREPL work lacked a bounded host turn | Upstream mechanisms adopted. E7 uses budgeted admission and explicit accepted-work limits. Overload, interruption, and recovery pass. |
| CLJ-ASYNC-004 | The background GC service kept the LocalSet runnable while idle | Closed for native integration. The initialized executor suspends without self-wakes. |

These identifiers are local tracking labels, not GitHub issue numbers.
Product queue implementation and resource limits remain separate work under D1.
These results do not establish wasm32 behavior or a bound on total process memory.

## Execution model

Native `go` bodies and `^:async` calls run on stackful fibers through `corosensei`.
Tokio schedules those fibers on the runtime thread's current-thread executor and LocalSet.
Ordinary helpers can wait through their call stack without stopping other fibers.
The Rust bridge uses ordinary futures, channels, and one-shot replies.
Servo and native windows stay on the main thread.

`go` returns a result channel. `^:async` returns a future.
The exported `await_value` supports both, plus promises.
`Thread/sleep` parks inside a fiber and blocks the OS thread outside one.
Raw native calls can still stop the runtime thread.
Evaluator safepoints cannot preempt arbitrary Rust code.

## CLJ-ASYNC-003: Budgeted admission and accepted-work limits

Upstream now supplies `Poller::spawn_pending_budget` and `PollBatch { status, jobs, budget_exhausted }`.
The job budget bounds admission work by job count. It does not bound elapsed time for one job.
E7 admits at most four jobs per turn and returns to the LocalSet between turns.
The next one-millisecond timer tick supplies another turn, including after budget exhaustion.
The driver records batch size, sampled pending count, exhaustion count, and call duration.

`Config::limits` counts accepted jobs until completion or cancellation cleanup.
The count includes network queues, per-session queues, running evaluations, and parked evaluations.
E7 sets 32 global jobs, 24 jobs per session, 64 KiB per request, and 256 KiB queued output per connection.
The network thread answers `describe` and `interrupt` before ordinary admission.
An overloaded request receives an error with `overloaded` and `done` status.

The integration probe parks two sessions, fills one session's 24 slots, and requires refusal of its next evaluation.
A third session still calls the browser while global capacity remains.
The probe then fills all 32 slots and requires global overload refusal.
Protocol interrupts cancel the parked and queued evaluations. Both sessions recover, and cancelled queued forms cause no side effects.
The existing order, heartbeat, browser-call, redefinition, and shutdown assertions remain active.

The upstream output limit permits one oversized response when its queue is empty.
It does not cap one response allocation, session count, connection count, or the language heap.
Disconnected clients can leave evaluations active until completion or interruption.
Product adoption must retain these limits explicitly rather than claim bounded process memory.

Run the integration probe:

```sh
cargo build --locked -p bulwark-experiment-e7
python3 experiments/run_e7_nrepl.py
```

Source: [embedded poller and limits](https://github.com/csm/clojurust/tree/b847a29b46e2b6b3fbe624f6d2394cb656ea5882/crates/cljrs-nrepl).

## CLJ-ASYNC-004: GC service suspends until a request

The upstream GC service registers its waker before it checks for collection or configuration cancellation.
Without a request, the service suspends. Collection requests wake it, including requests from another thread.
The service yields after collection, then returns to the request wait.
Dropping the LocalSet removes the service's waker registration.

Both Bulwark controls report zero self-wakes across sixteen sampled polls.
The initialized control now passes `--require-idle`.
Whole-process idle measurements used 0.026 CPU-seconds without initialization, 0.035 with initialization, and 0.039 with a Clojure timer.
Each runtime waited approximately one second. Startup and the mock engine driver contribute to these measurements.
The previous initialized result was 1.054 CPU-seconds.
These measurements used macOS 26.5.2 arm64, Rust 1.97.1, and debug builds.

Run the deterministic controls:

```sh
cargo run --locked -p bulwark-experiment-e7 --example gc_idle -- --baseline --require-idle
cargo run --locked -p bulwark-experiment-e7 --example gc_idle -- --require-idle
```

Run the CPU controls:

```sh
python3 experiments/run_e7.py --idle-baseline
python3 experiments/run_e7.py --idle-executor
python3 experiments/run_e7.py --idle
```

The focused upstream `gc_service` suite also passes all seven tests.
It covers idle suspension, same-thread and cross-thread requests, configuration cancellation, parked roots, repeated initialization, and service shutdown.
The focused upstream `limits` suite covers admission budgets, overload, parked jobs, request size, and output pressure.
The complete upstream suite remains outside this run.

Source: [GC service and regression tests](https://github.com/csm/clojurust/tree/b847a29b46e2b6b3fbe624f6d2394cb656ea5882/crates/cljrs-async).
The [earlier upstream comparison](clojurust-upstream-review.md) preserves the original sixteen-of-sixteen failure at the previous revisions.

## Native nested waits

The strict regression covers keyword, map, set, vector, function-literal, and computed callees.
It also covers `apply`, `reset!`, `swap!`, a synchronous helper, a promise, and a concurrent heartbeat.
All eleven callable and promise cases pass at the new pin.

```sh
python3 experiments/run_e7.py --require-keyword-await
```

## Historical evidence at the old pin

The original idle probe used 1.028 CPU-seconds for a 1.089-second process duration.
The original keyword lookup blocked until the parent terminated it after three seconds.
The old E6 poll reached 237.554 ms, and its blocking `go` task reached a 202.183 ms LocalSet slice.
The new fiber model changes those scheduling assumptions.
The [async bridge report](async-bridge.md) preserves the earlier transport measurements.

The imported Linux report at release 0.2.34 (`4e9dfd0516b9fd7623c6a8fa1a20e39e80f3e30b`) also found approximately one CPU-second per idle second.
Its keyword-await probe completed. These observations precede the current notification-wait inspection and GC-service diagnosis.
The imported E6 fiber probe and its historical findings remain in [the experiment guide](../experiments/README.md).
