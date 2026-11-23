# P3.1 mechanism evaluation

Status: initial probes and contract drafts complete, 2026-11-22. P3.1 remains open on the prerequisites listed here.
The owner authorized P3.1 after preparation of the [Phase 3 draft plan](phase-3-plan.md).
That instruction starts the mechanism work. It does not settle the remaining scope decisions by itself.

The baseline is `main` at `b9b0ded`, with accepted Phase 2 product source at `cf3e6be`.
Servo remains at `0.7.0`. Clojurust remains at `540e42181ad06046116f0e2007e7f0c449efaa65`.
This checkpoint adds diagnostic examples, local fixtures, an E4 contract mode, and documentation.
The installed product natives, engine behavior, capability policy, dependency pins, and transport limits do not change.

## Results

| Mechanism | Evidence | Disposition |
|---|---|---|
| Product DOM round trip | Two sequential DOM requests pass through the existing native table, product bridge, runtime worker, and real Servo. Both evaluator modes pass. | Channel works. Structured handles and C6 remain pending. |
| Document replacement | Replacement clears a page-local node slot. The first probe also reproduced stale `Complete` status immediately after navigation admission. | JavaScript slots and host load status do not establish authenticated document identity. |
| Deferred navigation | A retained `NavigationRequest` leaves the old document active. Page timers progress. Explicit allow and deny produce the expected results. | Public pinned API supports asynchronous decisions. |
| Deferred resources | A retained `WebResourceLoad` keeps a fetch pending. A later response completes it. Explicit cancellation reaches the page's rejection handler. | Public pinned API supports deferred response and block decisions. |
| Source-frame provenance | The top document and its same-origin iframe issue the same fetch with referrers suppressed. Their complete public request metadata is identical. | Resource interception cannot authenticate the source frame or document. |
| Protocol-source provenance | The custom protocol receives distinct pipelines for top, same-origin, remote, and sandboxed-frame fetches. Client origins and nested-frame status remain distinct. | Public API candidate exists. Current-document binding and obsolete-message rejection remain pending. |
| Observer transport | A 500-mutation burst retains 64 records and terminal overflow. A second drain has no repeated records and retains the overflow result. | Bounded request/reply drain is a viable prototype. Product scheduling remains pending. |
| Agent integrity | Page JavaScript replaces the agent and returns a forged successful batch. | Agent data provides no host authority or authenticated lifecycle evidence. |
| Restricted evaluation | An empty-capability namespace navigates through a direct native, uses a captured native after await, mutates trusted state, and reads a local fixture. | Namespace-only restriction fails in both evaluator modes. No restricted loader is exposed. |
| Local custom protocol | A public `ProtocolHandler` serves `bulwark://` HTML. Switching back preserves the hidden web document. The custom page reports `origin=null` and no `navigator.servo`. | One-visible-view rendering works in software. Privileged bridge, physical input, and the D6 clarification remain pending. |

Every successful product probe joins its runtime worker.
All pages are local files or intercepted fixtures. The probes require no remote websites or services.
The initial sandboxed Servo run failed during macOS rendering initialization.
The successful graphics runs used the normal macOS graphics services outside that filesystem sandbox.
Servo emitted its previously observed GL texture warning. Debug linking also emitted a compact-unwind size warning.
Neither warning changed a probe assertion.

## Product-channel latency

Hardware: Apple M3 Max, 16 CPU cores, 48 GB RAM.
Software: macOS 26.5.2, Rust 1.97.1, single-process Servo 0.7.0, software rendering.
The product worker and engine retain their separate threads.
The example has no profile load, init file, nREPL, or live-site traffic.

Timing starts in Clojure before the query request and ends after the text result returns to Clojure.
It includes two sequential native futures, engine requests, callbacks, bridge transfers, and Clojure result assertions.
Each run uses 20 warmup operations and 200 measured operations.
The query stores one heading in a page-local slot. The text request reads that slot.
This prototype has no native node registry, JSON result schema, or public `bulwark.dom` API.
These timings do not establish C6 or a latency guarantee.

| Build | Evaluator | p50 | p95 | Maximum |
|---|---|---|---|---|
| Debug | Tiered | 0.404125 ms | 0.426292 ms | 0.484584 ms |
| Debug | TreeWalk | 0.397375 ms | 0.417667 ms | 0.471000 ms |
| Release | Tiered | 0.098083 ms | 0.145000 ms | 0.225416 ms |
| Release | TreeWalk | 0.120917 ms | 0.149250 ms | 0.180917 ms |
| Release, final protocol run | Tiered | 0.095333 ms | 0.130125 ms | 0.155125 ms |
| Release, final protocol run | TreeWalk | 0.090500 ms | 0.100083 ms | 0.112000 ms |

The final E4 baseline uses the debug engine-only binary.
Its trivial-call p50 is 49.875 µs. Its 5,000-element subtree p50 is 22.677250 ms.
The subtree contains 232,781 UTF-8 bytes and passes the original content assertions.
The original intercepted Unicode message and acknowledgement also pass.
The release runner uses the release product example and the debug E4 binary. It does not imply a release E4 measurement.

