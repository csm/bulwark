# P2.5 and P2.6 validation

Date: 2026-11-14. macOS physical checks passed within the scope recorded here.
The final GitHub run passed all four jobs, including Linux runtime, Servo, and AOT.

## Sources and method

The first physical session used `8058ed7` in Tiered mode.
The corrected session used `e5e39aa`, which restores the caller namespace after init evaluation.
The later `090e2c5` changes native test synchronization only. It changes no product code.

Each temporary app bundle contained a byte-identical copy of the compiled product executable.
The sessions used the ordinary shell, an isolated profile, and opt-in loopback nREPL.
Computer Use supplied native keys and pointer input. nREPL supplied observations and disposable fixture setup.
No call to `core/dispatch!` supplied a physical result.

| Session | Executable SHA-256 |
|---|---|
| `8058ed7` | `f9dd2ee1b4cf436f26ca3c7431b1af4acb6113f5b74dd80a2e5d7c408bddf411` |
| `e5e39aa` | `e1798213b903e7d2db888c13ea58edb99059cf4bb9682f36bb239e2e1d85b14e` |

The page fixture was `experiments/fixtures/phase-2-windows.html`.
The local evidence directory is `target/physical-shell-evidence/`.
It contains nREPL observations, final composed frames, binary hashes, close outcomes, and the first CI failure log.
The final frames are `8058ed7-messages-final.png` and `e5e39aa-final.png`.

## P2.5 physical backfill

| Check | Observed result |
|---|---|
| Palette entry | Logical `C-c x` replaced the echo line with `M-x` and moved keyboard focus to the prompt. |
| Query and documentation | Native typing produced `relo`. The first candidate was `reload` with its docstring. |
| Page isolation | The page retained `a` and its earlier key log while the visible prompt received `relo`. It reported no document focus. |
| Candidate movement | ArrowDown selected `reload-init`. Logical `C-p` returned the selection to `reload`. |
| Completion | Tab replaced the input with `reload` and placed the cursor at its end. |
| Acceptance | Enter closed the prompt and reloaded the page. The frame counter reset and the load counter increased. |
| Focus restoration | A page click and native typing reached the page after acceptance. |
| Buffer cancellation | Logical `C-x b` opened buffer completion. Escape closed it. The next `e` extended retained page text from `a` to `ae`. |
| Literal URL | Native entry of `about:blank` navigated the selected page. The Back command restored the retained fixture and `ae`. |
| Unicode hostname | Option-A produced `å` through the active keyboard layout. Native ASCII entry completed `å.example`. |
| IDN acceptance | Enter closed the prompt and navigated to `https://xn--5ca.example/`. The reserved test domain returned an expected load error. |
| Unicode isolation | Before acceptance, the page key log stayed empty and its document reported no focus. |
| Progress during a prompt | The visible frame counter advanced from 37,226 to 39,676 during the first command query and selection checks. |
| Retained buffers | Physical buffer completion returned from Help to the page with `ae` intact. |

The active keyboard layout was Dvorak. Computer Use key labels describe physical positions.
The page and native event logs established the logical strokes before command checks.
For example, tool input `ctrl+i`, then `b`, produced logical `C-c x`.
Tool input `ctrl+b`, then `n`, produced logical `C-x b`.

The documented Option-key limitation reproduced.
The attempted logical `M-x` arrived as `M-≈`, so it reached the page instead of opening the palette.
The supported `C-c x` fallback passed. This result does not change the Option-as-Alt policy.

Prompt-key isolation applies after the native prompt appears.
One automation batch entered text before native focus changed. Those early ordinary letters reached the page.
Subsequent isolation checks waited for the visible prompt before entry. Reserved command strokes stayed outside the page key log.

Unicode Option-key entry passed. Multi-stage IME composition remains unverified.
An attempted dead-key sequence produced plain `e`. Automated Unicode text insertion omitted the accented character.
The character-viewer attachment timed out. These attempts do not establish an IME defect in the product.
The clipboard shortcut attempt arrived as logical `Cmd-K` on Dvorak, so it does not establish clipboard support.

## P2.6 physical checks

