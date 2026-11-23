# Development handoff

## Current state

The [Option setting](phase-2-option-mode.md) implements the owner's keyboard policy.
macOS defaults to native character input. Other platforms retain Alt as Meta.
`bulwark.keymap/set-option-mode!` accepts `:characters`, `:meta`, and `:platform-default` and applies without restarting.
Physical Mac checks passed Option symbols and acute-accent composition in a page, URL prompt, and internal REPL.
Meta mode opened the palette. Switching back restored native text in the same open prompt.
The owner selected immediate Escape cancellation inside prompts, including during pending command prefixes.
Outside prompts, Escape prefixes Meta in either Option mode. `Escape x` invokes `M-x`; double Escape or `C-g` cancels the prefix.
Context changes publish that policy with the key reservations. Failed publication preserves the previous context and policy.
Physical Escape checks passed prompt cancellation during a prefix, page focus after prefix cancellation, palette invocation, and REPL text afterward.
The adapter now preserves DOM focus when command prefixes temporarily disable native composition.
Pending prefixes still reset on focus changes. A burst during unfinished REPL activation crossed that existing reset; the settled-input repeat passed.
Local portable, native, Servo, nREPL, Clippy, and pinned AOT checks passed.
The [final keyboard CI](https://github.com/csm/bulwark/actions/runs/45107213807) passed all four jobs at `cf3e6be`.
No required keyboard validation remains. The owner requested the Phase 2 exit interview.
The [acceptance review](phase-2-acceptance.md) now consolidates the current evidence, policy review, and retained limits.
The owner deferred the small Linux investigation to focus on desktop behavior. Its failures remain unresolved; no minimum specification is established.
Phase 2 remains open. Phase 3 has not started.

The [small Linux follow-up](phase-2-small-linux-follow-up.md) reconnected after the owner enabled Tailscale.
A clean isolated worktree at `e4dc233` passed formatting and Clippy, but workspace tests failed two of 55 attempted tests.
Browser history and the corrected REPL regression passed both evaluator modes.
Embedded-shell startup still exceeded two seconds. Introspection still exceeded its 60-second TreeWalk budget during retention checks.
The runner stopped at command three of seventeen. Later portable commands did not run.
The earlier browser-history failure did not reproduce; its earlier timing variation remains unexplained.

The latest [repeat checks](phase-2-repeat-checks.md) used the unchanged `4d406d6` product source and the normal Tiered release shell.
All five required physical site tasks passed from an empty profile before loading an init file.
GitHub's ordinary source click, Back, and Forward passed with zero loading placeholders.
Live init reload, reader mode, introspection, REPL history, interruption, mixed panes, and parked shutdown also passed.
All sixteen local repeat commands passed, including both native suites, four nREPL runs, and standalone AOT.
Startup had a 0.230-second median across seven samples, including a 2.458-second first sample.
During that physical repeat, the two-core Linux host was unreachable after two bounded SSH attempts.
The subsequent small-host follow-up records the restored connection and current test failures.
That repeat retained the earlier Option-as-Alt behavior. The later Option setting supersedes that keyboard policy.

The 2026-11-21 [preference decision](phase-2-preference-evaluation.md) starts from `c8a3bdc`, equal to fetched `origin/main`.
The product now enables IntersectionObserver and adopted stylesheets on the unchanged Servo `0.7.0` pin.
The engine correction path uses a future tagged upstream release containing the measured radio optimization.
Servo `0.8.0` predates that optimization. The experimental backport remains a diagnostic candidate.
All fourteen release comparisons completed their heartbeats and inspections across the five required sites and two additional URLs.
GitHub's placeholders fell from 83 to zero. Amazon displayed a gate, so its storefront performance remains unverified.
Three named Wikipedia banner repeats completed all heartbeats, with maximum delays of 302–766 ms.
After explicit Terms of Use approval, physical BBC article, scroll, and reload tasks passed for the current lead and original failing URL.
Native close returned zero, joined the runtime worker, and removed the port file.
The [preference correction CI](https://github.com/csm/bulwark/actions/runs/44986893459) passed all four jobs at `4d406d6`.
Linux passed runtime, Servo, and standalone AOT checks. The macOS runtime job also passed.

The 2026-11-21 [compatibility follow-up](phase-2-compatibility-follow-up.md) started from `1ef0723`, then equal to fetched `origin/main`.
That checkpoint integrated the work from `codex/phase-2-compatibility` into `main`.
Phases 0 and 1 retain owner acceptance. Phase 2 remains open. Phase 3 has not started.

The Linux REPL-completion failure reproduced in the output-truncation fixture.
A chunked fixture produces the same 20,000-character result and passes with the original five-second deadline.
The production scheduler and bounds did not change. Failures now include stage labels and bounded diagnostic context.
The corrected REPL check passed in both Linux evaluator modes.
Full portable runs on the small Linux host still failed separate browser-history, introspection, and embedded-shell deadlines.
The current Linux CI run passed. The separate small-host deadline failures remain unresolved.

The IntersectionObserver comparison covers the five required sites and both upstream performance URLs.
The [Wikipedia investigation](phase-2-wikipedia-investigation.md) reproduced a 34-second reply delay that later recovered.
A thread sample locates the pause in timer-driven radio-input validation, alongside a fundraising banner.
A local reduction reproduces the same expensive path with IntersectionObserver disabled and enabled.
This separates the measured bottleneck from observer intersection computation. That investigation left the product preference disabled.
The [engine evaluation](phase-2-engine-optimization.md) measures an isolated adaptation of upstream Servo PR #49855.
Median radio insertion improves by 55.6–56.1% in debug and 32.8–38.8% in release.
Release insertion remains 91–94 ms; both engines retain the same 20 existing validity failures across 220 checks.
Both release engines completed the same named fundraising-banner preview with all 44 heartbeats and three inspections.
That single live pair does not establish a stable speedup or cover every banner variant.
That engine evaluation left the product pin and preferences unchanged.
The earlier [BBC investigation](phase-2-bbc-investigation.md) identifies the missing WebCrypto feature and the later fatal advertising-placeholder observer call.
WebCrypto remains enabled. The earlier consented BBC physical task failed under the default observer preference.

The [physical follow-up](phase-2-physical-follow-up.md) passed active-window GitHub source navigation, Back, Forward, and link completion.
Initial clicks before keyboard activation still left the repository unchanged.
That physical round retained actual GitHub loading placeholders after document completion.
A controlled probe clears all 83 placeholders only after enabling adopted stylesheets and IntersectionObserver.
Adopted stylesheets alone expose the next missing-IntersectionObserver exception. That physical follow-up left product preferences unchanged.
The link selector now inspects up to 4,096 anchors, deduplicates URLs, and bounds entries and output text.
Its previous 100-anchor cutoff no longer excludes Cargo.toml solely because of its position.
After unlocking the Mac, the owner requested physical checks.
REPL truncation, history, interruption, page focus restoration, mixed panes, owned-job close, and native shutdown passed.

Clojurust issues #500 and #501 are closed upstream. The product pin remains `540e4218` with both AOT workarounds.
Rust owns native mechanisms and bounded transport. Clojure owns commands, state, bindings, and browser policy.
The installed inventory retains 25 documented natives. Only one webview is visible per native window.
Text and REPL panes support splits.

## Current checks

The Option and Escape settings passed 61 portable tests, 20 native unit tests, and the Servo contract locally.
Both 16-case native process suites, both software Servo modes, both native nREPL modes, and standalone pinned AOT passed.
Portable and native Clippy passed with warnings denied.
The [Option report](phase-2-option-mode.md) records the physical checks and their limits.
The [Option-setting CI](https://github.com/csm/bulwark/actions/runs/45087538289) passed all four jobs at `88037e1`, before the Escape follow-up.

The latest repeat passed all 58 portable tests, all 17 native unit tests, and the Servo contract.
Both 16-case native process suites, both Servo fixture modes, all four nREPL runs, and standalone pinned AOT passed again.
Portable and native Clippy passed. No deadline, assertion, engine preference, or key translation changed.
The [repeat report](phase-2-repeat-checks.md) retains every startup sample and the physical input qualifications.

The preference correction passed all 58 portable tests, all 17 native unit tests, and the Servo contract.
The local fixture passed actual observer callbacks and stylesheet adoption in both evaluator modes.
Both native process suites passed all 16 cases. Portable and native Clippy passed with warnings denied.
The native release build passed. The [decision report](phase-2-preference-evaluation.md) retains the commands, measurements, and limits.

The earlier compatibility checkpoint also passed the bounded link fixture in both evaluator modes.
Its fresh native GitHub query included Cargo.toml among 147 unique entries.
Formatting, whitespace, and Python compilation passed for the current correction.
The observer diagnostic now retains repeated runs and optional thread samples. Its local reduction and controls passed the diagnostic checks.
The isolated radio optimization passed the focused equivalence comparison, preparation checks, and final probe Clippy.
The [engine report](phase-2-engine-optimization.md) records the release timings and live banner coverage.
The [main CI run at `c8a3bdc`](https://github.com/csm/bulwark/actions/runs/44985004958) passed all four jobs.
The [preference correction CI at `4d406d6`](https://github.com/csm/bulwark/actions/runs/44986893459) also passed all four jobs.
Linux passed both Servo fixture modes, both native process suites, and both native nREPL modes.
The Linux runtime job passed the corrected REPL truncation regression without a deadline change.
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

1. Complete the owner exit interview against the current acceptance review. Owner acceptance remains pending.
   Keep inactive-window input and icon rendering limits distinct from the passing named site workflows.
2. Keep the small Linux startup and introspection investigation deferred under the owner's desktop focus.
   Retain the failed baseline, passing current browser-history repeat, and earlier timing variation.
3. Evaluate a fixed Clojurust pin for closed issues [#500](https://github.com/csm/clojurust/issues/500) and [#501](https://github.com/csm/clojurust/issues/501).
   Remove the two AOT workarounds only after that pin passes their regressions.

E5 remains deferred beyond Phase 2 under the accepted single-visible-webview scope.
Basic Mac Option composition passed. Broader IME, clipboard, display-scale transitions, and interactive Linux behavior remain unverified.
