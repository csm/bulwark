# Phase 2 repeat checks

Date: 2026-11-21. Checkout: `f695272` on `codex/phase-2-preference-evaluation`.
The tested product source remains `4d406d6`. This round changes no product code, dependency pin, preference, or key translation.
Phase 2 remains open for the owner review and the remaining decisions and limits.
Phase 3 has not started.

## Method

The physical round used the normal Tiered release shell in `/tmp/BulwarkPhase2Repeat.app`.
The bundle contained a byte-identical copy of the current release executable.
Its SHA-256 was `13ce7f88dd63b74fcb329cdade0a4d55dd017db660c7cc395964a18df8e9d829`.
The profile started empty, without an init file or saved site state.
All five default site tasks finished before the example init loaded.
WebCrypto, IntersectionObserver, and adopted stylesheets retained the evaluated product configuration.

Computer Use supplied physical keys, clicks, wheel input, and native close.
Loopback nREPL supplied read-only observations and one disposable parked client for the shutdown check.
No dispatcher call or programmatic page action supplied a physical site result.
The browser used no diagnostic page script, API shim, or user-agent override.
The keyboard layout remained Dvorak. Computer Use attachment took approximately two minutes.
That attachment delay is outside the browser checks and startup samples.

## Five-site results

| Site | Repeated physical task | Result |
|---|---|---|
| GitHub | Read the Servo repository, click Cargo.toml, then use Back and Forward. | Passed. Source displayed `[workspace]`. Repository and source each had zero skeleton elements. Back restored repository offset 792. |
| Wikipedia | Open Rust, click Mozilla, then use Back and Forward. | Passed. Each command restored the expected article. |
| Hacker News | Use link completion to open a story in a background buffer, select it, then return to the listing. | Passed. The original listing retained all 30 stories and its document time origin. |
| MDN | Open Array.map, find `callbackFn`, increase zoom, then reset zoom. | Passed. Find selected the text. Zoom changed to 110 percent, with viewport width 909, then reset to width 1,000. |
| BBC News | After approved consent, click the lead article, scroll, reload, and scroll again. Repeat the original failing URL. | Passed for both articles. Each reached offset 1,584 before reload and 792 afterward. Each reload created a new document without the application-error screen. |

GitHub used `https://github.com/servo/servo` and its `/blob/main/Cargo.toml` source page.
Wikipedia used the Rust programming-language article and `https://en.wikipedia.org/wiki/Mozilla`.
Hacker News retained `https://news.ycombinator.com/` while its story used `https://leanprover-community.github.io/blog/posts/riemann-hypothesis/`.
MDN used `https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/map`.
BBC used the current lead, `https://www.bbc.com/news/articles/cq7827z2841zo`, and the original URL, `https://www.bbc.com/news/articles/cwvg2l312ylqo`.
The owner approved BBC's Terms of Use button for this new isolated profile before the physical click.

The GitHub window was active during the source workflow.
This round does not resolve the earlier inactive-window click behavior.
Some site icons still appeared as solid blocks. These task results do not establish complete visual fidelity or site compatibility.
BBC still logged third-party initialization errors, but its final article observations reported `readyState=complete` and no application-error screen.

## Configuration, introspection, and REPL

The documented `examples/init.cljrs` loaded through the physical command palette after the default site tasks.
It changed `C-x b` from `bulwark.browser/switch-buffer` to `user.init/choose-buffer`.
The new binding physically selected a retained page.
Reader mode changed the Wikipedia header display from `flex` to `none` without a new document.
A second reload retained one reader style and one registered reader hook.

Physical apropos opened the text Help buffer with native webview documentation.
The installed inventory retained 25 documented natives.
A read-only comparison found every webview-tagged operation in the apropos results.

Physical REPL input returned `42`.
Previous-input recalled the source. Next-input restored the unfinished `(+ 1` draft.
A clean parked evaluation responded to `C-c C-c`, displayed `[Interrupted]`, and then evaluated another form to `42`.
The page and REPL appeared side by side. A pointer press selected the REPL pane and exposed its editable input.

The punctuation-key attempt did not open Find under Dvorak.
The literal punctuation token returned a Computer Use `keyNotFound` error.
Find and zoom therefore used the physical `C-c x` command palette.
This result does not establish the cause of the named-key attempt or change the Option-as-Alt policy.

An input-clearing attempt used move-to-start instead of select-all.
It left the unfinished draft in the submitted form, which produced a read error.
The next submission started from an empty prompt and passed interruption and recovery.
The transcript retains both attempts.

Native close stopped a parked internal evaluation and a separate parked nREPL client.
The process returned zero, joined its runtime worker, and removed the profile port file.
The parked client received EOF.

## Automated repeats

All sixteen recorded commands returned zero.

| Repeated check | Result |
|---|---|
| Portable workspace | All 58 tests passed. |
| Native unit tests and Servo contract | All 17 unit tests and the contract passed. |
| Local Servo fixture | Passed in TreeWalk and Tiered modes. |
| Native process suites | All 16 cases passed in each evaluator mode. |
| Mock and native nREPL | Passed all four runs across both evaluator modes. |
| Standalone pinned AOT | Shell model assertions passed with the existing workarounds. |
| Clippy | Portable and native checks passed with warnings denied. |
| Native debug and release builds | Passed. |

No assertion, production bound, or test deadline changed for these repeats.
The source remains the same source that passed all four GitHub Actions jobs at `4d406d6`.

## Release startup repeat

The host was an Apple M3 Max MacBook Pro with 48 GB memory, macOS 26.5.2 build `25F84`, and Rust 1.97.1.
Seven processes each used a new empty profile at `about:blank`, without an init file or nREPL.
The parent measured process creation to receipt of the first page/chrome presentation marker.
All builds and browser checks finished before the timing series.
The ordinary desktop remained active. Filesystem and shader caches were not cleared.
These are not controlled cold-start measurements.

Samples, in seconds: 2.458353, 0.213905, 0.227471, 0.235764, 0.212503, 0.229844, 0.234564.
The median was 0.229844 seconds.
The first sample took 2.458 seconds. The remaining six samples ranged from 0.213 to 0.236 seconds.
No sample was discarded. The cause of the first sample's delay remains unmeasured.
The 1.5-second target remains informational. These samples do not establish a worst-case startup bound.
Every process returned zero, produced one presentation marker, and joined its worker.

## Remaining limits and evidence

The existing two-core Linux host did not respond to either SSH connection attempt.
Each attempt used a ten-second connection timeout and no agent forwarding.
No small-host test ran in this round.
The earlier browser-history, introspection, and embedded-shell deadline failures remain unresolved.
The passing GitHub Actions run at `4d406d6` does not establish their causes.

The source review retained the Rust/Clojure policy boundary.
The focused Rust search found a reservation-publication comment and embedded Clojure test source.
It found no new Rust binding, reader rule, or browser command policy.
The tested engine change only enables two generic engine interfaces.

Option-as-Alt remains unchanged for the owner discussion.
Both Clojurust AOT workarounds remain on pin `540e4218`.
IME, clipboard, display-scale transitions, and interactive Linux behavior remain unverified.
This report supplies no owner acceptance and does not close Phase 2.

Local evidence resides in `target/phase-2-repeat-20261121/`.
`physical/` contains binary hashes, complete observations, a page summary, the session record, and shutdown evidence.
`checks/` contains each repeated command, status, raw log, fixture frame, and startup sample.
`host.json`, `policy-search.log`, and `linux-host-retry.json` retain the host and source-review evidence.
Browser state remains outside the repository.
