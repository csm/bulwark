# Phase 2 Option setting

Date: 2026-11-22. This change follows the owner's keyboard policy decision.
macOS preserves native Option character input by default. Other platforms retain Alt as Meta.
The small Linux investigation is deferred under the owner's desktop focus. Its failures remain unresolved.
Phase 2 remains open. Phase 3 has not started.

## Configuration

Use the public setter in `<profile>/init.cljrs` or the internal REPL:

```clojure
(bulwark.keymap/set-option-mode! :meta)
(bulwark.keymap/set-option-mode! :characters)
(bulwark.keymap/set-option-mode! :platform-default)
```

`:meta` uses Option for `M-` bindings. `:characters` preserves native character input.
`:platform-default` selects character input on macOS and Meta elsewhere.
Changes apply without restarting, including while a prompt is open.
Use the setter instead of changing the `bulwark.keymap/option-mode` atom directly.
`C-c x` remains available for the palette in either mode.
Escape cancels an open prompt immediately, including during a pending command prefix.
Outside prompts, Escape prefixes a Meta stroke in either Option mode. `Escape x` invokes `M-x`.
`Escape Escape` cancels the pending Meta prefix. `C-g` cancels a pending prefix and otherwise cancels an open prompt.

## Implementation

Clojure owns the setting and publishes it with the effective key reservations.
Validation and native publication precede state changes. Failed publication preserves the setting, keymaps, and pending prefix.
A successful setting change cancels the pending prefix.
The route publisher callback now accepts `[sequences cancel-strokes option-mode escape-meta?]`.
The native `set-key-routes!` accepts those optional third and fourth arguments. Omitted settings retain their current values.
The installed inventory remains 25 natives.

Rust applies the published macOS mode through Winit's `set_option_as_alt` and wakes the host after a mode change.
Printable Option characters bypass shell Meta matching in character mode.
The page receives those translated characters without the consumed Alt modifier, which otherwise suppresses Servo text insertion.
Named navigation keys retain Alt. Combined Control/Command shortcuts retain their modifiers.
The adapter forwards native composition events to the focused page and enables composition for editable chrome.
Reserved command prefixes temporarily disable native composition so the shell retains command ownership.
Prompt focus prevents composition events from reaching the underlying page.
No engine preference, deadline, or dependency pin changed.

## Physical macOS checks

The local debug app used a fresh isolated profile and a local HTML input fixture.
Computer Use generated physical key events with the host's Dvorak layout.
The assertions used read-only page queries and Clojure state inspection through nREPL.
Only the explicit mode setters changed test configuration through nREPL.

| Check | Result |
|---|---|
| Default page input | Physical Option symbol and acute-accent composition produced `≈é`. |
| URL prompt | The same keys produced `≈é`; the underlying page retained its prior value. |
| Internal REPL | Physical source entry evaluated `"≈é"` to `"≈é"`. |
| Live Meta mode | Option-x opened `M-x` with an empty prompt and retained the REPL source. |
| Live return to characters | The same open palette accepted `≈é` without a restart. |
| Existing Escape | Cancelled the prompt during these checks. |
| Native close | Returned zero, joined the runtime worker, and removed the port file. |

Initial physical checks exposed two missing mechanisms.
Servo suppressed printable text while Alt remained present. Removing the consumed modifier corrected symbol input.
Dead-key input then produced `ee` because native composition was disabled for the page.
Enabling focused composition and forwarding its events corrected the accent to `é`.
The retained final page log contains composition start, updates, and one committed end event.
A unit regression covers that protocol and prevents chrome composition from reaching the page.

The successful physical bundle SHA-256 was:

```text
02ec0abe0c568a7fe02be17af8a180d985a9e404d984fe9df9740798d8527044
```

The final source subsequently extracted the same composition handling into a tested helper and replaced a new `def` docstring with a comment.
The pinned AOT compiler had treated that docstring as the variable value. The corrected declaration passes the shared AOT assertions.
An initial Computer Use attachment expired before testing; a fresh attachment completed the checks. Attachment delay is outside product timing.
Local evidence resides in `target/option-mode/physical/`, including observations, fixture, screenshot, native logs, exit status, and bundle hashes.

## Automated checks and limits

