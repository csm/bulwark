# Phase 2 default browsing and internal REPL checkpoint

Status: implemented with passing main CI, 2026-11-14. Phase 2 acceptance remains open.
Base: `main` at `34b1f1a`, after P2.6.
This checkpoint implements P2.7 of the [implementation plan](phase-2-plan.md).
The Servo and Clojurust pins remain unchanged.

## Default configuration

`bulwark.default.browsing` installs browser keys, title history, pointer selection, and the last-web-buffer policy.
`bulwark.default.shell` installs echo hooks, recoverable dispatch errors, and Messages.
`bulwark.default.pages` supplies find, zoom, link selection, and popup policy.
These namespaces use the same public APIs as init files.
Existing browser commands and `browser/install!` remain available for earlier scripts.
The shell installs defaults before init and opens the initial page afterward.

| Action | Default key |
|---|---|
| New web buffer | `C-c n` |
| Close buffer | `C-x k` |
| Switch buffer | `C-x b` |
| Next or previous buffer | `C-x ArrowRight`, `C-x ArrowLeft` |
| Address entry | `C-l` |
| Back, forward, reload | `C-c b`, `C-c f`, `C-c r` |
| Command palette | `M-x`, or `C-c x` |
| Find text, next match | `C-s`, `C-c s` |
| Increase, decrease, reset zoom | `C-c +`, `C-c -`, `C-c 0` |
| Open a link in another buffer | `C-c l` |
| Internal REPL | `C-c e` |

Find selects and scrolls to a match through bounded JavaScript.
The search is case-sensitive and wraps after the final match.
Each match stays within one text node in the main document.
Scripts, styles, form inputs, and nodes without visible rectangles stay outside the search.
The scan stops at 10,000 text nodes, 1,048,576 characters, or 1,000 matches.
The echo reports a search limit when the scan stops early.
The query contains at most 256 characters and enters JavaScript as a quoted string.
This mechanism does not search across nodes, inside frames, or inside shadow roots.

The link command reads the first 100 anchors and offers their resolved URLs through completion.
Each URL contains at most 2,048 characters.
The accepted schemes are HTTP, HTTPS, file, and exactly `about:blank`.
Executable schemes and URLs with whitespace are rejected.
The selected link opens in a retained background buffer.
Link data never supplies a command name or Clojure source.

`webview-zoom` accepts an explicit view and a finite factor from 0.25 to 5.
Servo applies page zoom through `set_page_zoom`.
Clojure retains the last accepted factor in the buffer-local `:page/zoom` variable.
A rejected factor or stale handle leaves other views unchanged.

## Auxiliary views and opener ownership

Servo reports a popup as `:page/popup-requested`, with the parent `:webview` and a fresh `:popup/request` token.
The engine retains the original `CreateNewWebViewRequest` on its own thread.
The callback returns immediately. Clojure decides the request later through the ordered event stream.

`webview-accept-popup` consumes the token and returns a retained webview handle.
The original request builder preserves Servo's opener relationship and destination navigation.
`webview-deny-popup` consumes the token without a new view.
Acceptance leaves selection unchanged until Clojure publishes a layout.
Buffer publication failures close a new view that no buffer owns.

The engine retains at most 16 requests globally and four per parent.
Each request expires after ten seconds.
Parent close, parent crash, and engine shutdown release outstanding responders.
Consumed, expired, and orphaned tokens return `:stale-handle`.
The existing request, event, payload, and shutdown limits remain in effect.

The trusted `bulwark.default.pages/popup-policy` function returns `:background`, `:foreground`, or another value to deny.
The default accepts auxiliary views in background buffers.
The callback API supplies no destination URL or user-gesture field for this decision.
This default is basic popup support, without a gesture-based popup blocker.
Init files can replace the policy without a native change.

## Lifecycle hooks

Existing `:page/url-changed` and `:page/load-status-changed` hooks retain navigation and load events.
Page hooks see the owning buffer id under `:buffer`.
Existing `:buffer/created`, `:buffer/selected`, and `:buffer/killed` hooks remain replaceable by registration id.
Creation runs the created hook before the selected hook.

`:window/changed` runs after a committed layout change and after its buffer lifecycle hooks.
Its event contains the current `:layout` and the earlier layout under `:previous`.
This hook covers buffer selection, splits, window selection, and removal.
`:frame/focus-changed` reports the native frame's focus as a boolean under `:focused`.
Native input reset still cancels prefixes and releases input ownership.

## Internal REPL

`bulwark.repl/open-repl` selects an existing internal REPL or creates one.
`bulwark.repl/create!` creates another independent REPL buffer.
Each buffer shows a scrollable transcript, a namespace label, and one editable source line.
Enter submits the line. `C-p` and `C-n` recall source history and restore an unfinished draft.
`C-c C-c` interrupts the oldest unfinished evaluation in the selected REPL.
The one-line input accepts compound forms and explicit async waits.
It does not provide multiline editing or a separate runtime.

