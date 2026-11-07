# Native async bridge and shutdown

Status: E7 native workflow and six lifecycle cases passed on macOS, 2026-11-06.
This code remains an experiment. The product shell still contains the Phase 0 bootstrap.

## Ownership and wakeups

The main thread owns winit, Servo, the webview, the GL context, and the native window.
A dedicated thread owns Clojurust, its fibers, GC values, Tokio executor, and LocalSet.
Only owned request, reply, and event data crosses the boundary.

The bridge now accepts a thread-safe wake callback.
The headless driver uses this callback to unpark its thread.
The native driver sends an `EventLoopProxy` event after a request enters the bounded queue.
Servo wakeups use the same event loop.
Worker completion and panic also wake the native loop.

Each native turn handles at most four requests before a Servo turn.
Any remaining queued requests schedule another proxy event.
Frame notifications request a native redraw. The redraw callback paints and presents the webview.
Normal operation has no millisecond polling timer.
The loop uses short timed checks only during shutdown or the worker completion race.
The 30-second experiment deadline supplies a separate failure boundary.

The native probe permits one live webview at a time.
It forwards the E3 input mechanisms, including pointer, keyboard, wheel, focus, resize, and scale changes.
It does not add product chrome, nREPL, or a production event overload policy.

## Shutdown contract

Window close, timed close, runtime completion, runtime failure, and deadline expiry start shutdown.
The host raises the runtime interrupt flag and sends cancellation.
It then closes the request receiver and drops the engine, pending replies, and window on the main thread.
Queued operations do not run after this point.
An operation that already ran can retain its side effects.

The host joins a completed worker without blocking the native event loop.
During shutdown, it checks completion every five milliseconds for at most two seconds.
Intentional close can race with cancellation and disconnected replies.
The host records the worker result and accepts an orderly join for this intentional stop.
An unsolicited runtime error fails the probe. A worker panic always fails the probe.

If a raw native call prevents shutdown, the host reports `shutdown=unjoined` and exits with an error.
The process exit terminates the remaining thread.
This outcome does not claim that runtime destructors, fiber cleanup, or native operation cancellation completed.
The Python runner also imposes an independent 35-second deadline for each child.

The headless driver now applies the same two-second limit before it joins a worker.
It no longer unconditionally waits for a failed worker to return.

## Reproduction

Build the probe:

```sh
cargo build --locked -p bulwark-experiment-e7 --features servo
```

Run the native workflow:

```sh
python3 experiments/run_e7.py --native
```

Run the six lifecycle cases:

```sh
python3 experiments/run_e7_native.py
```

Open the interactive fixture:

```sh
python3 experiments/run_e7.py --native --interactive
```

The interactive program creates its page from Clojure and updates a visible counter every 100 ms.
It uses a parking `Thread/sleep` between JavaScript requests.
The page also contains an input, button, and scroll marker.
Closing the window cancels the program. The experiment deadline still applies.

On macOS, these commands need native graphics access outside the Codex filesystem sandbox.
The lifecycle cases open and close real windows and assert process outcomes.
The failure cases deliberately emit errors or panic messages.

## Observed results

Platform: macOS 26.5.2 arm64, Rust 1.97.1, Servo 0.7.0, debug build, single process.
Clojurust pin: `c6199faea544e029db7222aa3e9f5c9f64952773`.

| Case | Observation | Outcome |
|---|---|---|
| Native workflow | 119 requests completed and 20 presentations during the raw 200 ms runtime stall | Runtime joined with success |
| Close during a pending await | Timed close after 100 ms | Runtime joined with success |
| Interactive command loop | Five requests handled before timed close at 500 ms | Runtime joined with success |
| Runtime exception | Injected exception woke the host | Runtime joined with the expected error |
| Runtime panic | Injected panic woke the host | Runtime joined with the expected error |
| Unresponsive native call | Five-second Rust sleep exceeded the two-second shutdown allowance | Error after 2.314 seconds, worker unjoined |

The first native workflow run presented 18 frames during its stall.
The initial six-case run presented 17 frames. The final run presented 20 and recorded 414 proxy wakeups during the full workflow.
These counters measure callbacks and presentations, not display refresh rate.
The final workflow's runtime duration was 476.512 ms.
Its child took 5.930 seconds including executable startup, graphics setup, and shutdown.

The interactive fixture also completed 295 native requests before its 30-second experiment deadline.
It joined the worker and reported the deadline as an error.
Computer Use inspection timed out, so this continuation does not claim manual E7 input observations.
The earlier [E3 native input results](native-macos.md) remain separate evidence.
The existing GL texture warning still appears.

## Remaining integration work

The runtime's background GC service still spins while idle.
The native wakeup path does not correct that upstream issue or claim efficient whole-process idle operation.
The [async gap register](clojurust-async-gaps.md) records its reproducer.

The [nREPL continuation](nrepl-runtime.md) now passes its runtime-thread integration probes.
Next work includes upstream admission and pending-work bounds, idle GC scheduling, and product event queue policies.
The ownership review must also settle payload limits and shutdown behavior for application state persistence.
These requirements still gate promotion into the product crates.
