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
The final view handle invokes `notify_document_scope_released` synchronously, without a view argument.
The host delegate must revoke its local state in this notification.
The close probe retains the view handle so it can observe the engine's close notification.

## Pending callbacks and script-thread failure

The evaluator records the view and expected document for each pending callback.
An activation change removes callbacks for obsolete scoped requests.
`Active(None)` removes every pending callback for that view, including unscoped requests.
The delegate receives revocation before those callbacks receive `StaleDocument`.
The evaluator removes callbacks before it calls them, so a callback can submit another request.
Later engine replies for removed requests have no effect.

This cleanup rejects pending results and closes their execution permits.
The script thread rejects a closed permit before execution.
A closed permit also interrupts the evaluation while it runs.
Effects that happened before the interrupt remain in the document.
Terminal callbacks from one revocation have no specified order.

The actual failure fixture sends a diagnostic message that panics on the active script thread.
The fixture enables Servo recovery with `Opts::hard_fail = false`.
It uses `Servo::setup_logging()` and a panic hook that reports through `log::error!`, as the published Servo crash test does.
The engine logger attaches the current event-loop identity and forwards the panic to the constellation.
That path revokes every affected view and creates the engine crash document.
The fixture requires the unwind notification, eight terminal errors, released callback captures, an empty pending table, and successful recovery.
A retained successful reply from before the unwind must also fail the host allocation check.

This fixture covers a Rust unwind after document construction in both process modes.
The process audit below covers child exit without an unwind.
Aborts, constructor failures, and failures inside the failsafe remain untested.
The report path uses published engine APIs. Only the panic trigger and document-scope mechanisms require the experimental patch.
The default Servo option exits the process on a reported panic, so this recovery result requires the explicit fixture configuration.

## Final handle release and shutdown

The experimental patch removes pending callbacks when the host releases its final view handle.
It calls `notify_document_scope_released` before it returns `StaleDocument` to those callbacks.
No view handle exists for this notification.
Release of an earlier handle preserves the scope and pending callbacks.
Callback removal precedes invocation, so a callback can submit work to a surviving view.

The final-release fixture holds a successful reply across release and rejects it before host allocation.
Eight pending callbacks mix scoped and unscoped evaluation.
A separate callback submits work to a surviving view.
The fixture requires synchronous host revocation, terminal errors, released callback captures, and successful survivor evaluation.
Another fixture releases a new view before the host receives any document activation.
That fixture requires cleanup of eight unscoped callbacks without a known document identity.

The experimental shutdown entry point starts the engine's existing `Exit` path with live view handles.
It closes evaluation admission, removes all pending callbacks, and revokes every retained view before callback delivery.
It rejects reentrant and subsequent evaluation with `StaleDocument` without an evaluator borrow.
Queued lifecycle messages cannot reactivate a stopped engine.
Repeated shutdown admission has no effect.
The engine destructor uses the same cleanup path.

The shutdown fixture retains sixteen callbacks across two views.
One view has an active document.
The other view has no host activation, and only its pending callbacks retain its handles.
Explicit shutdown breaks this ownership cycle and releases the final handle.
The fixture also requires rejection of new work and actual engine shutdown completion.

A pending callback can retain a view or Servo handle and prevent the engine destructor from starting.
The explicit experimental shutdown entry point handles this cycle.
Destructor cleanup alone cannot handle a destructor that ownership prevents from starting.
This entry point is probe instrumentation, not an installed product API.
The prototype closes execution permits for queued and running embedder evaluations.
Unresponsive page tasks outside embedder evaluation and backend disconnection remain outside these cleanup checks.

## Callback unwind audit

The earlier cleanup loop removed all callbacks before delivery.
If one callback panicked, Rust dropped the remaining callbacks without terminal delivery.
Rust dropped their captures even though their terminal handlers never ran.
The new fixture checks callback counts as well as capture release.

The experimental patch catches each callback unwind in a revoked batch.
It attempts every terminal callback before it resumes the first panic.
The host receives scope revocation before the first terminal callback.
The evaluator holds no callback-table borrow during delivery.
If cleanup starts during an existing host unwind, it preserves that outer panic after terminal delivery.
These rules apply to lifecycle revocation, final handle release, and explicit shutdown.

