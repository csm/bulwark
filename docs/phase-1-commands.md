# Phase 1 commands and browser behavior

Status: implemented locally, 2026-11-08. Phase 1 remains in progress.
Base: `main` at `fc68765`.

The product now includes a Clojure command registry, keymap, hooks, and browser commands.
These namespaces use the existing native API and bounded product bridge.
Rust embeds the sources and supports an explicit evaluator mode for integration checks.
The native window, input routing, echo geometry, and dependency pins remain unchanged.

## Commands and keys

`bulwark.commands/defcommand` defines a function with a required docstring and registers its Var.
`command-info` returns the qualified name, documentation, and Var.
`invoke!` dereferences the current Var and awaits its result.
A later function definition therefore changes the next invocation without another key binding.

`bulwark.keymap/bind-key!` accepts a sequence string and command symbol.
Qualified symbols select an exact command.
An unqualified symbol must match exactly one registered command.
Unknown or ambiguous names cause an error.
`unbind-key!` removes a binding.

Sequences use Emacs-style notation, such as `C-x C-f`.
The modifiers `C-`, `M-`, `S-`, and `s-` mean Control, Alt, Shift, and Meta.
Modifier order does not affect a binding.
The aliases `RET`, `SPC`, `TAB`, `ESC`, and `DEL` select Enter, space, Tab, Escape, and Backspace.
Shifted letters use `S-`, and printable punctuation includes its own Shift effect.
For example, `X` and `S-x` select the same stroke.

The keymap retains an incomplete sequence for its originating view.
Key releases and modifier keys do not advance the sequence.
`C-g`, Escape, an unmatched stroke, a view change, or a close event cancels the prefix.
A binding change also cancels the prefix.
Prefixes do not expire on a timer.
A complete command cannot also serve as the prefix of another command.
Conflicting bindings cause an error before the keymap changes.

The native host still sends keys to Servo before it sends ordered events to Clojure.
Bound keys therefore also reach the page.
Command interception requires a separate host mechanism before the full keyboard acceptance review.

## Hooks and event delivery

`bulwark.hooks/add-hook!` accepts an event keyword and a handler.
Its three-argument form also accepts a registration id.
A repeated id replaces its handler at the same position.
The two-argument form uses the handler itself as the id.
`remove-hook!` removes that id.

`bulwark.core/dispatch!` routes key events through the keymap, then runs hooks.
Key hooks receive `:keymap/status` with `:command`, `:prefix`, `:cancelled`, `:unbound`, or `:ignored`.
The default echo hook displays only unbound pressed keys.
Command output can therefore remain visible after its final key.

Each event uses a snapshot of its handlers in registration order.
A handler can return a plain value or a future.
The dispatcher awaits each handler before it starts the next handler or event.
Hook additions and removals affect the next event.
An unhandled command or hook error ends the worker through the existing dispatcher error path.

## Browser behavior

`bulwark.browser` owns the active view and public browser helpers.
`navigate!` navigates that view, and `set-echo!` displays text.
Both helpers await native replies and raise structured errors for failed requests.
`bulwark.shell/run!` installs browser hooks and default keys before it opens the initial view.

| Default sequence | Command |
|---|---|
| `C-c r` | `bulwark.browser/reload` |
| `C-c b` | `bulwark.browser/back` |
| `C-c f` | `bulwark.browser/forward` |
| `C-x h` | `bulwark.browser/history-list` |

URL and title hooks record `{:url :title :at}` visits in `bulwark.browser/history`.
The timestamp uses milliseconds since the Unix epoch.
The hooks omit consecutive identical URL/title pairs and retain the latest 256 visits in memory.
The history command displays the latest eight titles, newest first.
`visit-history!` navigates to a retained visit by its index, oldest first.
The history feature contains no Rust code specific to its behavior.
Persistence and a history selection prompt remain later work.

## Startup customization

Save this program as `/tmp/bulwark-commands.cljrs`:

```clojure
(ns user
  (:require [bulwark.commands :refer [defcommand]]
            [bulwark.keymap :refer [bind-key!]]
            [bulwark.hooks :refer [add-hook!]]
            [bulwark.browser :as browser]
            [bulwark.shell :as shell]))

(defcommand ^:async reload-twice
  "Reloads the visible page twice."
  []
  (await (browser/reload))
  (await (browser/reload)))

(bind-key! "C-c t" 'reload-twice)
(add-hook! :page/title-changed :user/title
  (fn [event] (browser/set-echo! (:title event))))

(shell/run! user/initial-url)
```

Run the program:

```sh
cargo run --locked --features native -- https://servo.org --script /tmp/bulwark-commands.cljrs
```

Press Control+C, release Control, then press T without Shift.
The sequence runs both reload operations on the visible page.
The native host also forwards those keys to the page.
The product does not start an nREPL listener yet.

## Checks

Run the portable checks:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo run --locked -- --headless-smoke
```

The workspace tests run `clj/tests/commands.cljrs` and `clj/tests/browser.cljrs` in both TreeWalk and Tiered modes.
Those programs exercise live Vars, parsing, prefixes, cancellation, hook snapshots, awaited handlers, browser commands, history retention, and closed-view cleanup.
The existing Linux and macOS CI jobs include these tests through `cargo test --workspace`.
Remote CI has not run these changes.
Local formatting, workspace Clippy, all 30 workspace tests, and the headless foundation smoke passed.
The Servo contract tests and software smoke also passed. The software smoke painted 14 frames and joined the worker.

Run the native checks:

```sh
cargo build --locked --features native
cargo test --locked -p bulwark-shell --features native
python3 experiments/run_product_native.py
```

The native runner retains its five existing cases.
It also runs Clojure commands and history against Servo, and requires joined shutdown after an injected hook error.
The new browser case sends key maps through the product dispatcher.
It does not simulate physical keyboard events.
The previous [interactive macOS results](phase-1-native.md#interactive-macos-checks-2026-11-08) remain historical evidence for native input.

All three native unit tests, native/Servo Clippy, and all seven native process cases passed locally on macOS arm64.
Every native process case reported `shutdown=joined`.
The final browser case painted 21 frames and made 24 presentations.
The existing GL texture and large `__eh_frame` linker warnings remain.

## CI continuation

The [post-merge main run](https://github.com/csm/bulwark/actions/runs/42578603502) passed both runtime jobs but failed the Servo job.
SpiderMonkey failed to use its prebuilt archive and attempted a source build.
That build stopped because `llvm-objdump` was absent.
The workflow now installs `llvm` alongside `clang`.
This dependency change still requires a Linux CI run.

## Remaining Phase 1 work

1. Add opt-in loopback nREPL and the profile port file.
2. Preserve the E7 admission budgets, overload assertions, interrupts, and shutdown cleanup in that product integration.
3. Add key interception without a runtime wait on ordinary page input.
4. Run the full Phase 1 acceptance review, including default native startup and real nREPL customization.

This checkpoint does not complete the Phase 1 acceptance criteria.
