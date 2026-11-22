# Phase 2 physical follow-up

Date: 2026-11-21. Source baseline: `1ef0723` with the compatibility changes in the working tree.
The owner requested physical checks after unlocking the Mac.
This report continues the [compatibility follow-up](phase-2-compatibility-follow-up.md).
Phase 2 remains open. This report supplies no owner acceptance.

## Source and method

The normal Tiered shell ran in `/tmp/BulwarkPhysicalRound.app` with a new, isolated profile and no init file.
The bundle contained a byte-identical copy of the current native debug executable.
Both executable hashes were `e0902131127f2e55ca7792685ea2a0ac6d112b1ae75e140c002eeeae0f2a5a53`.
WebCrypto remained enabled. IntersectionObserver and adopted stylesheets retained their default disabled preferences.

Computer Use supplied pointer input, scrolling, keys, REPL source, and the native close action.
Loopback nREPL supplied observations and a disposable parked client for the shutdown check.
No nREPL navigation, scrolling, dispatcher call, or DOM click supplied a physical result.
The keyboard layout remained Dvorak. Logical command names describe the intended strokes.
Computer Use key positions required translation for that layout.

The first URL-entry attempt reached `ile:///tmp/bulwark-physical.html` and produced an unsupported-scheme error.
Typing started before the native prompt was ready. A second attempt waited for the visible prompt and loaded the complete URL.
An observation also referenced the nonexistent `bulwark.buffer/windows` symbol.
That diagnostic error reached Messages. It was not an application failure.
The retained observations include these unsuccessful attempts.

## Physical results

| Check | Result |
|---|---|
| GitHub pointer navigation | After keyboard activation, a physical Cargo.toml click loaded the source page and displayed `[workspace]`. |
| GitHub Back and Forward | Physical `C-c b` restored the repository and its scroll position. `C-c f` restored the source file. |
| Updated link selector | Physical `C-c l` and `Cargo.toml` input displayed the source URL as the candidate. |
| Background link submission | Enter created another source buffer while the repository buffer remained selected. |
| Buffer completion | Physical `C-x b` and Enter displayed a retained source buffer containing `[workspace]`. |
| REPL truncation | Physical source produced the explicit `[Output truncated]` marker. The retained transcript contained 16,000 characters. |
| REPL recovery and history | The next form returned `42`. Previous-input recalled the large-output form. Next-input restored the unfinished `(+ 1` draft. |
| Parked interruption | Physical `C-c C-c` displayed `[Interrupted]`. The next form returned `42`. |
| URL entry and page input | Physical address entry loaded the local focus fixture. Pointer selection and typing produced `kept`. |
| Prompt focus and cancellation | The URL prompt removed document focus. Escape restored it. Continued input produced `kept!`. |
| Reserved keys | The fixture recorded only `k`, `e`, `p`, `t`, and `!`. Reserved prompt and cancellation keys did not reach it. |
| Mixed panes | The fixture and REPL appeared side by side. A pointer press selected the REPL and exposed its editable control. |
| REPL buffer close | Physical `C-x k` removed buffer/session 17 while job 38 was parked. Polling job 38 reported `Stale evaluation job`. |
| nREPL isolation | After REPL buffer close, nREPL still returned `42`. |
| Native close | The close button stopped an internal evaluation and a separate parked nREPL request. The process exited zero and joined its worker. |

The shutdown client received EOF, and the profile port file disappeared.
The final log recorded 1,220 painted frames and 1,590 presentations.
These totals include observation delays and do not measure performance.
The final native frame shows the parked internal evaluation before teardown.

The shutdown client used nREPL's default session.
A later observation on that same session queued behind its parked evaluation and produced no observation result before close.
The earlier physical screenshot establishes the parked internal source. The client log establishes the parked nREPL request and EOF.
This sequencing does not establish a failure of independent nREPL sessions.

### GitHub qualifications

