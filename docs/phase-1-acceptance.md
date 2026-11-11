# Phase 1 acceptance review

Status: complete, 2026-11-11. Base: `main` at `0e86c4a`.
The physical macOS checks passed at `fe243f8` on the acceptance branch.
The branch includes current main. No product-code fix was necessary during these checks.

## Acceptance evidence

| Criterion | Evidence |
|---|---|
| A1: default native startup | Passed. Default startup opened the native window and echo. Physical close joined the worker and returned status zero. |
| A2: navigation from nREPL | Passed. `bulwark.browser/navigate!` visibly navigated to `https://example.com` and back to the fixture. |
| A3: live title hook | Passed. A hook installed through nREPL displayed navigation, click, and input titles without restart. |
| A4: live command binding | Passed. Physical `C-c r` invoked the command. Redefinition changed the existing binding. Captured strokes stayed outside the page. |
| A5: Clojure behavior | Passed. Physical `C-x h` displayed retained titles. `visit-history!` visibly revisited example.com and the fixture. |
| A6: mechanisms and policy | Passed. The source review below accepts the current boundary. |
| A7: two-second command stall | Passed. Input and animation continued during the loop. The queued marker ran after recovery. Native close also passed with a parked evaluation. |
| A8: Linux and interpreter | Passed locally and in the acceptance CI run. Linux runs both product modes, including software Servo and native process suites. |

## Startup and interpreter execution

`cargo run --locked` opens the native shell at `about:blank`.
`--no-default-features` omits native and Servo dependencies for portable runtime checks.
The namespace bootstrap remains available with `cargo run --locked --no-default-features`.
Software Servo alone remains available with `--no-default-features --features servo`.

The product flag `--interpreter` selects `ExecutionMode::TreeWalk` for the worker and nREPL.
The default remains `ExecutionMode::Tiered`.
Both native process runners accept `--interpreter` and pass it to each product process.
The foundation, command, and browser worker tests exercise both modes.
CI also runs both foundation smokes, both mock nREPL suites, both software Servo smokes, and both native process suites.

## Mechanism and policy review

`crates/bulwark-runtime/src/native.rs` exports these native functions:

| Functions | Mechanism |
|---|---|
| `webview-create`, `webview-close` | Create and invalidate explicit view handles. |
| `navigate`, `reload`, `back`, `forward` | Apply an explicit request to the supplied view. |
| `webview-info`, `eval-js` | Return view information or a JavaScript result. |
| `log`, `set-echo!`, `quit` | Write output, replace echo text, or request shutdown. |
| `next-event` | Read the next ordered event. |
| `set-key-routes!` | Publish bounded key reservations and cancellation strokes from Clojure. |

Rust owns thread affinity, input conversion, native resources, composition, bounded transport, nREPL admission, and shutdown.
The reservation matcher contains no command names or binding strings.
Clojure defines command bindings, prefix cancellation strokes, hooks, echo policy, navigation commands, and title history.
The native shell accepts an explicit startup URL and an optional Clojure script.
The startup fallback is `about:blank`.
No Rust code implements the history behavior.
This review accepts the Phase 1 boundary without a vision change.

## Physical macOS results, 2026-11-11

Platform: macOS 26.5.2 arm64, Rust 1.97.1, Servo 0.7.0, debug build, default Tiered execution.
Computer Use sent native macOS pointer and keyboard events.
It observed the page and echo through native screenshots.
The page content has no accessibility tree, so page actions used screenshot coordinates.

Default `cargo run --locked` opened the native host.
Computer Use requires an app identity to attach to this executable.
A temporary app bundle contained a byte-identical copy of the product binary, with no wrapper code.
Both copies had SHA-256 `e494bb6c54fc24b262307ad72d987ee965726062b93fd453861773de717b451f`.
The bundled default window showed `about:blank` and the echo area.
Its physical close returned status zero with eight painted frames, eight presentations, and `shutdown=joined`.

The fixture session used an isolated profile and the product's opt-in loopback nREPL.
All command definitions, hooks, and observation helpers came through that listener.
The active keyboard layout mapped the tool's key positions differently from its key labels.
The page log identified those logical strokes before the command checks.
The checks then sent logical `C-c r`, `C-c t`, `C-c s`, and `C-x h` through native input.

