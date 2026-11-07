# Phase 0 experiments

These programs exercise integration assumptions before product implementation.
Promote their code only after the Phase 0 review.

## E2: Rust calls Clojurust

```sh
cargo run --locked -p bulwark-experiment-e2
```

The probe delivers 10,000 ordered maps in each execution mode: tree-walk and tiered.
It forces GC inside every 100th callback and again after that call returns.
It measures live objects after warmup.
It also checks exception propagation and dispatcher redefinition.
The tiered probe uses the IR interpreter. It does not install a Cranelift JIT backend.

The host resolves the dispatcher var for every call.
`apply_value` roots the function and argument values during the call.
Allocation frames end after each event, so temporary maps can become unreachable.
The host must root any returned GC value that survives another evaluation or collection.
E2 registers `bulwark.native/collect!` only for the probe. The product bootstrap does not expose this function.

Initial result on macOS arm64, Rust 1.97.1, debug build:

| Mode | Elapsed for the probe | Live objects after warmup |
|---|---|---|
| Tree-walk | 3.444 s | 5,226 throughout |
| Tiered | 422.489 ms | 9,933 throughout |

E2 passed for these execution modes. This result does not cover JIT or AOT callbacks.

## E1: Host drives async and nREPL

```sh
cargo build --locked -p bulwark-experiment-e1
python3 experiments/run_e1.py
```

The headless driver targets 60 ticks per second for 60 seconds.
The Python client evaluates forms on the loopback nREPL connection at the same time.
The driver reports async ticks, nREPL evaluations, and timing gaps.

For a native window, run:

```sh
cargo run --locked -p bulwark-experiment-e1
```

For a repeatable native run with the concurrent client, run:

```sh
python3 experiments/run_e1.py --native
```

The driver measures skipped frame deadlines, nREPL poll duration, and LocalSet turn duration.
It reports runtime ticks and native redraw callbacks separately.
Extra redraw requests from window creation or exposure do not advance the schedule.
The native clock starts at the first redraw callback.
The report records the delay from window creation to that callback separately.
These counters measure callbacks, not display presentation times.

For the strict timing assertion, add `--require-no-skips`.
The default command requires async progress, nREPL progress, and native redraws.
Its success status alone does not establish the E1 timing criterion.

For the dedicated-thread comparison, run:

```sh
python3 experiments/run_e1.py --native --worker
```

The worker owns all Clojurust values, the LocalSet, and the nREPL poller.
The shell thread owns the native window.
This probe measures scheduling isolation. It does not implement a browser command queue.

The window title shows the redraw count. Its client connection is optional for this command.
In another terminal, connect the probe to the port from the output:

```sh
python3 experiments/nrepl_probe.py PORT --seconds 50
```

For a synchronous stall, run:

```sh
python3 experiments/run_e1.py --seconds 8 --code '(do (Thread/sleep 2000) (swap! bulwark.experiment/nrepl-evals inc))'
```

The LocalSet slice is cooperative. It does not interrupt a long Clojure function.
The nREPL zero-timeout poll also permits an unbounded amount of evaluation work.

Initial 60-second headless result:

- 3,432 host ticks.
- 541 async ticks.
- 463 successful nREPL evaluations.
- 112 gaps greater than 25 ms. The maximum gap was 159.001 ms.

The run overlapped Servo compilation and used a locked Mac.
It establishes concurrent progress, but does not pass the E1 frame criterion.
The 25 ms counter is diagnostic. It does not measure every missed 16.67 ms deadline.
After the main Servo build, another 60-second run recorded 3,600 ticks and 498 nREPL evaluations.
Its async loop advanced 545 times, with no gaps greater than 25 ms and a maximum gap of 22.846 ms.
A five-second native window run delivered 291 redraws, but no nREPL client was active.
The two-second synchronous stall probe recovered and recorded a maximum gap of 2.023 seconds.
These initial results predate the skipped-deadline counter.
The later native measurements and scheduling limits appear in [the scheduling report](../docs/scheduling.md).
Visible stall/recovery observation remains open because native UI inspection timed out in this session.

## E6: Interruption and cooperative execution

```sh
python3 experiments/run_e6.py
python3 experiments/run_e6.py --native
python3 experiments/run_e6.py --native --worker
```

The probe interrupts an active infinite loop from a second connection.
It requires a successful evaluation in the same session after interruption.
It then runs a blocking `go` task and twenty queued blocking evaluations.
It requires async progress after these stalls.