The first source-link clicks left the repository unchanged, including after an accessibility Raise action.
The read-only observation then reported `document.hasFocus()` as false.
After keyboard input activated the window, another ordinary pointer click loaded Cargo.toml.
Back and Forward subsequently worked through physical command strokes.

This run establishes a successful active-window source workflow.
It does not establish the cause of every earlier failed click or correct the initial activation behavior.
No product input or engine preference change occurred in this physical round.

The owner observed persistent gray loading fields during the run.
The document inspection found actual `div` elements with classes `Skeleton Skeleton--text` and nonbreaking-space content.
The document reported `readyState: complete` while those elements remained.
The renderer therefore displayed existing placeholders rather than losing their expected text during painting.

The page also reported `typeof document.adoptedStyleSheets` as `undefined`.
Its console logged an exception in GitHub's `86048-f53467c0683118f6.js` bundle at line 285, column 215.
The exception reports `e.adoptedStyleSheets is undefined`.
That missing API is a candidate cause of incomplete client initialization.
A completed document load does not establish completed client rendering.
The controlled comparison below isolates the missing interfaces.
The source-file task passed, but the product's remaining loading fields remain an explicit compatibility limit.

### Controlled placeholder comparison

Three standalone Servo runs used the same GitHub URL, default user agent, WebCrypto, and thirty-second duration.
The first run retained default preferences. The second enabled only adopted stylesheets.
The third enabled adopted stylesheets and IntersectionObserver. No page script supplied an API shim.
Each process returned zero and completed its document inspection.

| Configuration | Skeleton elements after twenty seconds | First relevant exception |
|---|---:|---|
| Default preferences | 83 | `e.adoptedStyleSheets is undefined` |
| Adopted stylesheets enabled | 83 | `IntersectionObserver is not defined` |
| Both interfaces enabled | 0 | Neither exception occurred. The probe logged no console errors. |

The final screenshot displays actual commit messages and dates in place of the loading rectangles.
This result supports missing interfaces as the cause of incomplete GitHub client initialization in these runs.
It also explains why document completion alone did not clear the fields.
The physical source link worked before this diagnostic change, so the comparison does not establish the cause of the initial inactive-window clicks.

The probe binary SHA-256 was `efba357a6679c8dc1723b1cbd8526db74a3e0f8577ecc7e1633382cd9eaebced`.
The probe now supports `--adopted-stylesheets` and records the interface type and skeleton count.
The third configuration uses both `--adopted-stylesheets` and `--intersection-observer`.
These flags apply only to the standalone diagnostic process.
Product preferences remain unchanged pending the broader compatibility and performance checks, including the intermittent Wikipedia result.

## Native regression suites

After the physical session closed, the full native process runner repeated on the unlocked desktop.
All 16 cases passed in Tiered mode. All 16 cases passed in interpreter mode.
Both runs passed minibuffer page focus, prompt ownership, and restored focus.
The earlier locked-desktop failure remains historical evidence in the compatibility report.
No test deadline or assertion changed for these repeats.

The Linux runtime deadlines remain open.
The later [Wikipedia investigation](phase-2-wikipedia-investigation.md) locates the observed pause in radio-input validation rather than intersection computation.
A product correction and the broader preference evaluation remain outstanding.
BBC did not receive a product correction in this round, so this follow-up does not replace its failed consented acceptance result.
The required five-site scope, Option-as-Alt decision, and dependency pin remain unchanged.

## Evidence

Local evidence resides in `target/compatibility-physical-20261121/`.
`binary-hashes.json` identifies both executables. `observations.jsonl` records the read-only observations and diagnostic attempts.
`focus-fixture.html` retains the local page source.
`native.log`, `exit-status`, `parked-client.json`, and `final.png` record native teardown.
`native-tiered.log` and `native-interpreter.log` retain the complete passing process suites and their fixture frames.
`github-stylesheets-summary.json` and `github-stylesheets-both.json` record the three diagnostic configurations.
Their corresponding logs and PNGs retain the exceptions, placeholder counts, and visible result.
Computer Use screenshots in this chat show the source page, link candidate, REPL results, retained input, and pane selection.
