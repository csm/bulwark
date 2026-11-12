# P2.3 window validation

Date: 2026-11-12. Physical macOS checks passed. Final NUC validation remains in progress.

## Source and artifacts

Physical input used `e01cd60`, after the publication gate fix.
The temporary app bundle contained a byte-identical copy of the product executable.
Its SHA-256 was `2e69f0cefe4cd5c7d03686f7c321871f7e6768a80e2dde8768ef3678a5fe740e`.
The session used Tiered execution, an isolated profile, and the opt-in loopback nREPL.

The later `fb30e86` commit changes the Clojurust pin, its regression, and dependency documentation.
The Rust input and renderer sources remain unchanged.
Portable tests, AOT, native tests, both native process suites, and both native nREPL suites cover that pin.

The fixtures are `experiments/fixtures/phase-2-windows.html` and `clj/tests/windows-physical.cljrs`.
The local evidence directory is `target/physical-window-evidence/`.
It contains observation files, the parked-client result, and `e01cd60-final.png`.
The native host saved the final composed frame during physical close.

## Physical macOS results

Computer Use supplied native mouse, wheel, and keyboard input.
The product nREPL supplied observations of page state, layout state, and interaction payloads.
No synthetic page event supplied a physical result.

| Check | Observed result |
|---|---|
| Mixed frame | A scrollable text pane appeared on the left, a web pane on the right, with modelines and an echo area. |
| Press ownership | A 1200 ms held button click selected the page from the text pane. Both mouse events and one click reached the page. |
| Text focus isolation | A key sent while text was selected changed neither page. The visible page reported no document focus. |
| Window command | Logical `C-x o` selected the page. Subsequent text reached its retained input field. Command strokes stayed outside its key log. |
| Text scroll | The left pane scrolled from its heading to later numbered lines. Both pages retained scroll offset zero. |
| Page and echo scroll | The page scrolled to 592 pixels in the resized frame. A wheel action over the echo area left that offset unchanged. |
| Physical split | Logical `C-x 3` added a third pane while one page remained visible. |
| Small-frame fallback | At 300×400 logical pixels, only the selected page appeared. The registry retained all three windows. |
| Layout restoration | A larger frame restored the split tree. Logical `C-x o`, then `C-x 0`, deleted the added text window. |
| Resize and input | At 800×560, the offset web pane measured 400×510. A held click and typing still reached its new rectangle. |
| Retained state | The first page retained `ace`, two clicks, four mouse downs, and four mouse ups. |
| Hidden isolation | The second page received `b` and one click. The hidden first page kept its text, counters, and key log. |
| Editable chrome | The control returned text changes `a`, `am`, `a`, `ab`, followed by submission of `ab`. All five events carried revision 20 and element id 10. |
| Page isolation during editing | Neither page received the editable control's keys. Both reported no document focus. |
| Physical close | The window closed with an nREPL evaluation parked. The worker joined, the client reached EOF, and the profile port file disappeared. |

The page key log contained only `a`, `c`, and `e` after the command checks.
The active keyboard layout was Dvorak.
Computer Use labels identify physical key positions, so the observations established the logical strokes before the command checks.
For example, tool input `ctrl+b`, then `s`, produced logical `C-x o`.

The final session painted 350504 frames and made 350507 presentations.
Computer Use attachment took an unusually long time. These totals cover that wait and do not measure input latency or startup performance.
The parked-client timer also includes the interval before the physical close action.
The host retained its existing two-second worker shutdown deadline.

## Linux runs and corrections

The configured NUC used a separate checkout and its existing Cargo build cache.
The earlier checkout and its uncommitted work remain intact.
Every complete-suite report records the source revision, worktree status, limits, and command logs.
The runner uses one build job, one Rust test thread, and a 60-second Clojure assertion budget.

| Revision | Result and correction |
|---|---|
| `c4bd370` | The window suite exposed overlapping UI publications. A lifecycle assertion also read state before replacement hooks completed. |
| `e01cd60` | The publication and lifecycle regressions passed. Startup then exposed a lost atom update. |
| `fb30e86` | The final run uses the atom retry fix. Its result remains in progress. |

A publication gate now covers revision assignment, native submission, and settlement.
A contender parks on the current publisher's promise, without blocking the engine thread.
The window suite holds one publisher before native submission and requires the second publisher to wait.
The lifecycle suite waits for the replacement selection hook before it reads the complete hook sequence.

Clojurust's bootstrap `swap!` used an unconditional `reset!` after the update function returned.
A sibling fiber could change the registry during that function and lose its update.
[Draft PR #496](https://github.com/csm/clojurust/pull/496) restores native compare-and-set retries.
The current product pin is `540e4218`.
Its tests cover direct and applied calls, watches, parked fibers, AOT, and no-GC execution.
The product model suite also requires the conflicting update to survive.

The NUC reports remain under the dedicated checkout's `target/linux-runs/` directory.
The final report name is `p23-fb30e86`.
Local failure reports are `target/linux-runs/nuc-p23-c4bd370/` and `target/linux-runs/nuc-p23-e01cd60/`.
The final report will use `target/linux-runs/nuc-p23-fb30e86/`.

## GitHub Actions

[Run 43277594137](https://github.com/csm/bulwark/actions/runs/43277594137) passed all four jobs at `fb30e86`.
Both runtime jobs, Linux Servo, and the standalone AOT job passed.
This CI result is separate from the NUC run and the physical macOS observations.

## Repeat the physical checks

The fixture's `bulwark.tests.windows-physical/state` atom contains the buffer and window identifiers.
Browser and buffer helpers can switch the retained pages through nREPL.

```sh
fixture_url=$(python3 -c 'from pathlib import Path; print(Path("experiments/fixtures/phase-2-windows.html").resolve().as_uri())')
cargo run --locked -- --nrepl --profile /tmp/bulwark-p23-physical \
  --script clj/tests/windows-physical.cljrs "$fixture_url"
```

1. Inspect `(await (bulwark.tests.windows-physical/observe!))` through the product nREPL.
2. With the text pane selected, hold a page-button click.
3. Type into the page.
4. Select the text pane.
5. Type another key.
6. Run the window commands.
7. Compare the page key logs.
8. Scroll each pane and the echo area.
9. Split the page window.
10. Shrink the frame.
11. Restore the frame.
12. Switch retained pages.
13. Compare their input counters.
14. Call `(await (bulwark.tests.windows-physical/input!))`.
15. Edit the native control.
16. Park an nREPL evaluation.
17. Close the native window.
18. Require a joined worker, client EOF, and removal of the profile port file.

Physical coverage uses horizontal splits at scale 1.0 on macOS.
IME, clipboard, interactive display-scale transitions, and physical Linux input remain outside this result.
Headless Linux graphics checks and GitHub Actions results remain distinct evidence.
