# BBC Phase 2 compatibility investigation

Date: 2026-11-14. Baseline: `5d2b40f`, equal to the fetched `origin/main`.

The initial BBC exception before consent came from a missing Bulwark build feature.
Bulwark disabled Servo's default Cargo features and enabled only `bundled` and `js_jit`.
That build omitted `webcrypto`, so page scripts did not have access to `window.crypto`.
BBC's UUID code threw during client initialization and Next.js replaced the page with its application error screen.

The correction adds Servo's `webcrypto` Cargo feature.
The Servo version remains `=0.7.0`. No user-agent override, page replacement, or JavaScript polyfill supplies the correction.
The physical follow-up below still failed after consent. The [acceptance review](phase-2-acceptance.md) remains open.

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
This follow-up did not capture the failing script exception, so its cause remains unresolved.
The next diagnosis needs console evidence from the consented article path.

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

Bulwark's Servo delegate does not implement `show_console_message`.
Servo's default callback discards those messages, so the original process logs omitted BBC's actionable error.
A JavaScript exception also differs from a Servo crash: the existing `notify_crashed` callback does not report this page failure.
A future product console mechanism needs explicit bounds consistent with the bridge contract.
The standalone probe supplies console output for this investigation without changing that contract.

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

Each probe runs for approximately 30 seconds and inspects the page after 20 seconds.
A native graphics context and network access are necessary, including for the software surface on macOS.
A zero exit status means that the diagnostic completed. It does not declare site acceptance.

The optional `--diagnostics` flag adds console stack formatting and error listeners through a user script.
The optional `--intersection-observer` flag enables that experimental preference for the probe process alone.
Neither flag supplies a passing physical acceptance result.

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
