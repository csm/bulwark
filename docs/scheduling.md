# Phase 0 scheduling evidence and policy

Status: historical shared-thread evidence, 2026-10-27.
The owner now requested investigation of separate engine and runtime threads.
The [E7 async bridge report](async-bridge.md) describes that direction and its results.

## Measurements

The E1 driver now reports separate clocks for runtime work and native redraw callbacks.
Each clock targets one callback every 16.67 ms.
`skipped_deadlines` counts schedule slots that expire during a callback or a delay.
The clock advances to the next future slot instead of issuing catch-up callbacks.
`gaps_over_25ms` remains a separate diagnostic counter.
Neither counter measures display presentation times.

`max_poll` measures one nREPL poll, including all evaluations that it drains.
`max_slice` measures one LocalSet turn, including the one-millisecond timer.
Native measurement starts at the first redraw callback.
The report records the delay from window creation to that callback separately.
Extra redraw requests from window creation or exposure do not advance the schedule.
Unit tests cover the schedule and skipped-slot calculation without wall-clock delays.

The E6 probe uses two real loopback connections.
A streamed marker proves that the infinite Clojure form started before the second connection sends its interrupt.
The probe requires an `interrupted` response and a successful evaluation in the same session afterward.
It also requires async progress after recovery.

The probe then runs a `go` task with a 200 ms blocking call.
Finally, it queues twenty evaluations that each block for 10 ms.
The host counters distinguish LocalSet stalls from nREPL stalls.
These deliberately blocking calls establish the limits of cooperative scheduling.

Measurements used macOS 26.5.2 arm64, Rust 1.97.1, and debug E1 executables.
The final comparisons ran sequentially after compilation finished.

| Probe | Runtime evidence | Native redraw evidence |
|---|---|---|
| Initial 60-second inline run | 544 async ticks, 497 evaluations, maximum poll 0.913 ms | 3,597 callbacks, two skipped deadlines, maximum gap 58.276 ms |
| Initial 60-second worker run | 542 async ticks, 498 evaluations, no skipped runtime deadlines | 3,598 callbacks, one skipped deadline, maximum gap 49.938 ms |
| E6 inline, 8 seconds | Interrupt 0.307 ms, maximum poll 237.554 ms, maximum slice 202.183 ms | 439 callbacks, 40 skipped deadlines, maximum gap 253.029 ms |
| E6 worker, 8 seconds | Interrupt 0.286 ms, maximum poll 275.862 ms, maximum slice 209.961 ms | 477 callbacks, two skipped deadlines, maximum gap 50.211 ms |
| Inline 60-second repeat after initial redraw | 544 async ticks, 496 evaluations, maximum poll 0.753 ms | 3,598 callbacks, one skipped deadline, maximum gap 46.597 ms at scheduled frame 1 |
| E6 worker repeat after initial redraw, 8 seconds | Interrupt 0.314 ms, maximum poll 269.708 ms, maximum slice 210.230 ms | 478 callbacks, one skipped deadline, maximum gap 44.750 ms at scheduled frame 1 |
| E6 headless final check, 8 seconds | Interrupt 0.292 ms, session and async recovery passed | No native window |

The first four runs counted window startup as a frame gap.
A separate five-second trace placed the maximum native gap at frame 1 (53.146 ms).
Current runs report that delay separately and measure the full interval after the first redraw.
This distinction does not remove later frame gaps or change the timing assertion.
The 60-second repeat still failed `--require-no-skips`.
It recorded 7.400 ms from window creation to the initial redraw, then a 46.597 ms gap at the first scheduled frame.
The remaining delay therefore extends beyond the initial redraw callback.
The full E1 criterion remains unmet.

The E6 results prove interruption and recovery for this tiered interpreter host.
They do not cover JIT, AOT, or interruption inside a blocking native call.
The E6 worker result shows that runtime stalls do not force equal native redraw stalls.
It does not establish display frame rate or a scheduling guarantee.

## Earlier shared-thread policy

- Servo and all native window resources stay on the shell thread.
- Runtime values stay on their owning thread. Only plain data can cross a thread boundary.
- Event dispatch stops between callbacks after its time budget expires. One synchronous callback can still exceed that budget.
- Shell-thread callbacks must return promptly. Async tasks must await between bounded units of work.
- A `go` block does not make a blocking call safe for the shell thread.
- Blocking I/O and expensive computations require background execution.
- The current nREPL poll is unrestricted trusted execution. Its timeout limits the wait for work, not evaluation time or queue length.
- nREPL interruption can stop instrumented Clojure evaluation. It cannot preempt a blocking native call such as `Thread/sleep`.
- A long native call must return before the shell-thread model can recover. Evaluation errors must not invalidate live browser handles.

The experiment keeps the single-thread model as its default.
`--worker` constructs the runtime, LocalSet, and nREPL poller on a dedicated thread.
That thread also destroys its runtime values.
The main thread continues native redraw callbacks independently.
This comparison measures scheduling isolation. It does not implement the browser command queue or measure queue latency.
It therefore does not settle D1.

## Earlier remaining decisions

D1 remains provisional until E1 passes and the scheduling policy is final.
Interruption already works in the pinned Clojurust revision, so this probe needs no upstream interrupt patch.
Bounded nREPL polling remains an upstream candidate under Q4.
That change needs a job limit or elapsed-time limit between jobs, plus regression coverage.
It cannot bound one synchronous evaluation.

The dedicated-thread alternative also needs a command/event queue experiment before adoption.
That experiment must measure callback ordering, request latency, shutdown, and resource invalidation.
E3 still needs interactive input observation. Automated redraw counters do not satisfy that check.

E7 now implements the command/event queue experiment with real Servo.
The [D1 ADR](adr/0001-shell-thread.md) records the current direction and remaining review.
The measurements on this page remain historical evidence, not the acceptance gate for the separate-thread design.
