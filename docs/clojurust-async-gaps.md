# Clojurust async status

Status: inspected and tested at `c6199faea544e029db7222aa3e9f5c9f64952773`, 2026-11-06.
Bulwark now pins this revision. The previous observations used `c74e54df68185a3852dcd6d6682af5f0f98c1d61`.
The owner requires upstream fixes rather than permanent dependency patches or language restrictions.

## Current findings

| ID | Finding | Current status |
|---|---|---|
| CLJ-ASYNC-001 | Future and promise awaits repeatedly polled | The wait implementation now uses notifications. Whole-runtime idle CPU remains high because of CLJ-ASYNC-004. |
| CLJ-ASYNC-002 | Await under a non-symbol call blocked | The native fiber model removes this evaluator split. The original reproducer completes. |
| CLJ-ASYNC-003 | Embedded nREPL work lacked a bounded host turn | Partially addressed by `spawn_pending`; admission remains unbounded, and E1 still uses the synchronous poll API. |
| CLJ-ASYNC-004 | The background GC service keeps the LocalSet runnable while idle | Confirmed in the current revision. Required upstream work before efficient idle operation. |

These identifiers are local tracking labels, not GitHub issue numbers.
The first two findings no longer require the previously proposed evaluator patches on native targets.
The remaining work concerns idle GC scheduling, nREPL integration, and queue limits.
No permanent local workaround closes these items.

## Execution model

Native `go` bodies and `^:async` calls now run on stackful fibers through `corosensei`.
Tokio schedules those fibers on the existing current-thread executor and LocalSet.
Waiting operations park the fiber and let other tasks run on the same carrier thread.
Ordinary helpers called inside a fiber can also wait through their call stack.
The Rust bridge continues to use ordinary async futures, channels, and one-shot replies.
Servo still owns the main thread.

`go` now returns a result channel. `^:async` still returns a future.
The exported Rust `await_value` supports both, as well as promises.
`Thread/sleep` parks inside a fiber and blocks the OS thread outside one.
Raw blocking Rust calls can still stall the carrier thread.
Automatic yields at evaluator safepoints do not preempt arbitrary native code.

Sources at the new pin:

- [Native dispatch](https://github.com/csm/clojurust/blob/c6199faea544e029db7222aa3e9f5c9f64952773/crates/cljrs-async/src/runtime.rs).
- [Task settlement and notification waits](https://github.com/csm/clojurust/blob/c6199faea544e029db7222aa3e9f5c9f64952773/crates/cljrs-async/src/task.rs).
- [Fiber convergence regressions](https://github.com/csm/clojurust/blob/c6199faea544e029db7222aa3e9f5c9f64952773/crates/cljrs-async/tests/converged.rs).

## CLJ-ASYNC-001: Wait implementation corrected

Future and promise waits now register an event listener before they inspect settlement.
Unsettled waits suspend on that listener rather than repeatedly call `yield_now`.
Settlement and cancellation paths notify listeners.
The old await loop therefore no longer explains the remaining idle CPU use.

The upstream tests cover native parking, GC during suspension, channel results, and error propagation.
This continuation inspects those tests but does not claim a complete upstream test run.
Bulwark's idle integration test still fails the efficiency goal because initialization starts the busy GC service.

## CLJ-ASYNC-002: Native nested waits corrected

The original expression `(:value (await pending))` now completes without a binding workaround.
Native execution uses the ordinary evaluator inside a fiber, so non-symbol callees do not enter a separate blocking evaluator.
Bulwark's strict regression also exercises map, set, vector, function-literal, and computed callees.
It includes `apply`, `reset!`, `swap!`, a synchronous helper, a promise, and a concurrent heartbeat.
The normal bridge example now uses a direct keyword lookup around an awaited native reply.

Run the strict regression:

```sh
python3 experiments/run_e7.py --require-keyword-await
```

A timeout fails this command and terminates the child.
The older `--probe-keyword-await` command retains diagnostic behavior for comparisons with the old revision.
This native result does not establish wasm32 evaluator behavior or every cancellation and GC interleaving.

## CLJ-ASYNC-003: nREPL integration and admission limits remain

The new `Poller::spawn_pending` starts evaluations on fibers and returns before they finish.
Jobs within one session retain arrival order. Different sessions can interleave at parks.
This API addresses the host waiting for one parked evaluation.
It also supports interruption of parked evaluations.

However, `spawn_pending` still drains the incoming queue without a job or time budget.
The existing `poll` and `poll_timeout` APIs still evaluate queued work synchronously.
Bulwark's E1 comparison still calls `poll_timeout`; E7 does not yet integrate nREPL.
The current E6 run recorded a 246.686 ms maximum poll during a burst of twenty 10 ms evaluations.
Interruption and subsequent session recovery passed, with 0.251 ms measured interrupt latency.
The maximum LocalSet slice fell to 2.087 ms because the `go` sleep now parks.

Required follow-up:

1. Use `spawn_pending` for runtime-thread nREPL integration.
2. Bound admission and pending work under a continuous request stream.
3. Assert cross-session progress, same-session order, interruption, and shutdown in the embedded host.

The admission bound remains a missing mechanism, rather than proof that the new API deadlocks.

Source: [embedded poller](https://github.com/csm/clojurust/blob/c6199faea544e029db7222aa3e9f5c9f64952773/crates/cljrs-nrepl/src/lib.rs).

## CLJ-ASYNC-004: Idle background GC service spins

`cljrs_async::init` starts `spawn_gc_service` on native targets.
That task repeats `yield_now().await` and `async_gc_collect()` without an event wait or timer.
The task therefore stays runnable even with no language futures or fibers.
The idle executor control starts this service and waits only on a one-second Rust timer.
The baseline performs the same wait without async initialization.

Run the controls:

```sh
python3 experiments/run_e7.py --idle-baseline
python3 experiments/run_e7.py --idle-executor
python3 experiments/run_e7.py --idle
```

The initialized control used 1.054 CPU-seconds for a 1.017-second runtime wait.
The baseline used 0.098 CPU-seconds. Its first large-binary startup increased process elapsed time to 6.197 seconds.
The ordinary Clojure idle probe used 1.025 CPU-seconds for a 1.019-second runtime wait.
These CPU figures cover the entire child process, including the small engine driver.
They do not measure the GC task in isolation, but agree with the unconditional runnable loop in its source.
Measurements used macOS 26.5.2 arm64, Rust 1.97.1, and debug builds.

The upstream fix must retain GC progress while allowing an idle executor to suspend.
It needs notification or bounded scheduling for collection requests, plus coverage for parked roots and shutdown.
A deterministic pending-task test must accompany CPU measurements.
Bulwark does not disable the service locally.

Source: [background GC service](https://github.com/csm/clojurust/blob/c6199faea544e029db7222aa3e9f5c9f64952773/crates/cljrs-async/src/runtime.rs#L172).

## Historical evidence at the old pin

The original idle probe used 1.028 CPU-seconds for a 1.089-second process duration.
The original keyword lookup blocked until the parent terminated it after three seconds.
The old E6 poll reached 237.554 ms, and its blocking `go` task reached a 202.183 ms LocalSet slice.
The new fiber model changes those scheduling assumptions.
The [async bridge report](async-bridge.md) preserves the earlier transport measurements.
