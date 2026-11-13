# Phase 2 mode and contextual keymap checkpoint

Status: implemented, 2026-11-12. Linux CI and physical keyboard checks remain open.
Base: `codex/phase-2-3-windows` at `17f0750`, merged with `main` at `c689d8c`.

This checkpoint implements the six P2.4 items in the [implementation plan](phase-2-plan.md).
It changes Clojure only. The native key reservation mechanism from Phase 1 is unchanged.
[PR #12](https://github.com/csm/bulwark/pull/12) still holds the final P2.3 validation. This work includes its commits.

## Mode definitions

`bulwark.mode` holds mode definitions as data under a keyword:

```clojure
(define-minor-mode reader-mode
  "Shows the article text alone."
  {:keymap {"C-c q" 'my.config/leave-reader}
   :locals {:fill-column 80}
   :on-enable (fn [buffer-id] ...)
   :on-disable (fn [buffer-id] ...)})
```

`define-major-mode` and `define-minor-mode` name the mode by the symbol's keyword, here `:reader-mode`.
Each macro also defines a documented command of the same name.
The minor-mode command toggles the mode in the selected buffer. The major-mode command gives the selected buffer that mode.
`(mode/register! kind keyword doc options)` is the function form.

A repeated definition replaces the previous one, so a reloaded file adds no duplicates.
A redefinition without `:keymap` keeps the mode's current keymap. A mode cannot change its kind.
`bulwark.browser` defines `web-mode`, `text-mode`, and `repl-mode` without keys.
The default bindings stay in the global keymap until P2.7 moves the default policy.

| Operation | Behavior |
|---|---|
| `(mode/set-major! buffer mode)` | Changes the registry and keymaps, then disables the previous mode, then enables the new one. |
| `(mode/enable! buffer mode)` | Puts a minor mode ahead of the enabled ones. An enabled mode keeps its place. |
| `(mode/disable! buffer mode)`, `(mode/toggle! buffer mode)` | Removes or toggles a minor mode. |
| `(mode/enabled? buffer mode)`, `(mode/mode-info mode)` | Read a buffer's modes or a definition. |

An enable or disable runs the mode's own function, then the `:mode/enabled` or `:mode/disabled` hooks.
The hook event contains `:mode`, `:mode/kind`, and `:buffer`. All results are awaited.
A new buffer enters its major mode before the other `:buffer/created` hooks run.
A killed buffer runs no disable lifecycle. `:buffer/killed` hooks receive the buffer record with its modes.
A request that changes nothing runs no lifecycle.
An unknown name raises `:unknown-mode`. A mode of the other kind raises `:wrong-mode-kind`.

`(buffer/local id key default)` now reads the buffer's own value first.
It then reads the `:locals` defaults of the minor modes in order, then the major mode, then the supplied default.

The pure `bulwark.buffer` transitions still accept any mode keyword.
A keyword without a definition supplies no keymap, no defaults, and no lifecycle function.

## Keymap precedence

`bulwark.keymap` now holds the global keymap, named keymaps, and the active context.
A mode's keymap is the named keymap with the mode's keyword.
`bind-key!` and `unbind-key!` accept a keymap id as an optional first argument.

| Precedence | Keymap | Source |
|---|---|---|
| 1 | Input context | `(keymap/set-context! id)`, for the P2.5 minibuffer |
| 2 | Minor modes | The selected buffer, most recently enabled first |
| 3 | Major mode | The selected buffer |
| 4 | Global | `keymap/bindings` |

Clojure merges these into one flat effective keymap. Dispatch and the native reservations both use it.
`(keymap/lookup "C-c d")` returns the command and its source keymap, `:prefix`, or nil.

One keymap still rejects a sequence that starts or continues another of its sequences.
Between keymaps, precedence decides:

| Higher keymap | Lower keymap | Result |
|---|---|---|
| `C-c d` command | `C-c d` command | The higher command runs. |
| `C-x` command | `C-x x` command | `C-x` runs the higher command. `C-x x` is unreachable. |
| `C-y y` command | `C-y` command | `C-y` is a prefix. The lower command is unreachable. |
| `C-z y` command | `C-z z` command | Both run. They share the prefix. |

Bindings name commands, so a redefined command takes effect on the next key.

## Publication and consistency

Every change computes a candidate effective keymap and publishes its sequences first.
The global keymap, named keymaps, active context, and effective keymap change only after that succeeds.
A failed publication therefore retains all of them, and the native snapshot.
A change to a keymap outside the active context publishes nothing.

The buffer registry publishes a candidate selection's keymaps before it commits the selection.
This covers buffer selection, window selection, layout changes, and mode changes in the selected buffer.
A refused publication leaves the selection, the buffer's modes, and the frame unchanged, and runs no lifecycle.
A context change always publishes, so the native host also cancels its pending prefix.
Clojure cancels its own pending prefix after every committed change.

A removal cannot be refused. After a failed publication there, the previous effective keymap and reservations stay.
`buffer/keymap-failure` holds the error until a later removal publishes successfully.
The next successful selection or mode change publishes the correct context.
The native limit of 256 sequences applies to the effective keymap, not to each keymap.

## AOT limits

The AOT shell model now compiles seven shell namespaces, including `bulwark.mode`.
The mode macros expand to async commands. An AOT binary interprets such forms in a startup preamble.
The preamble runs before every compiled top-level form of the same file.
The AOT model therefore uses `mode/register!`. The bridge suite covers the macros in the interpreter and Tiered mode.

The pinned compiler `540e4218` also miscompiled the first form of `mode/register!`.
That form bound `:locals`, both lifecycle functions, and the existing definition in one `let`.
In the AOT binary, the bound defaults later read as nil or as the `:keymap` value.
An interpreted caller also made the function reject valid arguments.
The current form builds the definition map in a separate function and passes all AOT assertions.
A standalone namespace with the same function body did not reproduce the stored-value defect.
A standalone compiled namespace with one async function did reproduce the interpreted-caller rejection.
No upstream issue exists yet. The defect needs a minimal reproduction before a pin change.

## Checks and remaining evidence

| Check | Evidence |
|---|---|
| Portable workspace | 53 tests. The two new tests run in both evaluator modes. |
| `clj/tests/shell-model.cljrs` | Precedence table, prefix conflicts, failed publication, live redefinition, context changes, mode defaults. Interpreter, Tiered, and AOT. |
| `clj/tests/modes.cljrs` | Macros, lifecycle order, key dispatch through `core/dispatch!`, buffer and window changes, the real 256-sequence refusal, removal recovery. |
| Reservation test | The native snapshot follows the selected buffer's modes before a synchronous runtime stall. |
| Accepted stall regression | `key_reservations_publish_before_a_runtime_stall_and_unbind_live` still passes. |
| Native processes | Ten cases per evaluator mode, unchanged. |
| Native nREPL, mock nREPL, both smokes, Servo smoke | Passed in both evaluator modes where a mode flag exists. |
| Formatting and Clippy | Portable workspace and native targets. |

All results come from local macOS runs on 2026-11-12. Linux CI has not run on this change.

The default Clojure assertion budget in `tests/worker.rs` is now 30 seconds instead of 10.
The macOS runner in [run 43293702744](https://github.com/csm/bulwark/actions/runs/43293702744) exceeded 10 seconds in the Tiered shell model.
The same commit passed in the preceding push run. `BULWARK_TEST_PROGRAM_TIMEOUT_SECS` still overrides the budget.

Physical keyboard checks remain open. No test sends a real key to a mode-specific reservation.
`clj/tests/modes-physical.cljrs` prepares them:

```sh
fixture_url=$(python3 -c 'from pathlib import Path; print(Path("experiments/fixtures/phase-2-windows.html").resolve().as_uri())')
cargo run --locked -- --nrepl --profile /tmp/bulwark-p24-physical \
  --script clj/tests/modes-physical.cljrs "$fixture_url"
```

1. With the text pane selected, press `M-m`. The echo area must show `Major-mode key`.
2. Press `C-x o`, then `M-m`. The page key log must show the key, and the echo must not change.
3. Return to the text pane. Evaluate `(await (bulwark.tests.modes-physical/probe-zen-mode))` through nREPL.
4. Press `M-m` and `M-z`. The echo area must show `Minor-mode key` each time.
5. Press `C-x`, select the page with the pointer, then press `o`. The window command must not run.
6. Compare `(await (bulwark.tests.modes-physical/observe!))` with the observed keys.

The script started in the native shell and reached its ready message. No physical step has run.

After these checks, the next milestone is P2.5: the minibuffer and command palette.
