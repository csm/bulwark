# Upstream dependencies

Last inspection: 2026-11-12.

## Pins

| Dependency | Pin | Source |
|---|---|---|
| Servo | `=0.7.0`, published 2026-10-05 | [Published crate](https://crates.io/crates/servo/0.7.0) |
| Clojurust | `174557b3eec18117f0b4886076961fe3223316c0` | [Pinned commit](https://github.com/csm/clojurust/commit/174557b3eec18117f0b4886076961fe3223316c0) |

`Cargo.toml` records direct pins. `Cargo.lock` records the complete dependency graph.
Clojurust packages use one git revision. No dependency points at a local checkout.
The bootstrap disables Clojurust's git-backed package loader through `default-features = false`.
It enables the full regex implementation.

Phase 2 adopts two compiler fixes after `b847a29b`.
Compiled symbol literals retain namespace and version fields. Compiled string counts use Unicode characters instead of UTF-8 bytes.
The [mechanism checkpoint](phase-2-mechanisms.md#aot-path-and-upstream-fix) records the failing reproduction and passing upstream checks.
The [window checkpoint](phase-2-windows.md#unicode-compiler-fix) records the Unicode regression and upstream draft PR.
The optional AOT test feature uses the same revision as the product runtime.

Servo enables `bundled` and `js_jit` with default features disabled.
The initial probe excludes multiprocess, GStreamer, WebGL, WebGPU, and WebXR.
These choices establish the E3 baseline. They do not settle the Phase 1 feature set.
The owner confirmed single process first, followed by a multiprocess comparison, under Q5 on 2026-10-27.
Keep multiprocess disabled in the baseline until the comparison supports a change.

## Changes from the original plan

The plan inspected Servo main and cited version 0.1.0.
The bootstrap inspected the published 0.7.0 sources and its `winit_minimal` example.
The crate re-exports the waker, rendering contexts, input events, and geometry types.
Bulwark does not import Servo component crates.

The pinned Clojurust runtime exposes `env::apply::apply_value` for host calls with an explicit environment.
This function roots its callee and arguments before its GC safepoint.
The runtime also exposes `env::gc_roots::force_collect` and `root_value`.
Its `tiered::invoke` helper requires an active evaluation context, so the host uses `apply_value`.

Source: [apply.rs](https://github.com/csm/clojurust/blob/b847a29b46e2b6b3fbe624f6d2394cb656ea5882/crates/cljrs-runtime/src/env/apply.rs).

Release 0.2.34 adds stackful fibers (virtual threads) on native targets.
See the upstream [virtual threads plan](https://github.com/csm/clojurust/blob/4e9dfd0516b9fd7623c6a8fa1a20e39e80f3e30b/docs/virtual-threads-plan.md).
`go`, `future`, `thread`, and `io-thread` bodies run on fibers. `<!`, `<!!`, `deref`, and `Thread/sleep` park inside a fiber.
Safepoints yield when a fiber's time slice expires (`CLJRS_FIBER_SLICE_US`, default 10 ms).
The host API used by E1 and E6:

- `cljrs_async::fiber::run_root(rt, local, carrier, body)` evaluates on a root fiber. The host keeps one `MutatorState` carrier per LocalSet.
- `fiber::spawn_rooted(roots, body)` starts a fiber from Rust and roots the captured values.
- `fiber::park_on(future)` parks the current fiber. A root that parks on a timer gives the LocalSet a bounded slice.
- `fiber::drop_local_with_carrier` destroys the LocalSet with its carrier state installed.
- `cljrs_value::parking::blocking(job)` moves owned Rust work to the worker pool and parks the caller.

The earlier pin required `(await (timeout 100))` inside `go`. E1 now uses the plan's `<!` form.
The upgrade needed no source change in E2 or the bootstrap.

## Gaps and follow-up

The owner approved preparation of required Clojurust fixes under Q4 on 2026-10-27.
Experiments must establish each gap before a change is necessary.
Prepare upstream patches with regression coverage under D10's contribution policy.

The [Clojurust async gap register](clojurust-async-gaps.md) defines the required fixes, reproducers, and acceptance criteria.
The new pin uses native stackful fibers and notification-based future and promise waits.
CLJ-ASYNC-002 no longer reproduces. E7 passes the expanded nested-await regression and real Servo workflow.
CLJ-ASYNC-001 and CLJ-ASYNC-004 now pass notification-wait and idle-suspension assertions.
E7 uses `spawn_pending_budget` with explicit limits on accepted work.
CLJ-ASYNC-003 now passes session and global overload refusal, interruption, and capacity recovery.
The focused upstream GC and nREPL limit suites pass all twelve tests.
D1 records the product queue contract for Phase 1 implementation.

The table below preserves evidence from the earlier pins, including release 0.2.34.
The [gap register](clojurust-async-gaps.md) supplies the current status and new measurements.
In particular, the current E7 probes demonstrate parked interruption and non-blocking nREPL evaluation admission.

| Area | Evidence | Next action |
|---|---|---|
| Host callbacks | E2 passed with stable live-object counts in two execution modes, at both pins | Use the existing API. No upstream patch is necessary for this probe. |
| Host async loop | E1 on fibers: 60 s headless, 497 nREPL evaluations, 2 gaps over 25 ms. Native runs report runtime ticks, redraw callbacks, and skipped deadlines; deadlines can expire | Compare the dedicated-thread fallback before settling D1. |
| nREPL budget | E6 queues twenty 10 ms calls. One zero-timeout poll spends more than 200 ms on them. | Prepare bounded polling with a limit between jobs and regression coverage under Q4. |
| nREPL parked eval | E6: a parked nREPL evaluation blocks the frame loop until it completes | [csm/clojurust#447](https://github.com/csm/clojurust/issues/447) requests a non-blocking poller. |
| nREPL interruption | E6 interrupts an active infinite loop from a second connection. The session and async loop recover. | Use the existing interrupt support. No upstream patch is necessary for this path. |
| nREPL interrupt | E6: interrupts stop running evaluations, not parked ones | [csm/clojurust#446](https://github.com/csm/clojurust/issues/446). Bug. |
| Cooperative tasks, pre-fiber | E6's blocking `go` task spends about 200 ms in a nominal one-millisecond LocalSet slice | Keep blocking work off the shell thread. See [scheduling policy](scheduling.md). |
| Cooperative tasks, fibers | E6: slice yields bound CPU-bound fibers to about one slice per drive | Run Bulwark with a 1–2 ms slice. Keep long work off the carrier and out of root fibers. |
| Stall diagnostics | E6: the watchdog missed a 300 ms block beside a yielded fiber | [csm/clojurust#445](https://github.com/csm/clojurust/issues/445). Bug. |
| GC pauses | E6: automatic collection inside fibers is correct but stops the shell thread for 200 ms or more on large heaps | Measure with Bulwark's real heap. Consider collection at frame boundaries. |
| Async thread bridge | E7 passes pure async user code against real Servo on the main thread, with owned messages and continuous runtime execution. | Continue the separate-thread investigation. See [E7 report](async-bridge.md). |
| Idle async waits | `await_value` repeatedly checks state and yields. E7's one-second idle timer used about one CPU-second, before and after the 0.2.34 pin. | Prepare notification-based future and promise waits, with completion, cancellation, lost-wakeup, and GC coverage. |
| Async call evaluation | Before 0.2.34, E7 reproduced a blocked `(:value (await pending))` inside `^:async`. On 0.2.34 the probe completes. | Confirm the fix upstream and add regression coverage. |
| Servo build | The clean debug target built in 2m 30s. The release build took 194.83 s and produced a 126 MiB executable. | Complete E3 input observation. Build timings used cached sources and include concurrent runtime checks. |
| Servo process model | Q5 selects a single-process baseline | Compare multiprocess after E3 works, including E4's JavaScript and interception paths. |

The bootstrap opened no upstream issue. The fiber experiments opened #445, #446, and #447 on 2026-11-02.
That historical experiment did not prepare an upstream pull request.
