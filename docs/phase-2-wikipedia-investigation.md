# Wikipedia pause investigation

Date: 2026-11-21. Source baseline: `1ef0723` with the current compatibility changes.
Servo remains pinned to `0.7.0`. Product preferences and dependency pins did not change.
This report continues the [compatibility follow-up](phase-2-compatibility-follow-up.md).

## Finding

The apparent Wikipedia freeze reproduced as a long, recoverable script-thread pause.
The sampled thread was inside a timer callback that appended radio inputs and validated their groups.
The recovered document displayed a fundraising banner. The other seven documents did not display that banner.
This association identifies variable page content as a likely trigger. It does not identify the exact banner-selection condition.

A local reduction reproduces the same expensive native path without creating an IntersectionObserver.
It pauses with the observer preference disabled and enabled.
The evidence therefore identifies a radio-validation bottleneck separate from observer intersection computation.
It does not establish that every observer performance limitation is resolved.

## Live reproduction

Eight separate processes loaded the original Wikipedia Rust article URL.
Four retained default preferences. Four enabled only `dom_intersection_observer_enabled`, in alternating order.
Each process ran for 45 seconds with a sixty-second external deadline.
The runs used the same debug binary, software surface, default user agent, and WebCrypto feature.
No init file, saved browser state, user script, or JavaScript API shim supplied the result.

The binary SHA-256 was `efba357a6679c8dc1723b1cbd8526db74a3e0f8577ecc7e1633382cd9eaebced`.
The machine was the M3 Max MacBook Pro used for the earlier acceptance work.
No other browser test ran concurrently with these comparisons.
The ordinary desktop environment remained active. These are debug diagnostics, not release benchmarks.

| Runs | Result |
|---|---|
| Four default-preference runs | Each completed 44 heartbeats and three document inspections. Maximum heartbeat delays ranged from 191 to 369 ms. |
| Three enabled-preference runs | Each completed 44 heartbeats and three inspections. Maximum delays ranged from 495 to 808 ms. |
| Enabled run 04 | Completed only seven early heartbeats, then paused. The longest delayed request completed after 33,988 ms. |

Run 04 recovered within its 45-second window and eventually returned three document inspections and 43 heartbeats.
The inspections queued at twenty, thirty, and forty seconds returned after the pause.
Their body text included the fundraising message and donation amount choices.
The other runs contained the ordinary article without that message.

All eight processes returned zero.
A zero diagnostic exit status did not establish continuous JavaScript responsiveness.
The earlier thirty-second run ended before this longer sample's recovery point.
That observation explains why its missing replies alone did not establish a permanent freeze.

## Captured stack

After a three-second reply gap, the runner sampled only the affected probe process.
The two-second sample captured `Script#1` executing a JavaScript timer callback.
Its native call path included:

```text
JsTimerTask::invoke
Node_Binding::appendChild
Node::pre_insert / Node::insert / Node::add_child
HTMLInputElement::bind_to_tree
HTMLInputElement::value_changed
HTMLInputElement::update_related_validity_states
perform_radio_group_validation
RadioInputType::suffers_from_being_missing
radio_group_iter / TreeIterator
```

