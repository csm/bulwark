# Upstream dependencies

Last inspection: 2026-10-27.

## Pins

| Dependency | Pin | Source |
|---|---|---|
| Servo | `=0.7.0`, published 2026-10-05 | [Published crate](https://crates.io/crates/servo/0.7.0) |
| Clojurust | `c74e54df68185a3852dcd6d6682af5f0f98c1d61` | [Pinned commit](https://github.com/csm/clojurust/commit/c74e54df68185a3852dcd6d6682af5f0f98c1d61) |

`Cargo.toml` records direct pins. `Cargo.lock` records the complete dependency graph.
Clojurust packages use one git revision. No dependency points at a local checkout.
The bootstrap disables Clojurust's git-backed package loader through `default-features = false`.
It enables the full regex implementation.

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

Source: [apply.rs](https://github.com/csm/clojurust/blob/c74e54df68185a3852dcd6d6682af5f0f98c1d61/crates/cljrs-runtime/src/env/apply.rs).

The pinned async namespace uses `(await (timeout 100))` inside `go`.
The E1 source uses that syntax instead of the plan's assumed `<!` form.

## Gaps and follow-up

The owner approved preparation of required Clojurust fixes under Q4 on 2026-10-27.
Experiments must establish each gap before a change is necessary.
Prepare upstream patches with regression coverage under D10's contribution policy.

The [Clojurust async gap register](clojurust-async-gaps.md) defines the required fixes, reproducers, and acceptance criteria.
CLJ-ASYNC-001 and CLJ-ASYNC-002 must be fixed upstream before product adoption of general async user code.
CLJ-ASYNC-003 covers bounded polling before runtime-thread nREPL integration.
Temporary probe workarounds do not close these items.

| Area | Evidence | Next action |
|---|---|---|
| Host callbacks | E2 passed with stable live-object counts in two execution modes | Use the existing API. No upstream patch is necessary for this probe. |
| Host async loop | E1 native async, nREPL, and redraw callbacks advance together, but deadlines can expire | Compare the dedicated-thread fallback before settling D1. |
| nREPL budget | E6 queues twenty 10 ms calls. One zero-timeout poll spends more than 200 ms on them. | Prepare bounded polling with a limit between jobs and regression coverage under Q4. |
| Cooperative tasks | E6's blocking `go` task spends about 200 ms in a nominal one-millisecond LocalSet slice | Keep blocking work off the shell thread. See [scheduling policy](scheduling.md). |
| nREPL interruption | E6 interrupts an active infinite loop from a second connection. The session and async loop recover. | Use the existing interrupt support. No upstream patch is necessary for this path. |
| Async thread bridge | E7 passes pure async user code against real Servo on the main thread, with owned messages and continuous runtime execution. | Continue the separate-thread investigation. See [E7 report](async-bridge.md). |
| Idle async waits | `await_value` repeatedly checks state and yields. E7's one-second idle timer used about one CPU-second. | Prepare notification-based future and promise waits, with completion, cancellation, lost-wakeup, and GC coverage. |
| Async call evaluation | E7 reproduces a blocked `(:value (await pending))` inside `^:async`. Binding the reply first works. | Prepare async evaluation for non-symbol call heads and arguments, with regression coverage. |
| Servo build | The clean debug target built in 2m 30s. The release build took 194.83 s and produced a 126 MiB executable. | Complete E3 input observation. Build timings used cached sources and include concurrent runtime checks. |
| Servo process model | Q5 selects a single-process baseline | Compare multiprocess after E3 works, including E4's JavaScript and interception paths. |

No upstream issue or pull request was opened during this bootstrap.