The REPL uses the browser's globals and preserves namespace changes across queued forms.
Command definitions and later Var redefinitions take effect immediately.
The pinned evaluator writes the shared `*ns*` root during namespace changes.
Each evaluation fiber poll installs its namespace and restores the caller namespace before another task runs.
This boundary preserves macro registration across parked evaluations without a dependency change.
Each buffer retains 64 source entries and the newest 16,000 transcript characters.
History, transcript, errors, and results retain strings rather than evaluation values.
The nREPL listener remains opt-in.

Rust supplies four evaluation mechanisms with documentation, signatures, and resource tags:

| Native function | Contract |
|---|---|
| `evaluation-start` | Queues trusted source for an owner and returns a fresh job id. |
| `evaluation-poll` | Drains output and reads status and namespace. A terminal poll consumes the job id. |
| `evaluation-interrupt` | Interrupts one job and wakes its parked waits. Queued cancellation runs no forms. |
| `evaluation-close` | Cancels one owner's jobs and discards their output. Other owners and nREPL remain active. |

There are at most 32 retained jobs globally and eight per owner.
The scheduler admits at most four jobs per one-millisecond turn.
Jobs from one owner run in order. Other owners can progress during a parked evaluation.
The scheduler waits without timer polling when no job can start.
Each source contains at most 4,096 UTF-8 bytes. The UI also limits input to 1,024 characters.
Each job retains at most 16,384 output bytes and reports truncation.
Terminal jobs retain capacity until their final poll or owner close.

The ordered dispatcher queues evaluations and returns without waiting for them.
A separate Clojure pump reads output and updates the UI every 50 milliseconds.
Programmatic input changes create a fresh control identity so submission clears native edits.
Ordinary text edits preserve the control and cursor.
Returning to a hidden REPL buffer restores its unfinished draft.
A REPL buffer shown in several panes has one editable control in the selected pane.
Other panes show its transcript without duplicate control identifiers.

Closing a REPL buffer cancels its queued and running jobs.
Running jobs retain admission capacity until cancellation and fiber cleanup finish.
Shutdown cancels all remaining fibers through the existing worker contract.
These limits do not bound the language heap, retained globals, or arbitrary child tasks from trusted code.
Interrupts act at evaluator safepoints and parked waits. They cannot preempt arbitrary blocking native code.

## Checks and remaining evidence

The portable REPL suite runs in TreeWalk and Tiered modes.
It covers globals, live commands, namespace changes, output, errors, history, native edit retention, interrupts, limits, and owner cleanup.
The product nREPL suites also check cancellation isolation with an internal REPL buffer.
The shared shell model adds a split-REPL assertion for the interpreter and standalone AOT binary.

The native browsing fixture uses the local Servo page.
It checks find and wrapping, literal query isolation, zoom geometry, link completion, and background selection.
Its popup checks require actual opener access and communication in both directions.
It also checks denial, parent-close invalidation, REPL output, and shutdown with an owned evaluation parked.
The final composed REPL frames are under `target/phase-2-browsing-evidence/`.

| Local check | Result |
|---|---|
| Portable workspace | 58 tests passed. |
| Standalone AOT | Shared shell assertions passed. |
| Formatting and Clippy | Portable and native checks passed. |
| Native unit tests and Servo contract | 17 unit tests and the contract passed. |
| Native process suite | 16 cases passed in each evaluator mode. |
| Mock nREPL suite | Three cases passed in each evaluator mode. |
| Native nREPL suite | Four cases passed in each evaluator mode. |
| Foundation and software Servo | Passed in each evaluator mode. |

## Main CI close-out

The [main CI run](https://github.com/csm/bulwark/actions/runs/43629036365) passed all four jobs at `9835f77`.
Every job step completed successfully.

| Job | Evidence |
|---|---|
| [Linux runtime](https://github.com/csm/bulwark/actions/runs/43629036365/job/132551294202) | 58 portable tests, both foundation and mock nREPL modes, and the existing experiment regressions passed. |
| [macOS runtime](https://github.com/csm/bulwark/actions/runs/43629036365/job/132551294209) | The same portable tests and runtime checks passed. |
| [Linux Servo](https://github.com/csm/bulwark/actions/runs/43629036365/job/132551294217) | 17 native unit tests, the contract, both software modes, 32 native process cases, and eight native nREPL cases passed. |
| [Standalone AOT](https://github.com/csm/bulwark/actions/runs/43629036365/job/132551293969) | The pinned compiler built the binary, which completed the shared shell assertions. |

The native process totals cover 16 cases in each evaluator mode.
Both runtime hosts and Linux Servo reported internal REPL cancellation isolation in both modes.
The local CI log is `target/phase-2-browsing-evidence/main-ci-43629036365.log`.
This result closes the P2.7 automated checkpoint.
The [handoff](handoff.md) now starts with P2.8 acceptance.
The earlier P2.5/P2.6 CI and physical results remain in the [validation report](phase-2-shell-acceptance.md).
P2.8 still requires clean-profile physical workflows, the five site tasks, release startup measurements, and the native policy review.
IME, clipboard, display-scale transitions, interactive Linux behavior, and Option-as-Alt policy retain their earlier limits.