| Action | Observation | Result |
|---|---|---|
| Navigate through nREPL | Example.com appeared, followed by the fixture after another navigation. | Passed |
| Install a title hook, then click and type | The echo showed `Example Domain`, `Clicked: 1`, and `Typed: native test`. | Passed |
| Bind `reload-twice`, press `C-c r`, redefine it, and repeat | The page reloaded and `command-hits` became `[:first :second]` without another binding. | Passed |
| Invoke a command without a reload, then type | The command ran once. The page key log contained only the letters in `native test`. | Passed |
| Press `C-x h` | Retained page titles appeared in the echo area. | Passed |
| Run the two-second stall and send more input | The field became `native testz`, the button count became one, and animation continued before the stall ended. | Passed |
| Send `C-c t` during the stall | Recovery order was `[:started :finished :marker]`. The page log contained only `z`. | Passed |
| Edit after recovery | Backspace, Shift+Left, and replacement text produced `native tesx`. | Passed |
| Scroll the page, then scroll over the echo | The end marker appeared. Echo scrolling left the page offset at 1123 pixels. | Passed |
| Resize the window | The page viewport changed from 1000×732 to 700×482. The text remained intact and the echo stayed at the bottom. | Passed |
| Cancel a prefix with Escape, then type | The marker did not run. The ordinary `t` reached the field. | Passed |
| Run commands and click after resize | The marker ran, the button count became two, and history remained visible. | Passed |
| Revisit retained entries | Clojure history restored example.com, then the fixture. | Passed |
| Close with an evaluation parked | The worker joined, the parked connection and listener closed, and the port file disappeared. | Passed |

The measured command loop lasted 2000.015375 milliseconds.
A separate nREPL client collected 37 page observations during that loop.
The page animation counter advanced from 7562 to 7757 across those observations.
The field edit and button click appeared before the loop ended.
The marker timestamp followed the loop's end timestamp.
The screenshot during the loop showed the edited field and incremented button while the echo still displayed `Stall started`.
These observations establish progress during this check, not a display frame-rate guarantee.

The final fixture session painted 25,256 frames, made 25,258 presentations, and returned status zero with `shutdown=joined`.
The session covered setup, observations, navigation, and interaction. These totals do not measure the stall alone.

## Repeat the physical checks

1. Run `cargo run --locked` and observe the native window and echo area.
2. Close the window and require `shutdown=joined` with exit status zero.
3. Open the acceptance fixture with an isolated profile:

   ```sh
   fixture_url=$(python3 -c 'from pathlib import Path; print(Path("experiments/fixtures/phase-1.html").resolve().as_uri())')
   cargo run --locked -- --nrepl --profile /tmp/bulwark-phase-1-acceptance "$fixture_url"
   ```

4. Connect an nREPL client to `127.0.0.1` at the port in the profile's `.nrepl-port` file.
5. Navigate to `https://example.com` through the public helper:

   ```clojure
   (await (bulwark.browser/navigate! "https://example.com"))
   ```

6. Observe the visible navigation, then return to the fixture through the same helper.
7. Install a title hook and a command through nREPL:

   ```clojure
   (require '[bulwark.browser :as browser]
            '[bulwark.commands :refer [defcommand]]
            '[bulwark.keymap :as keymap]
            '[bulwark.hooks :as hooks])
   (hooks/add-hook! :page/title-changed :acceptance/title
     (fn [event] (browser/set-echo! (:title event))))
   (def command-hits (atom []))
   (defcommand ^:async reload-twice "Reloads twice from a live binding." []
     (swap! command-hits conj :first)
     (await (browser/reload))
     (await (browser/reload)))
   (keymap/bind-key! "C-c r" 'reload-twice)
   ```

