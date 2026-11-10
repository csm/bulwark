# D1: Engine and runtime thread ownership

Status: ownership review complete for the Phase 1 contract, 2026-11-07.
Product implementation remains Phase 1 work. The [Phase 0 exit review](../phase-0-exit.md) accepts this contract.

## Ownership decision

The main thread owns `winit`, Servo, webviews, rendering contexts, input, and egui chrome.
A dedicated thread owns Clojurust, GC values, its Tokio executor, its LocalSet, and the embedded nREPL poller.
The runtime drives its LocalSet continuously, independently of display frames.
Only owned Rust requests, replies, events, and immutable key reservation snapshots cross the thread boundary.
Neither thread accesses the other's runtime or engine objects.

Browser operations return futures. Native Clojure entry points run on fibers and await those futures.
Ordinary helpers inside a fiber can also wait.
The runtime event loop resolves the current dispatcher var for each event, so redefinition takes effect without restart.
No Servo delegate waits for Clojure or calls a Clojure handler on the main thread.
Requests, Servo wakeups, worker completion, and worker panic wake the native loop through `EventLoopProxy`.

## Evidence and adoption requirements

E2 establishes callback rooting, errors, redefinition, and stable live objects in tree-walk and tiered modes.
E3 establishes the initial macOS rendering and input baseline.
E4 establishes the JavaScript callback channel for DOM results.
E7 establishes cross-thread requests, reply correlation, overload errors, handle invalidation, native wakeups, and bounded shutdown.
Its nREPL integration establishes cross-session progress, ordered session work, interruption, and recovery.
The [native report](../native-async-bridge.md) and [nREPL report](../nrepl-runtime.md) retain measurement limits.

Clojurust `b847a29b46e2b6b3fbe624f6d2394cb656ea5882` supplies both remaining upstream mechanisms.
Idle initialization now suspends without self-wakes, and collection still preserves parked roots.
Budgeted nREPL admission and accepted-work limits now support overload refusal with interrupts available.
The [gap register](../clojurust-async-gaps.md) records the focused upstream suites and Bulwark regression commands.
No local dependency patch or restriction on valid Clojure expressions supplies these results.

## Product queue contract

These values are initial product limits. E7 retains smaller transport capacities to exercise overload.
Phase 1 must implement and verify this contract before product acceptance.

| Resource | Initial product limit | Overload behavior |
|---|---|---|
| Browser requests | 32 queued, 64 unfinished | Return structured `:busy` without waiting for capacity |
| Engine admission | 16 requests or 2 ms between requests per turn | Return to native event processing, then schedule another turn |
| Ordered engine events | 256 queued | Record one fatal bridge error and begin controlled shutdown |
| Key reservations | One snapshot, 256 sequences, 16 strokes per sequence, 128 UTF-8 bytes per stroke, 16 cancellation strokes | Throw before replacement and retain the previous snapshot |
| Command string data | 1 MiB combined UTF-8 bytes per request | Return structured `:payload-too-large` before queue admission |
| Reply string data | 1 MiB combined UTF-8 bytes per reply | Return structured `:payload-too-large` |
| Event string data | 64 KiB combined UTF-8 bytes per event | Record one fatal bridge error and begin controlled shutdown |
| nREPL admission | Four jobs per one-millisecond host turn | Return to the LocalSet before another batch |
| Accepted nREPL work | 32 global jobs, 24 per session | Refuse with `overloaded` and `done` |
| nREPL request frame | 64 KiB | Close the connection after an oversized complete or incomplete frame |
| nREPL queued output | 256 KiB per connection | Use the upstream disconnect policy |

The unfinished request count includes queued requests, active engine operations, and replies that still await runtime consumption.
Each admitted request owns one permit until settlement or cancellation cleanup.
The main thread uses nonblocking channel operations. It never waits for runtime queue capacity.
The runtime also rejects requests without capacity immediately, so pending send futures cannot grow without a limit.
Each request carries an identifier and deadline. Replies can complete out of order and use their identifiers for correlation.

Ordered events preserve arrival order, including lifecycle and key input events.
The first implementation does not coalesce or silently discard these events.
An event overflow stores a terminal error outside the full event queue and wakes shutdown through a separate signal.
This policy makes lost browser state explicit. Later coalescing requires an event-specific contract and regression coverage.