| Check | Local result |
|---|---|
| Portable workspace | 60 tests passed, including synchronous mode publication in both evaluator modes. |
| Native unit tests and Servo contract | 19 unit tests and the contract passed. |
| Portable and native Clippy | Passed with warnings denied. |
| Native process suites | All 16 cases passed in each evaluator mode. |
| Software Servo fixtures | Passed both evaluator modes. |
| Native nREPL suites | Passed both evaluator modes. |
| Standalone pinned AOT | Shared shell assertions passed after correcting the declaration. |
| Formatting and whitespace | Passed. |

Logs and the command summary reside in `target/option-mode/final-checks/`.
The initial summary retains the failed AOT attempt; `aot-retry.log` records the corrected passing run.
The runtime tests cover invalid settings, refused publication, prefix cancellation, and retention across keymap changes and legacy native calls.
The input tests cover character ownership, named navigation, combined modifiers, and composition forwarding.

These checks establish basic Option symbols and acute-accent composition on this Mac.
They do not establish complete IME language coverage, candidate placement, or every composition and focus transition.
Interactive Linux keyboard behavior, clipboard, and display-scale transitions remain unverified.
The [Option-setting CI](https://github.com/csm/bulwark/actions/runs/45087538289) passed all four jobs at `88037e1`, before the Escape follow-up.
The owner subsequently selected immediate Escape cancellation inside prompts. The next section records that implementation.

## Agreed Escape behavior

Clojure enables Escape as a Meta prefix outside input contexts and disables it in prompt contexts.
The effective key reservations and this flag publish together before a context change commits.
Failed publication preserves the previous context and Escape policy.
The native adapter mirrors the published prefix mechanism so command tails never reach the page or editable chrome.
It sends the original key event to Clojure, which interprets and dispatches the command.

Outside prompts, Escape modifies the next non-modifier press with Meta and preserves its other modifiers.
This also works within a longer sequence, such as `C-x Escape C-f` for `C-x C-M-f`.
`Escape Escape`, `C-g`, focus resets, and keymap changes clear the pending prefix.
An invalid continuation is consumed and clears the prefix. Key releases and physical repeats do not advance it.
Inside prompts, Escape returns the quit result immediately, even after another command prefix.
Prompt cancellation restores the previous focus and Escape policy.

The first physical Escape follow-up exposed a composition adapter error.
Winit's disabled event also occurs when the shell temporarily disables composition for a command prefix.
Forwarding that event as Servo's dismissed event moved DOM focus away from the input.
The corrected adapter ends an active composition without sending a dismissed event.
A regression covers both idle and active composition disable paths and forbids the focus-changing event.
The failed physical baseline remains in the local evidence.

The corrected physical repeat passed on the same local fixture and Dvorak layout.
Double Escape and `C-g` each cancelled a Meta prefix; the following `x` presses entered the same field, producing `xx`.
`Escape x` opened an empty palette while the page retained `xx`.
Inside that prompt, `C-x Escape` closed it immediately and cleared both pending state and the input context.
The restored page accepted `x≈é`, producing `xxx≈é` without a focus click.
After REPL activation settled, `Escape x` opened an empty palette without changing its source.
One Escape closed it. Physical source entry then evaluated `"≈é"` to `"≈é"`.
Native close returned zero, joined the worker, and removed the port file.

One earlier burst sent Escape during REPL activation. The existing owner-change reset discarded that prefix, and its following `x` entered the REPL.
The repeat removed that character and waited for the displayed input before starting Escape Meta.
These checks do not establish prefix retention across an unfinished focus transition. Focus changes deliberately cancel pending key sequences.

The corrected physical bundle SHA-256 was:

```text
2d6b48d5f64bb963819d1f296e6b934eae12b07c774fda9c8659b42ccef543f6
```

All twelve commands in `target/option-mode/escape-final-checks/summary.json` passed after the focus correction.
This includes 61 portable tests, 20 native unit tests, and the Servo contract.
Both 16-case native suites, both software Servo modes, both native nREPL modes, Clippy, and pinned standalone AOT passed.
Formatting and whitespace checks passed.
The bridge checks retain the two- and three-argument native forms and cover synchronous Escape policy changes across prompt contexts.
The shared shell model covers failed context publication in the interpreter, Tiered mode, and standalone AOT.
Physical observations, both bundle hashes, the failed baseline, the final screenshot, and shutdown logs reside in `target/option-mode/escape-physical/`.
