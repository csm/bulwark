# Clojurust async gaps for upstream fixes

Status: required upstream work, observed at `c74e54df68185a3852dcd6d6682af5f0f98c1d61`.
The observations do not establish the status of current upstream HEAD.
No upstream issue or pull request exists for these findings yet.

The owner requires fixes in `csm/clojurust`.
Bulwark must not adopt a permanent fork, local dependency patch, or language restriction as the resolution.
The E7 binding workaround remains a temporary diagnostic aid.
The [async bridge report](async-bridge.md) records the architecture and transport results.

## Tracking and priority

| ID | Upstream change | Evidence | Adoption gate |
|---|---|---|---|
| CLJ-ASYNC-001 | Suspend future and promise waiters until a state change | Future idle CPU reproduced. Promise path confirmed by source inspection. | Required before product adoption of the async runtime. |
| CLJ-ASYNC-002 | Evaluate non-symbol calls asynchronously inside async bodies | Keyword lookup with an awaited argument blocks in E7. | Required before general async user code is supported. |
| CLJ-ASYNC-003 | Add bounded nREPL polling for embedded async hosts | E6's zero-timeout poll drains more than 200 ms of queued work. | Required for runtime-thread nREPL integration with bounded queue work. |

Each item needs an upstream issue or PR, regression coverage, and a deliberate dependency update after merge.
The identifiers in this document are local tracking identifiers, not GitHub issue numbers.

## CLJ-ASYNC-001: Idle awaits repeatedly poll

### Evidence and impact

`cljrs_async::await_value` repeatedly reads future state and calls `tokio::task::yield_now` until settlement.
Its promise branch follows the same pattern.
Yielding permits other tasks to run, but keeps the waiter runnable.
The runtime therefore consumes CPU during an otherwise idle wait.

The final mock timer probe recorded 1.089 seconds elapsed and 1.028 CPU-seconds for a one-second timer.
This measurement covers the child process, including its small engine driver.
It does not isolate the waiter as a microbenchmark.
The source inspection establishes the repeated polling mechanism.

Sources at the pinned revision:

