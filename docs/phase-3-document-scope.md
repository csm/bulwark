# P3.1 document scope prototype

Status: experimental S1 slice. The product keeps Servo 0.7.0 and all pinned dependency versions.
The [mechanism contract](phase-3-mechanisms.md#s1-engine-document-identity-and-scoped-evaluation) remains open.

## Mechanism

The [engine patch](../experiments/phase3/patches/servo-0.7-document-scope.patch) adds document scope to the existing evaluation channel.
The identity contains a pipeline and an engine-issued incarnation.
A checked counter allocates each incarnation without reuse within its process.
The pipeline distinguishes different processes. Page data does not supply either field.

`request_document_identity` returns the active top document's identity through an empty script evaluation.
`evaluate_javascript_in_document` carries the expected identity through the constellation to the script thread.
The constellation rejects a different active pipeline.
Immediately before execution, the script thread rejects a different incarnation or an inactive document.
A successful result includes the identity that executed the script.

Document construction and a successful `document.open()` reset allocate fresh incarnations.
Top-document reactivation also allocates a fresh incarnation before script resumes.
This conservative prototype policy prevents an old request from regaining validity after a history round trip.
Fragments and `pushState` preserve the incarnation.
The script thread checks the identity again after result serialization because getters can reset the document.
The constellation also rejects successful replies whose complete identity no longer matches its active document.

The original public evaluation signature remains available for the negative control.
The prototype changes its internal result channel and can reject replies after replacement.
This patch is an experiment against published sources, not an upstream submission or a product dependency override.
The modified engine files retain their MPL-2.0 license headers.

## Lifecycle and host delivery

The experimental delegate receives `DocumentLifecycle` events through the same constellation channel as evaluation replies:

- `Active(Some(identity))` activates a top document and revokes its predecessor.
- `Active(None)` revokes the current top document on close or crash recovery.
- `DomReady(identity)` and `LoadComplete(identity)` identify the top document at those lifecycle points.

Construction registers the identity before activation.
A reset sends the new incarnation before the engine clears the DOM.
The constellation forwards reset and readiness events only for its current top document.
Events from inactive documents and nested frames cannot change the host's active document.
The constellation deletes identity records when pipelines exit and rejects later reset events from those pipelines.

The probe maintains a host activation generation for each view.
Admission records the current identity and generation in a request ticket.
Delivery checks the ticket, result identity, current activation, and cancellation state before the allocation counter changes.
The counter models the allocation boundary. It is not a node registry or a public DOM API.
History never revives a ticket from an earlier activation.
Repeated snapshots of the same active identity preserve the generation.

The delegate processes events as the embedder drains the channel.
These events do not provide a synchronous snapshot of concurrent engine activity.
A production registry must revoke existing handles on every observed transition and check scope again when operations execute.
A host that drops its final view handle must also revoke its local state immediately.
The close probe retains the view handle so it can observe the engine's close notification.

## Pending callbacks and script-thread failure

The evaluator records the view and expected document for each pending callback.
An activation change removes callbacks for obsolete scoped requests.
`Active(None)` removes every pending callback for that view, including unscoped requests.
The delegate receives revocation before those callbacks receive `StaleDocument`.
The evaluator removes callbacks before it calls them, so a callback can submit another request.
Later engine replies for removed requests have no effect.

This cleanup rejects pending results. It does not cancel engine execution.
JavaScript that already ran can still change the document.
The prototype does not define cancellation of queued engine work or interruption of active JavaScript.
Terminal callbacks from one revocation have no specified order.

The actual failure fixture sends a diagnostic message that panics on the active script thread.
The fixture enables Servo recovery with `Opts::hard_fail = false`.
It uses `Servo::setup_logging()` and a panic hook that reports through `log::error!`, as the published Servo crash test does.
The engine logger attaches the current event-loop identity and forwards the panic to the constellation.
That path revokes every affected view and creates the engine crash document.
The fixture requires the unwind notification, eight terminal errors, released callback captures, an empty pending table, and successful recovery.
A retained successful reply from before the unwind must also fail the host allocation check.

This fixture covers a Rust unwind after document construction in the single-process engine.
Process termination, aborts, constructor failures, and failures inside the failsafe remain untested.
The report path uses published engine APIs. Only the panic trigger and document-scope mechanisms require the experimental patch.
The default Servo option exits the process on a reported panic, so this recovery result requires the explicit fixture configuration.

## Regression scope

Ordinary fixture resources use interception.
The redirect fixture uses a temporary loopback HTTP listener for exactly two requests.
This exercises Servo's HTTP redirect path. An intercepted 302 response did not follow that path in the preliminary run.
The failed-load fixture uses an unsupported scheme and requires no network request.
No remote website or external service is necessary.
Fixture markers establish test order, without authentication claims.

| Case | Required assertion |
|---|---|
| Initial document and result | Navigation replaces the initial `about:blank` identity. A successful result carries the expected identity. |
| Lifecycle and current delivery | The host receives activation and tagged readiness events. A current reply reaches the allocation counter. |
| Script-created initial document | `window.open()` creates a distinct authenticated top document. A script-driven reset revokes its initial scope without changing the opener. |
| JavaScript URLs | A non-string result preserves scope. A string result replaces the document and revokes both engine requests and retained host replies. |
| Same-document navigation | Fragments and `pushState` preserve both identity and host generation. |
| Held request and negative control | A request for A fails after B activates. The original unscoped API executes the same mutation in B. |
| Queued reset | The first request resets the document. Both queued replies fail, and the second mutation never runs. The pipeline stays equal. |
| Serialization reset | A result getter resets the document. The engine rejects the result. |
| Delayed host delivery | An already successful reply survives in a host queue. A later reset revokes its ticket before allocation. |
| Host cancellation | Cancellation rejects an otherwise successful current reply before allocation. |
| Reload | Reload replaces the identity and rejects the previous scope. |
| History | Reactivation assigns a fresh incarnation. An old request cannot mutate the restored document. The host rejects the old successful reply. |
| Redirect | A real HTTP 302 reaches the final fixture. The host rejects the departed reply, and the engine rejects its old scope. |
| Failed load | An unsupported scheme activates an error document. The host and engine reject the departed scope. |
| Different views | Each view rejects the other view's document identity. |
| Subframe isolation | A nested document reset preserves the top identity and cannot emit a different top lifecycle identity. |
| Close | The engine revokes the document before the close notification. The host rejects the retained reply, and the evaluator releases a pending callback. |
| Injected crash | The constellation's panic handler revokes the document before the crash notification. The host rejects the retained reply. |
| Script-thread failure | An actual Rust unwind revokes scope and releases eight pending callbacks exactly once. A new document accepts evaluation after recovery. |

The runner repeats these nineteen cases three times and retains the existing Servo contract and E4 baseline results.
Host cancellation suppresses delivery. It does not cancel JavaScript execution or undo a mutation.
The injected crash calls the existing constellation panic handler through an experimental probe method.
The separate script-thread fixture terminates the receiving thread through a Rust unwind. Neither fixture proves process-failure detection.

## Local evidence

The [audit evidence](../experiments/phase3/document-audit-evidence-2026-11-23.json) records the current runs, logs, source hashes, and binary hash.
The [lifecycle evidence](../experiments/phase3/document-lifecycle-evidence-2026-11-23.json) preserves the sixteen-case checkpoint.
The [earlier evidence](../experiments/phase3/document-scope-evidence-2026-11-22.json) preserves the nine-case checkpoint before lifecycle notifications.
That earlier checkpoint retained identities across history activation. The current prototype deliberately changes that policy.

All nineteen cases passed in three final runs on macOS arm64 with software rendering.
Both existing Servo controls, all 61 portable tests, formatting, and portable/probe Clippy passed locally.
Clippy denied warnings for the probe and portable workspace.
Python syntax checks and runner rejection checks also passed.

One preliminary repeat timed out at the redirect after two successful repeats.
The final fixture waits for explicit history-traversal completion and exposes HTTP worker errors before the page deadline.
Five diagnostic repeats passed after that change. The exact cause of the earlier timeout remains unproven.
The lifecycle evidence retains that failed repeat and its source hashes.

The current audit exposed an immediate nonblocking read error on an accepted loopback socket.
The fixture now explicitly selects blocking reads with a timeout.
The audit evidence retains that diagnostic log and a successful diagnostic run before the three final repeats.
The earlier redirect timeout's exact cause remains unproven.

The initial sandboxed graphics run failed during macOS surface initialization.
Graphics runs require normal macOS graphics services outside that sandbox.
Servo emits its known GL texture warning and debug linker warning.
The copied engine packages also emit warnings from published code under disabled features.
The panic hook logs the intentional script-thread panic. Recovery also logs pipeline teardown warnings.
The experiment adds an optional dependency on the already locked `log` crate. No dependency version changes.
No remote CI, physical input, Linux, or release latency result applies to this checkpoint.

## Remaining requirements

This prototype does not close S1 or permit a restricted DOM API.
Process failure detection still needs a dedicated fixture.
Cancellation of pending engine work still needs a defined contract.
Callback cleanup needs further checks for final-handle release, engine shutdown, and failures without a document.
The host allocation counter does not establish registry revocation, retained-node memory bounds, or production scheduling.
The lifecycle audit must cover remaining replacement paths, including `srcdoc`, parser-time resets, and cross-origin history reactivation.
Upstream review must settle the conservative history policy and lifecycle API.

S2 still needs the complete current document identity on protocol messages.
A matching protocol pipeline cannot distinguish two incarnations after a reset or reactivation.
This patch does not authorize a privileged action bridge.
Evaluator authority, redirect/rewrite mechanisms, and owner scope decisions also remain open.

## Reproduction

Prepare copies of the six published Servo packages and an isolated Bulwark workspace:

```sh
python3 experiments/prepare_document_scope.py --out target/phase3-document-lifecycle
```

Build the candidate:

```sh
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="$PWD/target" cargo build --offline --locked \
  --manifest-path target/phase3-document-lifecycle/candidate/Cargo.toml \
  -p bulwark-experiment-e4 --features document-scope
```

Run the candidate with independent 35-second process deadlines:

```sh
python3 experiments/run_document_scope.py \
  --binary target/debug/bulwark-experiment-e4 \
  --preparation target/phase3-document-lifecycle \
  --output target/phase3-document-audit-evidence
```

The preparation refuses an existing output directory.
It records the patch hash, original source hashes, product manifest hashes, and candidate probe hashes in `provenance.json`.
The runner rejects a preparation whose patch, product manifests, or probe files differ from the current sources.
Only candidate manifests reference the copied packages.
The runner records the binary hash, probe hashes, all nineteen case markers, the summary marker, exit statuses, and log hashes.
The `document-scope` feature requires the patched candidate and cannot compile against the product engine.
The normal E4 feature and the portable workspace retain their existing build commands.