A normal result callback still propagates its panic directly after removal from the pending table.

Five fixtures cover normal results, lifecycle revocation, final release, release during a host unwind, and shutdown.
Each new group contains eight callbacks that panic after they record terminal delivery.
The shutdown fixture also retains the existing sixteen ordinary callbacks and the reentrant admission check.
The fixtures require terminal delivery exactly once, host revocation, capture release, and successful continuation where the engine remains active.
The shutdown fixture requires actual engine shutdown after the embedder catches the panic.

The negative control restores the earlier cleanup loop in the candidate engine.
It fails at lifecycle revocation with `Expected eight terminal callbacks`.
Both existing Servo controls still pass with that loop.

These fixtures use Rust unwinds with string panic payloads.
They do not cover process aborts, panicking panic hooks, panic-payload destructors, or delegate panics.
This experimental policy still needs upstream review before product integration.

## Delegate unwind audit

The callback fix did not cover the host delegate.
Each revocation path removed callbacks from the pending table before it notified the delegate.
If `notify_document_lifecycle` or `notify_document_scope_released` panicked, Rust dropped the removed callbacks without terminal delivery.
During shutdown, one panicking delegate also prevented revocation of the remaining views.

The experimental patch now catches each delegate unwind in these three paths.
Shutdown notifies every retained view before it delivers terminal callbacks.
The first panic propagates to the embedder after delivery. A delegate panic precedes any callback panic in the same batch.
Later panics in that batch are dropped. Cleanup during an existing host unwind still preserves the outer panic.
The ordering rule does not change: the delegate receives revocation before the first terminal callback.

The engine cannot verify host state after a delegate panic.
The host must revoke its local scope before any fallible work in these notifications.
The fixture delegate records revocation and then panics once.
A delegate that panics before it revokes can still admit a retained reply at its own allocation check.

Four fixtures cover these paths:

- Lifecycle revocation on close, with eight panicking callbacks.
  The delegate panic reaches the embedder, all callbacks receive terminal errors, and the close notification follows.
- Final handle release, with eight ordinary callbacks. A surviving view still accepts evaluation.
- Final release during an existing host unwind, with eight panicking callbacks. The outer panic reaches its handler.
- Explicit shutdown, where the delegates of both retained views panic.
  Both views receive revocation, all twenty-five callbacks receive terminal errors, and engine shutdown completes.

The shutdown fixture now propagates a delegate panic. It still requires delivery to its eight panicking callbacks.
Callback-panic propagation without a delegate panic remains covered by the lifecycle and release fixtures.

The negative control restores the three engine files from the callback checkpoint.
It fails at lifecycle revocation with `Expected eight terminal callbacks`.
Both existing Servo controls still pass with those files.
These fixtures do not cover panics in other delegate methods, panic hooks, or panic-payload destructors.

## Queued execution cancellation

The experimental `evaluate_cancellable_javascript_in_document` method returns a pending evaluation id.
Admission after shutdown returns no id and delivers `StaleDocument` synchronously.
The view can cancel its own pending id through `cancel_javascript_evaluation`.
Cancellation removes the callback, closes its execution permit, and delivers `Cancelled` synchronously.
The callback runs without an evaluator borrow. A callback panic propagates after removal and permit closure.
Repeated, revoked, and foreign-view cancellation attempts return false.
A request whose callback already ran also returns false.
Cancellation preserves the active document and its host generation.

Each pending evaluation retains the sender of a private channel from Servo's generic transport.
The request carries the receiver through the constellation to the script thread.
The script thread requires an open, empty channel immediately before JavaScript execution.
A closed channel or transport error rejects execution. Later closure interrupts execution or result serialization.
Callback removal also closes the permit during lifecycle revocation, final release, and shutdown.
The prototype does not queue a cancellation message behind the JavaScript that it must cancel.

