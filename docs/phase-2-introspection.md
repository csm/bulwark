# Phase 2 introspection and configuration checkpoint

Status: implemented with macOS physical validation, 2026-11-14. The [validation report](phase-2-shell-acceptance.md) records CI and physical results.
Base: `main` at `17bb3ed`, after P2.5 and PR #14.

This checkpoint implements P2.6 in the [Phase 2 plan](phase-2-plan.md).
Clojure owns help commands, diagnostic retention, configuration paths, and reload policy.
Rust supplies native metadata, a bounded source reader, and nREPL evaluation error reports.

## Help commands

Each command accepts an argument or opens a minibuffer prompt without one.
The commands show results in one reusable, read-only `:text` buffer named `*Help*`.
They use live Vars and the current keymap context.

| Command | Argument | Result |
|---|---|---|
| `bulwark.help/describe-command` | Command symbol | Registry documentation and the current Var signature. |
| `bulwark.help/describe-key` | Emacs key string, such as `C-x b` | Command and source keymap, prefix, or unbound status. |
| `bulwark.help/describe-variable` | Qualified Var symbol or buffer-local keyword | Documentation and current value. A keyword uses the selected buffer's effective local value. |
| `bulwark.help/apropos` | Search string | Names, documentation, and resource tags that contain the query, without regard to case. |
| `bulwark.messages/show-messages` | None | The retained error text. |

`M-x` and `C-c x` expose these commands through the existing palette.
Variable and apropos results include loaded `bulwark.*`, `user`, and `user.*` namespaces.
`describe-key` takes key notation as text. It does not capture a physical key sequence.

All 18 installed native functions have `:doc`, `:arglists`, and `:resources` metadata.
The native inventory supplies these fields from one table.
The regression compares every installed native Var with this metadata contract.
It also compares every `:webview` resource operation with `apropos "webview"` results.

The webview resource set contains `webview-create`, `webview-close`, `webview-select`, `navigate`, `reload`, `back`, `forward`, `webview-info`, and `eval-js`.
It also contains `set-ui!`, which selects a visible webview, and `next-event`, which returns webview events.
Apropos can return additional matches from names or documentation.

## Messages and error recovery

`bulwark.diagnostics` retains at most 256 entries and 16,384 characters of combined text.
Each entry contains at most 2,048 characters. New errors displace older text.
The registry retains strings rather than exception objects.

Command failures retain the command name. Hook failures retain the event type and hook id.
Configuration failures retain the source path and available location data.
The nREPL evaluator reports evaluation errors with the source file, line, and column.
It preserves the original nREPL reply and session namespace.
A forced-collection regression protects the exception value while the reporter runs.
The upstream nREPL parser reports syntax errors directly to its client before the evaluator callback runs.
Those nREPL syntax errors do not enter Messages through this callback.

`bulwark.messages` copies diagnostics to a read-only `*Messages*` buffer every 100 milliseconds.
The buffer keeps pane selection unchanged. Concurrent updates use a publication gate.
A failed UI update retries on the next turn. Killing the buffer preserves the retained diagnostics.
A later update or `show-messages` command creates the buffer again.

The ordinary shell installs a recoverable event boundary.
A command or hook error records diagnostics and updates the echo area. The next event still runs.
Lower-level script hosts preserve their fatal dispatcher behavior unless they install this boundary.
Bridge overload, engine failure, cancellation, and bounded shutdown retain their existing contracts.

## Configuration startup and reload

The default init path is `~/.bulwark/init.cljrs`.
An explicit `--profile DIR` selects `DIR/init.cljrs`, with or without `--nrepl`.
Profile selection alone creates no listener or discovery file.

The ordinary shell installs defaults, loads init, then opens the initial page.
A startup gate holds ordered input and page events until this sequence finishes.
A missing init file keeps the defaults. An init error records a diagnostic and leaves the shell usable.
Test scripts supplied through `--script` retain explicit control over startup and configuration.

`bulwark.native/read-source` reads and parses a UTF-8 file of at most 256 KiB.
It returns top-level forms with line and column numbers, or nil for a missing file.
Other filesystem and parse errors include the source path. Filesystem IO parks the caller.
The reader runs no code and has no profile policy.

Clojure evaluates the forms in one namespace context, initially `user.init`.
An explicit `ns` form changes that context for later forms.
The loader restores the caller namespace after success or failure.
Each runtime error records the start location of its top-level form.
Parse errors preserve the reader's source diagnostic.
This location identifies the top-level form, rather than a complete stack trace inside a called function.

`M-x reload-init` loads the same path again. Reload is incremental.
Earlier forms remain effective after a later form fails. Forms after the error do not run.
Repeated named commands, hooks, bindings, and modes replace their earlier definitions.
A concurrent load raises `:config-busy`. This milestone writes no profile EDN files.

## Example configuration

[examples/init.cljrs](../examples/init.cljrs) rebinds `C-x b` to a documented command.
It also enables a small reader transformation for `https://en.wikipedia.org/wiki/` pages.
The mode hides navigation chrome through one removable style element.
A named load hook applies it to later article loads. Reload also applies it to retained article pages.
Disabling the mode removes the style. Repeated reloads duplicate neither the hook nor the style.

To use the example:

```sh
mkdir -p /tmp/bulwark-example
cp examples/init.cljrs /tmp/bulwark-example/init.cljrs
cargo run --locked -- --profile /tmp/bulwark-example 'https://en.wikipedia.org/wiki/Rust_(programming_language)'
```

The example uses trusted Clojure and bounded JavaScript through public APIs.
It does not perform general article extraction. The `reader-prefix` atom selects the target URL prefix.
The native regression points that atom at a local fixture and checks the resulting DOM style.
Live Wikipedia compatibility remains part of the later browsing acceptance matrix.

## Validation

The automated suites cover interpreter and Tiered execution separately.
The standalone AOT binary preserves the existing buffer, window, mode, keymap, and prompt assertions.
It now compiles ten shell namespaces, including the new diagnostics dependency.

| Check | Result |
|---|---|
| Portable workspace | 57 tests passed. |
| New bridge suite | Native metadata, interactive help, Messages, init failures, partial reloads, and repeated example reloads. |
| Native startup | Missing init, defaults before init, and broken-init recovery without nREPL passed. |
| Native configuration fixture | Reader enable, disable, repeated reload, event recovery, and Help frame passed. |
| Native process runner | All 15 cases passed in each evaluator mode. |
| Mock and native nREPL | Passed in each evaluator mode, including evaluation error reports. |
| Foundation and Servo smokes | Passed in each evaluator mode. |
| Native unit tests and Servo contract | All 17 unit tests and the contract passed. |
| Formatting and Clippy | Portable workspace and native targets passed. |
| Standalone AOT | Passed with the product's pinned compiler. |

The native runner saves `configuration-<mode>.png` through `--artifacts-dir DIR`.
The Help frame received visual inspection on macOS.
The [physical report](phase-2-shell-acceptance.md#p26-physical-checks) records passing help, Messages, and configuration checks.
Its CI section records the passing four-job run at `090e2c5` and the validation corrections.
The existing GL texture and large unwind-section warnings remain.

The next milestone is P2.7: default browsing commands and the internal REPL.
