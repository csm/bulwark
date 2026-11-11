# Phase 1 acceptance review

Status: in progress, 2026-11-10. Base: `main` at `0e86c4a`.
Physical checks remain blocked because the Mac is locked.

## Acceptance evidence

| Criterion | Evidence and remaining work |
|---|---|
| A1: default native startup | `native` is now a default Cargo feature. Local startup and shutdown pass. Visible display still needs an unlocked Mac. |
| A2: navigation from nREPL | Mock and native process suites navigate through the product bridge. Visible navigation remains open. The public helper is `bulwark.browser/navigate!`. |
| A3: live title hook | Both nREPL suites install a title hook and receive subsequent title events. Physical echo observation remains open. |
| A4: live command binding | Process suites cover live command redefinition. Native matcher tests cover capture and ownership. Physical command sequences remain open. |
| A5: Clojure behavior | `bulwark.browser` retains titles, lists history, and revisits entries. Native process checks cover this behavior. Physical `C-x h` remains open. |
| A6: mechanisms and policy | The source review below accepts the current boundary. |
| A7: two-second command stall | Worker tests cover reservation publication during a shorter loop. Physical input, animation, queued commands, and recovery remain open. |
| A8: Linux and interpreter | Local product suites pass in TreeWalk and Tiered modes. CI now runs both modes, including software Servo. Remote results remain pending. |

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

## Physical checks after unlock

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

## Local results

Formatting, portable workspace Clippy, all 40 workspace tests, and native/Servo Clippy passed on macOS arm64.
All nine native shell unit tests, both Servo unit tests, and the Servo contract passed.
Both software Servo smokes painted 13 frames and joined their workers.
Default native startup painted four frames and made seven presentations before timed closure with a joined worker.
Both evaluator modes passed all seven native process cases, all four native nREPL cases, and all three mock nREPL cases.
Both foundation smokes and the E7 mock and nested-await checks passed.
The existing GL texture and large `__eh_frame` linker warnings remain.

The acceptance continuation still requires a new CI run and the physical results before Phase 1 can close.
