# Phase 1 product nREPL

The product now supports opt-in nREPL on `127.0.0.1`.
The server shares the dedicated runtime thread and its `LocalSet` with the browser program and event dispatcher.
The native host retains engine ownership and its two-second shutdown deadline.

## Start and connect

Run the native shell with nREPL:

```sh
cargo run --locked --features native -- --nrepl --profile /tmp/bulwark-profile about:blank
```

The operating system selects a free port.
The shell prints `nREPL: 127.0.0.1:PORT` and writes the port to `/tmp/bulwark-profile/.nrepl-port`.
Without `--profile`, the directory defaults to `$HOME/.bulwark`.
Without `--nrepl`, the shell creates no listener or profile port file.
The profile flag currently requires nREPL.

Connect an nREPL client to that loopback address and port.
Run these forms to navigate the active view and install a title hook:

```clojure
(await (bulwark.browser/navigate! "https://example.com"))
(bulwark.hooks/add-hook! :page/title-changed :repl-title
  (fn [event] (bulwark.native/set-echo! (:title event))))
```

Define a command and bind its name:

```clojure
(bulwark.commands/defcommand reload-from-repl "Reloads the active view." []
  (bulwark.browser/reload))
(bulwark.keymap/bind-key! "C-c t" 'reload-from-repl)
```

Later definitions change the function that the existing binding calls.
Command keys still reach Servo until key interception is complete.

## Admission and cleanup

Each admission turn starts at most four jobs.
A one-millisecond timer supplies subsequent turns and skips missed ticks.
The server permits 32 unfinished jobs globally and 24 per session, including parked and queued jobs.
Request frames have a 64 KiB limit. Queued output has a 256 KiB limit per connection.
Protocol `describe` and `interrupt` remain available under evaluation overload.

The profile port file uses exclusive creation and Unix mode `0600`.
A second process cannot overwrite or remove an existing port file through normal startup.
The runtime closes the listener and connections before it removes the port file.
This cleanup also occurs after runtime errors, dispatcher errors, and native window closure.

Forced process termination can leave a stale port file.
After the owning process exits, remove its stale `.nrepl-port` file or select another profile.
The profile directory itself remains after shutdown.

The inherited upstream output limit permits one oversized reply when the connection queue is empty.
Connection count, session count, retained values, and the language heap remain outside these limits.
The limits do not bound total process memory.
Local clients can evaluate code with the process permissions. The listener has no authentication.

## Product checks

Run the mock checks:

```sh
cargo build --locked -p bulwark-shell
python3 experiments/run_product_nrepl.py
```

Run the native checks:

```sh
cargo build --locked -p bulwark-shell --features native
python3 experiments/run_product_nrepl.py --native
```

The runner checks overload refusal, protocol interruption, capacity recovery, and cancellation before queued forms produce side effects.
It also checks browser calls from another session during parked and CPU-bound evaluations, a 20-form ordered burst, and live command redefinition.
The live command check dispatches a key sequence through the Clojure keymap after redefinition.
A hook installed through nREPL receives a title after navigation through the product bridge.

The mock suite covers normal completion, runtime errors, and dispatcher errors with a parked client evaluation.
The native suite adds window closure with a parked evaluation.
Every case requires listener closure and port-file removal.
The normal case also starts a second process against the active profile and requires refusal without damage to the first listener.

Local macOS checks passed for all three mock cases and all four native cases.
The native normal case painted eight frames, made eleven presentations, and joined the runtime worker.
All native cases reported `shutdown=joined`.
Formatting, workspace Clippy, all 33 workspace tests, native/Servo Clippy, and all five native unit tests passed.
The Servo contract, seven existing native process cases, and both product smokes also passed.
The software Servo smoke painted thirteen frames and joined the worker.
These results do not establish interactive Linux behavior or rendering progress during a raw runtime stall.

The [previous CI run](https://github.com/csm/bulwark/actions/runs/42580601137) passed both runtime jobs, Servo Clippy, and native unit tests.
Its Servo job reached the 30-minute deadline during another SpiderMonkey compilation.
The workflow now uses one native feature configuration for product Servo checks and a 60-minute job deadline.
Both runtime jobs run the product nREPL suite. The Linux Servo job also runs the native nREPL suite.
The [first branch CI run](https://github.com/csm/bulwark/actions/runs/42675768819) passed both runtime jobs, including the mock nREPL suite.
Linux Servo Clippy, native unit tests, the Servo contract, and the software smoke also passed.
The native browser-history case timed out before the native nREPL suite ran.
The browser-history test now waits for fresh load-completion events after reloads and navigations.
Its waits report the unfinished phase, and the process runner retains captured diagnostics after a timeout.
The browser-history case passed ten consecutive local runs after the fix.
The [final branch CI run](https://github.com/csm/bulwark/actions/runs/42681801088) passed all three jobs at code commit `e9ecc8d`.
Linux passed all seven native process cases and all four native nREPL cases.
Both runtime jobs passed the mock nREPL suite.
