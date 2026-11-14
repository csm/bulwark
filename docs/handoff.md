# Development handoff

## Current state

Snapshot: `main` at `84a26f3`, 2026-11-14. [Earlier handoff](handoff-2026-11-14.md).

Phases 0 and 1 are complete and accepted. Phase 2 is implemented through P2.6.
The shell supports retained buffers, mixed web/text layouts, modes, contextual keymaps, completion, and minibuffer prompts.
P2.6 adds Help, Messages, recoverable command errors, and profile init loading with live reload.
The [introspection checkpoint](phase-2-introspection.md) records these interfaces.

The [validation report](phase-2-shell-acceptance.md) records passing macOS physical checks and four CI jobs at `090e2c5`.
Coverage includes 57 portable tests, standalone AOT, and native process and nREPL suites in both evaluator modes.
The physical checks cover P2.5 and P2.6 within the documented input limits.

Rust owns native mechanisms and bounded transport. Clojure owns application state, commands, bindings, and browser policy.
Only one webview is visible per native window. Hidden webviews retain their state, and text panes support splits.
Clojurust remains pinned to `540e4218`, with AOT workarounds in `completion/rank` and `mode/register!`.

macOS Option-key translation can prevent `M-x` from matching. The `C-c x` palette fallback works.
IME, clipboard, display-scale transitions, and interactive Linux behavior remain unverified.
Live-site compatibility and the Phase 2 exit review remain open.

## Next steps

1. Implement [P2.7](phase-2-plan.md#p27-complete-default-browsing-and-the-internal-repl): default browsing policy through public APIs in `clj/bulwark/default/`.
   Complete browsing commands, find, zoom, link opening, popup handling, and lifecycle hooks.
   Preserve opener semantics and keep popup policy in Clojure.
2. Add the internal REPL with bounded jobs and output, history, interruption, and cancellation when its buffer closes.
   Preserve the ordered dispatcher and opt-in nREPL listener.
3. Run [P2.8 acceptance](phase-2-plan.md#p28-run-acceptance-and-record-the-exit-review) from a clean profile.
   Record the five agreed site tasks, startup measurements, platform evidence, native policy review, and remaining engine limitations.
   Obtain owner acceptance after the required criteria pass.
4. Resolve the Option-as-Alt policy with the owner before changing key translation.
   Track Clojurust issues [#500](https://github.com/csm/clojurust/issues/500) and [#501](https://github.com/csm/clojurust/issues/501).
   After a fixed dependency pin passes the regressions, remove the two AOT workarounds.
