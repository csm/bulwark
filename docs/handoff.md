# Development handoff

## Current state

P2.8 acceptance evidence is recorded in the [Phase 2 review](phase-2-acceptance.md), 2026-11-14.
The source baseline is `d628a87`, after P2.7 at `9835f77`.
The P2.8 code checkpoint is `689ba2d` on `codex/phase-2-acceptance`.
That checkpoint added a native presentation diagnostic and a reproducible release startup runner.
Phases 0 and 1 remain accepted. Phase 2 remains open.

B2 does not pass under the agreed five-site scope.
BBC News and a direct article URL produce a client-side exception page, including after reload.
GitHub renders repository and source pages, but a normal source-link click did not navigate.
Address entry loads the file. Its anchors fall outside the default selector's first 100 links.
Wikipedia, Hacker News, and MDN passed their named tasks.
No site substitution or scope exception is accepted by the report.

The [BBC investigation](phase-2-bbc-investigation.md) identified the missing Servo `webcrypto` build feature in Bulwark.
The correction exposes `crypto.getRandomValues` and restores BBC page content in native diagnostic runs.
The original BBC URLs also support programmatic scrolling after the correction.
The physical follow-up at `833ad5f` still failed after user-approved consent: article click and reload produced a client-side exception.
WebCrypto remained available. The failing script remains unidentified. Servo's experimental IntersectionObserver remains disabled.
The [CI run at `833ad5f`](https://github.com/csm/bulwark/actions/runs/43680513338) failed Linux REPL completion.
Its Servo, macOS runtime, and standalone AOT jobs passed.

Rust owns native mechanisms and bounded transport. Clojure owns commands, state, bindings, and browser policy.
The native policy review found no browser policy in the eleven Phase 2 native additions.
The installed inventory contains 25 documented natives.
Only one webview is visible per native window. Text and REPL panes support splits.
Clojurust remains pinned to `540e4218`, with the two earlier AOT workarounds.

## P2.8 evidence

The portable workspace passed 58 tests. Native unit tests and the Servo contract passed with display access.
All 16 native process cases passed in each evaluator mode.
Mock and native nREPL suites, foundation checks, and software Servo checks passed in both modes.
The standalone pinned AOT binary passed the shared shell assertions.
Formatting and native Clippy passed.

Physical Tiered checks passed address entry, retained page input, completion, and page focus restoration.
The internal REPL passed source entry, output, errors, history, draft restoration, namespaces, live commands, and interruption recovery.
Mixed page/REPL panes passed pointer focus selection.
Closing a REPL buffer cancelled its owned job without stopping nREPL.
Native close with both interfaces parked returned zero, joined the worker, and removed the port file.
Live Wikipedia init reload changed `C-x b` and applied reader mode without duplicate hooks or styles.
Physical apropos opened documented native entries in a text buffer.

Two long Computer Use typing bursts reached the existing fatal event-overflow path.
The retained reproduction joined the worker and returned status one.
The 256-event queue and its explicit overflow contract remain unchanged.
Shorter input after interruption passed. This evidence does not establish clipboard behavior or an input-rate limit.

Seven final release samples had a median of 0.264579 seconds on the M3 Max laptop.
Each used a new empty profile at `about:blank`, without nREPL, with warm filesystem and shader caches.
Timing runs from parent process creation to receipt of the first page/chrome presentation marker.
The report records all samples, hashes, hardware, and timing limits.
Local logs and composed frames reside in `target/phase-2-acceptance-evidence/`.

The [P2.8 branch CI run](https://github.com/csm/bulwark/actions/runs/43670796887) passed all four jobs at `689ba2d`.
Linux passed both native process and nREPL modes, including the new presentation assertion.
Linux and macOS passed runtime checks. The standalone Linux AOT job passed.
The [P2.7 checkpoint](phase-2-browsing.md) and [P2.5/P2.6 physical report](phase-2-shell-acceptance.md) retain earlier evidence.

## Next steps

1. Capture the consented BBC article's script exception and investigate the remaining GitHub navigation limits.
   The BBC report records the failed physical follow-up, remaining Servo limitations, and dependency constraint.
   Diagnose the latest Linux `Timed out: REPL completion` CI failure.
2. Repeat the required checks after compatibility corrections.
   Obtain owner acceptance only after the required criteria pass. Phase 3 has not started.
3. Resolve the Option-as-Alt policy with the owner before changing key translation.
   `C-c x` remains the palette fallback.
4. Track Clojurust issues [#500](https://github.com/csm/clojurust/issues/500) and [#501](https://github.com/csm/clojurust/issues/501).
   Remove the two AOT workarounds only after a fixed dependency pin passes their regressions.

E5 remains deferred beyond Phase 2 under the accepted single-visible-webview scope.
IME, clipboard, display-scale transitions, and interactive Linux behavior remain unverified.
