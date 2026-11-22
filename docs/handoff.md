# Development handoff

## Current state

The 2026-11-21 [compatibility follow-up](phase-2-compatibility-follow-up.md) started from `1ef0723`, then equal to fetched `origin/main`.
This checkpoint integrates the work from `codex/phase-2-compatibility` into `main`.
Phases 0 and 1 retain owner acceptance. Phase 2 remains open. Phase 3 has not started.

The Linux REPL-completion failure reproduced in the output-truncation fixture.
A chunked fixture produces the same 20,000-character result and passes with the original five-second deadline.
The production scheduler and bounds did not change. Failures now include stage labels and bounded diagnostic context.
The corrected REPL check passed in both Linux evaluator modes.
Full portable runs on the small Linux host still failed separate browser-history, introspection, and embedded-shell deadlines.
A passing current Linux CI run remains required.

The IntersectionObserver comparison covers the five required sites and both upstream performance URLs.
The [Wikipedia investigation](phase-2-wikipedia-investigation.md) reproduced a 34-second reply delay that later recovered.
A thread sample locates the pause in timer-driven radio-input validation, alongside a fundraising banner.
A local reduction reproduces the same expensive path with IntersectionObserver disabled and enabled.
This separates the measured bottleneck from observer intersection computation. The product preference remains disabled.
The [engine evaluation](phase-2-engine-optimization.md) measures an isolated adaptation of upstream Servo PR #49855.
Median radio insertion improves by 55.6–56.1% in debug and 32.8–38.8% in release.
Release insertion remains 91–94 ms; both engines retain the same 20 existing validity failures across 220 checks.
Both release engines completed the same named fundraising-banner preview with all 44 heartbeats and three inspections.
That single live pair does not establish a stable speedup or cover every banner variant.
The product engine pin and preferences remain unchanged.
The earlier [BBC investigation](phase-2-bbc-investigation.md) identifies the missing WebCrypto feature and the later fatal advertising-placeholder observer call.
WebCrypto remains enabled. The consented BBC physical task still fails under the default observer preference.

The [physical follow-up](phase-2-physical-follow-up.md) passed active-window GitHub source navigation, Back, Forward, and link completion.
Initial clicks before keyboard activation still left the repository unchanged.
The product retains actual GitHub loading placeholders after document completion.
A controlled probe clears all 83 placeholders only after enabling adopted stylesheets and IntersectionObserver.
Adopted stylesheets alone expose the next missing-IntersectionObserver exception. Product preferences remain unchanged.
The link selector now inspects up to 4,096 anchors, deduplicates URLs, and bounds entries and output text.
Its previous 100-anchor cutoff no longer excludes Cargo.toml solely because of its position.
After unlocking the Mac, the owner requested physical checks.
REPL truncation, history, interruption, page focus restoration, mixed panes, owned-job close, and native shutdown passed.

Clojurust issues #500 and #501 are closed upstream. The product pin remains `540e4218` with both AOT workarounds.
Rust owns native mechanisms and bounded transport. Clojure owns commands, state, bindings, and browser policy.
The installed inventory retains 25 documented natives. Only one webview is visible per native window.
Text and REPL panes support splits.

## Current checks

The portable macOS workspace passed all 58 tests. Native unit tests and the Servo contract passed.
The final browsing fixture passed both evaluator modes, including the new link bounds.
A fresh native GitHub query included Cargo.toml among 147 unique entries.
Formatting, whitespace, portable Clippy, native Clippy, and Python compilation passed.
Both full native process suites passed all 16 cases on the unlocked desktop, including minibuffer focus restoration.
The observer diagnostic now retains repeated runs and optional thread samples. Its local reduction and controls passed the diagnostic checks.
The isolated radio optimization passed the focused equivalence comparison, preparation checks, and final probe Clippy.
The [engine report](phase-2-engine-optimization.md) records the release timings and live banner coverage.
Current main CI remains unverified. A passing Linux CI run remains required.
The [follow-up report](phase-2-compatibility-follow-up.md) retains the additional Linux failures and all limits.

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

1. Use the [engine evaluation](phase-2-engine-optimization.md) to choose the upstream engine correction path.
   The tested iterator change reduces overhead but retains nested document scans and existing validity failures.
   Evaluate product preferences and repeat physical BBC acceptance under the existing five-site scope.
2. Repeat current Linux CI after the REPL fixture correction.
   Keep the additional small-host deadline failures visible until their causes are resolved.
3. Evaluate the GitHub correction with both adopted stylesheets and IntersectionObserver enabled.
   The standalone probe clears all 83 placeholders, but broader compatibility remains required before a product change.
   Keep initial window activation separate from the successful active-window source workflow.
4. Repeat the required checks after compatibility corrections.
   Obtain owner acceptance only after the required criteria pass. Phase 3 has not started.
5. Resolve the Option-as-Alt policy with the owner before changing key translation.
   `C-c x` remains the palette fallback.
6. Evaluate a fixed Clojurust pin for closed issues [#500](https://github.com/csm/clojurust/issues/500) and [#501](https://github.com/csm/clojurust/issues/501).
   Remove the two AOT workarounds only after that pin passes their regressions.

E5 remains deferred beyond Phase 2 under the accepted single-visible-webview scope.
IME, clipboard, display-scale transitions, and interactive Linux behavior remain unverified.
