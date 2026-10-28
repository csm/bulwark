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

| Area | Evidence | Next action |
|---|---|---|
| Host callbacks | E2 passed with stable live-object counts in two execution modes | Use the existing API. No upstream patch is necessary for this probe. |
| Host async loop | E1 headless async and nREPL both advance | Measure native redraw timing without concurrent compilation. |
| nREPL budget | `Poller::poll_timeout(0)` drains every queued job | Evaluate bounded polling during E6. Prepare an upstream fix if the probe establishes a gap. |
| Cooperative tasks | A LocalSet slice cannot preempt synchronous work | Complete the stall and interrupt probes during E6. |
| Servo build | Debug link emits an oversized `__eh_frame` warning on macOS | Record release build size and clean build time during E3. |
| Servo process model | Q5 selects a single-process baseline | Compare multiprocess after E3 works, including E4's JavaScript and interception paths. |

No upstream issue or pull request was opened during this bootstrap.
