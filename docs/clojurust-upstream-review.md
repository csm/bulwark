# Clojurust upstream comparison

Review date: 2026-11-07 UTC (2026-11-06 in America/Los_Angeles).
Bulwark baseline: `7c88979` after the requested pull from `main`.

## Result

Both remaining findings still apply to current upstream Clojurust.
CLJ-ASYNC-004 remains the next upstream fix, followed by CLJ-ASYNC-003.
The [gap register](clojurust-async-gaps.md) retains both adoption requirements.

| Revision | Role |
|---|---|
| `c6199faea544e029db7222aa3e9f5c9f64952773` | Bulwark dependency pin |
| `8ef4a4c17c6e320318425de5a9b75adc017df83b` | Upstream `main` at inspection |

The [commit comparison](https://github.com/csm/clojurust/compare/c6199faea544e029db7222aa3e9f5c9f64952773...8ef4a4c17c6e320318425de5a9b75adc017df83b) contains two commits, including a merge.
[PR #467](https://github.com/csm/clojurust/pull/467) reduces fiber overhead and increases safepoint accounting for native calls.
It changes only the async crate README, `builtins.rs`, `fiber.rs`, and `fiber/scheduler.rs`.

The source comparison found identical bytes in these files and directories:

- `crates/cljrs-async/src/runtime.rs`, including `spawn_gc_service`.
- `crates/cljrs-async/src/task.rs`, including notification waits.
- `crates/cljrs-runtime/src/env/gc_roots.rs`, including `async_gc_collect`.
- `crates/cljrs-gc/src/lib.rs`.
- The complete `crates/cljrs-nrepl` directory, including its tests and README.

## CLJ-ASYNC-004: Idle GC still wakes the executor

The [GC service](https://github.com/csm/clojurust/blob/8ef4a4c17c6e320318425de5a9b75adc017df83b/crates/cljrs-async/src/runtime.rs#L172) still repeats `yield_now().await` and `async_gc_collect()` without an idle wait.
The [collector](https://github.com/csm/clojurust/blob/8ef4a4c17c6e320318425de5a9b75adc017df83b/crates/cljrs-runtime/src/env/gc_roots.rs#L426) returns immediately without a GC request or configuration cancellation.
The surrounding task nevertheless stays runnable.

The new [diagnostic example](../experiments/e7-async-bridge/examples/gc_idle.rs) samples executor wakeups without a timer or CPU threshold.
It creates a current-thread executor and a LocalSet whose root future never completes.
After four setup polls, it samples sixteen polls with a counting waker.
The initialized case adds only `cljrs_async::init` to the control.
There are no language futures, fibers, sockets, or browser tasks.

| Revision | Async initialization | Self-waking polls | Wake calls |
|---|---|---|---|
| Bulwark pin | No | 0 / 16 | 0 |
| Bulwark pin | Yes | 16 / 16 | 16 |
| Upstream snapshot | No | 0 / 16 | 0 |
| Upstream snapshot | Yes | 16 / 16 | 16 |

Both builds used macOS arm64, Rust 1.97.1, Tokio 1.54.1, and debug compilation.
The upstream run used an extracted source snapshot and a temporary Cargo manifest with path dependencies.
That manifest compiled the same example with `cljrs-runtime`, `cljrs-async`, `tokio`, and `anyhow`.
The runtime dependency enabled `regex-full`, and both Clojurust dependencies disabled default features.

This diagnostic establishes repeated self-wakes in the sampled idle state.
It does not measure CPU consumption, GC progress, root safety, or shutdown correctness.
The earlier CPU measurements remain separate evidence in the gap register.

Run the control and diagnostic at the Bulwark pin:

```sh
cargo run --locked -p bulwark-experiment-e7 --example gc_idle -- --baseline
cargo run --locked -p bulwark-experiment-e7 --example gc_idle
```

Run the future idle acceptance check:

```sh
cargo run --locked -p bulwark-experiment-e7 --example gc_idle -- --require-idle
```

The acceptance check currently fails because the executor wakes itself.
The diagnostic always finishes after a fixed number of polls, including with the current spinning service.

Upstream [helper parking tests](https://github.com/csm/clojurust/blob/8ef4a4c17c6e320318425de5a9b75adc017df83b/crates/cljrs-async/tests/helper_parking.rs) explicitly initialize outside Tokio to exclude this service.
Their successful suspension checks therefore do not establish idle behavior after normal runtime initialization.

## CLJ-ASYNC-003: Admission and accepted work remain unbounded

The [embedded poller](https://github.com/csm/clojurust/blob/8ef4a4c17c6e320318425de5a9b75adc017df83b/crates/cljrs-nrepl/src/lib.rs#L342) still drains requests until the incoming channel empties or disconnects.
Its signature has no job budget, and `PollStatus` reports only `Open` or `Closed`.
Per-session lanes still use unrestricted `VecDeque<Job>` queues.
The incoming channel still uses `std::sync::mpsc::channel`.

The [network dispatcher](https://github.com/csm/clojurust/blob/8ef4a4c17c6e320318425de5a9b75adc017df83b/crates/cljrs-nrepl/src/server.rs#L154) handles interrupts before evaluation admission.
This separation must survive overload handling.
Replies still use unbounded channels, and incomplete input frames can grow the connection buffer without a size limit.
An admission batch limit alone does not bound accepted jobs, running evaluations, parked evaluations, input bytes, or output bytes.

These conclusions come from source inspection and exact file comparisons.
This review did not repeat the finite-load nREPL integration suite or claim a measured overload failure.

## Upstream follow-up

The review of recent issues and PRs found no dedicated fix for either remaining finding.
[PR #450](https://github.com/csm/clojurust/pull/450) already supplies concurrent nREPL scheduling.
[PR #449](https://github.com/csm/clojurust/pull/449) already supplies interruption of parked evaluations.
Those completed changes do not supply admission limits.

The next change belongs upstream under D10's contribution policy:

1. Replace unconditional GC self-wakes with notification or bounded scheduling.
2. Cover suspension without a GC request and collection after a request.
3. Cover parked roots, repeated initialization, service shutdown, and configuration cancellation.
4. Repeat idle CPU measurements alongside the deterministic checks.
5. Prepare admission budgets and limits for accepted work after the GC fix.

The GC change needs its own upstream regression coverage.
This diagnostic supplies the idle reproducer but does not close the root-safety or lifecycle requirements.
This review leaves the dependency pin and lockfile unchanged.

## Local checks

Workspace formatting and E7 Clippy with all targets passed.
All six E7 unit tests passed.
The control, initialized diagnostic, and both acceptance modes produced the expected results at both revisions.
The initialized acceptance mode failed with `Idle executor continues to wake itself`, as expected for the open defect.
This review did not run the complete upstream suite or repeat native Servo checks.
