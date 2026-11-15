# Phase 2 acceptance review

Date: 2026-11-14. P2.8 evidence is recorded. Phase 2 remains open.
B2 does not pass: BBC News produces a client-side exception page.
GitHub source navigation also needs the workaround recorded here.
No site substitution or scope exception is accepted by this report.
Owner acceptance remains outstanding.

The subsequent [BBC investigation](phase-2-bbc-investigation.md) identified a missing Bulwark build feature and tested a correction.
Its diagnostic results do not replace the physical acceptance results below.

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
| B1: preserve Phase 1 | Passed the current automated regression checks. Earlier accepted physical input and stall evidence remains in the Phase 1 review. |
| B2: default browser | Open. Default workflows pass, but BBC fails and GitHub needs a navigation workaround. |
| B3: live configuration | Passed on live Wikipedia. Reload changed the binding and enabled reader mode without restart or duplicate hooks. |
| B4: introspection | Passed. The installed inventory contains 25 documented native functions. Physical apropos opened a text buffer. |
| B5: policy boundary | Passed the source review below. Native additions provide mechanisms. Clojure retains browser policy. |
| B6: startup | Recorded. Seven release samples have a median of 0.265 seconds under the conditions below. |
| B7: language coverage | Passed interpreter assertions and the standalone pinned AOT binary locally. The P2.8 Linux CI also passed both paths. |
| Other Phase 2 deliverables | Automated checks passed in both modes. Physical mixed-pane REPL focus, interruption, close, and native shutdown passed. |

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

## Exit disposition

P2.8 now has physical macOS workflows, live-site results, release measurements, and a native policy review.
The required BBC task still fails, and GitHub retains the documented navigation limits.
Phase 2 cannot close under the agreed site scope.
The next work must diagnose compatibility or obtain an explicit scope decision before any failed task changes.
Owner acceptance follows the required passing criteria. This report does not supply that acceptance.

E5 remains deferred beyond Phase 2 under the owner's single-visible-webview scope decision.
It remains necessary before simultaneous live web splits can be claimed.
IME, clipboard, display-scale transitions, and interactive Linux behavior remain unverified.
The Option-as-Alt policy remains unresolved. `C-c x` supplies the palette fallback.
The two Clojurust AOT workarounds remain until a fixed pin passes their regressions.
