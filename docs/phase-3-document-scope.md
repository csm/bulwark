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

All fifty-six shared cases also run in multiprocess mode, including queued cancellation, active interruption, construction failures, and history recovery.
The exit and kill fixtures add two cases in that mode. The [shared-process audit](#shared-process-audit) adds four more.
The [recovery audit](#pending-process-and-recovery-audit) adds four pending-process cases.
The [cross-site parsing audit](#cross-site-parsing-audit) adds four parsing-process cases.
The [pending frame audit](#pending-frame-audit) adds eight cases, for seventy-eight cases in total.
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
Crash recovery renders the JavaScript result again as the crash document.
That document has the source URL and the page's real origin, `https://bulwark.invalid`. It does not contain the failure reason.
Its own inline script and an inserted script both run.
Reload replaces the crash document with its original load, and that load renders the same result again with the same origin.
A new URL recovers, and an independent view stays usable. The behavior is the same in both modes.
The host still receives revocation and the crash notification, but the crash document is page content, not an error page.
This case characterizes that path. It is not a required product behavior, and this audit does not change the path.

These cases characterize existing behavior. The engine patch is unchanged, so they have no negative control.
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

These cases characterize existing behavior. The engine change adds only probes, so they have no negative control.
The [pending frame audit](#pending-frame-audit) covers a pending navigation in a child frame and process loss of the sandboxed `srcdoc` child.
History traversal inside a frame after recovery remains unchecked.

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
That clause's negative control stops at the first pending-process case, so no control shows the dependence of these cases.

Two more multiprocess cases end the process of the sandboxed `srcdoc` child from the [cross-site parsing audit](#cross-site-parsing-audit):
`sandboxed-srcdoc-process-exit` and `sandboxed-srcdoc-process-kill`.
The child's process differs from the host's and from the parent's process. The parent-loop popup shares the parent's process.
The lifeline revokes the healthy parent before eight callbacks that a barrier holds on the parent's script thread.
The reason is `Content process disconnected`. The parent-loop popup keeps its scope and its retained reply.
The crash page, late-data, reload, new URL, and survivor checks of the unwind case also pass.

These cases characterize existing behavior. The engine change adds only probes, so they have no negative control.
Each case navigates a loaded frame through its `src` attribute.
A frame's initial navigation, a navigation that the child starts, and a pending traversal inside a frame remain unchecked.
History traversal inside a frame after recovery also remains unchecked.

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
The fix applies to every path that reaches the panic handler. Nested browsing-context history remains unchecked.

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
| `javascript:` URL crash recovery | A `javascript:` URL document unwinds. Eight callbacks are released. The crash document and its reload render the JavaScript result again, with the page's origin and running scripts. A new URL recovers. |
| Cross-site navigation parse failure | A cross-site document parses in its own event loop with no pending top-level pipeline, and unwinds. Eight callbacks are released. A popup in the source event loop keeps scope. Reload, back, and a new URL recover. |
| Cross-site subframe parse failure | A cross-site child unwinds while it parses. The healthy parent and a popup in the child's event loop each release eight callbacks and get crash documents. A popup in the parent's event loop keeps scope. Reload and a new URL recover. |
| Sandboxed `srcdoc` parse failure | A sandboxed `srcdoc` child in a new event loop unwinds while it parses. The healthy parent releases eight callbacks and gets a crash document. A popup in the parent's event loop keeps scope. |
| Cross-site parse process exit and kill (multiprocess only) | The process of a parsing cross-site document exits or is killed. The lifeline releases eight callbacks. The source-loop popup keeps scope, and reload, back, and a new URL recover. |
| Cross-site subframe process exit and kill (multiprocess only) | The process of a parsing cross-site child exits or is killed. The parent and the child-site popup each release eight callbacks. The parent-loop popup keeps scope. |
| Sandboxed `srcdoc` process exit and kill (multiprocess only) | The process of a parsing sandboxed `srcdoc` child exits or is killed. The healthy parent releases eight callbacks and gets a crash document. The parent-loop popup keeps scope. |
| Same-site pending frame failure | A frame's same-site navigation waits for its response headers, and the parent's event loop unwinds. The parent and a popup in that loop each release eight callbacks and get crash documents. Late data is ignored. Reload and a new URL recover. |
| Cross-site pending frame failure | A frame's pending pipeline joins a child-site popup's event loop, which unwinds. The healthy parent and that popup each release eight callbacks and get crash pages. A popup in the parent's event loop keeps scope. |
| Lone cross-site pending frame failure | A frame's pending pipeline is alone in a new event loop, which unwinds. The healthy parent releases eight callbacks and gets a crash page. A popup in the parent's event loop keeps scope. |
| Pending frame process exit and kill (multiprocess only) | The process of a frame's pending pipeline exits or is killed in each of the three layouts. The same scope results, callback release, late-data, and recovery checks pass. |
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

The runner repeats fifty-six cases in single-process mode or seventy-eight cases in multiprocess mode.
The summary marker must report the same case count.
Each repeat also runs the two backend loss cases in a separate engine process.
Both modes retain the existing Servo contract and E4 baseline results.
The host-ticket fixture suppresses allocation after delivery. Engine cancellation separately rejects closed permits before and during execution.
The injected crash calls the existing constellation panic handler through an experimental probe method.
The separate script-thread fixture terminates the receiving thread through a Rust unwind. Neither fixture proves process-failure detection.

## Local evidence

The [pending frame evidence](../experiments/phase3/document-pending-frame-evidence-2026-11-26.json) records the current runs, source hashes, binary hashes, and local checks.
Its baseline is the cross-site parsing checkpoint at `ffac748`, whose three final runs in each mode passed with both Servo controls.
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

All fifty-six shared cases passed in three single-process runs on macOS arm64 with software rendering.
Each single-process run took 8.3 to 11.1 seconds.
All seventy-eight cases passed in three multiprocess runs. Each multiprocess run took 54.0 to 54.1 seconds, within the 80-second fixture deadline.
The eleven new cases added about 13.1 seconds to each multiprocess run. The margin to the fixture deadline is now about 26 seconds.
This checkpoint keeps both deadlines. A later checkpoint that adds cases at the same rate must split the run or raise the deadline.
Two development runs in each mode passed before the final preparation. The first run preceded the lone pending-frame cases. No final run failed.
A fresh preparation produced the final binary. Its sources matched the development sources exactly.
Both backend loss cases passed in three separate engine runs in each mode. Cleanup completed 1,483 to 1,504 milliseconds after the crash request.
Both existing Servo controls, all 61 portable tests, formatting, and portable/probe Clippy passed locally.
Clippy denied warnings for the probe and portable workspace.
Python syntax checks and ten runner tests also passed. The runner test pins the documented case counts.
These builds retained the previous line-table debug configuration.

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

The initial sandboxed graphics run failed during macOS surface initialization.
Graphics runs require normal macOS graphics services outside that sandbox.
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
Unexpected content-process exit, forced kill, queued cancellation, and active interruption now have unsandboxed multiprocess evidence.
Their contracts still need upstream review.
The kill fixture covers only a child blocked at a barrier.
Kills during JavaScript execution or IPC writes remain untested.
Script-thread unwinds, child exit, and child kill before construction now have evidence for a new view and a pending cross-site navigation.
Script-thread unwinds, child exit, and child kill now also cover a parsing cross-site document and a parsing cross-site subframe.
Script-thread unwinds, child exit, and child kill also cover a parsing sandboxed `srcdoc` child in its own event loop.
Servo has no pending top-level document that parses, because it activates the document at the response headers.
A failure of a child frame's event loop replaces the whole view, including a healthy parent in another event loop.
It also revokes every other view with a pipeline in that event loop. A product contract must accept or change this.
Script-thread unwinds, child exit, and child kill now cover a child frame's pending navigation before its response headers.
The pending pipeline shares the parent's event loop, joins a child-site popup's event loop, or runs alone in a new event loop.
In each layout, the failure replaces the whole view in the same way as a failure of an active frame.
The source shows that, for a lone pending pipeline, only the lifeline's pending-change clause makes the process live. No negative control isolates this dependence yet.
A frame's initial navigation, a navigation that the child starts, and a pending traversal inside a frame remain untested.
Error-page recovery through reload, back, forward, and a new URL has evidence for top-level history only.
Embedder interception no longer replaces crash error pages. Crash pages for `about:blank` documents remain blank. They have the origin `null` and run inserted scripts.
Script-thread unwinds after response headers now cover a top-level document before its DOM exists and while its parser is blocked.
Script-thread unwinds now also cover `about:blank`, a same-thread `srcdoc` child, `document.write`, `document.open()`, and `javascript:` URL construction.
A crash of a `javascript:` URL document renders its result again, with the page's origin and running scripts.
Shared-process views now have exit, kill, close, and one ordered close-kill race. Other teardown orders remain untested.
A product contract must decide how to revoke or tolerate work that an interrupted script already scheduled.
Constellation loss now has single-process and unsandboxed multiprocess evidence. Its local shutdown contract needs upstream review.
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

Run the multiprocess cases with independent 90-second process deadlines:

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
The runner records the binary hash, probe hashes, case markers, the summary marker, exit statuses, and log hashes.
Both document-scope features require the patched candidate and cannot compile against the product engine.
The normal E4 feature and the portable workspace retain their existing build commands.