Payload limits apply before admission to Bulwark queues and before conversion to GC values.
These limits do not bound allocations inside Servo, one upstream nREPL response, or the language heap.
The upstream output policy permits one oversized response when the connection's output queue is empty.
Session count, connection count, and retained language values also remain outside these bounds.
The local nREPL is a trusted development interface, enabled explicitly and bound to loopback.

## Cancellation, handles, and shutdown

A request timeout revokes queued work before engine execution when the receiver has already closed.
A timeout after engine execution starts does not promise rollback of browser side effects.
Closing a view invalidates its handle and settles its outstanding evaluation replies with a structured error.
Registry identifiers never allow an old handle to address a replacement resource, as required by D3.

Shutdown first stops new admission and invalidates native handles.
The engine rejects queued requests and drops outstanding replies before it signals runtime cancellation and interruption.
The runtime drops its nREPL poller and LocalSet on its owning thread.
The host joins the worker only after completion, with a two-second deadline.
A worker panic wakes the native loop and follows the error path.
If the worker does not stop, the host reports failure and exits the process.
Forced exit does not establish runtime cleanup, successful cancellation, or rollback.

## Native key reservations

The [key interception checkpoint](../phase-1-keys.md) adds a bounded snapshot outside the request queue.
Clojure publishes canonical sequences and cancellation strokes through the synchronous `bulwark.native/set-key-routes!` mechanism.
Publication replaces one immutable snapshot before the binding function returns.
The native loop clones its pointer under a mutex, then matches keys without runtime execution or a future reply.
Both threads release that mutex before they process keys or execute Clojure.

This exception to future-returning browser operations prevents command keys from reaching Servo during runtime stalls.
Clojure owns the binding map and command behavior. Rust applies the reservation data and retains physical key ownership through release.
The existing event queue carries runtime key delivery and ordered prefix resets.
Its capacity and fatal overflow policy still apply during a stall.

## Scheduling policy and remaining scope

Fiber-aware waits park. Evaluator safepoints let other fibers run during CPU-bound Clojure code.
Long native work must use fiber-aware blocking support or run outside the runtime thread.
Raw Rust blocking calls and stop-the-world GC can delay runtime commands and hooks.
Separate ownership lets Servo continue during such delays. It does not establish a display frame-rate guarantee.
A per-turn count or elapsed-time limit cannot preempt one native call.

E1 and E6 preserve the earlier shared-thread comparison. Their historical display deadlines do not gate this ownership decision.
The E6 policy uses dedicated runtime fibers, explicit native offload, bounded nREPL admission, and cooperative interruption.
Linux native behavior, IME, clipboard, display-scale changes, and multiprocess Servo remain follow-up work.
The initial product uses one visible webview per window. E5 remains necessary for web content splits.

## Historical fiber update at release 0.2.34, 2026-11-02

Clojurust 0.2.34 runs `go`, `future`, `thread`, and `io-thread` bodies on fibers on the shell thread.
E6 measured them in a 60 Hz loop. The provisional policy for Clojure work on the thread that owns the runtime
(the runtime thread under the separate-thread direction, or the shell thread under the earlier proposal):

- The host drives the LocalSet once per frame for a fixed budget, through a root fiber parked on a timer.
- Run Bulwark with `CLJRS_FIBER_SLICE_US` at 1–2 ms. A CPU-bound fiber can overrun the budget by one slice.
- Host callbacks call handlers directly. A handler that waits spawns a fiber or sends to a channel.
- Long or waiting work belongs in a fiber, never on the carrier or in a root-fiber evaluation.
- Native functions that block call `cljrs_value::parking::blocking`. A thread-blocking call freezes every fiber.
- nREPL forms run on root fibers. Until csm/clojurust#447, a parked nREPL form blocks the loop that polls it.
  Until csm/clojurust#446, interrupt cannot stop it.

Stop-the-world collection remains a frame risk for whichever thread owns the runtime.
On 0.2.34, E7's keyword-await probe completes, so CLJ-ASYNC-002 no longer reproduces. Idle await polling still costs about one CPU-second per idle second.

This section preserves the earlier shared-thread experiment and its findings at that release.
At the later `c6199fae` pin, E7 uses `spawn_pending` and demonstrates interruption of parked evaluations.
The current design drives the runtime LocalSet continuously on its dedicated thread.
The current upstream fixes and host limits supersede those admission and idle GC requirements.