Cancellation cannot undo an existing mutation.
The [interruption section](#active-javascript-interruption) describes cancellation after the execution check.
The host removes the callback before terminal delivery, so later engine replies cannot deliver it twice.
Replies that already reached a host queue still need the host ticket's cancellation check before allocation.
The channel uses Servo's generic transport.
The original cancellation checkpoint covers its single-process backend. The process audit below adds multiprocess evidence.
Channel allocation failure remains untested.
This experimental contract needs upstream review before product integration.

Three new fixtures use a script-thread barrier with a five-second deadline.
The barrier releases on normal return or host unwind and requires no page timer or busy loop.
The queued fixture cancels a mutation while the receiving thread waits at the barrier.
It requires an unchanged mutation counter, one terminal callback, released captures, and successful reentrant evaluation.
It also rejects foreign-view and repeated cancellation without changing the active document.
The completed fixture places the barrier after execution and serialization, before the host drains the reply.
Cancellation suppresses delivery, but the mutation counter still records the completed operation.
The callback-panic fixture requires permit closure before the panic and an unchanged mutation counter after recovery.

The negative control disables only the script thread's check before execution.
Host cancellation and the check after serialization remain active.
The queued fixture detects the mutation despite successful host cancellation. Both existing Servo controls still pass.

## Active JavaScript interruption

The script thread starts one watcher thread when it receives its first embedder evaluation.
After the execution check, the script thread records the evaluation as active and sends its permit to the watcher.
The watcher waits on that permit. Closure wakes it immediately.
If the evaluation is still active, the watcher marks it interrupted and calls SpiderMonkey's thread-safe interrupt request.
The engine's existing interrupt callback then returns false for that evaluation only.
SpiderMonkey terminates the script without a catchable exception, so `catch` and `finally` blocks do not run.
The script thread reports `Cancelled`. The host has already removed and called the callback.
The script thread, its other documents, and later evaluations continue.

The interrupt covers JavaScript execution and result serialization, including getters.
It does not undo effects that happened before the interrupt check.
It does not revoke work that the script scheduled before termination.
A promise reaction runs at the microtask checkpoint after termination, and a timer runs later.
A product contract must revoke or tolerate that scheduled work.

The interrupt request is asynchronous. Straight-line code can finish before SpiderMonkey checks for an interrupt.
The execution check therefore remains necessary for queued work.
The watcher clears its state on every exit path, including a Rust unwind.
A late interrupt request after the evaluation finishes has no effect.
Interrupt requests become no-ops after the JavaScript runtime is destroyed.
Embedder evaluations do not nest. An inner request would run without interruption.

The watcher polls a completed evaluation's permit for at most 10 milliseconds.
A new evaluation can wait that long before the watcher observes its permit.
The check after serialization now uses the watcher's observation rather than a direct channel read.
A closure that the watcher has not yet observed can produce a success reply.
The host has already removed that callback, so it cannot receive the reply.

The permit uses Servo's generic transport, and the watcher runs in the script's process.
The interruption checkpoint covers the single-process backend.
The process audit below also checks interruption through multiprocess transport.
This experimental mechanism needs upstream review before product integration.

Four new fixtures use page console messages only to establish test order.
Each running script logs entry before a bounded four-second loop. The host cancels only after that message arrives.
If interruption fails, the loop completes and the mutation counter records the later increments.
The active fixture requires a skipped `finally` block, one callback, unchanged scope, and no crash.
The serialization fixture interrupts a result getter.
The scheduled-work fixture records the microtask and waits for the timer after interruption.
The control fixture runs a 300-millisecond loop with an open permit and requires its success.
In the final runs, the next result arrived 77 to 7,491 microseconds after cancellation.
Those values only confirm that the follow-up did not wait for the four-second loop. They are not latency measurements.

One negative control makes the interrupt callback ignore closed permits.
Host cancellation still delivers `Cancelled`, but the loop, `finally` block, and trailing increment all run.
The second negative control disables only the execution check, with the watcher still active.
The cancelled queued mutation runs, so asynchronous interruption does not replace that check.
Both existing Servo controls pass with each negative control.

## Content-process failure audit

The process audit uses Servo's public multiprocess feature and content-process entry point.
The fixture enables `Opts::multiprocess` and keeps `hard_fail = false`.
Content processes use the same panic hook as the host for the existing script-thread unwind case.
These runs use unsandboxed content processes on macOS arm64. The product keeps its existing single-process configuration.

The source audit found that the process manager only reaped a child after its lifeline closed.
It did not notify the affected views. Host scope and pending callbacks remained live until another send failed.
The experimental patch records each child's script event-loop identity beside its lifeline.
An unexpected lifeline closure now enters the existing panic recovery path for that event loop.
Recovery revokes affected views before terminal callbacks and creates a crash document.
Service-worker processes have no script event-loop identity and retain their existing behavior.

Ordinary close removes each pipeline from its browsing context before script teardown.
The patch checks that connection before recovery because normal pipeline-exit messages can still be queued.
During engine shutdown, the patch completes pipeline teardown without new crash recovery.
These checks preserve normal teardown. Dedicated race controls for these checks remain open.

The new fixture blocks an independent content process at an experimental barrier.
The host retains a successful reply and queues eight callbacks, with both scoped and legacy requests.
A constellation acknowledgment proves that those requests reached the blocked child before release.
The child then calls `std::process::exit(86)` without an unwind, panic report, or destructor cleanup.
The host sends no further work until the lifeline delivers the crash notification.
The fixture requires that notification within five seconds and checks its exact recovery reason.

A second fixture forces termination from outside the child.
The barrier reports the child's process id, and the host checks that it differs from its own.
After the same acknowledgment, the host runs `kill -KILL` on that id while the child is still blocked.
In this mode the child does not exit on release, so only the signal can end it.
The fixture then applies every assertion of the exit fixture to a new view and content process.

The fixture requires host revocation before all eight terminal callbacks and release of their captures.
It rejects the retained reply before host allocation and requires an empty pending table.
It evaluates the crash document, navigates to a new document, and rejects the failed document's identity.
An independent view keeps its scope and accepts evaluation.

All thirty-eight shared cases also run in multiprocess mode, including queued cancellation and active interruption.
The exit and kill fixtures add two cases in that mode, for forty cases in total.
The exit checkpoint's negative control disables only lifeline recovery after process reaping.
It reaches the process fixture deadline with eight callbacks and obsolete host scope still live.
The kill negative control keeps the barrier, process id report, and acknowledgment, but sends no signal.
The child then resumes and answers all eight evaluations normally, and the fixture fails at the crash deadline.
Recovery in the passing kill case therefore depends on the signal.
Both existing Servo controls pass with each negative control.

The runner uses an independent process group for each case.
It stops the group after each case, including a deadline, so child processes cannot survive a failed probe.
Six portable runner tests check mode admission, marker rejection, and child cleanup after a deadline.
The marker test rejects a multiprocess run that omits either process case.
Single-process cases retain a 35-second deadline. Multiprocess scope cases use a 90-second deadline for child startup and recovery.

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
| Queued engine cancellation | The queued mutation never runs. One callback receives `Cancelled`. Captures release, reentrant evaluation succeeds, and foreign-view cancellation fails. |
| Completed engine cancellation | Cancellation suppresses the pending reply but preserves the completed mutation. Cancellation after host delivery returns false. |
| Cancellation callback panic | The execution permit closes before the callback panic. The mutation never runs, captures release, and later evaluation succeeds. |
| Active interruption | Cancellation terminates a running loop and skips its `finally` block. One callback receives `Cancelled`, and scope is unchanged. |
| Serialization interruption | Cancellation terminates a result getter during serialization. Later evaluation succeeds. |
| Interrupted scheduled work | A promise reaction and a timer from the interrupted script still run. |
| Uninterrupted long evaluation | A 300-millisecond loop with an open permit completes successfully. |
| Reload | Reload replaces the identity and rejects the previous scope. |
| History | Reactivation assigns a fresh incarnation. An old request cannot mutate the restored document. The host rejects the old successful reply. |
| Redirect | A real HTTP 302 reaches the final fixture. The host rejects the departed reply, and the engine rejects its old scope. |
| Failed load | An unsupported scheme activates an error document. The host and engine reject the departed scope. |
| Different views | Each view rejects the other view's document identity. |
| Subframe isolation | A nested document reset preserves the top identity and cannot emit a different top lifecycle identity. |
| Close | The engine revokes the document before the close notification. The host rejects the retained reply, and the evaluator releases a pending callback. |
| Injected crash | The constellation's panic handler revokes the document before the crash notification. The host rejects the retained reply. |
| Script-thread failure | An actual Rust unwind revokes scope and releases eight pending callbacks exactly once. A new document accepts evaluation after recovery. |
| Content-process failure (multiprocess only) | A child exits without an unwind. Its lifeline revokes scope and releases eight callbacks without a later send. Recovery and an independent view accept evaluation. |
| Content-process kill (multiprocess only) | The host sends SIGKILL to a blocked child. The same lifeline revocation, callback release, recovery, and survivor assertions apply. |
| Final handle release | Release revokes host scope and mixed callbacks synchronously. A callback can submit work to a surviving view. |
| Release before activation | Release cleans eight callbacks before the host receives a document identity. |
| Engine shutdown | Shutdown releases sixteen ordinary callbacks, breaks a retained-view cycle, rejects new evaluation, and completes the engine shutdown. |
| Result callback panic | The embedder catches the panic. The evaluator releases captures and accepts a later request. |
| Lifecycle callback panics | All eight callbacks receive terminal errors after host revocation. The embedder catches the first panic and continues the event loop. |
| Final release callback panics | All eight callbacks receive terminal errors. A surviving view still accepts evaluation. |
| Release during host unwind | All eight callbacks receive terminal errors before the original host panic reaches its handler. |
| Shutdown callback panics | All callbacks receive terminal errors. Admission stays closed, the retained-view cycle breaks, and engine shutdown completes. |
| Lifecycle delegate panic | The delegate panic precedes eight callback panics. All callbacks receive terminal errors, and the close notification follows. |
| Release delegate panic | All eight callbacks receive terminal errors. A surviving view still accepts evaluation. |
| Delegate panic during host unwind | All eight callbacks receive terminal errors before the original host panic reaches its handler. |
| Shutdown delegate panics | Both retained views receive revocation despite delegate panics. Engine shutdown completes. |

The runner repeats thirty-eight cases in single-process mode or forty cases in multiprocess mode.
Both modes retain the existing Servo contract and E4 baseline results.
The host-ticket fixture suppresses allocation after delivery. Engine cancellation separately rejects closed permits before and during execution.
The injected crash calls the existing constellation panic handler through an experimental probe method.
The separate script-thread fixture terminates the receiving thread through a Rust unwind. Neither fixture proves process-failure detection.

## Local evidence

The [kill evidence](../experiments/phase3/document-kill-evidence-2026-11-24.json) records the current runs, source hashes, binary hashes, and local checks.
It also records one run of the previous checkpoint in each mode, with both Servo controls, before development.
The [process evidence](../experiments/phase3/document-process-evidence-2026-11-24.json) preserves the thirty-nine-case multiprocess checkpoint.
The [interruption evidence](../experiments/phase3/document-interruption-evidence-2026-11-24.json) preserves that checkpoint and its earlier cancellation baseline.
The [cancellation evidence](../experiments/phase3/document-cancellation-evidence-2026-11-24.json) preserves the thirty-four-case checkpoint.
The [delegate evidence](../experiments/phase3/document-delegate-evidence-2026-11-24.json) preserves the thirty-one-case checkpoint.
The [callback evidence](../experiments/phase3/document-callback-evidence-2026-11-24.json) preserves the twenty-seven-case checkpoint.
The [cleanup evidence](../experiments/phase3/document-cleanup-evidence-2026-11-23.json) preserves the twenty-two-case checkpoint.
The [audit evidence](../experiments/phase3/document-audit-evidence-2026-11-23.json) preserves the nineteen-case checkpoint.
The [lifecycle evidence](../experiments/phase3/document-lifecycle-evidence-2026-11-23.json) preserves the sixteen-case checkpoint.
The [earlier evidence](../experiments/phase3/document-scope-evidence-2026-11-22.json) preserves the nine-case checkpoint before lifecycle notifications.
That earlier checkpoint retained identities across history activation. The current prototype deliberately changes that policy.

All thirty-eight shared cases passed in three single-process runs on macOS arm64 with software rendering.
All forty cases passed in three multiprocess runs.
Both existing Servo controls, all 61 portable tests, formatting, and portable/probe Clippy passed locally.
Clippy denied warnings for the probe and portable workspace.
Python syntax checks and six runner tests also passed.
These builds retained the previous line-table debug configuration.

The prior interruption checkpoint records the following preliminary fixture defects and negative controls.

Two preliminary fixture defects failed before the final runs.
A top-level `const` in an interrupted script persisted as a global binding and broke a later evaluation.
A 300-millisecond lower bound failed at 299.3 to 299.8 milliseconds because `Date.now()` has millisecond precision.
The final fixtures use block scopes and a 250-millisecond bound.

The cancellation checkpoint's preliminary validation reused a negative-control binary after source restoration preserved an older timestamp.
Each negative control here uses a separate preparation, so Cargo compiles separate engine packages.
The evidence records distinct binary hashes for the final candidate and both negative controls.

One preliminary repeat timed out at the redirect after two successful repeats.
The final fixture waits for explicit history-traversal completion and exposes HTTP worker errors before the page deadline.
Five diagnostic repeats passed after that change. The exact cause of the earlier timeout remains unproven.
The lifecycle evidence retains that failed repeat and its source hashes.

The prior audit exposed an immediate nonblocking read error on an accepted loopback socket.
The fixture now explicitly selects blocking reads with a timeout.
The audit evidence retains that diagnostic log and a successful diagnostic run before the three final repeats.
The earlier redirect timeout's exact cause remains unproven.

The initial sandboxed graphics run failed during macOS surface initialization.
Graphics runs require normal macOS graphics services outside that sandbox.
Servo emits its known GL texture warning and debug linker warning.
The copied engine packages also emit warnings from published code under disabled features.
The panic hook logs the intentional script-thread panic, callback and delegate panics, and host unwinds. Recovery also logs pipeline teardown warnings.
Explicit shutdown logs script termination, profiler disconnection, and timer warnings.
Release of retained handles after shutdown also logs constellation disconnection warnings.
All assertions and process exit checks passed despite these warnings.
The experiment adds an optional dependency on the already locked `log` crate. No dependency version changes.
No remote CI, physical input, Linux, or release latency result applies to this checkpoint.

## Remaining requirements

This prototype does not close S1 or permit a restricted DOM API.
Unexpected content-process exit, forced kill, queued cancellation, and active interruption now have unsandboxed multiprocess evidence.
Their contracts still need upstream review.
The kill fixture covers only a child blocked at a barrier.
Kills during JavaScript execution, IPC writes, or document construction remain untested.
Shared-process views and normal teardown races need dedicated process fixtures.
A product contract must decide how to revoke or tolerate work that an interrupted script already scheduled.
Callback cleanup still needs checks for backend disconnection and failures before document construction.
Upstream review must settle delegate and callback panic propagation, precedence, and suppression during an existing unwind.
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
python3 experiments/prepare_document_scope.py --out target/phase3-document-kill
```

Build the candidate:

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_TARGET_DIR="$PWD/target" cargo build --offline --locked \
  --manifest-path target/phase3-document-kill/candidate/Cargo.toml \
  -p bulwark-experiment-e4 --features document-scope-multiprocess
```

Run the candidate with independent 35-second process deadlines:

```sh
python3 experiments/run_document_scope.py \
  --binary target/debug/bulwark-experiment-e4 \
  --preparation target/phase3-document-kill \
  --output target/phase3-document-kill-single-evidence
```

Run the multiprocess cases with independent 90-second process deadlines:

```sh
python3 experiments/run_document_scope.py \
  --binary target/debug/bulwark-experiment-e4 \
  --preparation target/phase3-document-kill \
  --output target/phase3-document-kill-multiprocess-evidence --multiprocess
python3 -m unittest discover -s experiments -p test_document_scope_runner.py -v
```

The preparation refuses an existing output directory.
It records the patch hash, original source hashes, product manifest hashes, and candidate probe hashes in `provenance.json`.
The runner rejects a preparation whose patch, product manifests, or probe files differ from the current sources.
Only candidate manifests reference the copied packages.
The runner records the binary hash, probe hashes, case markers, the summary marker, exit statuses, and log hashes.
Both document-scope features require the patched candidate and cannot compile against the product engine.
The normal E4 feature and the portable workspace retain their existing build commands.
