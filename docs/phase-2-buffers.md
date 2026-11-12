# Phase 2 buffer checkpoint

Status: implemented, 2026-11-12. Phase 2 remains in progress.
Branch: `claude/cool-faraday-q2ev46`. Base: main at `283a925`, which contains the [mechanism checkpoint](phase-2-mechanisms.md).

This checkpoint implements P2.2 of the [implementation plan](phase-2-plan.md): buffers and explicit lifecycles.
It does not add windows, layouts, mode definitions, completion, configuration, or the internal REPL.

## Buffer registry

`bulwark.buffer` holds the registry, its pure transitions, and the lifecycle operations.
A buffer is an ordinary map:

| Key | Meaning |
|---|---|
| `:buffer/id` | Process-wide identifier. Never reused. Distinct from every webview handle. |
| `:buffer/kind` | `:web`, `:text`, or `:repl`. |
| `:buffer/name` | Optional explicit name, unique among live buffers. |
| `:buffer/view` | The owned webview handle of a `:web` buffer. Nil for the other kinds. |
| `:buffer/major-mode` | A keyword. Defaults are `:web-mode`, `:text-mode`, and `:repl-mode`. |
| `:buffer/minor-modes` | Ordered keywords, most recently enabled first. |
| `:buffer/locals` | Buffer-local variables. |
| `:buffer/text` | Content of a `:text` or `:repl` buffer. Nil for web buffers. |

The registry atom holds one immutable map with buffers, recency order, selection, view ownership, and page state.
The transitions `add`, `select`, `remove-buffer`, `page-event`, `set-local`, `set-major-mode`, `enable-minor-mode`, `disable-minor-mode`, and `set-text` are pure.
They accept supplied identifiers and need no engine, so the interpreter and the AOT binary run the same assertions.
Mode keywords are data in this checkpoint. P2.4 adds mode definitions, keymaps, and mode hooks.

Page state lives beside the buffers, keyed by webview handle: URL, title, load status, and history flags.
`(buffer/page id)` reads it for a web buffer. `(buffer/view-page view)` reads it for any live view.
Events update only their own view's page state. They never create a buffer or change another buffer.

## Identifiers and the native host

`bulwark.native/allocate-id!` is a new synchronous mechanism.
It returns an identifier from the allocator that also issues webview handles, per D3.
No identifier names two resources in one process. Exhaustion throws.

The runtime installs the native mechanisms through `bulwark.buffer/set-host!`, like the key-route publisher.
The host map supplies `:create`, `:close`, `:select`, and `:next-id`.
A portable runtime has no host. Text and REPL buffers still work there, and web creation reports the missing engine.

## Lifecycles

```clojure
(await (bulwark.buffer/create! {:url "https://example.com"}))         ; a selected web buffer
(await (bulwark.buffer/create! {:url "about:blank" :select false}))   ; a background web buffer
(await (bulwark.buffer/create! {:kind :text :name "*scratch*"}))
(await (bulwark.buffer/select! id))
(await (bulwark.buffer/kill! id))
```

Creation publishes a buffer only after native creation succeeds.
If publication fails, creation closes the new webview and raises the original error.
The engine shows the first view of an empty registry. Creation then issues an explicit native selection, so the registry decides visibility.

Selection calls the native mechanism first and commits the registry second. A failed native call changes neither.
Selecting a text or REPL buffer hides every webview. Selection cancels a pending key prefix.

Kill removes the buffer from the registry before it closes the webview.
The id is stale at once, and a second kill fails with `:stale-buffer` without a native request.
The view's handle stays in a closing set until its close event arrives. Events for a closing view are discarded.
Killing the selected buffer selects the most recently selected survivor.
A close that the engine initiates kills the owning buffer through the same path, without a second native close.
A native close failure other than `:stale-handle` is raised after the registry has settled.

Hooks run through `bulwark.hooks`:

| Event | Fields | Order |
|---|---|---|
| `:buffer/created` | `:buffer` | After publication and native selection. |
| `:buffer/selected` | `:buffer`, `:previous` | After `:buffer/created` or `:buffer/killed`. Not for a reselection. |
| `:buffer/killed` | `:buffer`, `:buffer/record` | After the replacement selection commits. |

`:previous` can name a killed buffer.
`bulwark.core/dispatch!` routes each engine event to the registry before hooks run.
Hooks see the owning buffer's id under `:buffer` on page and key events.

## Browser policy

`bulwark.browser` no longer holds an `active-view` atom or its own page table.
`(browser/active-view)` returns the selected web buffer's view. The accepted helpers and commands use that lookup.
History reads each view's URL from the registry.

The policy for the last web buffer lives in Clojure, in `bulwark.buffer/last-web-buffer-policy`.
`browser/install!` installs a default that opens `@browser/new-buffer-url`, initially `about:blank`.
A nil policy leaves no buffer and no visible page.

New commands and provisional default keys:

| Command | Key |
|---|---|
| `bulwark.browser/new-buffer` | `C-c n` |
| `bulwark.browser/kill-buffer` | `C-x k` |
| `bulwark.browser/next-buffer` | `C-x ArrowRight` |
| `bulwark.browser/previous-buffer` | `C-x ArrowLeft` |

P2.5 adds buffer switching by name. P2.7 moves these defaults into `clj/bulwark/default/`.
The shell echoes a title only for the selected buffer, and echoes a buffer's name after selection.

## Checks and remaining evidence

`clj/tests/shell-model.cljrs` now covers registry transitions, rejection cases, late events, local variables, and modes.
It passed in the interpreter, Tiered mode, and the standalone AOT binary, which now compiles four shell namespaces.

`clj/tests/buffers.cljrs` runs against the product bridge and the mock engine in both evaluator modes.
It covers hook order, background creation, background page events, local variables, stale ids, failed publication, exactly-once close, late events, engine-initiated close, the last-buffer policy, and the buffer commands.

Local Linux results on 2026-11-12:

| Check | Result |
|---|---|
| Formatting and portable workspace Clippy | Passed. |
| Portable workspace tests | All 43 passed. |
| Foundation smokes, both evaluator modes | Passed. |
| Mock nREPL suite, both evaluator modes | Passed. |
| `python3 experiments/run_shell_aot.py` | Passed. |

`clj/tests/buffers-native.cljrs` is a new native process case, `retained-buffers`.
It checks retained JavaScript state, form values, and history across buffer switches, a text buffer, and kills.
This checkpoint did not build Servo locally. The native case, native Clippy, and the native nREPL suite need Linux CI.
Physical input checks across buffers remain open, with the existing IME, clipboard, and display-scale limits.

The next milestone is P2.3. The [window model checkpoint](phase-2-windows.md) begins it.