The main thread continued to sleep and process Servo events.
The sampled script path did not enter IntersectionObserver's intersection or containing-block computation.
This evidence differs from the containing-block stacks in the older [MDN observer report](https://github.com/servo/servo/issues/45915).
It does not establish whether that older failure remains reproducible under other conditions.

The pinned radio validation code validates the members of a radio group.
Each member's missing-value check traverses the document again to find its group.
Adding inputs repeats that work as the group grows.
These nested scans explain the local reduction's sensitivity to document size and radio count.
The [pinned source](https://github.com/servo/servo/blob/v0.7.0/components/script/dom/html/form_controls/input_type/radio_input_type.rs) contains these loops.

## Local reduction and controls

The [retained fixture](../experiments/e3-servo/fixtures/radio-group.html) constructs ordinary DOM nodes and then appends named inputs.
It contains no Wikipedia code, network request, or observer instance.
The default case creates 10,000 filler nodes and 40 same-group radio inputs.
The `type=text` query selects a control with the same document and input count.
The timer records the synchronous insertion duration with `performance.now()`.

An initial comparison without stack sampling recorded:

| Document and inputs | Observer preference | Insertion time |
|---|---|---:|
| 10,000 filler nodes, 40 text inputs | Disabled | 20.54 ms |
| 10,000 filler nodes, 40 radio inputs | Disabled | 5,277.14 ms |
| 10,000 filler nodes, 40 radio inputs | Enabled | 5,270.74 ms |

A smaller case with 5,000 nodes and 20 radio inputs took 734.34 ms disabled and 732.08 ms enabled.
These timings support the input-validation diagnosis. They do not measure every input type or define an asymptotic bound.

The final retained fixture also reproduced through the updated comparison runner.
Automatic samples captured the same radio-validation path in both preference configurations.
With sampling enabled, insertion took 6,431.27 ms disabled and 5,519.00 ms enabled.
The corresponding longest heartbeat delays were 5,773 and 4,883 ms.
The text controls completed every heartbeat, with maximum delays of 15 ms in each configuration.
Sampling adds overhead, so those insertion times remain separate from the initial comparison.

The final probe binary SHA-256 was `5798eec0b12eadfc56113779a8355cd3457548f1ab48bd2c07c4914e54c0fe07`.
Its inspections include radio counts and the fixture's timing result.
The fixture accepts bounded `nodes` and `radios` query values for later reductions and upstream comparisons.

## Reproduction tools

The comparison runner now supports repeated pairs, explicit URLs, and optional macOS thread sampling after a JavaScript reply gap.
It retains live output before a process ends and preserves a separate log for each case.
`wait4` records probe CPU time separately from the sampling process.
The existing external deadline still terminates and reaps an overdue probe.
The original seven live URLs remain the default selection. The radio fixtures require explicit selection.

Run the live comparison:

```sh
cargo build --locked -p bulwark-experiment-e3 --features servo --example bbc_probe
python3 experiments/compare_intersection_observer.py --site wikipedia --repeat 4 --seconds 45 --sample-on-stall
```

Run the deterministic radio case and text control:

```sh
python3 experiments/compare_intersection_observer.py --site radio-group --seconds 21 --sample-on-stall
python3 experiments/compare_intersection_observer.py --site radio-text --seconds 21
```

The `--sample-on-stall` flag requires macOS and uses `/usr/bin/sample` only on the child probe.
Its default gap threshold is three seconds. `--stall-seconds` changes that threshold.
The `--url` flag accepts a specific file or live URL instead of a named site.
Process completion remains diagnostic evidence rather than browsing acceptance.

## Disposition and next step

This investigation narrows the Wikipedia concern to a demonstrated radio-input validation bottleneck.
The fundraising banner coincides with the live pause. The deterministic reduction establishes that IntersectionObserver is unnecessary for the expensive path.
Further page instrumentation can identify the exact timer and banner assignment if an upstream report needs that detail.

[Servo PR #49855](https://github.com/servo/servo/pull/49855) optimizes the radio iterator and reports a small gain on another Wikipedia page.
That change is a candidate for comparison, not an established correction for this reduction.
The inspected `v0.8.0` [radio source](https://github.com/servo/servo/blob/v0.8.0/components/script/dom/html/form_controls/input_type/radio_input_type.rs) still contains the rooted iterator and nested scans.
The initial investigation used debug builds only.
The subsequent [engine evaluation](phase-2-engine-optimization.md) measures this upstream candidate in debug and release.
It reduces median fixture insertion time by 55.6–56.1% in debug and 32.8–38.8% in release.
Both engines retain the same two validity failures across ten rounds.

The engine evaluation retains an isolated backport and reproduction tools without changing the product pin.
The proposed product preferences still need the required five-site checks, including physical BBC acceptance.
The separate known observer limitations remain relevant to that evaluation.
No site-specific banner removal, JavaScript shim, product preference change, or dependency upgrade supplies this result.

## Checks and retained evidence

The final probe built and passed Clippy with warnings denied. Formatting and Python compilation passed.
The runner passed completion, live-output, timeout termination, child-reaping, repeat, explicit-URL, and inspection-parsing checks.
The retained radio and text cases completed in both configurations.
Production Rust and Clojure behavior did not change during this investigation.

Local evidence resides in `target/wikipedia-observer-investigation/`.
`summary.json` and the numbered logs retain all eight live runs.
`04-enabled.sample.txt` records the affected process stack. `script-stack.txt` extracts its script thread.
`local-control/` and `radio-large/` retain the initial local comparisons.
`retained-reduction/` and `retained-text-control/` contain the final runner results and radio stack samples.
The build and Clippy logs retain the focused checks.
