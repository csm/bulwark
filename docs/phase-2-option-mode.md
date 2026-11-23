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
Escape still cancels prompts and pending key sequences. Escape as a Meta prefix awaits the owner's prompt behavior decision.

## Implementation

Clojure owns the setting and publishes it with the effective key reservations.
Validation and native publication precede state changes. Failed publication preserves the setting, keymaps, and pending prefix.
A successful setting change cancels the pending prefix.
The route publisher callback now accepts `[sequences cancel-strokes option-mode]`.
The native `set-key-routes!` accepts that third argument. Its existing two-argument form retains the current mode.
The installed inventory remains 25 natives.

Rust applies the published macOS mode through Winit's `set_option_as_alt` and wakes the host after a mode change.
Printable Option characters bypass shell Meta matching in character mode.
The page receives those translated characters without the consumed Alt modifier, which otherwise suppresses Servo text insertion.
Named navigation keys retain Alt. Combined Control/Command shortcuts retain their modifiers.
The adapter forwards native composition events to the focused page and enables composition for editable chrome.
Reserved command prefixes temporarily disable native composition so the shell retains command ownership.
Prompt focus prevents composition events from reaching the underlying page.
No binding, prompt policy, engine preference, deadline, or dependency pin changed.

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
Escape prefix behavior remains unimplemented pending the owner's prompt decision.
