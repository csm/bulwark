# Development handoff

## Current state

Snapshot: P2.7 checkpoint, based on `main` at `34b1f1a`, 2026-11-14.
The pre-P2.7 state remains in the Git history.
The [earlier handoff](handoff-2026-11-14.md) records older checkpoints.

Phases 0 and 1 are complete and accepted. Phase 2 is implemented locally through P2.7.
The [browsing checkpoint](phase-2-browsing.md) records the new interfaces and limits.
Default configuration now lives in `clj/bulwark/default/` and uses the public shell APIs.
The shell adds find, zoom, link completion, deferred popup decisions, and an egui internal REPL.
Popup creation preserves Servo's opener relationship. Clojure controls acceptance and selection.

The internal REPL shares globals with the browser and nREPL.
It retains bounded jobs, output, transcript, and input history.
Closing its buffer cancels its own jobs without cancelling another REPL or nREPL.
Queued evaluations leave the ordered dispatcher available.
The pinned evaluator writes the shared `*ns*` root, so each internal evaluation poll installs and restores its namespace.
The regression covers live command registration, redefinition, namespace changes, and parked evaluations.

Rust owns native mechanisms and bounded transport. Clojure owns application state, commands, bindings, and browser policy.
Only one webview is visible per native window. Hidden webviews retain state, and text panes support splits.
Clojurust remains pinned to `540e4218`, with the earlier AOT workarounds in `completion/rank` and `mode/register!`.

## Local P2.7 evidence

The portable workspace passed 58 tests.
The new REPL assertions run in TreeWalk and Tiered modes.
The standalone AOT binary passed the shared shell assertions, including split-REPL focus and control identity.
Formatting and portable and native Clippy passed.

All 17 native unit tests and the Servo contract passed.
All 16 native process cases passed in each evaluator mode.
The browsing fixture passed find, zoom, safe link selection, popup denial, opener communication, and parent-close cleanup.
Its final REPL frames received visual inspection on macOS.
The local frames and copied command logs are under `target/phase-2-browsing-evidence/`.

Mock and native nREPL suites passed in both modes.
They include overload, interruption, live commands, port cleanup, and internal REPL cancellation isolation.
Foundation and software Servo checks passed in both modes.
Every native process joined its runtime worker, including shutdown with internal evaluation work parked.
The existing GL texture and large unwind-section diagnostics remain.

These results are automated macOS evidence. They do not establish P2.7 physical keyboard or pointer behavior.
GitHub CI for the P2.7 changes remains open.
The [earlier validation report](phase-2-shell-acceptance.md) records P2.5/P2.6 physical checks and four passing CI jobs at `090e2c5`.

## Next steps

1. Check GitHub CI for the P2.7 checkpoint, including native process, nREPL, and AOT jobs.
2. Run [P2.8 acceptance](phase-2-plan.md#p28-run-acceptance-and-record-the-exit-review) from a clean profile.
   Record physical default browsing and internal REPL workflows, including text input, history, interruption, and focus restoration.
3. Record the five agreed site tasks, release startup measurements, native policy review, and remaining engine limitations.
   Obtain owner acceptance after the required criteria pass.
4. Resolve the Option-as-Alt policy with the owner before changing key translation.
   Track Clojurust issues [#500](https://github.com/csm/clojurust/issues/500) and [#501](https://github.com/csm/clojurust/issues/501).
   Remove the two AOT workarounds after a fixed dependency pin passes their regressions.

macOS Option-key translation can prevent `M-x` from matching. The `C-c x` palette fallback remains available.
IME, clipboard, display-scale transitions, interactive Linux behavior, and live-site compatibility remain open.
