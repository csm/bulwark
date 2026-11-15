# BBC Phase 2 compatibility investigation

Date: 2026-11-14. Baseline: `5d2b40f`, equal to the fetched `origin/main`.
Continuation: 2026-11-15, from `efd265e`, with the diagnostic additions described below.

The initial BBC exception before consent came from a missing Bulwark build feature.
Bulwark disabled Servo's default Cargo features and enabled only `bundled` and `js_jit`.
That build omitted `webcrypto`, so page scripts did not have access to `window.crypto`.
BBC's UUID code threw during client initialization and Next.js replaced the page with its application error screen.

The correction adds Servo's `webcrypto` Cargo feature.
The Servo version remains `=0.7.0`. No user-agent override, page replacement, or JavaScript polyfill supplies the correction.
The physical follow-up below still failed after consent. The [acceptance review](phase-2-acceptance.md) remains open.
The subsequent controlled investigation reproduced that failure in BBC's advertising placeholder code.
Its unguarded `IntersectionObserver` constructor throws inside a React effect while Servo's default preference disables that API.

## Evidence and isolation

The unchanged release shell reproduced the error with a temporary empty profile.
A loopback nREPL observation returned these page capabilities:

```json
{"secureContext":true,"crypto":"undefined","getRandomValues":"undefined","intersectionObserver":"undefined"}
```

The secure-context result rules out an insecure origin as the reason for the absent Crypto API.
The original acceptance process also used no init file for BBC.

A separate software-rendered probe reproduced the same error with the same Servo build features.
The probe uses Servo's public API directly. It contains no Bulwark engine adapter, Clojure runtime, buffer policy, or key routing.
Both original URLs produced this console error:

```text
crypto.getRandomValues() not supported. See https://github.com/uuidjs/uuid#getrandomvalues-not-supported
```

The first error identifies BBC's `0iid2kl3pm46u.js` bundle, line 1.
The deployment path is `bbcdotcom/web/20261110-133831-0053d0afea-web-3.28.0-4/_next/static/chunks/` under `https://static.files.bbci.co.uk/`.
The bundle checks for `crypto.getRandomValues` before it generates a UUID.
Both runs then logged the Next.js client exception and changed the document title to the application error title.
A later `n is undefined` error followed the missing-Crypto error. It did not recur after the correction.

