# Runtime-thread nREPL integration

Status: budgeted E7 integration at `b847a29b`, 2026-11-07.
This code remains an experiment. Upstream admission mechanisms now support explicit host limits.

## Execution and ownership

E7's `--nrepl` program starts the loopback listener on the Clojurust runtime thread.
The network thread decodes requests and handles protocol interrupts.
`Poller::spawn_pending_budget` admits jobs onto the runtime's existing Tokio LocalSet.
Each evaluation runs on a Clojurust fiber and shares globals with the browser program.
Jobs in one session retain their arrival order. Other sessions can run while that session parks.

The host checks for admission every millisecond and skips missed timer ticks.
It passes `tiered::eval` directly to `spawn_pending_budget` because that API already creates an evaluation fiber.
The host does not enter a second root executor from an evaluation.
Only owned browser requests and replies cross to the engine thread.
Ordinary Clojure helpers can await browser futures through their evaluation fiber.

The host admits at most four jobs per turn and returns to the LocalSet between turns.
A later timer tick admits the next batch, including after budget exhaustion.
The server limits accepted work to 32 jobs globally and 24 per session.
These limits include queued, running, and parked evaluations.
It also limits request frames to 64 KiB and queued output to 256 KiB per connection.
The driver records maximum batch size, sampled pending count, budget exhaustion, and admission duration.
The timer does not impose an elapsed-time guarantee.

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

The Python probe uses three real client connections and separate nREPL sessions.
It first fills one session and requires overload refusal while another session still calls the browser.
It then fills global capacity and requires refusal there too.
`describe` and `interrupt` remain available under overload.
All cancelled jobs return interrupted status, accepted capacity recovers, and queued forms cause no side effects.
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

## Historical results at c6199fae

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

## Results at b847a29b

The pending maximum is a sample from host admission turns. It can miss the exact peak between turns.
The network-order barriers and refusal assertions establish saturation independently of that sample.

| Backend | Parked interrupt | CPU interrupt | Maximum admission | Sampled pending maximum | Frames during CPU work |
|---|---|---|---|---|---|
| Mock | 0.203 ms | 0.106 ms | 27.750 microseconds | 26 | No renderer |
| Software Servo | 0.540 ms | 1.737 ms | 32.291 microseconds | 32 | 9 painted |
| Native Servo | 0.239 ms | 0.149 ms | 16.666 microseconds | 32 | 16 presented |

All three backends passed overload refusal, interruption under overload, recovery, and the existing concurrency assertions.
Each backend admitted at most four jobs per call and reported twelve budget exhaustions.
Native close with a parked evaluation also passed cleanup, with a maximum admission duration of 14.917 microseconds.
These durations are observations, not guarantees.

## Remaining limits

The upstream output limit permits a single oversized response when its queue is empty.
Request and output limits do not bound total memory across connections, sessions, retained values, or active language tasks.
Product code must apply the [D1 queue contract](adr/0001-shell-thread.md) before it claims product acceptance.
The legacy `poll_timeout` API in E1 remains a historical comparison.
The [gap register](clojurust-async-gaps.md) records current adoption results and focused upstream coverage.
