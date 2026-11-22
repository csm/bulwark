# Phase 2 compatibility follow-up

Date: 2026-11-21. Source baseline: `1ef0723`, equal to fetched `origin/main`.
The working branch is `codex/phase-2-compatibility`.
Phase 2 remains open. This report supplies no owner acceptance.
The subsequent [physical follow-up](phase-2-physical-follow-up.md) records the checks after the owner unlocked the Mac.
Its controlled GitHub comparison clears the loading placeholders with adopted stylesheets and IntersectionObserver enabled together.
The subsequent [Wikipedia investigation](phase-2-wikipedia-investigation.md) isolates the long pause in radio-input validation and provides a local reduction.

## REPL completion failure

The unchanged source reproduced the Linux `Timed out: REPL completion` failure on the existing two-core test host.
Stage labels located the failure in the output-truncation fixture.
The fixture submitted `(apply str (repeat 20000 "x"))` through the internal REPL.
Its job remained pending at the five-second deadline.
The earlier interruption, ownership, and namespace assertions completed.

A temporary timing run increased the inner diagnostic deadline to twenty seconds.
That run exceeded the outer thirty-second program deadline before the output stage completed.
This result does not establish the exact cost inside Clojurust.
The lazy sequence realization and large argument list remain possible sources of that cost.

The corrected fixture submits `(apply str (repeat 100 (apply str (repeat 200 "x"))))`.
It produces the same 20,000-character string with a smaller lazy argument list.
The measured output stage completed in 207 ms in TreeWalk and 241 ms in Tiered mode.
The final fixture retains the original five-second completion deadline.
The assertions still require bounded transcript retention and the explicit truncation marker.
Production scheduling, output bounds, interruption, and shutdown deadlines did not change.

Completion failures now identify the stage and include bounded transcript and Messages excerpts.
The corrected REPL assertions passed both isolated and full-suite Linux runs in both modes.
The full Linux suite still failed three other tests on this host, including with serial execution.
Browser-history and introspection programs exceeded their bulk deadlines. The embedded-shell check exceeded its two-second deadline.
Those failures remain recorded. The REPL correction does not establish a passing Linux CI run.

## IntersectionObserver evaluation

The [comparison runner](../experiments/compare_intersection_observer.py) starts a separate Servo process for each configuration and URL.
The configurations differ only in `dom_intersection_observer_enabled`.
Each process runs for thirty seconds, with a parent deadline fifteen seconds longer.
The probe requests a JavaScript heartbeat about once per second and inspects the document after twenty seconds.
The runner records process CPU time, completed heartbeats, document observations, and raw logs.

The probe binary SHA-256 was `07533db92240c7dc28f08cce7c097c6094fe845cd51f525e875d2069920b2ebd`.
Both configurations use WebCrypto, the default user agent, and no saved browser state or page scripts.
The software surface is 1,000 by 800 pixels.
The preference does not enable every experimental Servo feature.

| URL | Default heartbeats | Enabled heartbeats | Observation |
|---|---:|---:|---|
| Wikipedia Rust article | 29 | 7 | The enabled run stopped completing JavaScript requests and returned no document inspection. |
| Hacker News | 29 | 29 | Both document inspections returned page content. |
| GitHub Servo repository | 29 | 29 | Both document inspections returned page content. |
| MDN Array.map | 29 | 29 | Both document inspections returned page content. |
| BBC News | 29 | 29 | Both document inspections returned page content before consent. |
| MDN `/en-US/docs/Web` | 29 | 29 | The reported freeze did not reproduce in this sample. |
| Amazon homepage | 29 | 29 | Both runs completed. CPU time was 32.31 and 32.61 seconds respectively. |

All fourteen processes returned zero. That status only establishes that each diagnostic process completed.
It does not establish successful browsing or completion of outstanding JavaScript requests.
The first enabled Wikipedia run completed only seven heartbeats, with a maximum delay of 2,068 ms.
Its main thread continued to spin and paint, but the twenty-second inspection never returned.

A second isolated Wikipedia comparison completed 29 heartbeats and a document inspection in each configuration.
The enabled failure is therefore intermittent in this evidence. Its cause remains unresolved.
These short samples do not establish stable CPU or memory behavior, cold-cache performance, or physical site acceptance.
Other development checks overlapped parts of the first comparison.

