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

A separate [frame failure notice](#frame-failure-notice) reports a failure that stayed inside child frames. It is not a lifecycle event and revokes nothing.

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
That path revokes every view whose own top-level document ran in the failed event loop and creates the engine crash document.
A failure inside a child frame of another view stays in that frame, as the [frame failure containment](#frame-failure-containment) section describes.
Before the [crash error page audit](#crash-error-page-audit), the fixture's interceptor replaced that document's error content.
Earlier checkpoints therefore evaluated the failed view's previous URL served again, not Servo's error page.
The fixture requires the unwind notification, eight terminal errors, released callback captures, an empty pending table, and successful recovery.
A retained successful reply from before the unwind must also fail the host allocation check.

This fixture covers a Rust unwind after document construction in both process modes.
The [construction failure audit](#construction-failure-audit) covers unwinds before construction.
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
Unresponsive page tasks outside embedder evaluation remain outside these cleanup checks.
The [backend loss audit](#backend-loss-audit) covers constellation loss in a separate engine.

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
Servo content-process sandboxing is disabled only in this local experimental fixture configuration on macOS arm64. The product keeps its existing single-process configuration.

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

All sixty-three shared cases also run in multiprocess mode, including queued cancellation, active interruption, construction failures, and history recovery.
The exit and kill fixtures add two cases in that mode. The [shared-process audit](#shared-process-audit) adds four more.
The [recovery audit](#pending-process-and-recovery-audit) adds four pending-process cases.
The [cross-site parsing audit](#cross-site-parsing-audit) adds four parsing-process cases.
The [pending frame audit](#pending-frame-audit) adds eight cases.
The [frame failure containment](#frame-failure-containment) change converts eight of those multiprocess cases.
The [frame path audit](#frame-path-audit) adds six cases.
The [frame group failure](#frame-group-failure) cases add four more, for ninety-five cases in total with the current shared cases.
The exit checkpoint's negative control disables only lifeline recovery after process reaping.
It reaches the process fixture deadline with eight callbacks and obsolete host scope still live.
The kill negative control keeps the barrier, process id report, and acknowledgment, but sends no signal.
The child then resumes and answers all eight evaluations normally, and the fixture fails at the crash deadline.
Recovery in the passing kill case therefore depends on the signal.
Both existing Servo controls pass with each negative control.

The runner uses an independent process group for each case.
It stops the group after each case, including a deadline, so child processes cannot survive a failed probe.
Eight portable runner tests check mode admission, marker rejection, and child cleanup after a deadline.
The marker test rejects a multiprocess run that omits any multiprocess-only case.
Two tests reject a backend loss run with a missing, reordered, or misplaced marker.
Single-process cases retain a 35-second deadline. Multiprocess scope and backend loss cases use a 90-second deadline for child startup and recovery.

## Shared-process audit

The earlier process fixtures each placed one affected view in its own content process.
These fixtures open two popups from the affected view. A popup shares its opener's content process.
A barrier in the child reports its process id to the host. Page JavaScript cannot supply this value.
The fixture requires the same process id for all three views, and a different id from the host.

In exit mode, the third view closes normally while the shared child waits at a barrier.
The closed view receives revocation before eight terminal callbacks, and the host rejects its retained reply.
The opener and the other popup keep their scope and accept evaluation. No view enters crash recovery.

The exit and kill fixtures then queue eight callbacks for both the opener and the popup.
The lifeline must revoke both views, release all sixteen callbacks, and report the exact recovery reason for each view.
Both views must evaluate their crash documents, navigate to new documents, and reject their failed identities.
The independent view keeps its scope.

The kill fixture also orders a teardown race.
It queues eight callbacks for the third view and closes that view while the child is blocked.
The host then kills the child before the child can acknowledge pipeline exit.
The constellation still holds the closed view's pipeline when the lifeline closes.
Without a guard, the panic handler created crash recovery for the closed view and sent it a crash notification.
The experimental patch now ignores views that the constellation no longer holds in this handler.
Close has already revoked those views. The fixture requires that revocation, released callbacks, and no crash notification.

The negative control removes only that guard. It fails the race with the exact crash reason `Content process disconnected` on the closed view.
The ordinary close, shared exit, and shared kill cases still pass with that control.
This race orders close before kill at a barrier. Other orders and timing remain untested.

## Construction failure audit

The script thread constructs a document only after the navigation response headers arrive.
The fixture host withholds selected navigation responses, so their pipelines exist without a document.
A new probe message sends the existing script-thread panic to a view's newest pending top-level pipeline.
Its acknowledgment reports whether that pipeline runs in an event loop other than the active document's.
Both fixtures require a separate event loop and the exact unwind reason. These checks prevent a vacuous pass.

The first fixture withholds a new view's first response.
The constellation has no top browsing context for that view until activation.
Nine identity, unscoped, and foreign-scoped requests fail with `InternalError` and retain no callback or capture.
The host also refuses admission, because no document is active.
After the unwind, the panic handler sends `Active(None)` and the crash notification.
It then finds no top browsing context and returns before crash recovery. Pipeline exit sends a second `Active(None)`.
The view then has no browsing context and no pending change.
Evaluation still fails with `InternalError`. Load and reload also require a top browsing context, so the view cannot recover.
Close and final release still notify the host. An independent view remains usable.

The first multiprocess run found an ordering hazard in the fixture.
The second `Active(None)` can arrive after the crash notification.
That event revoked the next request with `StaleDocument` instead of `InternalError`.
Both outcomes are terminal denials. A host must not assume that the crash notification is the last revocation from that failure.
The fixture now waits for the crash notification and both events, and requires exactly that sequence.

The second fixture withholds a cross-site navigation from a live view.
The pending pipeline runs in a separate event loop, and the active document stays live.
The host queues eight callbacks while the active document's script thread waits at a barrier.
It unwinds the pending pipeline's thread and drives the engine inside the barrier.
The barrier makes revocation the only path that can complete those callbacks.
Upstream recovery treats the failure as a failure of the whole view.
The panic handler revokes the healthy active document and releases all eight callbacks. A crash document then replaces it.
The host rejects the retained reply, the view navigates normally, and the engine rejects the old scope.
The owner accepted this replacement as product behavior.

These fixtures characterize existing engine and patch behavior. The only engine change is the probe message, so they have no negative control.
The [partial response audit](#partial-response-audit) covers failures after the response headers.
The [synchronous construction audit](#synchronous-construction-audit) covers `about:blank`, `srcdoc`, and `document.write` documents.
The recovery audit below covers content-process exit and kill before construction.

## Partial response audit

Servo activates a navigation's document when the response headers arrive, before the body is complete.
Its parser buffers the first 1,445 body bytes for MIME sniffing. It creates the DOM only after that buffer fills or the body ends.
Servo's request interceptor buffers an intercepted body until the load finishes, so an intercepted response cannot stream.
These fixtures therefore serve the document from a loopback HTTP server through the real loader.
The server sends the headers and part of the body, then holds the connection open.

The first case sends a body shorter than the sniffing buffer.
The active document has no document element, and its inline script has not run.
The second case pads the body past the buffer. The parser runs an inline script and then blocks on a classic script that the host withholds.
Servo's prefetch tokenizer and the parser each request that script, so the host withholds two requests.
The case requires the `loading` state and requires that the element after the script does not exist.

In each case, the host unwinds the script thread and then queues eight callbacks.
Revocation precedes all eight terminal callbacks, and the retained reply cannot allocate. The crash page shows the failure reason.
The host then releases the withheld scripts and the rest of the main body.
After 300 milliseconds the crash page keeps its identity, no other failure occurs, and the engine reports no error.
The failed document never reports DOM readiness or load completion, and its identity is stale.
Reload from the crash page restores the failed URL with its loopback origin. A new URL also recovers.
The server's write of the held tail succeeded in every run. This shows only that the socket accepted the bytes.
The fixture cannot tell whether Servo cancels or drains the failed document's open fetches.

These cases characterize existing behavior. The engine patch is unchanged, so they have no negative control.
Both use a top-level navigation from a new view and a script-thread unwind.
The [synchronous construction audit](#synchronous-construction-audit) covers `about:blank`, `srcdoc`, and `document.write`.
The [cross-site parsing audit](#cross-site-parsing-audit) covers a cross-site navigation, a cross-site subframe, a sandboxed `srcdoc` child, and process exit and kill during parsing.

## Synchronous construction audit

Servo constructs `about:blank` and `srcdoc` documents in the script thread without a navigation fetch.
The task that receives the load builds the document, so a script-thread unwind cannot reach such a pipeline before construction.
These fixtures therefore unwind the script thread of a complete `about:blank` document or of a document whose parser waits for a withheld script.
Five shared cases use a new view and the existing script-thread panic:

- `about-blank-failure`: The view's initial `about:blank` document unwinds after it reports DOM readiness and load completion.
- `about-blank-write-failure`: `document.write` into that document opens a script-created parser. The written script stays withheld.
- `srcdoc-parser-blocked-failure`: A `srcdoc` child waits for a withheld script.
  The parent reads the child's `loading` state synchronously, so both documents run in one script thread.
  The parent reports DOM readiness, but its load event waits for the child.
- `document-write-parser-blocked-failure`: An inline script writes an external script during the initial parse.
  The parser waits for that script, and the document reports no readiness.
- `document-open-write-failure`: `document.open()` resets a complete document, and the written script stays withheld.

Both write-after-load cases allocate a fresh incarnation in the same pipeline. The engine rejects the reset script's result.
Each parser-blocked case received exactly one request for its withheld script.
Each case requires revocation before eight terminal callbacks, released captures, and an empty pending table.
The host rejects the retained reply before allocation. The engine rejects the failed identity and, after a reset, the source identity.
The host then releases the withheld script. After 300 milliseconds the crash document keeps its identity.
No other failure occurs, the engine reports no error, and the failed document reports no further readiness.
Reload restores the original load. A new URL recovers, and an independent view stays usable.
For `document.open()` and `about:blank`, reload restores the document without the written content.

The crash document depends on the failed document's URL.
For an HTTP URL, the crash document is Servo's error page with the failure reason. Its origin is `null`, and an inserted script does not run.
For `about:blank`, Servo uses its synchronous load, which ignores the crash details. The crash document is blank.
That load also sets the active sandboxing flags from the target snapshot parameters and the CSP list.
Crash recovery uses default snapshot parameters, so these flags are empty.
The blank crash document has the origin `null`, and it runs inserted scripts. The reloaded `about:blank` document also has the origin `null`.
The error-page path skips that step and keeps the crash load's creation flags.
A host cannot tell a blank crash document from an ordinary `about:blank` document by content.
Only the crash notification and the fresh identity identify it. The product's native error state must cover these views.

A sixth shared case, `javascript-url-crash-recovery`, checks the same synchronous path for a document that a `javascript:` URL created.
The JavaScript result replaces the source document in a new pipeline. That pipeline's load data keeps the result.
The case requires the same revocation before eight terminal callbacks, released captures, and rejection of the retained reply.
At this audit, crash recovery rendered the JavaScript result again as the crash document.
That document had the source URL and the page's real origin, `https://bulwark.invalid`. It did not contain the failure reason.
Its own inline script and an inserted script both ran.
Reload replaced the crash document with its original load, and that load rendered the same result again with the same origin.
A new URL recovered, and an independent view stayed usable. The behavior was the same in both modes.
The [JavaScript URL crash page](#javascript-url-crash-page) section supersedes this finding. The patch now drops the result from the crash load.

The first five cases characterize existing behavior. The engine patch was unchanged at this audit, so they have no negative control.
They do not cover process exit or kill during these parses.
The [cross-site parsing audit](#cross-site-parsing-audit) covers cross-site subframes and a `srcdoc` child in another event loop.

## Cross-site parsing audit

This audit fails documents while their parser waits for a withheld script in an event loop of their own.
A loopback HTTP server serves each cross-site document through the real loader and holds the response open.
The server sends the headers and a body that is longer than the sniffing buffer.
The parser runs the first script and then waits for a withheld classic script.

Servo has no pending top-level document that parses.
The script thread creates the document when the response headers arrive and asks the constellation to activate it at that time.
The parser receives body bytes only after that request.
The cross-site navigation case checks this. While its document parses, the probe for a pending top-level pipeline finds none.
A failure before the headers is the [pending navigation failure](#construction-failure-audit).

The engine patch adds two probe messages and four probe methods. They select the newest child frame of the active top-level document.
One method unwinds that frame's script thread and reports whether its event loop differs from the top-level document's.
The other three use the existing barrier on that frame's content process to exit it, kill it, or read its process id.
The probes do not change engine behavior outside the fixture. A frame's state reaches the parent only through `postMessage`, which orders the fixture.

Three shared cases unwind a script thread:

- `cross-site-parser-blocked-failure`: A view loads two documents at the fixture origin, opens a popup, and navigates to the loopback site.
  The popup's `about:blank` document shares the source document's event loop.
  The parsing document runs in another event loop, and the source document stays in history.
- `cross-site-subframe-parser-blocked-failure`: A parent at the fixture origin has a child frame from the loopback site.
  The child waits for its withheld script, and the parent's load event waits for the child.
  One popup stays at `about:blank` in the parent's event loop. A second popup loads the loopback site, so it shares the child's event loop.
- `sandboxed-srcdoc-parser-blocked-failure`: A parent has a `srcdoc` child with `sandbox="allow-scripts"`.
  The child has an opaque origin. Servo never reuses an event loop for such a load, so the child runs in a new event loop.
  One popup stays at `about:blank` in the parent's event loop.

In both frame cases, the probe reports a separate event loop. The parent cannot read the child's document and receives `SecurityError`.
The cross-site child requested its withheld script twice, and the `srcdoc` child requested it once.

Four multiprocess cases end a content process at the barrier instead:

- `cross-site-parser-blocked-process-exit` and `cross-site-parser-blocked-process-kill` end the parsing document's process.
- `cross-site-subframe-process-exit` and `cross-site-subframe-process-kill` end the child frame's process.

In multiprocess mode, every case checks the engine-reported process ids.
The parsing document's process differs from the host's and from the source-loop popup's process.
A child frame's process differs from the host's and from the parent's process.
The parent-loop popup shares the parent's process, and the child-site popup shares the child's process.

A failure of an event loop revokes every view that has a pipeline in that event loop.
Upstream recovery then replaces each of those views with a crash document.
Views whose pipelines all run in other event loops keep their scope.
The cases assert these scope results:

- Cross-site navigation: The parsing document loses scope. The popup in the source event loop keeps its scope, and its retained reply still passes the host allocation check.
  Recovery also closes the source document's history pipeline, although that event loop stays healthy.
- Cross-site subframe: The healthy parent loses scope, although it runs in another event loop. The child is gone with its event loop.
  The child-site popup in the child's event loop loses scope. The parent-loop popup keeps its scope and its retained reply.
- Sandboxed `srcdoc` child: The healthy parent loses scope. The parent-loop popup keeps its scope and its retained reply.
- An independent view keeps its scope in every case.

The host holds each revoked document's eight callbacks. For a frame failure, a barrier on the healthy parent's script thread holds them, so only revocation can complete them.
For the subframe process cases, the child-site popup's eight callbacks reach the blocked child before it ends.
Each revoked view receives revocation before its terminal callbacks and the exact reason.
The reason is `S1 probe actual script-thread panic` for an unwind and `Content process disconnected` for exit and kill.
Captures release, the pending tables are empty, and each revoked view's retained reply fails the host allocation check.
Each crash document is Servo's error page with the failure reason.

The host then releases the withheld scripts and the held response tail.
After 300 milliseconds the crash document keeps its identity, no other failure occurs, and the engine reports no error.
The failed document never reports load completion. The server's write of the held tail succeeded in every run.
The engine rejects each failed identity.
For the navigation, reload restores the loopback document with its loopback origin. Back rebuilds the source document with a fresh identity and the fixture origin.
For the frames, reload restores the parent with the fixture origin, and the parent reports load completion after the child completes.
A new URL recovers each revoked view, and the independent view stays usable.

These cases characterized the behavior before the [frame failure containment](#frame-failure-containment) change.
That change replaces the frame results. The healthy parent now keeps its document and its scope, and only the frame shows an error page.
The navigation results do not change.
The [pending frame audit](#pending-frame-audit) covers a pending navigation in a child frame and process loss of the sandboxed `srcdoc` child.
The [frame path audit](#frame-path-audit) covers history traversal inside a frame after a contained failure.

## Pending frame audit

This audit fails a child frame whose navigation waits for its response headers.
It also ends the process of the sandboxed `srcdoc` child while its parser waits.

A parent at the fixture origin loads a same-site child frame and completes its load.
A host evaluation then sets the frame's `src` attribute.
The constellation creates a new pipeline for the frame's browsing context. That pipeline stays a pending change until its document activates.
The host withholds the navigation response, so the pending pipeline has no document.
The old child document stays active, and the parent still reads its marker.
The parent keeps its identity and reports no further readiness.
The probe for a pending top-level pipeline finds none.

The engine patch adds one probe message and four probe methods. They select the view's newest pending change for a child browsing context.
One method unwinds the script thread of that pending pipeline and reports whether its event loop differs from the top-level document's.
The other three use the existing barrier on that pipeline's content process to exit it, kill it, or read its process id.
The script thread accepts the unwind and the barrier for a pipeline without a document.
The probes do not change engine behavior outside the fixture.

Three shared cases unwind the event loop of the pending pipeline:

- `same-site-frame-pending-navigation-failure`: The frame navigates within the fixture origin.
  The pending pipeline shares the parent's event loop, so the parent's script thread unwinds.
  One popup stays at `about:blank` in the parent's event loop.
- `cross-site-frame-pending-navigation-failure`: The frame navigates to the loopback site.
  A popup loaded that site before the navigation, so the pending pipeline joins the popup's event loop.
  A second popup stays at `about:blank` in the parent's event loop.
- `cross-site-lone-frame-pending-navigation-failure`: The frame navigates to a site that no other document uses.
  The pending pipeline is the only pipeline in a new event loop.
  One popup stays at `about:blank` in the parent's event loop.

The probe reports the parent's event loop for the same-site case and a separate event loop for both cross-site cases.
The host withholds the cross-site navigations before they reach a network. The loopback server receives only the popup's request.

Six multiprocess cases end the process of the pending pipeline at the barrier instead:

- `same-site-frame-pending-process-exit` and `same-site-frame-pending-process-kill`.
- `cross-site-frame-pending-process-exit` and `cross-site-frame-pending-process-kill`.
- `cross-site-lone-frame-pending-process-exit` and `cross-site-lone-frame-pending-process-kill`.

In multiprocess mode, every case checks the engine-reported process ids.
The parent, its active child, and the parent-loop popup share one process. The same-site pending pipeline also runs in that process.
The cross-site pending pipeline runs in another process, which it shares with the child-site popup.
The lone pending pipeline runs in a process that differs from the host's, the parent's, and the independent view's.

The panic handler and the lifeline handler select every pipeline in the failed event loop, including pending pipelines.
Each view that owns one of those pipelines is revoked and recovered.
The cases assert these scope results:

- Same-site: The parent and the parent-loop popup lose scope. Both run in the failed event loop.
  The popup's crash document is blank, because the popup was at `about:blank`.
- Cross-site: The healthy parent loses scope. Its only pipeline in the failed event loop is the pending pipeline.
  The child-site popup loses scope. The parent-loop popup keeps its scope and its retained reply.
- Cross-site, alone: The healthy parent loses scope. The parent-loop popup keeps its scope and its retained reply.
- An independent view keeps its scope in every case.

The host holds eight callbacks for the parent and for each popup that loses scope.
In the cross-site cases, a barrier on the healthy parent's script thread holds the parent's callbacks, so only revocation can complete them.
In the process cases, the barrier holds the callbacks of each view in the failed process before that process ends.
Each revoked view receives revocation before its terminal callbacks and the exact reason.
Captures release, the pending tables are empty, and each revoked view's retained reply fails the host allocation check.
The parent's crash document is Servo's error page with the failure reason. So is the child-site popup's crash document.

The host then answers the withheld navigation with a document that sets a marker and posts a message to its parent.
After 300 milliseconds each crash document keeps its identity, no other failure occurs, and the engine reports no error.
The failed parent reports no further readiness. The engine rejects each failed identity.
Reload restores the parent with the fixture origin, and its frame loads the original child document.
A new URL recovers the parent and each revoked popup. The independent view stays usable.

In the lone process cases, no active document shares the failed process.
The source shows that the lifeline handler then treats the process as live only through the pending-change clause of the [recovery audit](#pending-process-and-recovery-audit).
In the other process cases, an active document in the failed process also makes it live.
That clause's negative control stops at the first pending-process case.
The [frame failure containment](#frame-failure-containment) section adds a control that isolates the clause for frames.

Two more multiprocess cases end the process of the sandboxed `srcdoc` child from the [cross-site parsing audit](#cross-site-parsing-audit):
`sandboxed-srcdoc-process-exit` and `sandboxed-srcdoc-process-kill`.
The child's process differs from the host's and from the parent's process. The parent-loop popup shares the parent's process.
The lifeline revokes the healthy parent before eight callbacks that a barrier holds on the parent's script thread.
The reason is `Content process disconnected`. The parent-loop popup keeps its scope and its retained reply.
The crash page, late-data, reload, new URL, and survivor checks of the unwind case also pass.

These cases characterized the behavior before the [frame failure containment](#frame-failure-containment) change.
That change replaces the healthy parent's results in both cross-site layouts and in the sandboxed `srcdoc` process cases.
The same-site results do not change.
Each case navigates a loaded frame through its `src` attribute.
The [frame path audit](#frame-path-audit) covers a frame's first navigation and history traversal inside a frame.
A navigation that the child starts and a pending traversal inside a frame remain unchecked.

## Frame failure containment

The owner decided that a child frame's failure must stay inside that frame, per the [recorded decisions](phase-3-plan.md#recorded-decisions-2026-11-26).
A healthy parent in another event loop keeps its document and its scope. Only the failed frame shows an error state.
This applies to an active frame and to a frame whose navigation is pending.

The panic handler selected every view with a pipeline in the failed event loop.
It then replaced each of those views with a crash page, as for a failure of the view's own document.
The experimental patch now sorts those views into two groups.
A view fails if its active top-level pipeline or a pending top-level pipeline ran in the failed event loop.
A view without a top-level browsing context also fails. Each failed view keeps the existing recovery.
It receives revocation, a crash notification, and a crash page, as before.
Every other view keeps its document and its scope. It receives no lifecycle event and no crash notification.
It receives one [frame failure notice](#frame-failure-notice) instead.
One failure can therefore send crash notifications to several views, and a host must expect each of them.
The lifeline handler uses the same panic handler, so process exit and kill follow the same rules.
A failed send to a frame's pipeline now follows the same rules for its view, per the [frame send failure](#frame-send-failure) section.

In a surviving view, the patch finds each outermost frame whose current or pending pipeline ran in the failed event loop.
It builds a crash load from that pipeline's load data, with every sandboxing flag, as the top-level recovery does.
After the failed pipelines close, the constellation sends the existing `NavigateIframe` message to the frame's parent.
That message replaces the frame's current history entry.
The parent's frame element then starts this navigation as its own, so it accepts the new pipeline at activation.
The source shows that a crash pipeline created only in the constellation would not match the frame element's pending pipeline.
The patch also closes any other pending pipeline of that frame, so a later activation cannot replace the crash page.
Inactive history pipelines of a surviving view in the failed event loop are marked for reload before they close, as the top-level recovery does.
The [frame path audit](#frame-path-audit) checks both paths.

The frame's error state is Servo's error page with the failure reason.
The page has every sandboxing flag, so it has an opaque origin and runs no scripts.
Servo never reuses an event loop for a sandboxed origin, so the page runs in a new event loop. In multiprocess mode, it runs in a new content process.
For a failed pending navigation, the error page replaces the old child document. Its load data is that of the failed navigation.
The parent cannot read the error page, and the host has no evaluation path into a frame.

The script thread loads `about:blank`, `about:srcdoc`, and `javascript:` URL documents synchronously, without the crash fetch.
For a `srcdoc` frame, that path would parse the frame's markup again as the crash document.
The patch therefore sends every crash load of a child frame through the fetch path.
Top-level crash loads keep the synchronous `about:blank` path. The owner's decision for failed views at `about:blank` applies to them.
The [JavaScript URL crash page](#javascript-url-crash-page) section removes a `javascript:` URL result from every crash load.

A new probe reports the constellation's record of the active top-level document's newest child frame.
It returns the frame's current pipeline, whether that pipeline is live, its crash details, and whether the frame has a pending pipeline.
Page JavaScript cannot supply these values.

Four shared cases and eight multiprocess cases now assert the contained behavior:

- `cross-site-subframe-parser-blocked-failure`, `cross-site-subframe-process-exit`, and `cross-site-subframe-process-kill`.
- `sandboxed-srcdoc-parser-blocked-failure`, `sandboxed-srcdoc-process-exit`, and `sandboxed-srcdoc-process-kill`.
- `cross-site-frame-pending-navigation-failure`, `cross-site-frame-pending-process-exit`, and `cross-site-frame-pending-process-kill`.
- `cross-site-lone-frame-pending-navigation-failure`, `cross-site-lone-frame-pending-process-exit`, and `cross-site-lone-frame-pending-process-kill`.

Each case holds the healthy parent's script thread at a barrier and queues eight callbacks there.
Inside the barrier, the host waits until the probe shows that the failed pipeline is gone.
For a pending navigation, the old child pipeline is still current and live, and the frame has no pending pipeline.
The host also waits for revocation of each other view in the failed event loop, and then for 200 milliseconds.
The parent has no terminal callback, no revocation, and no crash notification at that point.
It has exactly one frame failure notice.
After release, all eight callbacks succeed in the same document. Each scoped result carries the parent's identity.
The parent's retained reply passes the host allocation check, and its pending table is empty.
The probe then shows a new, live frame pipeline whose crash details begin with the exact reason.
The frame has no pending pipeline.

The parent keeps its identity. For a parsing child, its load event waited for the child.
That event now fires after the error page loads, and the host receives `LoadComplete` for the parent.
A parent with a pending frame navigation had already completed its load. Its readiness events do not change.
The parent still receives `SecurityError` when it reads the frame, because the error page has an opaque origin.
In the pending cases, this shows that the error page replaced the old same-origin child.
In multiprocess mode, the error page's process differs from the host's, the parent's, and the failed process.

The child-site popup's own top-level document ran in the failed event loop.
It still receives revocation before its eight terminal callbacks, the exact reason, and a crash page.
The parent-loop popup keeps its scope and its retained reply. An independent view keeps its scope.

The host then releases the withheld scripts, the held response tail, or the withheld navigation response.
After 300 milliseconds, the parent keeps its identity and readiness, and the frame keeps the same error page pipeline.
No crash notification reaches the parent, no other failure occurs, and the engine reports no error.
A late navigation response does not reach the parent's message listener.
Reload replaces the parent with a fresh identity and rejects the old scope. The frame then loads its original child.
A new URL recovers the parent, and the child-site popup recovers with a new URL.

The same-site pending cases keep their earlier assertions.
The pending pipeline shares the parent's event loop, so the parent's own top-level document fails.
The parent and the parent-loop popup receive revocation, crash notifications, and crash documents.
The cross-site navigation parse cases also keep their assertions, because the parsing document is the view's own top-level document.

Three negative controls each use a separate preparation. Each keeps the new probe and the converted fixture.
The first control restores the previous panic handler and the previous synchronous loads.
It passes thirty-nine cases in each mode and fails at `cross-site-subframe-parser-blocked-failure`.
Inside the barrier, the parent had eight terminal callbacks and the crash notification `S1 probe actual script-thread panic`.
The second control counts only top-level pending pipelines as live in the lifeline handler.
It passes all fifty-six shared cases, because single-process mode has no lifeline.
In multiprocess mode, it passes seventy cases and fails at `cross-site-lone-frame-pending-process-exit`.
The wait inside the barrier reached its four-second bound. The probe still reported the frame's pending pipeline.
The lone pending frame cases therefore depend on the lifeline's pending-change clause.
The third control keeps containment and restores the synchronous loads for frame crash loads.
It passes forty cases in each mode and fails at `sandboxed-srcdoc-parser-blocked-failure`.
The frame's crash load parsed the `srcdoc` markup again and requested the withheld script a second time. The parent's load never completed.
The first and third controls failed in all three runs of each mode. The second failed in all three multiprocess runs.
Each control stops at its first failure.
Both existing Servo controls pass with each control.

## Frame path audit

This audit checks three paths of the [frame failure containment](#frame-failure-containment) change.
Each case holds the healthy parent's script thread at a barrier and queues eight callbacks there, as the containment cases do.
One popup stays at `about:blank` in the parent's event loop.
A second intercepted site, `https://frame-site.invalid`, serves cross-site child documents. Each child posts its path to its parent.
The parent also counts its frame's load events in a page variable. These page values only order the fixture.

Three shared cases unwind a script thread:

- `frame-initial-navigation-failure`: The parent's markup contains a frame whose first navigation goes to a site that no other document uses.
  The host withholds that response, so the frame's first pipeline stays pending alone in a new event loop.
- `frame-healthy-pending-navigation-failure`: A loaded cross-site child runs in an event loop of its own.
  A host evaluation then navigates the frame to a third site, and the host withholds that response. The child's event loop fails.
- `frame-history-failure`: The frame loads a cross-site child and then a second child from the same site.
  Servo places both children in one event loop, because they share a site. That event loop fails while the second child is current.

Six multiprocess cases end the same process at the barrier instead:
`frame-initial-navigation-process-exit`, `frame-initial-navigation-process-kill`, `frame-healthy-pending-process-exit`,
`frame-healthy-pending-process-kill`, `frame-history-process-exit`, and `frame-history-process-kill`.
The failed pipeline's process differs from the host's, the parent's, and the independent view's.
The parent-loop popup shares the parent's process. In the healthy pending cases, the pending pipeline runs in a third process.

In every case, the parent has no terminal callback, no revocation, and no crash notification inside the barrier.
It has exactly one frame failure notice there.
After release, all eight callbacks succeed in the same document, and the parent's retained reply passes the host allocation check.
The frame then shows Servo's error page in a new pipeline, whose crash details begin with the exact reason.
In multiprocess mode, the error page's process differs from the host's, the parent's, and the failed process.
The parent-loop popup keeps its scope and its retained reply. An independent view keeps its scope.
No path needs an engine change. These cases characterize the existing containment patch.

### First navigation

Servo creates a frame's browsing context with an initial `about:blank` document in the parent's event loop.
The frame's first navigation is then an ordinary pending change of that browsing context.
`contained_failed_frames` skips a pending pipeline that creates a new browsing context.
That skip applies only to the initial `about:blank` pipeline, and that pipeline always runs in the parent's event loop.
Before the failure, the probe reports the `about:blank` pipeline as current and a pending pipeline for the frame.
The parent reads `about:blank` from its frame. The frame element has fired no load event.
The parent reports DOM readiness, but its load event waits for the frame.
The failure closes the pending pipeline. Inside the barrier, the `about:blank` document is still current and live.
The error page then replaces the `about:blank` document. The parent reads `SecurityError` from its frame.
The frame element fires one load event, and the host receives `LoadComplete` for the parent.
The source shows that the element fires its load event before it releases the parent's load blocker.
The joint session history keeps one entry. A late navigation response reaches no live document.
Reload replaces the parent, and its frame's first navigation waits again. A new URL recovers.
In multiprocess mode, no active document shares the failed process.
The source shows that only the lifeline's pending-change clause makes it live, as in the lone pending frame cases.

### Healthy pending navigation

Before the failure, the probe reports the child's pipeline as current and a pending pipeline for the frame.
The failure closes the child's pipeline. The patch also closes the healthy pending pipeline in the third event loop.
Inside the barrier, the probe reports no live current pipeline and no pending pipeline.
The error page replaces the child. The frame element fires its second load event, and the joint session history keeps one entry.
The host then answers the withheld navigation. That document reaches no live document, and the parent receives no message from it.
The frame keeps the same error page pipeline. No crash notification follows, and the engine reports no error.
Reload replaces the parent, and its frame loads the original child. A new URL recovers.

### Frame history

Before the failure, the parent's joint session history has two entries, and the frame element has fired two load events.
The first child is an inactive history entry in the failed event loop.
The patch marks that entry for reload before its pipeline closes, as the top-level recovery does.
The error page replaces the second child's entry. The history keeps two entries, and the frame element fires a third load event.
Back rebuilds the first child in a new pipeline. That pipeline differs from the error page and from the first child's original pipeline, which closed with the failed event loop.
In multiprocess mode, the rebuilt child runs outside the failed process.
The rebuilt child posts its path again, but the frame element fires no load event for it.
The source shows that the element accepts a load only from a navigation that it started, and a traversal rebuild is not such a navigation.
Forward restores the same error page pipeline with its crash details.
The parent keeps its identity, its activation generation, and its scope through both traversals.
A top-level navigation and back then restore the parent with a fresh identity, and the engine rejects the old one.
The restored parent's frame shows the same error page pipeline, and its history has three entries.

### Isolation controls

Two controls each use a separate preparation and the final fixture. Each removes one existing part of the containment patch.
The first control keeps a failed frame's healthy pending pipeline open.
The second control does not mark a contained frame's inactive history entries for reload.
Each control failed in all three runs of each mode.
The first control fails at `frame-healthy-pending-navigation-failure` in each mode, and at `frame-healthy-pending-process-exit` in multiprocess mode.
After the four-second barrier wait, the probe still reported a pending pipeline for the frame.
The second control fails at `frame-history-failure` in each mode, and at `frame-history-process-exit` in multiprocess mode, at the fixture deadline.
A diagnostic rerun with constellation logs showed no reload of the first child's entry after the error page activated.
Every earlier case passed with each control, including all fifty-one documents cases of that checkpoint.
The frame path cases therefore depend on both existing parts of the containment patch.
The [frame path evidence](../experiments/phase3/document-frame-paths-evidence-2026-11-27.json) records these controls.

## Frame failure notice

The owner decided that a contained frame failure must send an engine notification for the view that keeps its document, per the [recorded decisions](phase-3-plan.md#recorded-decisions-2026-11-27).
The view keeps its document and its scope. The notification does not revoke the view and does not replace it.

The experimental patch adds `WebViewDelegate::notify_frame_failure` and a `FrameFailure` record.
The record holds the view's active document identity, each outermost failed frame, the failure reason, and the backtrace.
Each failed frame is a `FailedFrame` with the frame's browsing context and the failed pipeline.
The failed pipeline is the frame's current pipeline, or its pending pipeline for a failed navigation.
The patch exports `BrowsingContextId` and `PipelineId` from the `servo` crate for this record.

The panic handler sends one notice for each surviving view and failure. The notice lists every failed frame of that view.
It sends the notices after the failed pipelines close and before it sends any crash navigation.
A view whose own document failed still receives revocation and a crash notification, and no frame notice.
The handler sends those notifications before every frame notice of the same failure.
A view with only inactive history entries in the failed event loop receives no notice. Those entries reload on traversal.
The embedder drops a notice after shutdown starts, because shutdown has revoked every view.

The notice has this contract:

- The named document is still active. Its identity, activation generation, and retained replies stay valid.
- The notice revokes no callback. Pending evaluations in that document continue.
- Each named frame then navigates to Servo's error page. The error page has an opaque origin and runs no scripts.
- The notice is distinct from `notify_crashed`, which reports a failure of the view's own document.
- The notice is not a lifecycle event. Later `DocumentLifecycle` events still govern scope.
- The notice grants no authority. A frame's browsing context or pipeline is not a host scope.

A new probe reports the browsing context of the active top-level document's newest child frame and that frame's newest pending pipeline.
Page JavaScript cannot supply these values.

Every contained case now requires the notice: seven frames cases, the frame send case, and fourteen processes cases.
Inside the barrier, the parent's script thread still waits, so its frame cannot navigate yet. The notice has already arrived.
It names the parent's identity, the failed frame's browsing context and pipeline, and the exact reason.
After the error page loads, the late data, and 300 milliseconds, the parent still has exactly one notice.
The parent-loop popup, the child-site popup, and the independent view receive no notice.
In the same-site pending cases, the parent's own document fails. Neither the parent nor the parent-loop popup receives a notice.
The [frame group failure](#frame-group-failure) section covers a view with several failed frames in one failure, and a failed frame inside a failed frame.
The [local evidence](#local-evidence) section describes the negative control.

## Frame send failure

The owner decided that a failed send to a frame's pipeline must also stay inside that frame.
Servo's `handle_send_error` treated every failed send to a known pipeline as a panic of that pipeline's whole view.
It called `handle_panic_in_webview` for the view, so the healthy parent lost its document and its scope.
`send_message_to_pipeline` calls it, and so do direct sends for focus, frame load events, frame navigation, and pipeline creation.

The experimental patch keeps that path when the view's active or pending top-level pipeline uses the failed event loop.
Otherwise the failed send follows the panic handler's rules for that view only.
The patch moves the body of the panic handler into `handle_event_loop_failure`, which can select one view.
That view's pipelines in the event loop close. Each outermost failed frame gets Servo's error page.
The view keeps its document and scope, and it receives one frame failure notice.
The reason is still `Send failed (...)` with the channel's error.
As before, the send path does not enter hard-fail mode and does not change other views in that event loop.

A new probe closes the constellation's channel to the event loop of the newest child frame.
Every later send to that event loop goes to a real channel whose receiver is already dropped. That channel returns its own error.
The probe then sends one message to the frame's pipeline through `send_message_to_pipeline`.
No script thread unwinds and no content process exits, so neither the panic handler nor the lifeline reports this failure.
Only `handle_send_error` builds the reason `Send failed (`. The frame's crash details and the notice begin with it.
In single-process mode, the error is `Disconnected`.
In multiprocess mode, it is `SerializationError("Error in IO: Bogus destination port..")` from the IPC channel.
The probe does not stop the frame's script thread. `Drop` still sends the exit message through the original channel.
That thread exits when its event loop drops. In multiprocess mode, its process then exits, and the lifeline finds no live pipeline.

The `frame-send-failure` case runs in the frames part in both modes.
The parent loads a cross-site child alone in an event loop of its own. The child has no pending navigation.
The parent's script thread waits at a barrier with eight callbacks, as in the frame path audit.
The probe reports that the frame runs in an event loop other than the parent's.
The checks inside the barrier, after release, and for the popups match the other frame path cases.
The error page replaces the child, and the frame element fires its second load event.
Reload loads the child again, and a new URL recovers.
The second negative control in the [local evidence](#local-evidence) restores the previous send-error path. The case then fails.

## Frame group failure

The [frame failure notice](#frame-failure-notice) review asked what one failure sends for a view with several failed frames, and for a failed frame inside another failed frame.
Both layouts keep the parent's own document in an event loop that the failure does not touch.
The current patch already meets the contract for both. This checkpoint adds fixtures and one read-only probe, and it changes no notice, containment, or recovery behavior.

A new probe lists every child frame of a view.
Each entry holds the frame's browsing context, its parent's browsing context, its current pipeline, whether that pipeline is live, its crash details, whether the frame has a pending pipeline, and the Debug text of its pipeline's event loop identifier.
Page JavaScript cannot supply these values. The list has no defined order, so the fixture finds each frame by its browsing context.

Two layouts each run for a script-thread unwind in both modes, and for content-process exit and kill in multiprocess mode:

- `frame-siblings-failure`, `frame-siblings-process-exit`, and `frame-siblings-process-kill`: the parent's markup has a healthy same-origin frame, then two frames of one cross-site origin.
  Frames of one site share an event loop, so one failure ends both siblings. The healthy frame runs in the parent's own event loop.
- `frame-nested-failure`, `frame-nested-process-exit`, and `frame-nested-process-kill`: the parent has one cross-site frame, and that frame contains a frame of its own origin.
  The outer and inner frames share one event loop, so the inner frame's parent fails in the same failure.

Before the failure, the probe must show every frame live, with no crash details and no pending pipeline.
The failed frames must share the newest frame's event loop identifier, and the siblings' healthy frame must have another.
A popup in the parent's event loop supplies a second document that must keep its scope.
Each case holds the parent's script thread at a barrier and queues eight callbacks there, as the other frame cases do.

Inside the barrier, the parent has no terminal callback, no revocation, and no crash notification.
It has exactly one frame failure notice.
The notice names the parent's identity and the exact reason. It names exactly the outermost failed frames, each with its failed pipeline, in any order.
The siblings notice therefore names both siblings, and the nested notice names only the outer frame.
The failed pipelines are closed, and the healthy frame's record is unchanged.
After release, all eight callbacks succeed in the same document, and the parent-loop popup keeps its retained reply.

Each outermost failed frame then has a new live pipeline with crash details that begin with the reason.
These pipelines run in an event loop other than the failed one. In multiprocess mode, the newest frame's error page runs in a new content process.
The siblings' two error pages are separate pipelines.
The nested inner frame's browsing context is gone, because it closed with its parent. No error page replaces it.
The parent reads `SecurityError` from each error page. For the nested frame, the cross-origin frame count that the parent reads changes from one to zero.
The parent's load event does not repeat. After 300 milliseconds, its frames, readiness, and notices are unchanged, and the engine reports no error.
Reload replaces the parent with a fresh identity, rejects the old scope, and loads every frame without crash details. A new URL recovers.

The patch enforces the nested rule twice.
`contained_failed_frames` skips a frame whose parent pipeline ran in the failed event loop, because the frame closes with that parent.
After the failed pipelines close, the notice loop also drops any frame whose browsing context or parent pipeline no longer exists.
Removing only the first guard passes every case, so either guard alone meets the contract.
The [local evidence](#local-evidence) section records the controls that remove each part of this behavior.

A frame inside a healthy frame, and a nested failure whose inner frame runs in an event loop other than its failed parent's, still have no fixture.
A group has no failed-send fixture. The content-process id probe (`frame_content_process_id_for_document_scope_probe`) reports only the newest child frame's process, so the nested inner frame's process is never read. Its membership in the failed event loop rests on the event loop identifier that the child frames probe reports before the failure, which equals the outer frame's.

## JavaScript URL crash page

The owner decided that crash recovery must not render a `javascript:` URL document's result again, per the [recorded decisions](phase-3-plan.md#recorded-decisions-2026-11-26).
The crash document must be an error page, and reload must not render the result again.

Servo evaluates a `javascript:` URL in the source document's script thread.
A string result becomes the body of a new load in a new pipeline. That load data keeps the result in `js_eval_result`.
The evaluation also sets the load's URL to the source document's URL, and it adds a `Content-Type` header for the result.
The script thread loads any load data with a result synchronously, and that path ignores the crash details.
Crash recovery copied the failed pipeline's load data, so the crash load rendered the result again.
The crash page's load data is also the reload record. Reload removed the crash details, so it rendered the result once more.

The experimental patch now drops the result from every crash load, at the top level and in a contained frame.
It also drops the `Content-Type` header that the evaluation added, because the result's response, not a request, set it.
A top-level crash load then fetches Servo's error page with the failure reason, as for other HTTP documents.
The error page has every sandboxing flag, so it has an opaque origin and runs no scripts.
Its URL is the document's URL, which is the source document's URL.

Reload from that error page now fetches the document's URL with the original sandboxing flags.
This is the source page, not the result. Servo's ordinary reload of a live `javascript:` URL document does the same.
`Location::reload_without_origin_check` navigates to the window's URL with new load data and no result.
A reload therefore never renders a result, and it sends no `Content-Type` request header.
The reload keeps the rest of the load data, including the source document's policy container as the request client's container.

A frame's crash load already used the fetch path, so it never rendered the result.
Its load data still kept the result, but no path removed the crash details from a frame's load.
The constellation's crash reload replacement reads only the top-level pipeline, and history keeps the crash details when it rebuilds a page.
The patch now drops the result from a frame's crash load too. No fixture covers a `javascript:` URL document in a frame.
A `javascript:` URL document whose source was at `about:blank` has the URL `about:blank`.
Its crash load now takes the synchronous blank path, and the owner's decision for failed views at `about:blank` applies. No fixture covers it.
At this checkpoint, recovery marked inactive history entries for reload with their own load data. An inactive `javascript:` URL entry kept its result.
The [JavaScript URL history entry](#javascript-url-history-entry) section supersedes this behavior.

The `javascript-url-crash-recovery` case now asserts this behavior in both modes.
Revocation before eight terminal callbacks, released captures, and rejection of the retained reply pass as before.
The crash document contains the failure reason. Its URL is the source page's URL, and its origin is `null`.
It has no fixture marker and no result text, and an inserted script does not run.
The crash fetch skips interception, so the host saw only the initial request for the source page.
Reload gives a new identity and rejects the crash document's scope.
The reloaded document is the source page, with the real origin, its own marker, and a running inserted script. It contains no result text and no failure reason.
The host saw a second request for the source page. Neither request carried a `Content-Type` header.
A new URL recovers, and an independent view stays usable.

Three negative controls each use a separate preparation and the converted fixture.
The first control is the previous patch. It passes thirty-seven cases in each mode and fails at `javascript-url-crash-recovery`.
The crash document did not contain the failure reason, so the check returned its URL.
The second control keeps the result in the crash load data, but the script thread ignores the result for a crash load.
This control isolates the reload record. The crash document is the error page, and the case passes those checks.
Reload then renders the result again, with the real origin, the result's marker, and a running inserted script.
The third control drops the result but keeps the `Content-Type` header. Reload then sends that header with its request.
Each control failed in all three runs of each mode, and each stops at its first failure.
Both backend loss cases and both existing Servo controls pass with each control.

## JavaScript URL history entry

The owner decided that crash recovery must drop an inactive `javascript:` URL history entry, per the [recorded decisions](phase-3-plan.md#recorded-decisions-2026-11-27).
Back and forward must not reach that entry after recovery, so its result cannot render again.

A `javascript:` URL navigation always replaces the source page's history entry.
A later navigation makes the result's entry inactive. Its pipeline keeps the result in its load data.
Crash recovery marked that entry for reload with this load data.
Back then rebuilt the document from the result, as the [JavaScript URL crash page](#javascript-url-crash-page) section describes.

The experimental patch adds `JointSessionHistory::remove_document_entries`.
It removes the entries in which one browsing context shows one document, unless the current entry shows it.
The diff that leaves the document then starts from the entry before the document.
The history length, the current index, and the embedder's back-forward list follow from the remaining diffs.
After a view fails, recovery removes every inactive top-level entry whose reload data keeps a result.
At the top level, every diff between those entries belongs to the document, such as its fragments and its frames.
In a surviving view, recovery also removes such an entry of a frame, if its document ran in the failed event loop.
There only the document's own diffs and those of frames inside it go. Sibling frames keep their diffs.
If no diff enters the document, as for a frame's first document, its reload data drops the result instead.
The active document's crash load already drops its result.

The `javascript-url-history-crash-recovery` case runs in the documents part in both modes.
The view loads a start page and a source page. A `javascript:` URL result then replaces the source page's entry.
A next page makes three entries. The next page's script thread then unwinds.
The crash page shows the failure reason, and its history has two entries.
Back reaches the start page with a fresh identity, the real origin, and no result text.
Forward restores the crash page with the failure reason, and back reaches the start page again.
Every earlier identity is stale. A new URL recovers, and an independent view stays usable.
No fixture covers a `javascript:` URL entry in a frame's history, an entry that no diff enters, or an entry that history eviction discarded earlier.
The third negative control in the [local evidence](#local-evidence) removes the top-level drop. Back then renders the result again.

## Pending process and recovery audit

The owner requires recovery from an error page through back, reload, or a new URL.
This audit checks those paths and process failure before document construction.

The existing exit and kill barrier now also accepts the child of a view's newest pending top-level pipeline.
The host withholds that pipeline's navigation response, so the child has no document.
Four multiprocess cases cover exit and kill for a new view and for a live view with a pending cross-site navigation.

The lifeline handler counted a pipeline as live only if its browsing context held it.
A pending pipeline joins its browsing context only at activation, so the handler treated its loss as ordinary teardown.
A new view then received the pipeline-exit `Active(None)` but no crash notification. The product could not show its error state.
A live view received no notification and kept a stale pending change. Servo refuses top-level navigation while that change exists.
A load of a new URL did not complete, so the view could not recover.
The experimental patch now also treats a pipeline as live while its view holds it as a pending change.
Lifeline loss then enters the existing panic handler, with the same revocation and recovery as the script-thread cases.
Both views report `Content process disconnected`. The live view releases eight callbacks held at a barrier and navigates again.
The negative control removes only this clause. The first pending-process case then fails without a crash notification.

The script-thread pending-navigation fixture now loads a first page before the source page.
After recovery, it reloads, goes back, goes forward, and loads a new URL.
Back failed before this audit. The view had no active document afterwards, and every later evaluation and load failed with `InternalError`.
Upstream crash recovery closes every pipeline in the top browsing context, including inactive history entries.
It does not mark those entries for reload, as history eviction and document discard do.
Its crash-document change also replaces `NeedsToReload::Yes(active)`, which never matches the stored `NeedsToReload::No(active)`.
Back and forward therefore activated closed pipelines.
The experimental patch now marks every pipeline of the top browsing context for reload before recovery closes it.
Reload, back, forward, and a new URL then recover in both modes, and each restored document receives a fresh identity.
The negative control removes only this marking. The history steps then fail with `InternalError` in both modes.
That control stops at the first failure, so it does not show which later cases also need the fix.
The fix applies to every path that reaches the panic handler. The [frame path audit](#frame-path-audit) checks frame history after a contained failure.
Frame history after whole-view recovery remains unchecked.

Servo also sent the crash load through embedder request interception after it set the crash network error.
An intercepted response replaced that error. The crash error page audit below fixes this and checks reload from the real error page.

## Crash error page audit

The owner approved a `servo-net` change, so the experimental patch now covers seven published packages.
`main_fetch` now skips embedder interception for a request that carries crash details.
Servo's `crash.html` embeds the failure reason. Each fetched crash document in the fixtures must now contain that reason.
That text check orders the fixture only. It grants no host authority.

The real error page exposed a reload defect.
The crash page carries every creation sandboxing flag, so it has an opaque origin and runs no scripts.
An embedder reload asked the crash page's script thread to navigate, and that navigation inherited the crash page's flags.
The reloaded page showed the previous URL's markup with an opaque origin and without running its scripts.
The crash page's load data now also carries the creation sandboxing flags of the load that it replaced.
Reload of a page whose load data has crash details replaces it through the constellation with that load.
The replacement removes the crash details, restores the original flags, and sets the reload-navigation flag.
The reloaded page now runs its scripts and has its real origin. Back, forward, and a new URL still recover.
For a failed pending navigation, reload restores the replaced source page. A new URL retries the failed target.

A crash document for a view at `about:blank` is blank. Servo loads `about:blank` synchronously, so the crash fetch never runs.
The [synchronous construction audit](#synchronous-construction-audit) found that this document has the origin `null` and runs inserted scripts.
The shared-process fixture asserts this for its popup. The product's native error state must cover such views.

One negative control restores interception for crash requests. The first fetched crash document then lacks the failure reason.
The other control disables only the reload replacement. Reload then produces a page whose script never runs.
Each control stops at its first failure, so it does not show which later cases also need its fix.
The reload replacement uses default target snapshot parameters. No fixture covers a view that started with sandboxing from its opener or a frame.

The first version of this fix stored the original load under the crash pipeline's id and removed it at pipeline exit.
A crash page can stay in history. A later failure in the same view closes every history pipeline and marks each entry for reload.
Back then rebuilt the crash page in a new pipeline without the record, and reload kept the crash page's sandboxing.
The flags now travel in the load data that history keeps.
A new case loads a page after a first failure, fails again, and goes back to the closed crash entry.
It requires a rebuilt error page with the first failure reason. Reload must then restore the real origin and run the page's scripts.
The previous patch is its negative control. It passes twenty-nine cases in each mode, and then reload from the rebuilt page has the origin `null`.
History eviction and document discard keep load data in the same way, but no fixture exercises them.

## Backend loss audit

Servo reports `LostConnectionWithBackend` only after an embedder send to the constellation fails.
The source audit found three defects after constellation loss:

- Pending callbacks and host scope stayed live, because no reply or lifecycle event could arrive.
- A request after a failed send entered the pending table without a terminal path.
- `ServoInner::drop` waited for a shutdown completion message that only the constellation sends.
  The destructor could therefore never return.

The experimental patch now detects closure of the constellation-to-embedder channel.
The constellation owns the only sender, so this channel closes when the constellation thread ends.
The embedder first handles every message that the constellation sent before the loss.
A failed send to the constellation is a second signal.
Either signal starts the existing shutdown revocation without a constellation reply.
Every retained view receives `Active(None)` before any terminal callback, and admission closes.
The embedder then finishes shutdown locally and reports `LostConnectionWithBackend` once.
If a delegate or callback panics, local shutdown completes before the panic propagates. No fixture checks this panic path yet.
A request between the loss and its detection stays pending until the next event-loop spin, which revokes it.

An experimental probe message unwinds the actual constellation thread.
Backend loss ends the engine, so these cases run in their own process after each scope run.
The fixture opens two views. A script-thread barrier holds eight scoped and legacy requests for the first view.
The constellation forwards those requests before it reads the crash message.
The host sends no further work until cleanup.

The first case requires cleanup within five seconds and revocation of both views before any callback.
It requires release of all callback captures and an empty pending table.
It rejects a retained reply before host allocation. New scoped and legacy requests fail synchronously.
The second case requires local shutdown completion, exactly one loss report, and destructor return.
A watchdog thread ends the process if the destructor does not return within five seconds.

In these debug builds, cleanup completed 1,543 to 1,627 milliseconds after the crash request.
The panic hook logs immediately, so the delay occurs during the constellation unwind.
The debug linker reports that the unwind tables exceed its compact-unwind limit. The cause of the delay is not established.
The destructor then returned in 13 to 18 milliseconds.

Two negative controls each override one line of the embedder handler in a separate preparation.
The first control keeps detection and the loss report but omits revocation and local completion.
It fails at the five-second cleanup deadline with eight pending callbacks and the document still active.
The loss report then repeats on every event-loop spin, more than 1,100 times in each mode.
The second control revokes views and callbacks but omits local completion.
The first case passes, and then the watchdog reports that the destructor waited for the lost constellation.
The ordinary scope cases and both existing Servo controls pass with each override in both modes.

This audit covers an unwind of the constellation thread only.
A hung constellation, a process abort, and loss of other engine threads remain untested.
The fixture does not check content-process exit after backend loss. The runner stops any remaining process.
The source review also found that the constellation ignores panic reports without an event-loop identity.
Such a report causes no revocation. Failures of threads without that identity remain untested.

## Regression scope

Ordinary fixture resources use interception.
The redirect fixture uses a temporary loopback HTTP listener for exactly two requests.
The partial response and cross-site parsing fixtures use loopback listeners that hold one response open and require their exact request paths.
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
| Unconstructed view failure | Requests without a top browsing context fail with `InternalError` and retain nothing. A pending pipeline unwinds with revocation and no crash document. The view can only close. |
| Pending navigation failure | A cross-site pending pipeline unwinds before construction. Its healthy active document is revoked before eight callbacks and replaced by a crash document. The view recovers. |
| Crash recovery history | After a pending navigation failure, reload, back to a closed history entry, forward, and a new URL all recover with fresh identities. The reloaded page keeps its origin and runs scripts. |
| Crash error page | Fetched crash documents show Servo's error page with the failure reason. A crash document at `about:blank` is blank. |
| Crash history reload | After a second failure closes a crash page in history, back rebuilds that error page. Reload from it restores the failed load's origin and scripts. |
| Headers before parse failure | An active document without a DOM unwinds with its response open. Late data is ignored, eight callbacks are released, and reload and a new URL recover. |
| Parser-blocked failure | An active document unwinds while its parser waits for a withheld script. The same release, late-data, and recovery checks pass. |
| Initial `about:blank` failure | A complete initial `about:blank` document unwinds. Eight callbacks are released. The blank crash document has the origin `null` and runs inserted scripts. Reload and a new URL recover. |
| `about:blank` write failure | `document.write` resets `about:blank`, and its parser waits for a withheld script. The same release and late-data checks pass. Reload restores a blank document. |
| `srcdoc` parser-blocked failure | A same-thread `srcdoc` child waits for a withheld script. The parent unwinds before its load event. The error page, release, late-data, and recovery checks pass. |
| `document.write` parser-blocked failure | The initial parser waits for a written external script. The same release, error page, late-data, and recovery checks pass. |
| `document.open()` write failure | A reset document's script-created parser waits for a written script. Reload restores the original URL without the written content. |
| `javascript:` URL crash recovery | A `javascript:` URL document unwinds. Eight callbacks are released. The crash document is the error page with the failure reason, the origin `null`, no running scripts, and no JavaScript result. Reload fetches the source page with its origin and scripts, without the result's `Content-Type` header. A new URL recovers. |
| `javascript:` URL history entry | A later page fails while a `javascript:` URL result is in history. Recovery removes the result's entry, so history has two entries. Back reaches the start page, forward restores the crash page, and the result never renders again. A new URL recovers. |
| Cross-site navigation parse failure | A cross-site document parses in its own event loop with no pending top-level pipeline, and unwinds. Eight callbacks are released. A popup in the source event loop keeps scope. Reload, back, and a new URL recover. |
| Cross-site subframe parse failure | A cross-site child unwinds while it parses. The healthy parent keeps its document and scope, eight held callbacks succeed, and its retained reply passes. Only the frame gets an error page, and then the parent's load completes. A popup in the child's event loop releases eight callbacks and gets a crash document. A popup in the parent's event loop keeps scope. Reload and a new URL recover. |
| Sandboxed `srcdoc` parse failure | A sandboxed `srcdoc` child in a new event loop unwinds while it parses. The healthy parent keeps its document and scope, and eight held callbacks succeed. The frame's crash load fetches the error page instead of parsing the `srcdoc` markup again. A popup in the parent's event loop keeps scope. |
| Cross-site parse process exit and kill (multiprocess only) | The process of a parsing cross-site document exits or is killed. The lifeline releases eight callbacks. The source-loop popup keeps scope, and reload, back, and a new URL recover. |
| Cross-site subframe process exit and kill (multiprocess only) | The process of a parsing cross-site child exits or is killed. The parent keeps its document and scope, and only the frame gets an error page in a new process. The child-site popup releases eight callbacks. The parent-loop popup keeps scope. |
| Sandboxed `srcdoc` process exit and kill (multiprocess only) | The process of a parsing sandboxed `srcdoc` child exits or is killed. The healthy parent keeps its document and scope, and only the frame gets an error page. The parent-loop popup keeps scope. |
| Same-site pending frame failure | A frame's same-site navigation waits for its response headers, and the parent's event loop unwinds. The parent's own document fails. The parent and a popup in that loop each release eight callbacks and get crash documents. Late data is ignored. Reload and a new URL recover. |
| Cross-site pending frame failure | A frame's pending pipeline joins a child-site popup's event loop, which unwinds. The healthy parent keeps its document and scope, and eight held callbacks succeed. An error page replaces the old child in the frame. The child-site popup releases eight callbacks and gets a crash page. A popup in the parent's event loop keeps scope. |
| Lone cross-site pending frame failure | A frame's pending pipeline is alone in a new event loop, which unwinds. The healthy parent keeps its document and scope, and eight held callbacks succeed. An error page replaces the old child in the frame. A popup in the parent's event loop keeps scope. |
| Pending frame process exit and kill (multiprocess only) | The process of a frame's pending pipeline exits or is killed in each of the three layouts. The same scope results, callbacks, error pages, late-data, and recovery checks pass. The lone layout depends on the lifeline's pending-change clause for frames. |
| Frame first navigation failure | A frame's first navigation waits alone in a new event loop, which unwinds. The healthy parent keeps its document and scope, and eight held callbacks succeed. An error page replaces the initial `about:blank` document. The frame element fires one load event, and then the parent's load completes. Late data is ignored. Reload and a new URL recover. |
| Frame healthy pending navigation failure | A loaded cross-site child unwinds while the frame's next navigation waits in a third event loop. The parent keeps its document and scope. The healthy pending pipeline closes, and an error page replaces the child. The late navigation response reaches no document. Reload and a new URL recover. |
| Frame history after failure | A frame's second child unwinds with the first child in history. The error page replaces the second child's entry. Back rebuilds the first child in a new pipeline, forward restores the same error page, and the parent keeps its scope. Top-level back restores the parent with a fresh identity and the same error page. |
| Frame path process exit and kill (multiprocess only) | The failed pipeline's process exits or is killed in each of the three frame paths. The same scope results, callbacks, error pages, late-data, history, and recovery checks pass. |
| Frame failure notice | Every contained case delivers exactly one notice to the parent before the frame's crash navigation. It names the parent's identity, the failed frame's browsing context and pipeline, and the exact reason. Popups, the independent view, and a parent whose own document fails receive none. |
| Frame group failure | One failure ends two sibling frames of a view, or a frame and the frame inside it, while the parent runs in another event loop. The parent keeps its document and scope, and eight held callbacks succeed. It receives one notice that names every outermost failed frame, and only those frames get error pages. A healthy sibling keeps its pipeline. |
| Frame group process exit and kill (multiprocess only) | The failed frames' shared process exits or is killed in both layouts. The same notice, scope, callback, error page, and recovery checks pass. |
| Frame send failure | A send to a loaded child's event loop fails in the constellation's send helper. The parent keeps its document and scope, eight held callbacks succeed, and one notice carries `Send failed (`. Only the frame gets an error page. Reload and a new URL recover. |
| Unconstructed process exit and kill (multiprocess only) | The child of a new view's pending pipeline exits or is killed. The host receives revocation and a crash notification, and the view closes. |
| Pending navigation process exit and kill (multiprocess only) | The child of a live view's pending navigation exits or is killed. Eight callbacks are released, and the view navigates again. |
| Content-process failure (multiprocess only) | A child exits without an unwind. Its lifeline revokes scope and releases eight callbacks without a later send. Recovery and an independent view accept evaluation. |
| Shared-process close (multiprocess only) | Normal close of one of three views in one child revokes only that view and releases eight callbacks. The other two views stay usable. |
| Shared-process exit and kill (multiprocess only) | The lifeline revokes two views in one child and releases sixteen callbacks. Both views recover, and an independent view stays usable. |
| Shared close-kill race (multiprocess only) | A view closes while its shared child is blocked, and then the child is killed. The closed view stays revoked without crash recovery. |
| Content-process kill (multiprocess only) | The host sends SIGKILL to a blocked child. The same lifeline revocation, callback release, recovery, and survivor assertions apply. |
| Backend disconnection (separate engine) | An actual constellation unwind revokes two views before eight callbacks. The host rejects the retained reply, and admission closes. |
| Drop after backend loss (separate engine) | Shutdown completes locally with one loss report. The engine destructor returns. |
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

The scope cases run in three parts. Each part runs in its own engine process with its own deadline.
The documents part has fifty-two cases, and the frames part has eleven cases. Both parts run in each mode.
The processes part has thirty-two cases. It runs only in multiprocess mode.
The frames and processes parts each start a fresh view at the fixture origin. Their cases check that view as the independent view.
Each part must print exactly its own markers. Its summary marker must report its own case count.
Each repeat must cover sixty-three cases in single-process mode or ninety-five cases in multiprocess mode, each case once.
Each repeat also runs the two backend loss cases in a separate engine process.
Both modes retain the existing Servo contract and E4 baseline results.
The host-ticket fixture suppresses allocation after delivery. Engine cancellation separately rejects closed permits before and during execution.
The injected crash calls the existing constellation panic handler through an experimental probe method.
The separate script-thread fixture terminates the receiving thread through a Rust unwind. Neither fixture proves process-failure detection.

## Local evidence

The [frame group evidence](../experiments/phase3/document-frame-groups-evidence-2026-11-27.json) records the current runs, source hashes, binary hashes, negative controls, and local checks.
Its baseline is `2a0a682`; the runs used the working tree that followed it. The only patch change is the new child frame probe.
The [owner decisions evidence](../experiments/phase3/document-owner-decisions-evidence-2026-11-27.json) preserves the previous checkpoint and its three negative controls.
The [frame path evidence](../experiments/phase3/document-frame-paths-evidence-2026-11-27.json) preserves the frame path checkpoint and its two isolation controls.
The [JavaScript URL crash evidence](../experiments/phase3/document-javascript-crash-evidence-2026-11-26.json) preserves the JavaScript URL crash checkpoint and its three negative controls.
The [frame containment evidence](../experiments/phase3/document-frame-containment-evidence-2026-11-26.json) preserves the frame containment checkpoint and its three negative controls.
The [pending frame evidence](../experiments/phase3/document-pending-frame-evidence-2026-11-26.json) preserves the first seventy-eight-case checkpoint, before frame containment.
The [cross-site parsing evidence](../experiments/phase3/document-cross-site-parse-evidence-2026-11-26.json) preserves the fifty-three-case checkpoint.
The [synchronous construction evidence](../experiments/phase3/document-synchronous-evidence-2026-11-26.json) preserves the fifty-case checkpoint and its fixture race.
The [partial response evidence](../experiments/phase3/document-partial-evidence-2026-11-26.json) preserves the forty-four-case checkpoint.
The [crash history evidence](../experiments/phase3/document-crash-history-evidence-2026-11-26.json) preserves the fifty-two-case checkpoint and its negative control.
That checkpoint's summary marker still reported fifty-one cases. The runner now rejects a stale summary count.
The [crash page evidence](../experiments/phase3/document-crash-page-evidence-2026-11-26.json) preserves the fifty-one-case crash page checkpoint and its two negative controls.
The [recovery evidence](../experiments/phase3/document-recovery-evidence-2026-11-24.json) preserves the first fifty-one-case checkpoint and its two negative controls.
The [construction evidence](../experiments/phase3/document-construction-evidence-2026-11-24.json) preserves the forty-six-case multiprocess checkpoint.
The [shared-process evidence](../experiments/phase3/document-shared-evidence-2026-11-24.json) preserves the forty-four-case multiprocess checkpoint and its negative control.
The [backend evidence](../experiments/phase3/document-backend-evidence-2026-11-24.json) preserves the forty-case multiprocess checkpoint and both backend loss controls.
The [kill evidence](../experiments/phase3/document-kill-evidence-2026-11-24.json) preserves the first forty-case multiprocess checkpoint.
The [process evidence](../experiments/phase3/document-process-evidence-2026-11-24.json) preserves the thirty-nine-case multiprocess checkpoint.
The [interruption evidence](../experiments/phase3/document-interruption-evidence-2026-11-24.json) preserves the first thirty-eight-case checkpoint and its earlier cancellation baseline.
The [cancellation evidence](../experiments/phase3/document-cancellation-evidence-2026-11-24.json) preserves the thirty-four-case checkpoint.
The [delegate evidence](../experiments/phase3/document-delegate-evidence-2026-11-24.json) preserves the thirty-one-case checkpoint.
The [callback evidence](../experiments/phase3/document-callback-evidence-2026-11-24.json) preserves the twenty-seven-case checkpoint.
The [cleanup evidence](../experiments/phase3/document-cleanup-evidence-2026-11-23.json) preserves the twenty-two-case checkpoint.
The [audit evidence](../experiments/phase3/document-audit-evidence-2026-11-23.json) preserves the nineteen-case checkpoint.
The [lifecycle evidence](../experiments/phase3/document-lifecycle-evidence-2026-11-23.json) preserves the sixteen-case checkpoint.
The [earlier evidence](../experiments/phase3/document-scope-evidence-2026-11-22.json) preserves the nine-case checkpoint before lifecycle notifications.
That earlier checkpoint retained identities across history activation. The current prototype deliberately changes that policy.

All sixty-three shared cases passed in three single-process runs on macOS arm64 with software rendering.
All ninety-five cases passed in three multiprocess runs.
Each run has three parts in multiprocess mode and two parts in single-process mode. Each part ran in its own engine process.
In multiprocess mode, the documents part took 37.8 to 38.1 seconds, the frames part 28.5 to 28.7 seconds, and the processes part 18.0 to 18.3 seconds.
Their margins to the 80-second fixture deadline are about 42, 51, and 62 seconds.
In single-process mode, the documents part took 6.3 to 9.2 seconds, and the frames part took 8.9 to 9.0 seconds, within the 30-second fixture deadline.
The runner times each part's whole process. The documents part has no new case and took about three seconds longer than in the previous checkpoint, so part times vary between checkpoints.
This checkpoint keeps all fixture and runner deadlines.
One development run of the frames part in each mode, and of the processes part, passed before the final runs. The final binary came from a fresh preparation of the same fixture source. No final run failed.

Two negative controls each used a separate preparation and the final fixture. Each edits only that preparation's copied `constellation.rs` after the preparation script applied the final patch.
The runner accepts them because it checks the patch file's hash, so the repository patch was never modified. Each failed in all three runs of each mode, and its documents part passed.
The first control sends one notice for each failed frame instead of one for each view.
The frames part passes the nine earlier cases and fails at `frame-siblings-failure`. It reports two notices, each naming one sibling.
In multiprocess mode, the processes part passes twenty-five cases and fails at `frame-siblings-process-exit` the same way.
The second control removes both guards that exclude a frame inside a failed frame.
The frames part passes ten cases and fails at `frame-nested-failure`, because the single notice names both the outer and the inner frame.
In multiprocess mode, the processes part passes twenty-six cases and fails at `frame-nested-process-exit`.
One single-process frames run of each control exited with signal 11 during teardown. The other runs exited with status 1.
Removing only the guard in `contained_failed_frames` is not a control. It passed every case in all three runs of each mode, because the notice loop's filter also drops the inner frame.
Both backend loss cases passed in three separate engine runs in each mode. Cleanup completed 1,486 to 1,533 milliseconds after the crash request.
Both existing Servo controls, all 61 portable tests, formatting, and portable/probe Clippy passed locally.
Clippy denied warnings for the probe and portable workspace. The probe command reports one unused-import warning in the copied `servo` package source and exits with status 0.
Python syntax checks and twelve runner tests also passed. The runner test pins each part's documented case count and the totals.
These builds retained the previous line-table debug configuration.

The previous checkpoint's negative controls and notes follow.

Three negative controls each used a separate preparation and the final fixture. Each failed in all three runs of each mode.
The first control is the previous patch with only the new probes and the inert notice API.
That API is the `FrameFailure` types, the delegate method with its empty default, and the identifier exports. The constellation never sends a notice.
The frames part fails at its first case, `cross-site-subframe-parser-blocked-failure`, because the parent has no frame notice.
In multiprocess mode, the processes part passes eleven cases and fails at `cross-site-subframe-process-exit` for the same reason.
The documents part passes thirty-eight cases and fails at `javascript-url-history-crash-recovery`. Back reached the rendered result instead of the start page.
One single-process frames run printed its error and then exited with signal 11 during process teardown. The other runs exited with status 1.
The second control restores the previous `handle_send_error` body in the final patch.
In single-process mode, `frame-send-failure` fails inside the barrier. The parent had eight terminal callbacks and the crash notification `Send failed (Disconnected)`.
In multiprocess mode, the frames part passes eight cases and reaches the 90-second runner deadline in `frame-send-failure`.
A diagnostic stack sample showed that the fixture had returned and the engine destructor waited for shutdown completion. The error message never printed.
The third control removes only the top-level `javascript:` URL entry drop from the final patch.
The documents part passes thirty-eight cases and fails at `javascript-url-history-crash-recovery`, as in the first control.
With each control, the parts that do not depend on the removed change pass. Later cases in a failed part did not run.
The [frame path audit](#frame-path-audit), [JavaScript URL crash page](#javascript-url-crash-page), and [frame failure containment](#frame-failure-containment) sections describe the earlier checkpoints' controls.
That checkpoint's builds retained the line-table debug configuration, and its parts had 52, 9, and 28 cases.

The synchronous construction checkpoint records the following development failures.
Two fixture errors failed development runs before its first final preparation.
An assertion expected sandboxing in the blank crash document, and an evaluation before activation failed with `InternalError`.
Its first final preparation then failed one of three single-process repeats in `document-open-write-failure`.
A fetched reload activates at its response headers. The polled state check ran before the parser created its element and threw a `TypeError`.
The fixture now waits for the original page's marker before that check.

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

The first graphics run inside the coding agent's execution environment failed during macOS surface initialization, because that environment has no window-server access.
Graphics runs require native macOS graphics and window-server access. Run them on the host.
Do not change or attempt to circumvent the agent's execution environment.
If that access is unavailable, complete static checks and leave the graphical execution commands for the owner.
Servo emits its known GL texture warning and debug linker warning.
The copied engine packages also emit warnings from published code under disabled features.
The panic hook logs the intentional script-thread panic, callback and delegate panics, and host unwinds. Recovery also logs pipeline teardown warnings.
Explicit shutdown logs script termination, profiler disconnection, and timer warnings.
Release of retained handles after shutdown also logs constellation disconnection warnings.
The backend loss fixture logs the intentional constellation panic. Single-process runs also log a hang monitor disconnection.
All assertions and process exit checks passed despite these warnings.
The experiment adds an optional dependency on the already locked `log` crate. No dependency version changes.
No remote CI, physical input, Linux, or release latency result applies to this checkpoint.

## Remaining requirements

This prototype does not close S1 or permit a restricted DOM API.
Unexpected content-process exit, forced termination of a fixture-owned child process, queued cancellation, and active interruption now have multiprocess evidence from the local fixture configuration.
Their contracts still need upstream review.
The kill fixture covers only a child blocked at a barrier.
Kills during JavaScript execution or IPC writes remain untested.
Script-thread unwinds, child exit, and child kill before construction now have evidence for a new view and a pending cross-site navigation.
Script-thread unwinds, child exit, and child kill now also cover a parsing cross-site document and a parsing cross-site subframe.
Script-thread unwinds, child exit, and child kill also cover a parsing sandboxed `srcdoc` child in its own event loop.
Servo has no pending top-level document that parses, because it activates the document at the response headers.
Script-thread unwinds, child exit, and child kill now cover a child frame's pending navigation before its response headers.
The pending pipeline shares the parent's event loop, joins a child-site popup's event loop, or runs alone in a new event loop.
The patch now contains a child frame's failure to that frame when the parent's own top-level document runs in another event loop.
The frame shows Servo's error page, and the parent keeps its document and its scope.
A view whose own top-level document ran in the failed event loop still fails. A host must expect crash notifications for several views from one failure.
The contained view now receives one frame failure notice. Its document and scope stay valid.
A negative control shows that process loss of a lone pending frame pipeline depends on the lifeline's pending-change clause.
A failed send to a frame's pipeline now stays inside that frame. The fixture closes the constellation's channel with a probe.
The send path treats every send error to such a pipeline as a failure of its event loop for that view. A serialization error to a live event loop has no fixture.
A frame's first navigation, a failed frame with a healthy pending navigation, and frame history after a contained failure now have evidence.
A failed frame in an inactive parent document, a navigation that the child starts, and a pending traversal inside a frame remain untested.
Error-page recovery through reload, back, forward, and a new URL has evidence for top-level history.
Back and forward inside a frame after a contained failure also have evidence. Reload of a frame's error page has no fixture.
Embedder interception no longer replaces crash error pages. Crash pages for `about:blank` documents remain blank. They have the origin `null` and run inserted scripts.
Script-thread unwinds after response headers now cover a top-level document before its DOM exists and while its parser is blocked.
Script-thread unwinds now also cover `about:blank`, a same-thread `srcdoc` child, `document.write`, `document.open()`, and `javascript:` URL construction.
A crash of a `javascript:` URL document now shows Servo's error page, and reload fetches the document's URL.
A `javascript:` URL document at `about:blank` and a `javascript:` URL document in a frame have no fixture.
Crash recovery now removes an inactive top-level `javascript:` URL history entry. No fixture covers such an entry in a frame's history.
Failed `about:blank` views rely on the product's native error state.
Shared-process views now have exit, kill, close, and one ordered close-kill race. Other teardown orders remain untested.
A product contract must decide how to revoke or tolerate work that an interrupted script already scheduled.
Constellation loss now has single-process and multiprocess evidence from the local fixture configuration. Its local shutdown contract needs upstream review.
A view that fails before its first document cannot recover. The owner decided that the product shows a native error state until the user recovers or kills the buffer.
The owner accepted the upstream recovery that replaces a healthy active document when its pending navigation fails.
The [recorded decisions](phase-3-plan.md#recorded-decisions-2026-11-24) describe both.
Upstream review must settle delegate and callback panic propagation, precedence, and suppression during an existing unwind.
The host allocation counter does not establish registry revocation, retained-node memory bounds, or production scheduling.
The lifecycle audit must cover remaining replacement paths, including `srcdoc`, parser-time resets, and cross-origin history reactivation.
Upstream review must settle the conservative history policy and lifecycle API.

S2 still needs the complete current document identity on protocol messages.
A matching protocol pipeline cannot distinguish two incarnations after a reset or reactivation.
This patch does not authorize a privileged action bridge.
Evaluator authority, redirect/rewrite mechanisms, and owner scope decisions also remain open.

## Reproduction

Prepare copies of the seven published Servo packages and an isolated Bulwark workspace:

```sh
python3 experiments/prepare_document_scope.py --out target/phase3-document-backend
```

Build the candidate:

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_TARGET_DIR="$PWD/target" cargo build --offline --locked \
  --manifest-path target/phase3-document-backend/candidate/Cargo.toml \
  -p bulwark-experiment-e4 --features document-scope-multiprocess
```

Run the candidate and the separate backend loss cases with independent 35-second process deadlines:

```sh
python3 experiments/run_document_scope.py \
  --binary target/debug/bulwark-experiment-e4 \
  --preparation target/phase3-document-backend \
  --output target/phase3-document-backend-single-evidence
```

Run the multiprocess cases with independent 90-second process deadlines.
The runner runs the documents, frames, and processes parts of each repeat as separate processes:

```sh
python3 experiments/run_document_scope.py \
  --binary target/debug/bulwark-experiment-e4 \
  --preparation target/phase3-document-backend \
  --output target/phase3-document-backend-multiprocess-evidence --multiprocess
python3 -m unittest discover -s experiments -p test_document_scope_runner.py -v
```

The preparation refuses an existing output directory.
It records the patch hash, original source hashes, product manifest hashes, and candidate probe hashes in `provenance.json`.
The runner rejects a preparation whose patch, product manifests, or probe files differ from the current sources.
Only candidate manifests reference the copied packages.
The runner records the binary hash, probe hashes, case markers, each part's summary marker, exit statuses, and log hashes.
It also records each repeat's total case count across its parts.
A single part runs directly, for example `target/debug/bulwark-experiment-e4 --document-scope-multiprocess frames`.
The fixture rejects a missing part and the processes part in single-process mode.
Both document-scope features require the patched candidate and cannot compile against the product engine.
The normal E4 feature and the portable workspace retain their existing build commands.
