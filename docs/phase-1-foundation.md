# Phase 1 foundation checkpoint

Status: implemented locally, 2026-11-07. Phase 1 remains in progress.
Base: `main` at `67c8182`. Working branch: `codex/phase-1`.
Servo and Clojurust pins are unchanged from the Phase 0 exit review.

This checkpoint begins deliverables 1–3 from the [development plan](development-plan.md).
It promotes the data boundary, bounded transport, native registry, and runtime ownership into product crates.
The Servo adapter, native window, and egui chrome remain the next integration work.
No Phase 1 browser acceptance criterion is complete yet.

## Run the product check

```sh
cargo run --locked -- --headless-smoke
```

The shell owns a mock engine on the main thread.
A dedicated worker owns Clojurust, GC values, Tokio, and its LocalSet.
Only owned requests, replies, and events cross the boundary.
The worker drives `bulwark.bridge/run-events!`, which awaits each call to the current `bulwark.core/dispatch!` var.
Handlers execute in a fiber and can await native operations without blocking the engine thread.
An event handler error ends the worker and reaches the host through its join result.

The mock stores navigation history and completes evaluation batches in reverse order.
Its `eval-js` accepts a quoted string literal only. It does not execute JavaScript or render a page.
The headless echo mechanism stores text for assertions; it does not display an egui area.
Running without flags prints `{:phase 1, :status :foundation}` and exits.

## Native API v0

Every function below returns a future. Awaiting it produces a response map:

```clojure
{:request/id 17 :ok true :value ...}
{:request/id 18 :ok false :error :stale-handle}
```

Admission failures use `nil` for `:request/id` because no request entered the queue.
Invalid argument types raise a language error before admission.
Errors from the engine can include `:message`.
All functions live in `bulwark.native`; public Clojure helpers remain later work.

| Function | Arguments | Successful value |
|---|---|---|
| `webview-create` | URL string | Positive handle |
| `webview-close` | Handle | `nil` |
| `navigate` | Handle, URL string | `nil` |
| `reload`, `back`, `forward` | Handle | `nil` |
| `webview-info` | Handle | URL, title, load status, and history flags map |
| `eval-js` | Handle, script string | Text in the current mock; Servo result conversion remains open |
| `log` | Text | `nil` |
| `set-echo!` | Text | `nil` |
| `quit` | None | `nil`; requests host shutdown |

The internal `next-event` function returns a future containing an event map, or `nil` when the stream closes.
Events contain `:sequence`, `:event/type`, and `:webview`, plus event-specific data.
The schema covers URL, title, load status, history flags, close, crash, and key input.
Key data includes the key string, control/alt/shift/meta flags, and pressed state.

## Transport and lifetime contract

The product bridge implements [D1](adr/0001-shell-thread.md):

- 32 queued requests and 64 unfinished requests. Admission uses immediate capacity checks and returns `:busy` on overload.
- A permit follows each request through engine execution and reply consumption. A cancelled request retains capacity until cleanup.
- Each admitted request has an id, a deadline, and its own reply channel. The native registry uses a five-second deadline.
- Engine admission stops after 16 requests or two milliseconds, checked between calls. A single engine call can exceed that time.
- Commands and replies permit one MiB of combined string data. Events permit 64 KiB. Counts use UTF-8 bytes.
- The event queue holds 256 ordered events. Overflow or an oversized event stores the first fatal error separately and wakes the host.
- Resource handles come from one process-wide monotonic allocator. Closing a resource invalidates its handle. Exhaustion never wraps.

The host stops admission, rejects queued requests, and invalidates engine resources before cancelling the worker.
Worker completion and panic wake the host. The shell joins only a finished worker and allows two seconds for shutdown.
Failure to stop within that deadline reports an error and exits the process.
Forced exit does not prove cleanup or rollback.
An operation that already started can retain side effects after timeout or cancellation.

## Validation

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo run --locked
cargo run --locked -- --headless-smoke
cargo run --locked -p bulwark-experiment-e2
```

All commands above passed locally on macOS arm64. The workspace now runs 26 Rust tests,
including 18 new foundation tests. E2 also passed 10,000 callbacks per execution mode,
forced collections, exception propagation, and live redefinition.

The foundation adds bridge/registry/mock tests and runtime worker integration tests.
Coverage includes overload, reply correlation, deadline revocation, payload boundaries, stale handles,
identifier exhaustion, ordered events, fatal overflow, cancellation, handler failure, and crash/key payload delivery.
The Clojure smoke covers native navigation/history calls, concurrent replies, handlers that await,
live dispatcher redefinition, ordered lifecycle events, and handle replacement.
The worker forces a collection after rooting the program and event futures.
CI includes the headless smoke in its Linux and macOS runtime jobs; remote results remain uninspected.

## Next integration

Implement the Servo adapter against this contract, then connect native window/input/presentation and egui chrome.
Preserve the Phase 0 single-visible-view fallback and bounded worker shutdown.
Add commands, keymaps, hooks, opt-in loopback nREPL, and the Clojure browser behavior after that connection.
The product does not start an nREPL listener yet. Its budgeted admission remains proven in E7.