Raw timing samples, binary/source hashes, and final case results are retained in the [evidence file](../experiments/phase3/evidence-2026-11-22.json).
Final local logs reside under `target/phase3-final-evidence/`.
The first release run remains under `target/phase3-evidence-release/`.
The earlier successful debug product run resides under `target/phase3-evidence/`.

## Navigation race and source audit

The initial DOM fixture awaited a successful navigation reply and then inspected `webview-info` until its status was `:complete`.
Both modes reached the subsequent assertion before the old page-local node slot disappeared.
Their retained logs report `Replacement unexpectedly retained the old page slot`.
The failed run remains under `target/phase3-evidence-initial/`.

Pinned `WebView::load` sends `LoadUrl` without synchronously changing the view's load status.
Therefore, the next information reply can still describe the old complete document.
The corrected diagnostic waits for a marker that the replacement fixture sets during parsing.
The marker is an assertion barrier for this controlled fixture. It does not authenticate arbitrary web content.
No production wait, deadline, or assertion changed.

Pinned-source inspection identifies the following constraints:

| Pinned source | Observation |
|---|---|
| `servo/webview.rs`: `evaluate_javascript` | The public request supplies a webview and script, without an expected document token. |
| `servo/javascript_evaluator.rs`: `evaluate` and `finish_evaluation` | The callback receives a result correlated by evaluation id, without source-document metadata. |
| `servo-constellation/constellation.rs`: `handle_evaluate_javascript` | The constellation selects the active pipeline when it handles the message. The embedder cannot bind an expected pipeline through the public API. |
| `servo-script/event_loop/script_thread.rs`: `handle_evaluate_javascript` | Script execution uses that internal pipeline. This internal identity does not reach the public callback. |
| `servo-embedder-traits/lib.rs`: `WebResourceRequest` | Public fields contain target URL, method, headers, destination, referrer, main-frame destination flag, and redirect flag. They omit source-document identity. |
| `servo/webview_delegate.rs`: `NavigationRequest` | The target URL is public. The pipeline is private. Allow and deny exist, while frame-preserving redirect does not. Dropping the request allows navigation. |
| `servo/webview_delegate.rs`: `WebResourceLoad` | Interception can replace a response or cancel it. It has no outbound URL, method, or header replacement operation. Dropping the load permits normal continuation. |
| `servo/lib.rs`: `protocol_handler` | Public reexports supply the handler, request, response, and timing types. Custom protocol HTML needs no direct dependency on Servo component crates. |
| `servo-net-traits/request.rs`: `Request` and `RequestClient` | The custom handler receives source pipeline, target view, client origin, and nested-frame status that ordinary interception omits. |
| `servo-script/fetch/fetch.rs`: `with_global_scope` | The fetch builder attaches the source global's pipeline and request client. The final protocol probe checks those fields across four source contexts. |
| `cljrs-runtime/runtime.rs`: `RuntimeBuilder::build` | Default construction registers core builtins. A new default runtime alone does not establish a restricted evaluator. |
| `cljrs-runtime/env/policy.rs`: transaction policy | The current policy rejects `await`, namespace changes, and selected effects. It does not provide the required asynchronous extension profile. |
| `bulwark-runtime/src/native.rs`: `install` | Product closures capture the bridge client and accept calls without an extension-principal check. Shared globals expose those closures across namespaces. |

These observations refer to the installed exact pins, rather than current upstream branches.
No pin update or upstream fix is claimed here.

## Required upstream and integration contracts

S1, S3, and R1 identify gaps in the current public integration.
S2 has a public protocol-handler candidate and needs further integration evidence before an upstream change becomes necessary.

### S1: Engine document identity and scoped evaluation

The engine must expose an opaque document identity with top-frame activation and revocation notifications.
DOM-ready and load-complete notifications must carry that identity and distinguish the top document from frames.
An evaluation request must accept an expected identity and reject a mismatch before JavaScript execution.
The callback must return the identity that actually executed the script.
The identity must distinguish reloads, redirects, replacement, and history activation.
The API must define same-document navigation and initial `about:blank` behavior.
It must also define handle behavior after `document.open` and other script-driven document resets.

Required regression evidence includes:

1. Queue a mutation for document A, then activate document B before execution.
2. Assert that the mutation neither runs in B nor returns a successful result for B.
3. Deliver A's callback after replacement and assert that no host node handle is allocated.
4. Repeat with reload, history, redirects, failed load, close, and crash.
5. Assert the documented identity behavior for fragments and `pushState`.

This contract gates restricted read-only DOM results as well as mutation. Incorrect document scope can disclose another document's data.

### S2: Protocol message provenance and current-document binding

Page-to-host messages must include engine-authenticated source view, document, frame, origin, and top-frame status.
The public custom-protocol request already supplies source pipeline, target view, client origin, and nested-frame status in the four-context probe.
It remains a candidate channel, rather than a privileged bridge.
The origin must describe the sender, rather than the requested destination or a payload claim.
Source identity must remain attached to queued messages after replacement.
The public API must distinguish opaque origins without treating every serialized `null` origin as equivalent.