`max_poll` shows the cost of the queued evaluations.
`max_slice` shows the blocking `go` task overrunning the cooperative slice.
The worker comparison separates runtime stalls from native redraw delays.
The [scheduling report](../docs/scheduling.md) records the policy and remaining decisions.
CI runs the headless interruption and recovery probe on macOS and Linux.

## E3: Servo window and input

```sh
cargo build --locked -p bulwark-experiment-e3 --features servo
cargo run --locked -p bulwark-experiment-e3 --features servo -- https://servo.org
cargo run --locked -p bulwark-experiment-e3 --features servo -- https://en.wikipedia.org
```

The probe forwards pointer movement, mouse buttons, wheel events, keys, modifiers, focus, resize, and display scale changes.
The initial keyboard mapping uses shared enum names and falls back to unidentified keys.
It maps Space explicitly and preserves key locations.
IME remains outside this probe.

Q5 selects single process first. Complete this baseline before the multiprocess comparison.
Compare page rendering, input, startup, memory, and crash containment with the same fixtures.
Include JavaScript evaluation and resource interception from E4 before a default change.

For a bounded screenshot run, use:

```sh
cargo run --locked -p bulwark-experiment-e3 --features servo -- --screenshot /tmp/bulwark-servo.png https://servo.org
```

The probe saves the image after Servo reports render readiness, then exits.
It returns an error if capture fails or takes more than 30 seconds.

For a local input fixture, use its absolute `file://` URL:

```sh
cargo run --locked -p bulwark-experiment-e3 --features servo -- "file://$PWD/experiments/fixtures/input.html"
```

Click the button and enter text in the input.
The window title changes for each action.
Scroll to the end marker, then resize the window.

Initial result: Servo built on macOS arm64 and rendered servo.org and Wikipedia.
Both bounded screenshot captures passed. Visual inspection showed page text, images, and layout.
Wikipedia showed black placeholder squares for some icons, which need investigation.
The initial debug executable was 397,281,016 bytes (approximately 379 MiB).
The linker reported an oversized `__eh_frame` section.
A later clean target build completed in 2m 30s with the local Cargo source cache.
That command built E1 and E3 together with E3's `servo` feature.
The resulting E3 debug executable was 400,960,584 bytes (approximately 382 MiB).

The first release build completed in 194.83 seconds with a warm source cache and an empty release target.
It overlapped runtime checks, so it is not an isolated build benchmark.
The release executable was 131,904,848 bytes (approximately 126 MiB).
The release log did not contain the debug linker's unwind warning.
These measurements used macOS 26.5.2 arm64 and Rust 1.97.1.
The release executable also captured the local input fixture successfully.
Visual inspection showed its heading, text field, and button.
The run emitted a GL texture warning despite the successful capture.
Linux behavior and interactive input checks remain open.

E4 and E5 remain follow-up experiments in the development plan.

## E7: Pure async user code and a separate engine thread

```sh
cargo build --locked -p bulwark-experiment-e7
python3 experiments/run_e7.py
python3 experiments/run_e7.py --idle
python3 experiments/run_e7.py --probe-keyword-await
```

For the real Servo backend, run:

```sh
cargo build --locked -p bulwark-experiment-e7 --features servo
python3 experiments/run_e7.py --servo
```

The runtime owns a continuously driven LocalSet on its dedicated thread.
Native operations return futures backed by bounded requests and one-shot replies.
The main thread owns Servo and sends lifecycle events through a bounded queue.
The [user example](e7-async-bridge/user.cljrs) runs browser helpers with `^:async` and `await`.
The [separate stall probe](e7-async-bridge/stall.cljrs) deliberately blocks the runtime for 200 ms.

The real Servo run passed its async assertions, advanced page timers, and painted six frames during that stall.
The mock also completes batches in reverse order to exercise reply correlation.
The [async bridge report](../docs/async-bridge.md) records the protocol, measurements, and limits.

The keyword-await command is a diagnostic for a pinned upstream limitation.
It terminates a blocked child after three seconds.
Idle await polling also consumes CPU. The probe does not claim efficient idle suspension or support for every await expression.
All runner modes have a process deadline because cooperative cancellation cannot stop every blocking native wait.

## Local validation

Rust formatting and Clippy passed for the workspace and the Servo probe.
The workspace test command passed, including two E1 schedule tests and five E7 bridge tests.
The experiment programs provide the current integration assertions.
Python syntax checks and the five-second concurrent nREPL smoke run passed.
The CI workflow remains unverified on remote runners.
