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
At the 0.2.34 pin, E2 passed again on Linux x86_64 (debug): 9.04 s tree-walk, 1.25 s tiered, constant live objects.

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

Since the Clojurust 0.2.34 pin, E1 uses fibers (virtual threads).
The go-loop uses the plan's `(<! (timeout 100))` form, and `<!` parks the go block's fiber.
Startup forms and nREPL forms run on root fibers through `cljrs_async::fiber::run_root`.
The LocalSet slice is a root fiber parked on a 1 ms timer.
Fibers yield at safepoints when their time slice expires.
An nREPL form that parks still blocks the frame loop until it completes ([csm/clojurust#447](https://github.com/csm/clojurust/issues/447)).

Fiber result on Linux x86_64, Rust 1.97.0, debug build, 2026-11-02:

- 3,596 host ticks, 538 async ticks, and 497 successful nREPL evaluations in 60 seconds.
- 2 gaps greater than 25 ms. The maximum gap was 83.4 ms.
- The two-second stall probe still froze the loop for 2.018 s per evaluation.
  The go-loop advanced 77 times in 8 seconds because the parked evaluation let it run.
- The native window run was not repeated on this Linux container.

Pre-fiber result at the previous pin (macOS arm64), kept for comparison.
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
On Clojurust 0.2.34 (Linux x86_64, debug), the probe still passes: interrupt latency 0.65 ms, `max_poll` 220 ms.
`max_slice` fell to 4.2 ms because the blocking `go` task now parks its fiber instead of blocking the slice.

## E6: Clojure time budgeting with fibers

This probe entered through `main` with its release 0.2.34 findings.
The commands now use the workspace's current `c6199fae` pin.
The recorded 0.2.34 timings and defects below remain historical evidence.
Current parked-interrupt and cross-session behavior also has coverage in `run_e7_nrepl.py`.

```sh
cargo run --locked -p bulwark-experiment-e6
cargo run --release --locked -p bulwark-experiment-e6 -- --only cpu-fiber
CLJRS_FIBER_SLICE_US=1000 cargo run --release --locked -p bulwark-experiment-e6 -- --only cpu-fiber
```

The headless probe runs a 60 Hz loop on one shell thread.
Each frame does host work and then gives the LocalSet a 4 ms slice (`--budget-ms`).
The slice is a root fiber parked on a timer.
Each scenario reports frame gaps, the longest LocalSet drive, and Clojure-side progress.
Options: `--only NAME`, `--mode tiered|tree-walk`, and `--budget-ms N`.
`E6_TRACE=1` prints heap counts around collections.
The probe asserts results only. Timing is reported, not asserted, because CI runners are noisy.

| Scenario | Workload |
|---|---|
| `idle` | E1's `<!` go-loop alone |
| `cpu-fiber` | A `future` runs a CPU loop with no parks |
| `cpu-inline` | A one-second CPU loop on the carrier, then on a root fiber |
| `parked-waits` | 100 futures run `(Thread/sleep 1000)`; a go block dereferences a future |
| `native` | A Rust function that blocks the thread, then the same wait through `parking::blocking` |
| `dispatch` | 10,000 host events, one fiber each, 100 per frame; every tenth handler parks; collection every tenth frame |
| `dispatch-cost` | Per-event cost of `apply_value` versus one fiber per call |
| `nrepl` | Parked and running evaluations, and interrupts from a second connection |

Result on Linux x86_64, Rust 1.97.0, release build, tiered mode, 2026-11-02:

| Scenario | Result |
|---|---|
| `idle` | 180 frames, maximum gap 21.0 ms, 30 go-loop ticks in 3 s |
| `cpu-fiber`, 10 ms slice | Maximum drive 10.4 ms, maximum gap 27.1 ms, 1 gap over 25 ms |
| `cpu-fiber`, 1 ms slice | Maximum drive 6.5 ms, maximum gap 22.4 ms, no gap over 25 ms |
| `cpu-inline`, carrier | 1.0 s freeze; the go-loop stopped |
| `cpu-inline`, root fiber | 1.0 s freeze; the go-loop continued at safepoint yields |
| `parked-waits` | All 100 sums and the go result were correct; maximum gap 21.6 ms |
| `native`, thread block | 317 ms freeze |
| `native`, offloaded | Maximum gap 21.8 ms |
| `dispatch`, run alone | 10,000 results correct, 1,000 parked handlers, at most 111 live fibers, maximum gap 23 ms |
| `dispatch-cost` | 3.3 µs per direct call, 25.6 µs per fiber call |
| `nrepl` | See below |

In the full suite, the first collection in `dispatch` also traced garbage left by earlier scenarios and took 1.7 s.
Tree-walk mode gave the same pattern. Debug builds are about 5 times slower per fiber.
In debug, dispatch fell behind 100 events per frame and peaked at 4,983 live fibers.

nREPL with root-fiber evaluation:

| Request | Frame gap | Outcome |
|---|---|---|
| `(Thread/sleep 500)` | 517 ms | The go-loop advanced 5 times during the parked evaluation |
| `(future (Thread/sleep 500))` | 16 ms | Returns at once |
| CPU loop, interrupted after 300 ms | 301 ms | `interrupted` 0.3 ms after the interrupt |
| `(Thread/sleep 3000)`, interrupted after 300 ms | 3.0 s | The interrupt waited 2.7 s for the sleep |

Findings:

- Parking works on the shell thread. Timer waits, future dereferences in `go`, and offloaded Rust calls do not delay frames.
- Safepoint time slices bound CPU-bound fibers. The worst LocalSet drive is the budget plus about one slice.
  Tokio defers a yielded fiber's wake, so one fiber does not run again in the same LocalSet tick.
  A 1 ms slice keeps the probe inside a 16.7 ms frame.
- Work on the carrier or on a root fiber still blocks the host until it completes.
  Host code and nREPL evaluations must spawn fibers for long work. A root fiber only keeps other fibers alive.
- A Rust function that blocks the thread pins every fiber. Native functions must use `parking::blocking`.
- One fiber per host event is correct under collection, but costs about 8 times a direct call.
  Use direct calls for handlers that cannot park, or a long-lived dispatcher fiber that takes events from a channel.
- An nREPL interrupt stops a running evaluation at once. It does not stop a parked evaluation
  ([csm/clojurust#446](https://github.com/csm/clojurust/issues/446)).
  `(deref (promise))` hangs that nREPL connection's evaluations permanently.
- The stall watchdog did not report a 300 ms thread block beside a CPU fiber
  ([csm/clojurust#445](https://github.com/csm/clojurust/issues/445)).
- Stop-the-world collection is the next frame risk. With `CLJRS_GC_SOFT_LIMIT_MB=64`, automatic collections inside fibers
  stayed correct but caused 203–255 ms drives. The default soft limit is one third of system memory.
- `HEAP.count()` includes objects in their one-cycle grace period. The first collection after allocation frees nothing; the second frees them.

E6's pass criterion requires an interrupt that works from a second nREPL connection.
It passes for running evaluations and fails for parked evaluations until #446 is fixed.

This failure describes release 0.2.34. The current E7 integration passes parked interruption at `c6199fae`.

After the main merge, the full E6 fiber probe passed its assertions on macOS arm64 at `c6199fae`.
Its parked-evaluation interrupt completed in 0.755 ms, and its busy-evaluation interrupt completed in 0.201 ms.
The synchronous root-fiber nREPL path still held the frame loop during evaluation, with a 512 ms maximum frame gap.
E7's `spawn_pending` integration remains the separate-thread path for the app.

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
The [native macOS report](../docs/native-macos.md) records the later interactive checks.
The fixture passed click, typing, editing, scrolling, resize, and close checks on 2026-11-06.
Native link navigation also passed on servo.org and Wikipedia.
Wikipedia's black icon squares remain visible. Linux behavior remains open.

## E4: DOM channel

```sh
cargo build --locked -p bulwark-experiment-e4 --features servo
python3 experiments/run_e4.py
```

The probe injects an agent through `UserContentManager` into a local intercepted HTTPS fixture.
It asserts 100 trivial replies and 30 serialized subtrees after five warmup calls per operation.
Every subtree contains 5,000 span records and produces 232,781 JSON bytes.
It also asserts a page-to-host `fetch` message and its acknowledgement through resource interception.
The runner imposes an independent process deadline.

The first debug run measured a trivial-call p50 of 49.292 µs and a subtree p50 of 24.741 ms.
The subtree maximum was 30.444 ms.
The [DOM channel report](../docs/dom-channel.md) records a second run, measurement limits, and remaining security work.
E4 passed its initial channel criterion. D5 selects JavaScript evaluation callbacks for DOM request results.
E5 remains necessary before multiple web pages can appear side by side.

## E7: Pure async user code and a separate engine thread

```sh
cargo build --locked -p bulwark-experiment-e7
python3 experiments/run_e7.py
python3 experiments/run_e7.py --idle
python3 experiments/run_e7.py --idle-executor
python3 experiments/run_e7.py --idle-baseline
python3 experiments/run_e7.py --require-keyword-await
python3 experiments/run_e7.py --probe-keyword-await
```

For the real Servo backend, run:

```sh
cargo build --locked -p bulwark-experiment-e7 --features servo
python3 experiments/run_e7.py --servo
```

For native window wakeups and shutdown assertions, run:

```sh
cargo build --locked -p bulwark-experiment-e7 --features servo
python3 experiments/run_e7_native.py
python3 experiments/run_e7.py --native --interactive
```

The six native cases passed on macOS, including completion, intentional close, error, panic, and unresponsive-call shutdown.
The final native workflow completed 119 requests and presented 20 frames during a 200 ms runtime stall.
The [native bridge report](../docs/native-async-bridge.md) records the contract, timing limits, and pending manual E7 input inspection.

For runtime-thread nREPL concurrency and shutdown, run:

```sh
python3 experiments/run_e7_nrepl.py
python3 experiments/run_e7_nrepl.py --servo
python3 experiments/run_e7_nrepl.py --native
python3 experiments/run_e7_nrepl.py --native --close-window
```

The mock command needs the default E7 build. The remaining commands need `--features servo`.
All four modes passed on macOS.
The probes assert cross-session engine calls, same-session order, interruption, redefinition, and shutdown with a parked evaluation.
The [nREPL report](../docs/nrepl-runtime.md) records finite-load measurements and the remaining upstream admission requirements.

The runtime owns a continuously driven LocalSet on its dedicated thread.
Native operations return futures backed by bounded requests and one-shot replies.
The main thread owns Servo and sends lifecycle events through a bounded queue.
The [user example](e7-async-bridge/user.cljrs) runs browser helpers with `^:async` and `await`.
The [separate stall probe](e7-async-bridge/stall.cljrs) uses a raw Rust call that blocks the runtime for 200 ms.
`Thread/sleep` now parks a native Clojure fiber, so it no longer supplies this deliberate stall.

The real Servo run passed its async assertions, advanced page timers, and painted six frames during that stall.
The mock also completes batches in reverse order to exercise reply correlation.
Fiber time slices can expire during the 16-request burst and let early replies free permits.
The Clojure probe therefore accepts either the correct reply or `:busy` after the first eight; a bridge unit test checks the limit deterministically.
The [async bridge report](../docs/async-bridge.md) records the protocol, measurements, and limits.

At `c6199fae`, native `go` and `^:async` bodies use stackful fibers on the Tokio LocalSet.
The strict keyword-await regression passes 11 callable and promise cases with a concurrent heartbeat.
The old diagnostic mode still terminates a blocked child after three seconds for comparisons with the previous pin.
The strict mode uses the normal process deadline, which also permits large-binary startup on a cold cache.
The new idle controls isolate a remaining busy GC service started by async initialization.
The initialized executor used 1.054 CPU-seconds for a one-second Rust timer; the control without initialization used 0.098 CPU-seconds.
The [gap register](../docs/clojurust-async-gaps.md) records current findings and historical results.
All runner modes have a process deadline because cooperative cancellation cannot stop every blocking native wait.

## Local validation

Rust formatting and Clippy passed for the workspace and the Servo probe.
The workspace test command passed, including two E1 schedule tests and six E7 bridge tests.
E4's Servo integration assertions passed on macOS, including its 5 ms trivial-call p50 target.
The experiment programs provide the current integration assertions.
Python syntax checks and the five-second concurrent nREPL smoke run passed.
The 0.2.34 update passed formatting, workspace Clippy, workspace tests, E2, E1, both E6 probes, and the E7 mock on Linux x86_64.
The CI workflow remains unverified on remote runners.