- [Future and promise await paths](https://github.com/csm/clojurust/blob/c74e54df68185a3852dcd6d6682af5f0f98c1d61/crates/cljrs-async/src/eval_async.rs#L277).
- [Async future settlement](https://github.com/csm/clojurust/blob/c74e54df68185a3852dcd6d6682af5f0f98c1d61/crates/cljrs-async/src/eval_async.rs#L90).
- [Promise delivery and future cancellation state](https://github.com/csm/clojurust/blob/c74e54df68185a3852dcd6d6682af5f0f98c1d61/crates/cljrs-value/src/types.rs#L1177).

### Reproduction

From the Bulwark repository root, run:

```sh
cargo build --locked -p bulwark-experiment-e7
python3 experiments/run_e7.py --idle
```

The program awaits `(timeout 1000)` inside an `^:async` function.
The runner reports elapsed time and child CPU time.
The promise behavior needs a separate runtime regression and measurement.

### Required upstream behavior

Pending future and promise waiters must suspend through a notification mechanism.
Settlement must wake every applicable waiter without a lost-wakeup race.
Completion before registration, during registration, and after registration must all work.
All producers must participate, including native completions, promise delivery, failure, and cancellation.
Synchronous `deref` waiters must retain their existing behavior.

GC roots must retain the awaited object and its required state across suspension.
The fix must preserve exception data, resource-limit errors, cancellation semantics, and plain-value awaits.
Cancellation notification does not imply cancellation of an underlying browser operation.

Regression coverage must include:

- An unsettled waiter returns `Poll::Pending` without repeatedly waking itself.
- Early settlement, settlement during registration, and late settlement all resume the waiter.
- Multiple waiters receive the result of one future or promise.
- Success, failure, cancellation, gas exhaustion, and memory-limit errors preserve their result semantics.
- Garbage collection during suspension does not reclaim a live waiter or its result.
- A sleeping timer still completes while unrelated async tasks make progress.

An upstream benchmark must record idle CPU before and after the fix.
The benchmark needs a stated platform and build mode.
A noisy wall-clock CPU threshold must not replace deterministic wakeup tests.

## CLJ-ASYNC-002: Non-symbol calls take the blocking evaluator path

### Evidence and impact

The async evaluator delegates calls with a non-symbol head to synchronous `eval`.
A keyword call is one such form.
An awaited argument can therefore take a blocking path despite the enclosing `^:async` function.
If completion requires the same LocalSet, that thread cannot make progress.

The E7 reproducer enters its async function, then blocks until the parent process terminates it.
Its pending future needs a 100 ms timer on that LocalSet:

```clojure
(defn ^:async reproduce! []
  (let [pending (go (await (timeout 100)) {:value :done})]
    (:value (await pending))))
```

The complete namespace and startup marker are in [await-keyword.cljrs](../experiments/e7-async-bridge/await-keyword.cljrs).
The temporary E7 control binds the awaited value before lookup:

```clojure
(let [reply (await pending)]
  (:value reply))
```

That control works, but does not satisfy the upstream acceptance criterion.
Users must not need this rewrite for a valid async expression.

Sources at the pinned revision:

- [Non-symbol call fallback](https://github.com/csm/clojurust/blob/c74e54df68185a3852dcd6d6682af5f0f98c1d61/crates/cljrs-async/src/eval_async.rs#L261).
- [Async call evaluation and form interception](https://github.com/csm/clojurust/blob/c74e54df68185a3852dcd6d6682af5f0f98c1d61/crates/cljrs-async/src/eval_async.rs#L758).

### Reproduction

From the Bulwark repository root, run:

```sh
cargo build --locked -p bulwark-experiment-e7
python3 experiments/run_e7.py --probe-keyword-await
```

The runner terminates the blocked child after three seconds.
`REPRODUCED` reports the defect. It is not a successful async feature test.
The upstream regression needs a process deadline because an executor timer cannot stop its own blocked thread.

### Required upstream behavior

Calls inside async bodies must evaluate their callee and awaited arguments through async evaluation.
The fix must preserve ordinary callable semantics, error behavior, and left-to-right evaluation order.
Each callee and argument expression must run exactly once.
GC roots must retain the callee and earlier arguments across a later argument's suspension.

The regression matrix must cover keyword, map, set, vector, function-literal, and computed call heads.
It must cover `await` in arguments and in a computed callee expression.
It must also cover failed awaited arguments, ordinary non-callable errors, and macro-expanded forms.
An independent heartbeat must advance during each pending await.

The keyword case is reproduced. The broader call matrix remains an upstream coverage requirement.
The source also delegates form-intercepted native calls to synchronous evaluation.
That audit must include `apply`, `reset!`, and `swap!`, which require special call handling.
Their behavior must remain correct without re-evaluating argument forms.
These additional cases are source-identified audit targets, not separately measured failures in E7.

## CLJ-ASYNC-003: The embedded nREPL poller cannot bound queued work

### Evidence and impact

`Poller::poll_timeout` limits the wait for the first job, then drains the queue through `poll`.
A zero timeout therefore does not limit the number or duration of evaluations.
E6 queued twenty evaluations that each called `Thread/sleep` for 10 ms.
One measured native poll took 237.554 ms.

On the separate-thread design, this delay stops Clojurust tasks and message handling, while Servo continues independently.
The [E6 report](scheduling.md) records the measurements and their limits.
The [upstream poller source](https://github.com/csm/clojurust/blob/c74e54df68185a3852dcd6d6682af5f0f98c1d61/crates/cljrs-nrepl/src/lib.rs#L248) establishes the queue-draining behavior.

From the Bulwark repository root, run:

```sh
cargo build --locked -p bulwark-experiment-e1
python3 experiments/run_e6.py
```

### Required upstream behavior

The embedded poller needs an API that limits jobs or elapsed work between jobs.
The existing unrestricted API can retain its current behavior.
The bounded API must leave excess jobs queued and report enough state for the host to schedule another turn.
It must preserve queue order, responses, sessions, shutdown, and interruption behavior.
Regression coverage must demonstrate async progress between bounded batches under a continuous request stream.

This limit cannot preempt one synchronous evaluation or a blocking native call.
It is a fairness mechanism between jobs.
E6 already demonstrates mid-form interruption of instrumented Clojure code from a second connection.
That working interruption path does not need a speculative replacement.

## Scope boundaries

Synchronous top-level `await` and a direct `Thread/sleep` are blocking operations in the current execution model.
Their blocking behavior is not evidence of an async evaluator defect by itself.
Async entry points, background work, and separate Servo ownership remain necessary after these fixes.
Hard cancellation of browser side effects also remains a Bulwark protocol concern.

## Delivery and adoption

1. Compare each finding with current upstream HEAD before a patch or issue.
2. Add a bounded failing regression in `csm/clojurust` for each reproducible defect.
3. Implement the upstream fix and run the async, GC, interpreter, and affected host suites.
4. Submit an upstream PR under D10 and link it from this document.
5. After merge, update the Bulwark pin and lockfile deliberately.
6. Rerun E7's workflow, idle benchmark, keyword-await probe, and real Servo stall probe.
7. For the nREPL change, rerun E6 and the runtime-thread integration checks.

The corrected keyword-await probe must complete through the original expression without its temporary binding workaround.
The idle benchmark must show that pending async waits no longer continuously consume a core.
The D1 review must record the merged revisions and reproduction results before product adoption.
