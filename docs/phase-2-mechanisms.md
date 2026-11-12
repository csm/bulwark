# Phase 2 mechanism checkpoint

Status: implemented locally, 2026-11-11. Phase 2 remains in progress.
Branch: `codex/phase-2`. Base: accepted Phase 1 at `e49a8ec`.
The owner confirmed the scope in the [implementation plan](phase-2-plan.md).

This checkpoint begins P2.1 with retained webviews and an actual AOT test path.
It does not complete the buffer, window, mode, completion, configuration, or internal REPL deliverables.

## Retained webviews

The engine now retains multiple live webviews in one rendering context.
Only the selected view receives native page input or paints into the shared surface.
Hidden views retain their documents, form state, JavaScript globals, and navigation history.
Their lifecycle and title events still reach Clojure through the existing ordered event queue.

The new mechanism is:

```clojure
(await (bulwark.native/webview-select view-id))
(await (bulwark.native/webview-select nil))
```

The first form selects a retained view. The second hides all views for future non-web panes.
Both return the standard future reply with `:ok`, `:value`, or `:error`.
A stale handle returns `:stale-handle` and leaves the previous selection intact.
Selection does not create, navigate, or close a view.
Closing the selected view leaves no selection. Clojure will define the replacement policy.

The first view in an empty registry becomes visible for compatibility with Phase 1 startup.
Further creations stay hidden, including creations after an explicit hide-all request.
Selection restores the native window's scale, size, and focus to the chosen view.
The shell redraws after selection changes and cancels page input ownership and pending prefixes.
Physical modifier state and cursor position survive a view change.
The host also tracks modifier changes while no page is selected.

Hidden frame notifications retain dirty state without requesting a paint.
The selected view receives an initial paint after each switch.
The existing queue, payload, cancellation, handle, and shutdown limits remain unchanged.
E5 remains deferred. Simultaneous web-page splits remain outside Phase 2.

## AOT path and upstream fix

`clj/tests/shell-model.cljrs` runs in the interpreter, Tiered mode, and a standalone AOT binary.
The assertions cover command documentation, live Var redefinition, key parsing, binding changes, publisher failures, and named hook replacement.
Later milestones must extend this source with buffer, layout, and mode assertions to satisfy B7.

The optional `bulwark-runtime/aot-tests` feature supplies the pinned compiler to the `shell-aot` example.
The product's default build does not include that compiler.
The runner prepares the upstream harness dependencies before its offline build on a fresh CI host.
It then requires a successful binary exit and the assertion completion marker.
The workflow adds a dedicated Linux `shell-aot` job.

Run the complete path with:

```sh
python3 experiments/run_shell_aot.py
```

The compiler generates native object code for the entry source and three required shell namespaces.
Its namespace and macro preambles still contain interpreted source.
This checkpoint establishes native AOT execution, not source-opacity compliance or an AOT build of the browser executable.

The initial probe exposed two compatibility gaps:

1. Compiled calls to the runtime's `resolve` sentinel fail.
2. Compiled symbol constants lose namespace fields, so qualified command symbols fail registry checks.

The shell now registers commands with direct Var references.
The runtime installs the native key-route publisher through `bulwark.keymap/set-route-publisher!`.
The portable keymap uses no native publisher until a host installs one.
Publisher replacement first publishes the current reservations. Failure retains the previous publisher and bindings.
Binding updates retain their existing publication-before-commit behavior.

The symbol defect required an upstream fix.
This checkpoint pinned all Clojurust packages to [`14bd7f9c`](https://github.com/csm/clojurust/commit/14bd7f9c6e05eed0349d802275cb7950f5a56f90).
That commit extends the previous pin by one commit on `codex/bulwark-phase-2-aot`.
The fix uses `Symbol::parse` in `rt_const_symbol`, preserving namespace and version fields.
The ABI regression failed before the fix and passed afterward.
All 77 compiler unit tests and a standalone native AOT regression passed.
The latter checks namespace, name, symbol equality, and qualified map-key lookup.

The [window continuation](phase-2-windows.md#unicode-compiler-fix) adds the next compiler fix and records the current pin.

General compiled runtime reflection remains a dependency risk for later features.
This change does not implement `resolve` in compiled code.

## API inventory for subsequent milestones

The following entries are mechanisms to design during their owning milestones.
Their names and schemas remain provisional until those implementations establish the contracts.

| Mechanism | State | Required contract |
|---|---|---|
| `webview-select` | Implemented | One selected handle or nil, stale rejection, explicit lifecycle. |
| Buffer and window identities | P2.2–P2.3 | Rust-issued process-wide identifiers, Clojure-owned logical records, no identifier reuse. |
| Viewport and UI descriptions | P2.3 | Bounded data, explicit rectangles and scale, revision-tagged UI events, native resource checks. |
| Contextual key reservations | P2.4 | One effective Clojure keymap, atomic native publication, prefix cancellation after context changes. |
| Editable chrome controls | P2.5 | Text and selection events, focus ownership, bounded payloads, prompt cancellation. |
| Native documentation | P2.6 | Complete docstrings, signatures, and resource tags for introspection. |
| Page zoom | P2.7 | Explicit view and finite zoom value through Servo's `set_page_zoom`. |
| Auxiliary view requests | P2.7 | Native pending request token, bounded lifetime, explicit accept/deny, parent-close cleanup. |
| Internal REPL jobs | P2.7 | Bounded admission and output, buffer ownership, interruption, cleanup. |

The pinned Servo API exposes `CreateNewWebViewRequest::builder`.
The request owns an automatic responder and can remain on the engine thread until Clojure decides its disposition.
A future native accept operation can consume that request and register the resulting webview.
The request builder preserves Servo's auxiliary-view relationship instead of recreating a URL-only view.
This checkpoint inspects that source contract. A popup fixture must establish runtime behavior during P2.7.

## Checks and remaining evidence

The Servo contract executable covers selection, hide-all, focus, scale, resize, stale handles, cancellation, and shutdown.
The retained-view fixture runs through both software Servo and the native process runner.
It checks independent JavaScript state, retained form values, history isolation, switching, hide-all, and close isolation.
The fixture uses `history.pushState` to isolate retained state from a subsequent navigation.
It waits for the fixture title and completed load before it edits either document.

Local checks include portable workspace tests, interpreter/Tiered shell assertions, native unit and contract checks, and standalone AOT execution.
The existing native process checks also pass with the retained-view case.
The portable workspace passed all 42 tests.
Native checks passed 13 unit tests and the Servo contract executable.
Both evaluator modes passed eight native process cases and four native nREPL cases.
Software Servo painted 48 frames in Tiered mode and 50 frames in the interpreter.
Both software runs completed the retained-view assertions and joined their runtime workers.
Formatting and Clippy passed for the portable workspace, native targets, and optional AOT example.
The standard GL texture diagnostic and large `__eh_frame` linker warning remain.
Native macOS graphics checks require graphics access beyond the filesystem sandbox.

The [Ubuntu test host](linux-testing.md) passed all 42 portable workspace tests and the full 17-command portable suite.
Its runner uses a recorded assertion budget for the small CPU. Shutdown deadlines remain unchanged.
AOT and Servo validation on that host remain in progress through the sequential queue.
The new GitHub Linux CI job still requires a remote run.
Physical keyboard and pointer checks across retained views remain separate from the automated process checks.
This checkpoint does not extend the existing IME, clipboard, display-scale, or interactive Linux evidence.

The next milestone is P2.2: the Clojure buffer registry, local variables, native resource ownership, and lifecycle hooks.