| Check | Observed result |
|---|---|
| Command help | Physical palette selection of `describe-command`, followed by `bulwark.browser/reload`, opened read-only `*Help*` with its signature and documentation. |
| Key help | `describe-key` with `C-x b` showed the global map and `bulwark.browser/switch-buffer`. |
| Variable help | `describe-variable` showed `bulwark.browser/new-buffer-url` and its current atom value, `about:blank`. |
| Apropos | Physical entry of `webview` showed native names, signatures, documentation, and resource tags. All eleven webview-tagged operations appeared. |
| Page retention | Help selection left page text and key state unchanged. Buffer completion restored page focus. |
| Live init | Physical `reload-init` changed `C-x b` to `user.init/choose-buffer` without restart. |
| Reader transformation | The documented example, with its URL prefix pointed at the local fixture, hid a visible navigation marker. Page text `ae` remained. |
| Repeated reload | A second physical reload retained one reader hook and one style element. |
| Broken init | A deliberate error appeared in Messages with `init.cljrs`, line 61, column 1. The page animation continued. |
| Command error | Physical palette invocation of a deliberate failing command closed its prompt, recorded diagnostics, and left the shell usable. |
| Messages | Physical `show-messages` displayed the source-located error in a read-only text buffer. |
| Missing init | Physical reload with the file absent returned `:missing` and retained the current binding and selected page. |
| Namespace restoration | The corrected build preserved `user` after successful and failing reloads. Subsequent command definitions registered in `user` and ran. |
| Recovery reload | Restoring the example and reloading returned `:loaded`, with one hook, one style, and the reader minor mode enabled. |

The reader check used a local navigation marker and the example's public APIs.
It establishes the documented transformation and live reload, rather than live Wikipedia compatibility.
The physical sessions used Tiered execution. Automated interpreter coverage remains separate evidence.

## Physical shutdown

Both sessions closed through the native close button while an nREPL evaluation was parked.
Both clients reached EOF. Both workers joined, and both profile port files disappeared.

| Session | Painted frames | Presentations | Shutdown |
|---|---:|---:|---|
| `8058ed7` | 74,139 | 75,240 | Joined |
| `e5e39aa` | 104,327 | 104,336 | Joined |

Computer Use attachment took several minutes in the first session and more than fifteen minutes in the second.
Frame totals include those waits. They measure neither startup performance nor input latency.
The parked-client elapsed times also include the interval before the close action.
The host retained its existing two-second worker shutdown deadline.

## Linux CI and corrections

The runs use GitHub Actions with Ubuntu software rendering and Xvfb.
The workflow also runs a macOS runtime job and a standalone Linux AOT job.

| Revision | Result and correction |
|---|---|
| P2.5 main, `17bb3ed` | Runtime and AOT jobs passed. The native minibuffer fixture timed out on document focus under bare Xvfb. |
| `8058ed7` | The current stable Clippy rejected a test module before the implementation. The inherited focus failure also reproduced. |
| `d052404` | Moving the test module and adding Openbox to native process checks produced a passing four-job run. |
| `e5e39aa` | Runtime and AOT jobs passed. A mixed-window readiness query reached JavaScript before initial document load and returned `InternalError`. |
| `090e2c5` | All four jobs passed. Both native process suites and both native nREPL suites passed on Linux. |

The successful intermediate run is [43620039060](https://github.com/csm/bulwark/actions/runs/43620039060).
The final passing run is [43622541434](https://github.com/csm/bulwark/actions/runs/43622541434).
It covers all 57 portable tests, native tests, both evaluator modes, nREPL limits and cleanup, and standalone AOT.

Openbox supplies real window focus under Xvfb. The assertions still require focus loss and restoration.
The fixtures wait for ordered `:complete` load state before they query JavaScript.
The correction does not retry whole failed processes or remove any native assertion.

Physical reload also exposed a namespace leak in the pinned evaluator's `ns` handling.
The loader now restores the caller's `*ns*` root after evaluation, including on errors.
The regression covers both evaluator modes and a later command definition.
The corrected physical session confirms the same behavior through the palette.