The final probe covers top, same-origin, remote, and sandboxed-frame source metadata.
Required authorization regressions still cover ordinary pages, copied messages, redirects, replacement, and obsolete documents.
The host must connect the source pipeline to the current instance's document identity from S1.
If that binding works through the public interface, S2 needs no separate source-metadata patch.
A registered custom scheme alone cannot satisfy those checks.
This contract gates the privileged action bridge. Untrusted subscription data can still use a nonprivileged drain channel.

### S3: Navigation redirect and outbound request rewrite

A redirect mechanism must retain the original navigation target, including an iframe target.
It must document method, request-body, referrer, history, and redirect-loop behavior.
A resource rewrite mechanism must apply checked URL, method, and header changes to the actual outbound request.
Changing a public metadata copy or replacement response URL is insufficient.

Required regressions cover top-level and iframe links, forms, script navigation, POST bodies, redirects, and target-scope enforcement.
Deferred allow/deny and response substitution can use the current public responders.
Full redirect/rewrite semantics remain gated by this contract or an explicit scope decision.

### R1: Evaluator authority across asynchronous work

The runtime must attach opaque host authority to each evaluation context and continuation.
Native calls must consult caller authority even through aliases, higher-order calls, trusted helpers, and `eval`.
Spawned tasks, parked fibers, timers, callbacks, and command dispatch must retain the originating authority.
Interleaved principals must not inherit one another's grants.

The restriction must also prevent trusted-global mutation and uncontrolled filesystem, process, loader, interop, and concurrency effects.
The API must define namespace visibility, callback ownership, revocation, and structured denial errors.
TreeWalk, Tiered, and any supported AOT path require equivalent enforcement at their final effect boundaries.
The host cannot implement this contract with a caller-supplied map or namespace prefix.

Required regressions convert the current counterexamples into denials and add interleaved restricted/trusted fibers, nested helpers, and cancellation.
A separately restricted runtime remains an alternative only after its complete effect surface passes those checks.
General isolate infrastructure is outside this checkpoint. The existing shared trusted runtime remains unchanged.

## Contract and scope disposition

The [native contract draft](phase-3-contracts.md) proposes operations, structured errors, ownership, and bounds.
The [trust ADR](adr/0007-phase-3-trust-boundaries.md) keeps agent data separate from host authority.
The [events ADR](adr/0008-phase-3-events-and-decisions.md) proposes bounded drains and explicit responder cleanup.
All three documents remain proposals until their mechanisms and applicable owner decisions pass review.

The proposed global node cap is 16,384, with explicit release and per-document/principal limits.
Native registry and retained-node memory measurements remain pending because this checkpoint adds no node registry.
The proposed drain scheduler also needs idle-CPU, fair-admission, payload, and hostile-agent checks.
The probe's 64-record cap alone does not satisfy those product requirements.

The custom-protocol result supports a D6 clarification for one visible Servo buffer.
It does not pass E5 or authorize simultaneous visible web splits.
The owner must still settle that clarification, extension format, reader tasks, grant procedure, and ordinary-traffic timeout policy.
The small Linux investigation remains deferred.

P3.2 cannot claim document-scoped restricted DOM access until S1 and the relevant authority mechanism exist.
P3.4 remains gated by R1. The P3.7 bridge remains gated by S2 and the D6 disposition.
P3.5 can use deferred allow/deny, while its complete redirect/rewrite API depends on S3.
The next mechanism work targets scoped engine evaluation, evaluator authority, and protocol-source binding, with the listed regressions.

The [document scope follow-up](phase-3-document-scope.md) adds an isolated Servo patch and nine regression cases.
Three local runs pass, along with the existing Servo contracts and E4 baseline.
The prototype checks document incarnations at execution and after serialization.
Its delayed-delivery case retains an obsolete successful callback, so S1 remains open on lifecycle revocation and host delivery.
The reset case also proves that S2 cannot bind the current incarnation through pipeline equality alone.
Product pins and installed behavior remain unchanged.

## Reproduction and checks

Build the probes:

```sh
cargo build --locked -p bulwark-experiment-e4 --features servo
cargo build --locked -p bulwark-shell --no-default-features --features servo --example phase3
cargo build --locked --release -p bulwark-shell --no-default-features --features servo --example phase3
```

Run the local checks with independent 35-second process deadlines:

```sh
python3 experiments/run_phase3.py
python3 experiments/run_phase3.py --binary target/release/examples/phase3 --output target/phase3-evidence-release
```

The runtime-only counterexamples need no graphics services:

```sh
cargo build --locked -p bulwark-shell --no-default-features --example phase3
python3 experiments/run_phase3.py --runtime-only --output target/phase3-authority-evidence
```

The final six-case runner passes both runtime modes, both release DOM modes, the Servo contracts, and the original E4 baseline.
Formatting, portable Clippy, Servo-probe Clippy, and the 61 portable workspace tests pass locally.
Clippy runs with warnings denied. Python syntax and documentation links receive separate checks.
Offline builds and checks use cached dependencies at the exact lockfile revisions.
No remote CI, physical native-input, new AOT, or Linux result is claimed for this checkpoint.