The [Amazon CPU issue #44907](https://github.com/servo/servo/issues/44907) remains open.
The [MDN freeze issue #45915](https://github.com/servo/servo/issues/45915) is closed.
The published Servo 0.7.0 source still defaults this preference to false.
The earlier [BBC investigation](phase-2-bbc-investigation.md#controlled-diagnosis-of-the-consented-failure) establishes its benefit for the consented article failure.
This comparison does not resolve the intermittent Wikipedia result or establish safe global enablement.
The product preference remains disabled. BBC physical acceptance still needs a product correction and another physical run.

Reproduction:

```sh
cargo build --locked -p bulwark-experiment-e3 --features servo --example bbc_probe
python3 experiments/compare_intersection_observer.py
python3 experiments/compare_intersection_observer.py --site wikipedia
```

The probe's `--measure` flag supplies the heartbeat and longest main-thread turn measurements.
The runner's `--binary`, `--seconds`, and `--out` flags select the executable, duration, and evidence directory.
An existing output directory is rejected to preserve earlier evidence.

## GitHub navigation and link inventory

A diagnostic DOM click loaded `https://github.com/servo/servo/blob/main/Cargo.toml` in standalone Servo with default preferences.
The click event did not report `defaultPrevented`.
A separate DOM click on the visible anchor also loaded the file in the native product.
The resulting document contained `[workspace]`.
These programmatic actions do not replace the earlier failed physical click.

The native document retained two Cargo.toml anchors at indexes 140 and 141.
The first anchor had an empty rectangle. The second had a nonempty rectangle below the initial viewport.
GitHub also logged an `adoptedStyleSheets` exception with the default preferences.
Successful DOM navigation means that this exception alone does not establish the physical failure's cause.
Native hit testing, viewport geometry, and physical input remain unverified for that failure.

The old link selector inspected only the first 100 anchors.
The correction inspects up to 4,096 anchors and removes duplicate and unsupported URLs before publication.
The result contains at most 1,000 links and 16,384 UTF-16 code units, including newline separators.
That text limit also bounds UTF-8 output to less than 49,152 bytes.
Each URL retains its 2,048-character limit. Clojure retains the final scheme check and background-buffer policy.

The native regression includes duplicate and executable links before a valid URL after anchor 100.
It also supplies 1,200 long URLs and requires the entry and text bounds.
A fresh native product run returned 147 unique entries, including Cargo.toml, through the corrected selector.
The process exited zero, joined its worker, and removed its port file.
Two early polls occurred before the page was ready and returned handle or engine errors.
The retained replies include those errors and the successful inventory.
This result removes the known inventory cutoff for the Cargo.toml anchors.
It does not establish a corrected physical click path.

Computer Use could not attach because the Mac was locked and automatic unlock failed.
The owner then requested automated work only. No physical result occurred in this follow-up.
The isolated native diagnostic exited zero, joined its worker, and removed its nREPL port file.

## Dependency status and remaining gates

Clojurust issues [#500](https://github.com/csm/clojurust/issues/500) and [#501](https://github.com/csm/clojurust/issues/501) are closed upstream.
The product remains pinned to `540e4218`.
Both AOT workarounds remain until a deliberate dependency update passes their regressions.
The issue states alone do not establish that a new pin is suitable.

The Option-as-Alt policy remains an owner decision. `C-c x` remains the palette fallback.
Phase 3 has not started. The required five-site scope remains unchanged.

Local evidence resides in `target/compatibility-evidence/`.
It includes the original Linux failures, stage timing comparisons, full-suite logs, observer comparisons, and GitHub observations.
| Current check | Result |
|---|---|
| Portable macOS workspace | All 58 tests passed after the corrections. |
| Native unit tests and Servo contract | All 17 unit tests and the contract passed. |
| Final browsing fixture | Passed in both evaluator modes, including the later-link and inventory-bound regressions. |
| Live GitHub inventory | Returned Cargo.toml among 147 unique entries. |
| Formatting, whitespace, and Clippy | Portable and native checks passed. The comparison runner passed Python compilation. |
| Full native process suite | The locked run stopped at page focus. Both later unlocked runs passed all 16 cases, as recorded in the physical follow-up. |
| Linux REPL regression | Passed in both evaluator modes after the fixture correction. |
| Full Linux portable workspace | Failed browser-history, introspection, and embedded-shell deadlines in parallel and serial runs. |
| Current GitHub Actions | Not run for this working branch. B1 remains open. |
