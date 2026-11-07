# Runtime-thread nREPL integration

Status: E7 integration probes passed on macOS, 2026-11-06.
This code remains an experiment. It does not close the upstream admission-limit gap.

## Execution and ownership

E7's `--nrepl` program starts the loopback listener on the Clojurust runtime thread.
The network thread decodes requests and handles protocol interrupts.
`Poller::spawn_pending` admits jobs onto the runtime's existing Tokio LocalSet.
Each evaluation runs on a Clojurust fiber and shares globals with the browser program.
Jobs in one session retain their arrival order. Other sessions can run while that session parks.

The host checks for admission every millisecond and skips missed timer ticks.
It passes `tiered::eval` directly to `spawn_pending` because that API already creates an evaluation fiber.
The host does not enter a second root executor from an evaluation.
Only owned browser requests and replies cross to the engine thread.
Ordinary Clojure helpers can await browser futures through their evaluation fiber.

The admission timer schedules checks when the runtime can run. It does not bound admission work.
The pinned API drains its incoming queue without a job or time limit.
The driver records admission-call duration and leaves this limit explicit.
No permanent local dependency patch or protocol proxy supplies a hidden workaround.

## Reproduction

Build the portable probe:

```sh
cargo build --locked -p bulwark-experiment-e7
```

Run the mock integration probe:

```sh
python3 experiments/run_e7_nrepl.py
```

Build the Servo version:

```sh
cargo build --locked -p bulwark-experiment-e7 --features servo
```

Run the real-engine probes:

```sh
python3 experiments/run_e7_nrepl.py --servo
python3 experiments/run_e7_nrepl.py --native
python3 experiments/run_e7_nrepl.py --native --close-window
```

The Servo commands need macOS graphics access outside the Codex filesystem sandbox.
All modes need permission to bind a local loopback listener.
Each child has an independent process deadline and temporary port file.
The native close case schedules window close after 1.5 seconds.

For an editor connection, start the native probe directly:

```sh
python3 experiments/run_e7.py --native --nrepl --nrepl-port-file /tmp/bulwark-e7-port
```

The listener address appears on stdout and the selected port appears in the file.
The experiment still has a 30-second deadline.
A synchronous helper can call the browser from an nREPL evaluation:

```clojure
(bulwark.experiment.nrepl/eval-js! "document.title")
```

## Probe coverage

The Python probe uses two real client connections and separate nREPL sessions.
It requires a streamed marker before it interrupts a long sleep or infinite loop.
While one session waits or computes, the other session calls the engine and reads the browser-program heartbeat.
Both active evaluations must interrupt, and their sessions must recover.

The probe queues another evaluation behind the parked evaluation in the same session.
It then requires that queued evaluation to complete after interruption.
A separate burst contains twenty evaluations with 20 ms sleeps and ordered atom updates.
Responses and atom contents must preserve the submission order while the other session continues browser calls.
Redefinition in one session must change the function seen by the other session without a restart.

The final evaluation parks for 60 seconds.
The probe then stops the browser program or closes the native window.
The runtime must join, the existing client must disconnect, the listener must close, and the port file must disappear.
The probe does not require the final stop request's response to drain before listener shutdown.
Forced process termination still has the cleanup limits in the [native shutdown contract](native-async-bridge.md).

## Observed results

Platform: macOS 26.5.2 arm64, Rust 1.97.1, debug builds.
Pins: Servo 0.7.0 and Clojurust `c6199faea544e029db7222aa3e9f5c9f64952773`.

| Mode | Parked-evaluation interrupt | CPU-evaluation interrupt | Engine turns during CPU test | Maximum admission call |
|---|---|---|---|---|
| Mock | 0.573 ms | 0.148 ms | 105 | 18.500 µs |
| Software Servo | 99.080 ms | 0.187 ms | 180 | 336.250 µs |
| Native Servo | 91.372 ms | 0.167 ms | 165 | 21.083 µs |

All three modes passed the twenty-job order assertion, cross-session browser calls, heartbeat progress, redefinition, and shutdown checks.
Interrupt duration covers the protocol request through receipt of the interrupted response.
Admission duration excludes the later evaluation work on the LocalSet.
Software Servo painted 17 frames during the CPU test. Native Servo presented 17 frames.
The separate native close case joined the worker with an evaluation still parked.
Its maximum admission call was 21.792 µs.
These are observed durations under finite load, not latency guarantees or limits on admission work.
The existing GL texture warning still appears.

## Remaining upstream mechanisms

CLJ-ASYNC-003 still requires an admission API with a job budget and an explicit batch result.
The host needs to know whether it exhausted the budget so it can schedule another turn.
Pending-job limits must include jobs waiting in per-session lanes and evaluations already running or parked.
Protocol interrupts must remain available when ordinary admission reaches its limit.
Request size and output buffering need separate limits before the host can claim bounded memory use.

The legacy `poll_timeout` API in E1 remains a historical comparison.
E7 now demonstrates the fiber-based integration path, but cannot impose these missing bounds through the pinned public API.
The background GC busy loop also remains open under CLJ-ASYNC-004.
The [gap register](clojurust-async-gaps.md) tracks both adoption requirements.
