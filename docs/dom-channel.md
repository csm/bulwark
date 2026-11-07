# DOM channel experiment

Status: E4 passed on macOS arm64, 2026-11-06.
The product crates still contain the Phase 0 bootstrap.

## Channel choice

The initial DOM API will use `WebView::evaluate_javascript` to call an injected JavaScript agent.
The agent returns small strings directly and serializes larger results as JSON.
The host receives results through Servo's callback API.
The existing E7 bridge can carry these results to the runtime thread.
E4 measures the Servo portion of this path.

Resource interception also supports page-to-host messages.
The injected agent sends a same-origin `fetch` request with an identifier and encoded data.
The delegate receives the request, records the message, and supplies an acknowledgement response.
E4 asserts the decoded Unicode data and the acknowledgement in the page.
This alternative works, but does not replace the callback path for ordinary DOM requests.

The agent shares the page's JavaScript world.
Pages can replace the agent or forge its requests.
Resource interception alone does not authenticate messages or grant host capabilities.
The product needs origin checks, document generations, payload limits, and permission enforcement before it exposes privileged operations.

## Reproduction

Build the probe:

```sh
cargo build --locked -p bulwark-experiment-e4 --features servo
```

Run the probe:

```sh
python3 experiments/run_e4.py
```

On macOS, the rendering context needs access to graphics services outside the Codex filesystem sandbox.
The Python runner terminates the child after 35 seconds.
The Rust driver also checks a 30-second deadline between Servo turns.

The probe uses `SoftwareRenderingContext` and an intercepted HTTPS fixture at `bulwark.invalid`.
The delegate supplies every response locally and cancels unknown requests.
No HTTP server, DNS response, or external page is necessary.
`UserContentManager` injects the agent before the measurements.

## Measurements

Platform: macOS 26.5.2 arm64, Rust 1.97.1, Servo 0.7.0, debug build, single process.
Each operation has five warmup calls.
The trivial operation has 100 samples, and subtree serialization has 30 samples.
Every subtree result contains 5,000 span records with asserted tags, identifiers, and text.
The JSON payload contains 232,781 UTF-8 bytes.

| Run | Operation | p50 | p95 | Maximum |
|---|---|---|---|---|
| First | Trivial agent call | 49.292 µs | 72.833 µs | 95.750 µs |
| First | 5,000-element subtree | 24.741 ms | 28.430 ms | 30.444 ms |
| Repeat during Clippy | Trivial agent call | 116.792 µs | 3.751 ms | 14.392 ms |
| Repeat during Clippy | 5,000-element subtree | 34.144 ms | 43.287 ms | 47.433 ms |
| Final rebuilt binary | Trivial agent call | 50.167 µs | 73.084 µs | 85.542 µs |
| Final rebuilt binary | 5,000-element subtree | 24.796 ms | 27.433 ms | 28.934 ms |

All three runs passed the trivial-call p50 target of less than 5 ms.
All three runs delivered one page message with the correct identifier, decoded data, and acknowledgement.
The maximum subtree duration is an observed sample maximum, not a guaranteed latency limit.

Timing starts before `evaluate_javascript` and ends after the host receives the result.
It includes JavaScript execution, serialization, Servo transport, and the host event loop.
The driver uses Servo wakeups with a one-millisecond fallback wait.
Timing excludes Rust JSON parsing, assertions, and the E7 runtime bridge.
The subtree includes 5,000 elements and their text, rather than every DOM node type.
The probe emitted the GL texture warning previously observed in E3 and E7.

## Remaining work

DOM node handles need document generations and invalidation after navigation.
The product bridge needs structured errors, cancellation, and bounded payloads.
This probe does not measure streaming events, MutationObserver delivery, multiprocess behavior, or runtime-to-DOM latency.
Those checks belong to later integration work.
