# Phase 2 acceptance review

Status: complete and accepted by the owner, 2026-11-22. Initial review: 2026-11-14.
The latest named five-site tasks passed after the compatibility corrections.
The original BBC failure and GitHub workaround remain historical evidence in this report.
No site substitution or scope exception is accepted by this report.
The owner accepted the current desktop scope, Linux deferral, and documented input limits.

## Current exit checkpoint

The product checkpoint is `cf3e6be` on `codex/phase-2-option-mode`.
The [keyboard report](phase-2-option-mode.md) records the owner's final Option and Escape decisions, local checks, and physical Mac evidence.
Mac Option defaults to native character input, with a live Meta setting.
Escape cancels an open prompt immediately, including during another prefix. Outside prompts, Escape prefixes Meta.
Local checks passed 61 portable tests, 20 native unit tests, the Servo contract, both native process suites, and standalone pinned AOT.
Both software Servo and native nREPL modes passed. Formatting and portable/native Clippy passed.
The [final keyboard CI](https://github.com/csm/bulwark/actions/runs/45107213807) passed all four jobs at `cf3e6be`.
Linux and macOS runtime checks passed. Linux Servo/native checks and standalone AOT also passed.
No required keyboard validation remained at the owner interview.

The five-site physical repeat used product source `4d406d6` before the keyboard changes.
The keyboard follow-up used local page, prompt, and REPL tasks. It did not repeat all five live-site workflows.
The owner deferred small Linux startup and introspection investigation to focus on desktop behavior.
Those failures remain unresolved and do not establish a minimum hardware specification.
Basic Option symbols and acute-accent composition passed on the Mac. Full IME language coverage remains unverified.
Clipboard, display-scale transitions, interactive Linux keyboard behavior, and initial inactive-window input retain their recorded limits.
Focus changes cancel pending prefixes. The keyboard report records the burst sent during unfinished REPL activation.
Long synthetic input bursts can reach the existing fatal event-overflow contract. The original physical report retains that reproduction.
E5 remains deferred under the accepted single-visible-webview scope.
The two Clojurust AOT workarounds remain until a fixed pin passes their regressions.

## Owner acceptance, 2026-11-22

During the exit interview, the owner reported:

> There are no blockers according to my experience with the app

The acceptance question retained the single-visible-webview scope, the Linux deferral, and the documented input limits.
The owner then accepted Phase 2:

> Yes, complete from my perspective

Phase 2 is complete at product checkpoint `cf3e6be` with the evidence and limits recorded in this review.
This acceptance does not claim simultaneous live web splits or support for the unverified input cases.
The small Linux failures and two AOT workarounds remain follow-up work.
Phase 3 implementation has not started. The next step is Phase 3 planning and the E4 DOM-channel evaluation.

## Earlier acceptance evidence

The subsequent [BBC investigation](phase-2-bbc-investigation.md) identified a missing Bulwark build feature and tested a correction.
Its diagnostic results do not replace the physical acceptance results below.
The [physical follow-up at `833ad5f`](phase-2-bbc-investigation.md#physical-follow-up-after-consent) still failed after consent.
The Linux runtime CI at that checkpoint also failed a REPL-completion check.
The [controlled continuation](phase-2-bbc-investigation.md#controlled-diagnosis-of-the-consented-failure) identifies BBC's unguarded IntersectionObserver constructor as a fatal path.
Enabling Servo's experimental preference resolves that probe case. This diagnostic result does not close physical acceptance.

The [2026-11-21 follow-up](phase-2-compatibility-follow-up.md) corrects the REPL truncation fixture and the link inventory cutoff.
It also records an intermittent Wikipedia response failure with IntersectionObserver enabled.
That checkpoint retained the disabled observer preference. Physical site acceptance and current Linux CI remained open.
The later [physical follow-up](phase-2-physical-follow-up.md) passed active-window GitHub source navigation, link completion, REPL checks, and native shutdown.
That physical round retained GitHub placeholders and left BBC acceptance and current Linux CI open.

The later [preference decision](phase-2-preference-evaluation.md) enables IntersectionObserver and adopted stylesheets on the unchanged engine pin.
It records the broader release comparison and three named Wikipedia banner repeats.
The consented physical BBC article, scroll, and reload tasks passed for the current lead and original failing URL.
The [CI run at `4d406d6`](https://github.com/csm/bulwark/actions/runs/44986893459) passed all four jobs after the preference correction.
Linux passed runtime, Servo, and standalone AOT checks. The macOS runtime job also passed.
The later [repeat checks](phase-2-repeat-checks.md) passed all five named physical site tasks, live configuration, introspection, and REPL checks.
All local automated suites passed again. Startup had a 0.230-second median, with a 2.458-second first sample retained.
The required five-site scope remains unchanged. The later keyboard follow-up resolved Option-as-Alt and Escape behavior.
The owner deferred the small-host investigation and accepted Phase 2 after the keyboard follow-up.
The [small Linux follow-up](phase-2-small-linux-follow-up.md) restored connectivity and passed browser history and the corrected REPL regression in both modes.
Embedded-shell startup and TreeWalk introspection still failed their existing deadlines. The complete small-host portable suite did not pass.

## Source and method

The source baseline is `d628a87`, after the P2.7 implementation at `9835f77`.
The P2.8 code checkpoint is `689ba2d` on `codex/phase-2-acceptance`.
This change adds a native presentation diagnostic and a release measurement runner.
It changes no browser policy, key translation, dependency pin, or queue limit.
Clojurust remains at `540e4218`. Servo remains at `0.7.0`.

Physical macOS checks used the normal Tiered shell in a temporary app bundle.
The bundle contained a byte-identical copy of `target/debug/bulwark-shell`.
Its SHA-256 was `d389d846c8c6ec950416d569aad08a26fe2382a04a05ecbe7757b99cfe3aba87`.
The launcher supplied an isolated profile and opt-in loopback nREPL for observations.
Computer Use supplied the keys, pointer input, and scroll actions.
nREPL recorded state and supplied a disposable command redefinition.
No dispatcher call supplied a physical result.

The profile started without an init file.
The configuration session added the unmodified `examples/init.cljrs` after the Wikipedia task.
The later process started with that file absent and no retained application state.
Hacker News, GitHub, MDN, BBC, and the final REPL checks used this no-init process.

The keyboard layout was Dvorak.
Key names in this report describe logical strokes, as in the earlier physical reports.
Computer Use attachment took several minutes. That delay is outside the startup measurements.

Local evidence resides in `target/phase-2-acceptance-evidence/`.
It includes raw nREPL observations, process logs, binary hashes, release samples, and composed PNGs.
`observations.jsonl` contains the physical-session observations.
`configuration-and-repl.log` retains the reproduced overflow outcome.
`physical.log`, `parked-client.json`, and `exit-status` contain the final native-close outcome.
`frames/` contains automated fixture frames. `live/` contains separate release captures of the site URLs.
Those release captures establish rendering at capture time, not additional physical task results.

## Acceptance matrix

| Criterion | Result |
|---|---|
| B1: preserve Phase 1 | Current CI passed all four jobs at `cf3e6be`. Earlier physical input and stall evidence remains in the Phase 1 review. The owner deferred the separate small-host deadline failures. |
| B2: default browser | Passed all five named physical workflows in the latest repeat. Initial inactive-window behavior and icon rendering limits remain separate. |
| B3: live configuration | Passed again on live Wikipedia. Reload changed the binding and applied reader mode without a new document or duplicate hooks and styles. |
| B4: introspection | Passed. The installed inventory contains 25 documented native functions. Physical apropos opened a text buffer. |
| B5: policy boundary | Passed the source review below, including the keyboard follow-up at `cf3e6be`. Native additions provide mechanisms. Clojure retains browser policy. |
| B6: startup | Recorded again. The latest seven release samples have a 0.230-second median, including a 2.458-second first sample. The target remains informational. |
| B7: language coverage | Interpreter assertions and the standalone pinned AOT binary passed again locally. Linux CI passed both paths at `cf3e6be`. |
| Other Phase 2 deliverables | Automated checks passed in both modes. Physical mixed-pane REPL focus, interruption, close, and native shutdown passed. |

## BBC physical follow-up at `833ad5f`

The [follow-up report](phase-2-bbc-investigation.md#physical-follow-up-after-consent) records the 2026-11-14 check with WebCrypto enabled.
Physical address entry loaded BBC News. The user approved the Terms of Use acceptance before the physical button click.
The article link opened the original article URL, then the client-side exception replaced its content.
Physical reload reproduced the exception after a brief article render.
Physical scrolling left the error page at offset zero. The required BBC task still failed.
Read-only observations confirmed that `crypto.getRandomValues` remained available.
No diagnostic script, user-agent override, or programmatic page action supplied this result.

The [latest CI run](https://github.com/csm/bulwark/actions/runs/43680513338) passed the Servo, macOS runtime, and standalone AOT jobs.
Its Linux runtime job failed `internal_repl_jobs_are_bounded_and_owned_in_both_modes` with `Timed out: REPL completion`.
The original P2.8 evidence below remains historical evidence from its recorded source checkpoint.

## Physical default workflows and internal REPL

| Check | Observed result |
|---|---|
| Address entry | `C-l` accepted a local fixture URL and the live-site URLs. |
| Retained input | The fixture retained `kept!` across REPL and page selection. Its key log excluded REPL source and reserved command strokes. |
| Page focus | Buffer completion restored document focus. Subsequent native text extended the retained input from `kept` to `kept!`. |
| New buffers | Default new-buffer, link-opening, and buffer-completion commands retained independent pages. |
| Evaluation and output | Typed source printed `physical-repl` and returned `42`. An evaluation error appeared with `<internal-repl>:1`. |
| History and draft | `C-p` recalled the earlier form. `C-n` restored the unfinished draft. Both checks also passed in the later no-init process. |
| Live commands | A command defined in the internal REPL appeared in the palette and displayed `physical v1`. An nREPL redefinition changed its next physical invocation to `physical v2`. |
| Namespace isolation | Physical `(ns user.physical)` changed the internal prompt. Its `marker` value was readable through nREPL, whose namespace stayed `user`. |
| Mixed layout | `C-x 3` displayed Hacker News beside the REPL. A pointer press selected the REPL and exposed its editable control. |
| Interruption | `C-c C-c` cancelled a parked evaluation and displayed `[Interrupted]`. The next physical evaluation returned `92`. |
| REPL-buffer close | `C-x k` killed a REPL with job 54 parked. The buffer and session disappeared. Polling the job reported `Stale evaluation job`. nREPL still evaluated normally. |
| Native close | The close button stopped a different internal job and a parked nREPL client. The worker joined, the client reached EOF, the port file disappeared, and the process returned zero. |

The final close logged 1,938 painted frames and 2,498 presentations.
These totals include attachment and observation delays. They do not measure performance.
`final.png` shows the parked internal evaluation before native teardown.

### Input burst limit

Two single Computer Use typing calls, each with a 158-character source line, ended the process during input.
The retained log from the reproduction reports `Error: event-overflow` and `shutdown=joined`.
The process returned status one.
The first session's log was overwritten on restart. Its earlier observations remain in `observations.jsonl`.

The ordered event queue retains 256 events.
The [D1 contract](adr/0001-shell-thread.md#product-queue-contract) requires explicit shutdown on overflow, without silent event loss.
This run preserves that contract and records the input-burst limitation.
Shorter input after interruption passed, including the evaluation that returned `92`.
This review does not establish an input-rate limit or clipboard support.

## Five live-site tasks

All checks occurred on 2026-11-14 with the pinned Servo engine.
These results describe the named tasks, not complete site compatibility.

| Site and URLs | Task and result |
|---|---|
| [Wikipedia Rust article](https://en.wikipedia.org/wiki/Rust_%28programming_language%29), [Mozilla](https://en.wikipedia.org/wiki/Mozilla) | Passed. The article rendered, a physical internal-link click opened Mozilla, Back restored Rust, and Forward restored Mozilla. |
| [Hacker News](https://news.ycombinator.com/), [story destination](https://www.crissyfield.de/research/lovable-en/) | Passed. `C-c l` opened the story in a background buffer. Buffer completion selected it and returned to the same listing, with all 30 entries retained. |
| [GitHub repository](https://github.com/servo/servo), [Cargo.toml](https://github.com/servo/servo/blob/main/Cargo.toml) | Qualified result. The public repository and source file rendered without authentication. A normal source-link click did not navigate. Address entry loaded the source, including `[workspace]`. |
| [MDN Array.map](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/map) | Passed. Find selected `callbackFn` and scrolled to it. Next-match advanced the selection. Zoom changed to 1.1, and reset restored 1.0. |
| [BBC News](https://www.bbc.com/news), [direct article](https://www.bbc.com/news/articles/cwvg2l312ylqo) | Failed. Both URLs produced `Application error: a client-side exception has occurred`. Reload reproduced the exception. Article reading and scrolling did not pass. |

The first Hacker News story attempt used `https://retrocomputing.stackexchange.com/questions/17957`.
That destination showed a Cloudflare challenge. No challenge completion occurred.
The later story loaded, and the original listing remained live throughout both attempts.

On GitHub, the two Cargo.toml anchors had zero-based indexes 140 and 141.
The default link selector inspects the first 100 anchors, so those anchors fall outside its inventory.
The source page itself renders through address entry.
The click-navigation failure and selector limit remain explicit compatibility limits.

MDN's JavaScript viewport width changed from 1,000 to 909 pixels at 110% zoom.
Reset restored 1,000 pixels. The selected text stayed `callbackFn`.
Next-match moved the selection's anchor offset to 20.

The BBC exception appeared before a later Terms of Use overlay.
No acceptance action occurred on that overlay.
The direct article and reload also failed, so the required article task remains incomplete.
This acceptance run did not identify the failing site script or missing engine feature.
The subsequent [investigation](phase-2-bbc-investigation.md) identifies the missing WebCrypto feature and records the remaining Servo limitations.
No alternate site, disabled script, user-agent override, or transformed page supplies a passing result.

Several pages also displayed solid black icon placeholders or incomplete navigation chrome.
These visual limits do not prevent the recorded Wikipedia, Hacker News, and MDN tasks.

## Live configuration and introspection

Physical `reload-init` loaded the documented example on the live Mozilla Wikipedia page.
`C-x b` changed to `user.init/choose-buffer`.
The selected buffer gained `:wikipedia-reader`. Its header computed to `display: none`.
A second physical reload retained one reader hook and one `bulwark-reader` style element.
The caller namespace remained `user`.
The [P2.5/P2.6 report](phase-2-shell-acceptance.md) retains the earlier broken-init, Messages, and recovery checks.
Both current native suites also passed those deterministic configuration assertions.

Physical `apropos` with `webview` opened read-only `*Help*` text.
All 14 natives tagged with the `webview` resource appeared.
`allocate-id!` also matched because its documentation mentions webview handles.
Thus the native subset of the query contains 15 entries, rather than only the resource-tagged set.
The installed inventory and metadata assertions passed in the portable suite.

## Native policy review

The review covered `bulwark-runtime/src/native.rs`, `evaluation.rs`, the Servo adapter, native input, and frame rendering.
It also covered the source-loader registrations in `bulwark-runtime/src/lib.rs`.
The eleven Phase 2 native additions have the following roles:

| Function | Mechanism and policy boundary |
|---|---|
| `allocate-id!` | Allocates process-wide identities. Clojure decides which resources need them. |
| `set-ui!` | Validates and applies bounded geometry, widgets, focus, and revisions. Clojure supplies the layout and content. |
| `ui-viewport` | Returns native dimensions and scale. Clojure computes the layout. |
| `read-source` | Reads bounded UTF-8 forms with source locations. Clojure chooses the path and evaluates the forms. |
| `webview-zoom` | Applies an explicit bounded factor. Clojure chooses increments and retains the factor. |
| `webview-accept-popup` | Consumes a deferred engine request and preserves its opener. Clojure chooses acceptance, buffer publication, and selection. |
| `webview-deny-popup` | Consumes a deferred request without a view. Clojure supplies the denial decision. |
| `evaluation-start` | Admits bounded trusted source under an explicit owner and namespace. Clojure supplies source, ownership, and submission policy. |
| `evaluation-poll` | Drains bounded output and terminal status. Clojure supplies transcript retention and presentation. |
| `evaluation-interrupt` | Requests cancellation for one explicit job. Clojure chooses the job and binding. |
| `evaluation-close` | Cancels one owner's jobs. Clojure associates owners with REPL buffers. |

Rust retains numerical bounds, stale-handle checks, resource cleanup, thread ownership, and overload behavior.
Popup request expiry and admission bounds protect retained native responders.
The engine callback does not decide foreground/background selection or wait for Clojure.
The evaluation scheduler has no command names, bindings, history, or transcript policy.
The new presentation marker reports a native event after presentation. It supplies no browser decision.

The source search was:

```sh
rg -n -i 'wikipedia|hacker.?news|github|bbc|mozilla|mdn|C-x|C-c|M-x|reader|popup-policy|new-buffer|switch-buffer|kill-buffer|find-text|zoom-in' crates --glob '*.rs'
```

The eleven matches are recorded in `policy-search.log`.
They consist of two Mozilla license lines, six `cljrs_reader` references, and three embedded Clojure test lines.
The test lines exercise arbitrary binding publication. They do not install product defaults.
Loader registrations for `bulwark.default.*` embed Clojure source and identify namespaces.
Native names such as `reload`, `back`, and `forward` identify explicit engine operations.
These matches do not move browser policy into Rust.

### Keyboard follow-up review at `cf3e6be`

The installed inventory remains 25 natives. `set-key-routes!` now accepts optional Option and Escape settings with the bounded reservation snapshot.
Its two- and three-argument forms retain omitted settings. Refusal preserves the previous snapshot.
Clojure chooses the live Option setting and whether an input context permits Escape Meta.
Rust applies the native Option interpretation, reserves prefix tails, converts composition events, and preserves held-key ownership.
The `Escape` literal implements the published prefix mechanism. It installs no default command binding.
The native Escape flag defaults to false until Clojure publishes its policy.

The same source search now returns 18 matches, retained in `target/option-mode/exit-review/policy-search.log`.
They comprise nine test lines, six reader API references, two Mozilla license lines, and one comment explaining global engine preferences.
The comment names BBC and GitHub. The engine enables those interfaces globally and contains no domain condition.
The test lines publish or inspect arbitrary reservations. They do not install product defaults.
No keyboard change moves command names, prompt decisions, or site policy into production Rust.

## Release startup measurements

Hardware: MacBook Pro `Mac15,9`, Apple M3 Max, 16 cores (12 performance and four efficiency), 48 GB RAM.
Software: macOS 26.5.2 (`25F84`), Rust 1.97.1, native release build with default Cargo release settings.
The release executable SHA-256 was `a73c51767df38548ec23c9f99c4dcffdb98b99ce48fa1e1e53171387e2856d6d`.

```sh
cargo build --locked --release -p bulwark-shell
python3 experiments/measure_product_startup.py > target/startup.json
```

Each sample starts a separate process at `about:blank` with a new empty profile and no nREPL listener.
The parent monotonic clock starts before process creation.
When the parent receives `Bulwark native: first-content-presented` through the output pipe, the clock stops.
The host emits that marker once, after a presentation with a painted webview and a runtime UI description.
The interval includes process creation, dynamic loading, bootstrap, native presentation, and pipe-delivery delay.
It measures the presentation call, not physical display scanout or live-site load completion.

The final run occurred after compilation and regression suites ended.
Filesystem and shader caches were warm from earlier runs. This is not a cold-machine benchmark.
The ordinary desktop applications remained open.
No sample was discarded, and the runner required one marker, status zero, and a joined worker for every process.

| Sample | Seconds |
|---|---:|
| 1 | 0.259894 |
| 2 | 0.260018 |
| 3 | 0.272053 |
| 4 | 0.287554 |
| 5 | 0.259610 |
| 6 | 0.280199 |
| 7 | 0.264579 |
| Median | 0.264579 |

The median is below the informational 1.5-second target under these conditions.
The earlier exploratory run had a 0.350435-second median and a 2.277616-second first sample.
That run overlapped native checks. `startup-initial.json` retains all its samples.
`startup.json` contains the final samples and their raw process output.

## Automated macOS checks and Linux CI

| Current local check | Result |
|---|---|
| Portable workspace | 58 tests passed. |
| Native unit tests and Servo contract | 17 tests and the contract passed with display access. |
| Native process suites | All 16 cases passed in each evaluator mode. The browsing case also requires the new presentation marker. |
| nREPL suites | Mock and native suites passed in both modes, including overload, interruption, live commands, cancellation isolation, and port cleanup. |
| Foundation and software Servo | Both evaluator modes passed. |
| Standalone AOT | The pinned compiler produced and ran `target/shell-model-aot`. Shared shell assertions passed. |
| Formatting and native Clippy | Passed. |
| Measurement runner | Rejected missing/duplicate markers, missing join, nonzero exit, and process timeout. A valid sample passed. |

The initial sandboxed Servo contract did not create its rendering surface and panicked in Surfman.
The same test passed with native display access. That failed attempt is not product acceptance evidence.

The [P2.8 branch CI run](https://github.com/csm/bulwark/actions/runs/43670796887) passed all four jobs at `689ba2d`.
Linux and macOS passed the portable suite and mock nREPL modes.
Linux passed native/Servo Clippy, unit tests, the contract, and both software checks.
Both Linux native process modes passed, including the new presentation assertion.
Both native nREPL modes and the standalone AOT job also passed.
These results do not establish interactive Linux behavior.
The earlier [P2.7 main CI run](https://github.com/csm/bulwark/actions/runs/43629036365) passed at `9835f77`.

## Original exit disposition and current status

P2.8 now has physical macOS workflows, live-site results, release measurements, and a native policy review.
At the original review, BBC failed and GitHub retained navigation limits. Those failures prevented closure under the agreed site scope.
The later compatibility corrections and physical repeat passed all five required workflows without a site substitution.
The keyboard follow-up resolved the owner's Option and Escape decisions.
The owner deferred the separate small-host investigation and completed the exit interview on 2026-11-22.
The owner accepted Phase 2 with the documented scope and limits. Phase 2 is complete.

E5 remains deferred beyond Phase 2 under the owner's single-visible-webview scope decision.
It remains necessary before simultaneous live web splits can be claimed.
Basic Mac Option composition passed. Broader IME, clipboard, display-scale transitions, and interactive Linux behavior remain unverified.
`C-c x` supplies the palette fallback in either Option mode. `Escape x` invokes the palette outside prompts.
The two Clojurust AOT workarounds remain until a fixed pin passes their regressions.
