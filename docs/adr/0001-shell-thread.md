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