Servo's pinned [Crypto.webidl](https://github.com/servo/servo/blob/v0.7.0/components/script_bindings/webidls/Crypto.webidl) starts with `skip-unless CARGO_FEATURE_WEBCRYPTO`.
This gate covers the `crypto` attribute and `getRandomValues`, including its use outside SubtleCrypto.
Changing the runtime `dom_crypto_subtle_enabled` preference cannot restore a Cargo-disabled interface.
The pinned [Cargo manifest](https://github.com/servo/servo/blob/v0.7.0/components/servo/Cargo.toml) exposes the `webcrypto` feature separately.

## Results after the correction

| Configuration | BBC News | Original direct article |
|---|---|---|
| Baseline features, direct Servo probe | Fatal application error | Fatal application error |
| WebCrypto, default Servo preferences | Page content present | Article content present |
| WebCrypto and experimental IntersectionObserver, diagnostic only | Page content present | Article content present |
| Rebuilt Bulwark, WebCrypto and default Servo preferences | Content present, `scrollY` reached 600 | Article content present, `scrollY` reached 600 |
| Physical follow-up at `833ad5f`, after consent | Homepage rendered | Article click and reload produced a client-side exception |

The native product runs used separate empty profiles, the ordinary Tiered shell, and the original acceptance URLs.
They injected no diagnostic user script and retained the default Servo user agent.
Each process returned zero, joined its runtime worker, and removed its nREPL port file.
The page inspection occurred after 20 seconds. The process closed after 35 seconds.
The scroll checks used `window.scrollTo` through nREPL. They do not establish physical wheel input or link navigation.

The captured native frames show BBC content behind the Terms of Use overlay.
No acceptance action occurred on that overlay.
These observations establish removal of the fatal application error, not completion of the physical article-reading task.

The local Servo suite now checks a real `crypto.getRandomValues` call against the file fixture.
It checks the returned typed-array identity and length, without asserting particular random bytes.
The suite passed in both Tiered and interpreter modes. The portable workspace passed all 58 tests.
All 17 native unit tests and the Servo contract passed. Native Clippy passed with warnings denied.
The final standalone probe also passed its diagnostic run without instrumentation.
Its final stack-script run produced no instrumentation redeclaration error.

## Physical follow-up after consent

The physical macOS check on 2026-11-14 used source commit `833ad5f` and a fresh profile without an init file.
The temporary app contained a byte-identical copy of the rebuilt `target/debug/bulwark-shell`.
Its SHA-256 was `907504ac7e844adf38adfd8b9cb222d10c0f8a8d3b49f42f1a916e4fc83495f3`.
The normal Tiered shell started at `about:blank` with an opt-in loopback nREPL listener.

Computer Use supplied address entry, pointer clicks, the reload shortcut, and scrolling.
Read-only nREPL observations recorded the resulting page state.
No nREPL navigation, scrolling, dispatcher call, or diagnostic script supplied a physical result.
The keyboard layout remained Dvorak. Computer Use attachment took approximately four minutes.

Physical address entry loaded BBC News and displayed its content behind the Terms of Use overlay.
The user explicitly approved acceptance in this chat. A physical click on “Accept and Continue” dismissed the overlay.
A physical click on the lead article changed the URL to `https://www.bbc.com/news/articles/cwvg2l312ylqo`.
The page then displayed the client-side exception screen.

The physical `C-c r` reload briefly displayed the article before the same exception returned.
The document time origin changed from `2208665431.35` to `2208680840.67` milliseconds, consistent with a new document.
The document title and body both reported the application error after initialization.
`crypto.getRandomValues` remained a function, so WebCrypto was available during this failure.
A physical downward scroll left `scrollY` at zero. The error document and viewport were both 710 pixels high.

The required article-open, scroll, and reload task failed. The WebCrypto correction resolves one failure but does not complete BBC acceptance.
The earlier diagnostic runs did not establish this consented workflow.
This physical follow-up did not capture the failing script exception.
The subsequent controlled investigation below captures the fatal IntersectionObserver stack.

After verification, an nREPL quit request stopped the test process and captured its final error frame.
This request was cleanup, outside the physical acceptance task.
The process returned zero, joined its worker, and removed its port file.

Local evidence resides in `target/bbc-physical-evidence/`.
`observations.jsonl` records the homepage, article failure, reload failure, and final scroll state.
`session.json` records the physical actions and consent approval. `binary-hashes.json` records the identical executable hashes.
`physical.log`, `cleanup.json`, and `exit-status` record cleanup. `final.png` shows the application error.
Computer Use screenshots in this chat also show the approved overlay dismissal, article click, reload, and scroll result.

The [CI run at `833ad5f`](https://github.com/csm/bulwark/actions/runs/43680513338) also failed.
The Linux portable runtime job failed `internal_repl_jobs_are_bounded_and_owned_in_both_modes` with `Timed out: REPL completion`.
The macOS runtime, Servo, and standalone AOT jobs passed.
`target/bbc-physical-evidence/ci-failed.log` retains the failure output. This investigation does not classify the timeout as transient.

## Controlled diagnosis of the consented failure

The investigation on 2026-11-15 reproduced the fatal article error with Servo 0.7.0 outside Bulwark.
The probe started at BBC News, navigated to the original article after 30 seconds, and reloaded it after 60 seconds.
It inspected the page after 20 seconds and every ten seconds thereafter.
The run lasted 90 seconds. Both runs used WebCrypto and the same saved BBC test state.
The only engine preference difference was `dom_intersection_observer_enabled`.

| Probe configuration | Article after navigation | Article after reload |
|---|---|---|
| Default preferences | Fatal application error | Fatal application error |
| IntersectionObserver enabled | Article content present | Article content present |

The comparison also passed its confirmation without console wrappers or page error listeners.
The default-preference confirmation reproduced the fatal page after navigation and reload.
The enabled-preference confirmation retained the article in every sampled document.
Both confirmations retained the same authorized state-restoration script.

The first fatal exception identifies this stack:

```text
ReferenceError: IntersectionObserver is not defined
S@.../00pv_7c.hjx18.js:78:437
C@.../00pv_7c.hjx18.js:78:1240
eo/<@.../15.__ak31mj1s.js:1:5369
```

Both bundles belong to the BBC deployment path recorded above.
The first bundle implements an advertising placeholder component.
Its `closePlaceholder` method calls a helper that constructs `IntersectionObserver` without an availability check.
The second bundle calls `closePlaceholder` from a React effect when its ad-service condition is false.
That exception propagates through React. Next.js then logs its client-side exception and replaces the article with the application error page.
The later `n is undefined` error follows this first fatal exception.

The inspected source and the preference comparison identify a concrete failure path.
The ad-service condition also explains why a URL alone does not establish reproducibility.
The exact causes of variation in that condition remain unmeasured.
Six earlier native article loads in the consented session displayed content despite other script errors.
The 45-second direct-article controls also retained content with the saved state.

The native session additionally logged a missing-IntersectionObserver exception in Tinypass/Piano's `tinypass.min.js`.
BBC's `dotcom-ads.js` also produced `TypeError: Pn.observe is not a function`.
That bundle initializes its observer variable to an empty object.
It constructs the observer only when the API exists, but another path calls `observe` without the corresponding guard.
These errors identify additional consequences of the missing API. They are distinct from the fatal React stack above.

The consented native session ran with opt-in console logging and no page instrumentation.
The user approved its Terms of Use acceptance before the click.
The user separately authorized reuse of its JavaScript-visible first-party cookies and local storage in isolated BBC diagnostic runs.
Those values remain under the ignored evidence directory. No identifiers or saved browser state enter the repository.
The probe's local user script restores those values for the same BBC origin.
It does not preserve every original cookie attribute or third-party storage entry.
This controlled replay supplies diagnostic evidence, not a physical acceptance result.

The missing API is a Servo runtime preference limitation, separate from the earlier WebCrypto Cargo feature omission.
The tested mitigation enables the actual Servo interface. It supplies no JavaScript observer shim.
The product preference remains at its default during this investigation.
Its broader compatibility and performance need evaluation before a product correction, particularly against the open issue #44907 below.
BBC physical acceptance and the remaining Phase 2 gates remain open.

Evidence resides in `target/bbc-consent-investigation/`.
`native.log` records consented native console errors. `repeat-articles.jsonl` records the six native attempts.
`restored-flow-default.log` contains the fatal React stacks after navigation and reload.
`restored-flow-intersection.log` contains the successful preference comparison.
`confirmed-flow-{default,intersection}.log` records the confirmation without stack instrumentation.
The corresponding PNG files show the final application error and readable article.
The corresponding `restored-{default,intersection}.log` files contain the direct-article controls.
`flow-{default,intersection}.log` contains the longer controls before consent.
The source bundles, comparison summaries, screenshots, and local replay files are also retained there.
The native unit suites passed all 17 tests and the Servo contract after the diagnostic additions.
Native Clippy, formatting, and whitespace checks passed.

## Known Servo limitations

The WebCrypto-enabled probe still logged `ReferenceError: IntersectionObserver is not defined` from BBC's `0x01rgvk49mtx.js` bundle.
BBC caught these component errors, and the main content remained present in the observed runs.
Servo 0.7.0 defaults `dom_intersection_observer_enabled` to `false`.
The [IntersectionObserver tracking issue #35767](https://github.com/servo/servo/issues/35767) remains open.

The probe's experimental preference removed the missing-IntersectionObserver errors.
It did not resolve every third-party messaging error.
The product correction leaves this preference at its default.
The open [Amazon CPU issue #44907](https://github.com/servo/servo/issues/44907) documents problems with experimental intersection computation.
The closed [MDN freeze issue #45915](https://github.com/servo/servo/issues/45915) records another experimental-mode failure.
These reports support a separate evaluation before a product preference change.

The closed [BBC CPU and memory issue #43680](https://github.com/servo/servo/issues/43680) describes partial rendering and resource growth.
Its closing comment reports success with experimental mode.
Its symptoms differ from the missing-Crypto exception reproduced here, so it does not establish this failure's cause.

The closed [BBC header issue #48526](https://github.com/servo/servo/issues/48526) identifies fractional SVG `viewBox` parsing.
The related [SVG color issue #48543](https://github.com/servo/servo/issues/48543) concerns `currentColor`.
These are relevant to the incomplete chrome in the acceptance review, but they do not explain the fatal UUID exception.
This investigation did not establish whether those fixes resolve every observed icon defect.

## Bulwark diagnostics and dependency constraint

Before this follow-up, Bulwark's Servo delegate did not implement `show_console_message`.
Servo's default callback discarded those messages, so the original process logs omitted BBC's actionable error.
A JavaScript exception also differs from a Servo crash: the existing `notify_crashed` callback does not report this page failure.
The new opt-in logger writes outside the ordered product event queue.
`BULWARK_LOG_PAGE_CONSOLE=1` records up to 64 distinct warnings or errors per view, including across reloads.
Each message retains at most 2,048 Unicode characters. The logger suppresses exact repeats and escapes text in its output.
The logger remains off by default. It adds no site rule or new native function.
The initial diagnostic budget included low-severity messages, which let advertising debug logs exhaust it.
The final logger reserves the budget for warnings and errors.

The open [muted-error issue #47129](https://github.com/servo/servo/issues/47129) documents another diagnostic limitation.
Cross-origin script errors can lose their details in page error events.
The probe records native console callbacks and can also capture Error stacks with its opt-in diagnostic script.

Enabling WebCrypto initially failed to compile against `rsa` 0.10.0-rc.18 and stable `pkcs1` 0.8.0.
RSA expected the earlier lifetime-based PKCS#1 types. The stable release changed those types to generic structs.
The correction locks `pkcs1` to `0.8.0-rc.4`, matching [Servo 0.7.0's lockfile](https://github.com/servo/servo/blob/v0.7.0/Cargo.lock).
The locked WebCrypto build then passed. A dependency refresh must retain a compatible pair or repeat this build check.

## Reproduction and retained evidence

The [probe source](../experiments/e3-servo/examples/bbc_probe.rs) supports the homepage and an explicit URL:

```sh
cargo run --locked -p bulwark-experiment-e3 --features servo --example bbc_probe -- https://www.bbc.com/news
cargo run --locked -p bulwark-experiment-e3 --features servo --example bbc_probe -- https://www.bbc.com/news/articles/cwvg2l312ylqo
```

Each default probe runs for approximately 30 seconds and first inspects the page after 20 seconds.
The `--seconds N` flag selects a duration from 21 to 300 seconds.
The `--navigate URL` flag defaults to a 90-second run, with navigation after 30 seconds and reload after 60 seconds.
Such a run needs at least 71 seconds. Further inspections occur every ten seconds.
A native graphics context and network access are necessary, including for the software surface on macOS.
A zero exit status means that the diagnostic completed. It does not declare site acceptance.

The optional `--diagnostics` flag adds console stack formatting and error listeners through a user script.
The optional `--intersection-observer` flag enables that experimental preference for the probe process alone.
Neither flag supplies a passing physical acceptance result.
The `--user-script PATH` flag loads a trusted local diagnostic script at document creation.
The `--screenshot PATH` flag captures a page frame approximately ten seconds before the run ends.

The consented comparison uses the locally retained state-restoration script:

```sh
cargo run --locked -p bulwark-experiment-e3 --features servo --example bbc_probe -- https://www.bbc.com/news --navigate https://www.bbc.com/news/articles/cwvg2l312ylqo --diagnostics --user-script target/bbc-consent-investigation/restore-state.js
```

Adding `--intersection-observer` selects the comparison preference.
The local replay file contains the explicitly authorized test state and is absent from source control.

Local evidence resides in `target/bbc-investigation/`:

| Evidence | Contents |
|---|---|
| `product-capabilities.json` | Unchanged release shell, missing Crypto API on a secure BBC origin |
| `servo-default-{home,article}.log` | Baseline direct-Servo errors |
| `servo-webcrypto-{home,article}.log` | Initial stack diagnostics after WebCrypto, including caught IntersectionObserver errors |
| `servo-webcrypto-intersection-{home,article}.log` | Experimental preference comparison |
| `servo-final-home.log`, `servo-final-diagnostics-article.log` | Final probe without instrumentation, and final stack-script check |
| `product-fixed-{home,article}.{json,log,png}` | Corrected native shell, page observations, scroll results, shutdown, and frames |
| `webcrypto-build.log` | Initial incompatible PKCS#1 build |
| `webcrypto-build-pinned.log` | Compatible locked probe build |
| `servo-regression-{tiered,interpreter}.log` | Local Crypto API and retained-view regressions |
| `portable-tests.log`, `native-tests.log`, `clippy.log`, `binary-hashes.json` | Regression checks and executable hashes |

The initial stack script also produced a `const` redeclaration error during repeated injection.
The retained final probe wraps that script in an idempotent closure.
The corrected product runs used no such script, so their results do not depend on that instrumentation.
