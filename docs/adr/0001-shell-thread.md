# D1: Shell thread ownership

Status: provisional, 2026-10-27. E1 is incomplete.

One shell thread owns Servo, Clojurust, the Tokio LocalSet, and the nREPL poller.
Background threads exchange plain data with that thread.

E1 shows that host-driven async and nREPL can make progress together.
The initial headless run did not meet the frame criterion.
The native redraw experiment and isolated timing measurements remain open.

A LocalSet time slice cannot preempt synchronous evaluation.
A zero timeout on the nREPL poller does not bound queued work.
E6 must establish the scheduling policy before this decision becomes final.

## Fiber update, 2026-11-02

Clojurust 0.2.34 runs `go`, `future`, `thread`, and `io-thread` bodies on fibers on the shell thread.
E6 measured them in a 60 Hz loop. The provisional policy for Clojure work on the shell thread:

- The host drives the LocalSet once per frame for a fixed budget, through a root fiber parked on a timer.
- Run Bulwark with `CLJRS_FIBER_SLICE_US` at 1–2 ms. A CPU-bound fiber can overrun the budget by one slice.
- Host callbacks call handlers directly. A handler that waits spawns a fiber or sends to a channel.
- Long or waiting work belongs in a fiber, never on the carrier or in a root-fiber evaluation.
- Native functions that block call `cljrs_value::parking::blocking`. A thread-blocking call freezes every fiber.
- nREPL forms run on root fibers. Until csm/clojurust#447, a parked nREPL form blocks the frame loop.
  Until csm/clojurust#446, interrupt cannot stop it.

Stop-the-world collection remains a frame risk. E1's native redraw run is still pending.