8. Click the page button and observe the title in the echo area.
9. Clear `window.keyEvents` through `eval-js`, focus the page input, and press `C-c r` with Control released before R.
10. Require `@command-hits` to equal `[:first]` and observe the page reload.
11. Redefine `reload-twice` with the marker `:second`, then repeat the sequence without another binding.
12. Require `@command-hits` to equal `[:first :second]`.
13. Read `JSON.stringify(window.keyEvents)` through `bulwark.native/eval-js` and require no captured `C-c` or `r` presses.
14. Type ordinary text and observe the field and echo update.
15. Press `C-x h` and observe retained titles in the echo area.
16. Install these stall and marker commands through nREPL:

   ```clojure
   (def stall-stages (atom []))
   (defcommand ^:async stall "Occupies command dispatch for two seconds." []
     (await (browser/set-echo! "Stall started"))
     (swap! stall-stages conj :started)
     (println "acceptance-stall-started")
     (let [end (+ (System/nanoTime) 2000000000)]
       (loop [] (when (< (System/nanoTime) end) (recur))))
     (swap! stall-stages conj :finished)
     (browser/set-echo! "Stall finished"))
   (defcommand mark "Records command recovery." []
     (swap! stall-stages conj :marker))
   (keymap/bind-key! "C-c s" 'stall)
   (keymap/bind-key! "C-c t" 'mark)
   ```

17. Clear the page key log, focus the field, and press `C-c s`.
18. During the two-second interval, type ordinary text, press `C-c t`, and click the page button.
19. Observe animation, field edits, and the button count before the stall ends.
20. Require `@stall-stages` to equal `[:started :finished :marker]` after recovery.
21. Require the page key log to omit captured command strokes while ordinary keys remain present.
22. Exercise scrolling, echo isolation, resize, and another command after recovery.
23. Close the native window and require worker join, listener closure, and port-file removal.

The synchronous loop occupies the ordered command dispatcher.
Clojurust evaluator safepoints can still schedule other fibers, including nREPL.
The native host sends ordinary keys directly to Servo without a runtime decision.
Captured commands retain their events in the existing 256-event queue and run in order after the dispatcher resumes.
The queue keeps its fatal overflow behavior. There is no timeout replay to the page.
The physical check must stay within that event capacity.

## CI continuation

The [main run for key interception](https://github.com/csm/bulwark/actions/runs/42770493413) passed the Linux Servo job.
That job included native matcher tests, software Servo, all seven native process cases, and all four native nREPL cases.
The macOS runtime job failed in the older E7 heartbeat assertion.
Its assertion required another fiber to resume during one 40-millisecond sleep.
A loaded host can resume the sleeper before the heartbeat after both timers become ready.
The check now observes at most ten sleeps and still requires heartbeat progress during a sleep.
A blocking sleep fails every observation.
The runtime matrix now disables fail-fast so one failure does not cancel the other platform's evidence.

The [acceptance CI run](https://github.com/csm/bulwark/actions/runs/42931611072) passed all three jobs at `f40c478`.
Both runtime jobs passed the foundation and mock nREPL suites in TreeWalk and Tiered modes.
The E7 mock, nested-await, idle controls, and nREPL checks also passed on Linux and macOS.
Linux passed native/Servo Clippy, unit and contract tests, default native startup, and both software Servo modes.
Both native process suites and both native nREPL suites also passed on Linux.

## Local results

Formatting, portable workspace Clippy, all 40 workspace tests, and native/Servo Clippy passed on macOS arm64.
All nine native shell unit tests, both Servo unit tests, and the Servo contract passed.
Both software Servo smokes painted 13 frames and joined their workers.
Default native startup painted four frames and made seven presentations before timed closure with a joined worker.
Both evaluator modes passed all seven native process cases, all four native nREPL cases, and all three mock nREPL cases.
Both foundation smokes and the E7 mock and nested-await checks passed.
Twenty repeated E7 mock checks passed with two concurrent processes.
The acceptance fixture also passed a diagnostic render with 122 painted frames, 125 presentations, and joined shutdown.
Its captured frame showed the input controls, key log, echo area, and animation counter at 118.
The existing GL texture and large `__eh_frame` linker warnings remain.

## Exit disposition

All eight Phase 1 acceptance criteria pass on this branch.
The exit question identifies no browser behavior that requires another Rust mechanism or a vision change.
Commands, hooks, echo policy, and title history remain in Clojure.
Phase 1 is complete. Phase 2 can begin after review and merge of the acceptance branch.

Interactive Linux behavior, IME, clipboard, display-scale transitions, and broader site compatibility remain follow-up work.
The physical check used the active keyboard layout. It does not establish support for every layout.
The accepted single-visible-view fallback remains in force until E5 passes.
