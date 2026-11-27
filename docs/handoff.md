# Development handoff

## Current work: P3.1 mechanisms

The owner authorized P3.1. The [mechanism report](phase-3-mechanisms.md) records the probes and unresolved prerequisites.
The [Phase 3 draft plan](phase-3-plan.md) proposes the remaining milestones and acceptance evidence.
Owner review remains pending for the D6/E5 prerequisite, extension format, reader-site tasks, and decision timeout policy.

P3.1 passed deferred navigation, deferred resource responses, bounded observer drains, and local `bulwark://` rendering under one visible view.
The final release DOM prototype used two requests, with p50 values of 0.095333 ms in Tiered and 0.090500 ms in TreeWalk.
These measurements exclude the future node registry and JSON schema conversion. C6 remains pending.

The [S1 document scope prototype](phase-3-document-scope.md) now includes lifecycle notifications, callback revocation, and a host allocation check.
The isolated engine patch assigns fresh scope identities on reset and history reactivation.
The probe rejects obsolete replies before its allocation counter changes.
Its fifty-nine shared cases cover JavaScript URLs, script-created initial documents, actual script-thread unwinds before construction, after response headers, during `about:blank`, `srcdoc`, `document.write`, and `javascript:` URL construction, while cross-site documents and frames parse, before a child frame's pending navigation receives its response headers, and after construction, three more frame paths, crash recovery history, and callback panics.
Multiprocess mode adds twenty-eight cases: content-process exit and kill, four shared-process cases, four pending-process cases, six parsing-process cases, six pending-frame process cases, and six frame path process cases.
The cases run in three parts, each in its own engine process: documents, frames, and processes.
A child frame's failure now stays inside that frame when the parent's own top-level document runs in another event loop.
A separate engine process adds two backend loss cases in each mode.
The unwind fixture releases eight pending callbacks and accepts new work after recovery.
Final handle release now revokes host scope and pending callbacks synchronously, including release before host activation.
Explicit shutdown releases sixteen ordinary callbacks, breaks a retained-view cycle, and rejects new evaluation before the engine stops.

The callback audit found that one panic skipped terminal delivery to the rest of a revoked batch.
The prototype now attempts every terminal callback before it resumes the first panic.
Cleanup during an existing host unwind preserves the outer panic.
Five cases cover result delivery, lifecycle revocation, final release, release during a host unwind, and shutdown.

The delegate audit found the same defect when a host revocation handler panicked.
Removed callbacks were dropped without terminal delivery, and one shutdown panic skipped the remaining views.
The prototype now attempts every delegate notification and terminal callback before it resumes the first panic.
A delegate panic precedes callback panics in the same batch.
The host must revoke local scope before any fallible work in those notifications.
Four new cases cover lifecycle revocation, final release, release during a host unwind, and shutdown with two panicking delegates.

The cancellation audit now adds a private execution permit to each pending request.
Explicit cancellation closes the permit before terminal delivery and prevents queued mutations at the script thread's execution check.
Three new cases cover queued cancellation, cancellation after execution, and cancellation callback panics.

The interruption audit extends the same permit to running JavaScript.
One watcher thread per script thread waits on the active evaluation's permit.
Closure requests a SpiderMonkey interrupt, and the existing callback terminates only that evaluation.
`catch` and `finally` blocks do not run, and the script thread continues serving other work.
Four new cases cover a running loop, a serialization getter, scheduled work, and an uninterrupted 300-millisecond control.
Interruption cannot undo earlier effects. A promise reaction and a timer that the script scheduled still run.
Two negative controls show that both the execution check and the interrupt are necessary.
Both process modes now pass the cancellation and interruption cases.

The process audit found that Servo reaped a dead child without scope revocation until another send failed.
The experimental patch now connects the process lifeline to the existing recovery path.
A blocked child exits with code 86 after the constellation forwards eight pending requests.
The fixture sends no further work until the lifeline revokes host scope and releases all eight callbacks.
It rejects a retained reply before allocation, evaluates the crash document, and accepts work after navigation.
An independent view remains usable. Ordinary close and engine shutdown bypass crash recovery.
The runner now stops the entire process group on a deadline, including content processes.

The kill audit forces termination from outside the child.
The barrier reports the child's process id. After the same acknowledgment, the host sends SIGKILL while the child is blocked.
In this mode the child does not exit on release, so only the signal can end it.
The same lifeline revocation, callback release, recovery, and survivor assertions pass.
The review of the process commit removed two formatting-only hunks from the engine patch and widened a runner test's timing margins.

The backend loss audit found three defects after the constellation thread ends.
Host scope and pending callbacks stayed live, because no reply or lifecycle event could arrive.
A request after a failed send entered the pending table without a terminal path.
The engine destructor waited forever for a shutdown completion message that only the constellation sends.
The experimental patch now detects closure of the constellation-to-embedder channel, after it handles every earlier message.
It revokes every view before terminal callbacks, closes admission, and finishes shutdown locally with one loss report.
An experimental probe unwinds the actual constellation thread with eight requests held at a script-thread barrier.
These cases run in their own process, because backend loss ends the engine.
The review of the kill commit corrected a stale evidence reference in the scope guide.

The shared-process audit opens two popups that share the affected view's content process.
An engine-reported process id confirms the shared child. Page data cannot supply that id.
Ordinary close of one shared view revokes only that view. Exit and kill revoke both affected views and release sixteen callbacks.
Both views recover, and an independent view stays usable.
A kill after a blocked close found that the panic handler sent crash recovery to the closed view.
The experimental patch now ignores views that the constellation no longer holds in that handler.
Another session started this audit and stopped before its final run. The shared-process checkpoint completes and verifies it.

The construction audit withholds navigation responses, so pending pipelines exist without a document.
A new probe message unwinds the script thread of a view's pending top-level pipeline.
A new view without a document rejects nine requests with `InternalError` and retains no callback.
Its failure sends revocation and a crash notification but no crash document. The view cannot recover and can only close.
A failed cross-site navigation revokes the healthy active document in another event loop and releases eight callbacks.
Upstream recovery then replaces that document with a crash document. The owner accepted this behavior.
In multiprocess mode, a second `Active(None)` can follow the crash notification and revoke a request sent between them.
The fixture now waits for both events. Both outcomes are terminal denials.
These cases characterize existing behavior and have no negative control. The engine change adds only the probe.
The review of the shared-process commit corrected a stale starting-commit reference in this handoff.

The owner then required recovery from an error page through back, reload, or a new URL.
The recovery audit extends the exit and kill barrier to the child of a pending pipeline. It found two engine defects.
First, the lifeline did not treat pending pipelines as live. Child loss before construction sent no crash notification.
A live view also kept a stale pending change, and Servo refused every later top-level navigation.
The patch now treats a pipeline as live while its view holds it as a pending change.
Second, crash recovery closed every history pipeline without marking its entry for reload. Back and forward then wedged the view.
The patch now marks those entries for reload, as history eviction does. Reload, back, forward, and a new URL recover in both modes.
A negative control for each fix fails at its first dependent case.
Servo also let embedder interception replace the crash error page, so earlier fixture crash documents were the previous URL served again.

The owner approved patching `servo-net`, so the experimental patch now covers seven published packages.
Interception now skips crash requests, and each fetched crash document must contain the failure reason.
The real error page exposed a reload defect: reload inherited the crash page's sandboxing, with an opaque origin and no scripts.
The patch now records each crash page's original load data, and reload replaces the crash page with that load.
A crash document for a view at `about:blank` is blank, because Servo loads `about:blank` without the crash fetch.
A negative control for each change fails at its first dependent case.

The review of the `servo-net` commit found that the reload record was keyed by the crash pipeline's id.
A later failure in the same view closes a crash page left in history. Back then rebuilds that page in a new pipeline without the record.
Reload from the rebuilt page again kept the crash page's sandboxing, with the origin `null` and no scripts.
The crash page's load data now carries the original sandboxing flags, and reload restores them from any crash pipeline.
The reload now also sets the reload-navigation flag, as a script-thread reload does.
A new shared case covers this path. The previous patch, as its negative control, fails only that case in each mode.

The partial response audit fails the script thread after the response headers arrive.
Servo activates the document at that point, but it creates the DOM only after 1,445 body bytes arrive for MIME sniffing.
Servo's request interceptor cannot stream a body, so a loopback HTTP server holds each response open.
One case fails a document before its DOM exists. The other fails a document whose parser waits for a withheld script.
Both revoke host scope before eight terminal callbacks and show the crash page.
Late scripts and body data then reach no live document and raise no engine error. Reload and a new URL recover.
These cases characterize existing behavior, so they have no negative control.
The review also found that the summary marker still reported the old case count. The runner now rejects a stale count.

The synchronous construction audit covers documents that Servo builds without a navigation fetch, and parsers that `document.write` feeds.
Servo builds `about:blank` and `srcdoc` documents in the task that receives the load, so no unwind can precede construction.
Five shared cases unwind a complete initial `about:blank` document, or a document whose parser waits for a withheld script.
The blocked parsers are a script-created parser in `about:blank`, a same-thread `srcdoc` child, a written external script, and a `document.open()` reset.
Each case revokes host scope before eight terminal callbacks and rejects the retained reply before allocation.
Late script data reaches no live document. Reload and a new URL recover, and an independent view stays usable.
A crash document at `about:blank` is blank. It has the origin `null`, and it runs inserted scripts.
Servo's synchronous load ignores the crash details, and it takes its sandboxing flags from the default snapshot parameters.
A sixth case unwinds a document that a `javascript:` URL created. Revocation and callback release pass in the same way.
At that audit, crash recovery rendered the JavaScript result again, with the page's real origin and running scripts. Reload rendered it again.
These cases characterized existing behavior, so they had no negative control. A later change below replaces the sixth case's result.
Remaining construction failures and replacement paths stay open.

The cross-site parsing audit fails documents whose parser waits for a withheld script in an event loop of their own.
Servo has no pending top-level document that parses. It activates the document at the response headers.
While the navigation case's cross-site document parses, the probe finds no pending top-level pipeline.
Three shared cases unwind the script thread of a parsing cross-site navigation, a cross-site subframe, and a sandboxed `srcdoc` child.
Servo always gives the sandboxed child a new event loop. Four multiprocess cases exit or kill the process of the parsing document or cross-site subframe.
The engine patch adds only frame probes: an unwind of a child frame's event loop, and the exit, kill, and process-id barrier for its process.
A failure revokes every view that has a pipeline in the failed event loop, before each view's eight terminal callbacks.
For a child frame, this includes the healthy parent in another event loop and a popup that shares the child's event loop.
Upstream recovery replaces each of those views with a crash page. A popup in the parent's or source document's event loop keeps its scope and its retained reply.
Late script and body data reach no live document and raise no engine error.
Reload and a new URL recover. After the navigation failure, back also rebuilds the source document.
These cases characterize existing behavior, so they have no negative control.
The product pins and installed natives remain unchanged.

The pending frame audit fails a child frame whose navigation waits for its response headers.
A host evaluation sets the frame's `src`, and the host withholds the response. The old child document stays active.
The engine patch adds only pending-frame probes: an unwind of the pending pipeline's event loop, and the exit, kill, and process-id barrier for its process.
Three shared cases unwind that event loop. Six multiprocess cases exit or kill its process.
The pending pipeline shares the parent's event loop, joins a child-site popup's event loop, or runs alone in a new event loop.
The probe and engine-reported process ids confirm each layout.
The failure revokes every view that has a pipeline in that event loop, including the healthy parent through its pending pipeline.
Each revoked view receives revocation before eight terminal callbacks, and upstream recovery replaces it with a crash page.
A popup at `about:blank` in the parent's event loop keeps its scope in both cross-site layouts. It loses scope in the same-site layout.
A late navigation response reaches no live document and raises no engine error. Reload and a new URL recover.
The source shows that, for a lone pending pipeline, only the lifeline's pending-change clause makes the process live. No negative control isolates this dependence yet.
Two more multiprocess cases exit or kill the sandboxed `srcdoc` child's process while it parses. The scope results match its unwind.
These cases characterize existing behavior, so they have no negative control.

The owner then decided that a child frame's failure must stay inside that frame.
The frame containment change sorts the views of a failed event loop into two groups.
A view whose active top-level pipeline or pending top-level pipeline ran in the failed event loop still fails as before.
Every other view keeps its document and scope, and it receives no lifecycle event and no crash notification.
Each outermost failed frame of such a view navigates to Servo's error page through the existing `NavigateIframe` message to its parent.
The parent's frame element accepts the crash pipeline as its own navigation. The navigation replaces the frame's history entry.
The error page has every sandboxing flag. It has an opaque origin, runs no scripts, and runs in a new event loop.
A frame's crash load now always uses the fetch path. Otherwise a `srcdoc` frame's crash load parsed its markup again.
A new probe reports the newest child frame's current pipeline, liveness, crash details, and pending state.
Four shared cases and eight multiprocess cases now assert the contained behavior.
The parent's eight callbacks, held at a barrier on its script thread, succeed in the same document after the failure.
Its retained reply passes the host allocation check. Only the frame shows the error page.
A parsing parent then completes its load. In the pending cases, the error page replaces the old child.
The child-site popup still fails, so one failure can notify several views. The parent-loop popup keeps its scope.
The same-site pending cases keep their results, because the parent's own document fails.
Three negative controls fail at their first dependent case. The first restores the previous panic handler.
The second isolates the lifeline's pending-change clause for frames. The third restores the synchronous frame crash loads.
A failed send to a frame's pipeline still replaces the whole view. No fixture checks that path.

The owner then decided that crash recovery must not render a `javascript:` URL document's result again.
Servo kept the result in the document's load data, and its synchronous load ignored the crash details.
The experimental patch now drops the result and its `Content-Type` header from every crash load.
The crash document is now Servo's error page with the failure reason. It has the origin `null` and runs no scripts.
Reload from the error page fetches the document's URL, which is the source page's URL.
Servo's ordinary reload of a live `javascript:` URL document also fetches that URL.
The converted case asserts the error page, the reloaded source page, and a reload request without a `Content-Type` header.
Three negative controls fail in that case. The first is the previous patch, and its crash document lacks the failure reason.
The second keeps the result in the reload record. Its reload renders the result again.
The third keeps the `Content-Type` header, and the reload request carries it.
A frame's crash load already fetched the error page. No fixture covers a `javascript:` URL document in a frame or at `about:blank`.

The multiprocess scope run had used about 57 of its 80 seconds in one engine process.
The runner now runs the scope cases in three parts, each in its own engine process with its own deadline.
The documents part has fifty-one cases. The frames part has eight cases. The processes part has twenty-eight cases and runs only in multiprocess mode.
Each part must print exactly its own markers and case count. Each repeat must cover every case of its mode once.
The frames and processes parts check a fresh independent view at the fixture origin.

The frame path audit then checked three containment paths that had no evidence.
A frame's first navigation waited for its response headers alone in a new event loop, which then failed.
Servo creates a frame's browsing context with an initial `about:blank` document in the parent's event loop.
The first navigation is therefore an ordinary pending frame navigation, and the patch contains its failure.
The healthy parent kept its document and scope. Its load event waited until the error page replaced `about:blank`.
The frame element then fired one load event, and the parent's load completed.
A loaded child also failed while the frame's next navigation waited in a third event loop.
The patch closed that healthy pending pipeline, and its late response reached no document.
A frame's second child failed with the first child in history. The error page replaced the second child's entry.
Back inside the frame rebuilt the first child in a new pipeline, and forward restored the same error page. The parent kept its scope.
Top-level back restored the parent with a fresh identity and the same error page.
Three shared cases and six multiprocess cases cover these paths. They characterize existing behavior. No engine change was needed.
A frame document that history rebuilds fires no load event at its frame element. The element accepts a load only from a navigation that it started.
Two isolation controls each remove one existing part of the containment patch, and each fails at its first dependent case.
The first keeps the healthy pending pipeline open. The second does not mark the frame's history entry for reload, and back never rebuilds the first child.

P3.1 remains open on these requirements:

- Engine document identity and protocol-source binding to the current privileged document.
  Custom-protocol metadata distinguishes four frame contexts, but the binding remains unproven.
- Navigation redirect and outbound request rewrite mechanisms.
- Evaluator authority across asynchronous work.
  Both evaluator modes reproduce namespace-only authority leaks. No restricted extension loader is exposed.
- Production subscription scheduling and registry memory measurements.

The [contract draft](phase-3-contracts.md) and two proposed ADRs record intended boundaries without changes to product behavior or pins.
The namespace-only counterexamples demonstrate missing restriction. They do not establish capability enforcement or authorize restricted extension loading.

## Current evidence

The [frame path evidence](../experiments/phase3/document-frame-paths-evidence-2026-11-27.json) records the current S1 runs, two isolation controls, engine controls, and local checks.
Its baseline is `b4318a2`, the JavaScript URL crash checkpoint. That checkpoint's three final runs in each mode passed.
The [JavaScript URL crash evidence](../experiments/phase3/document-javascript-crash-evidence-2026-11-26.json) preserves the JavaScript URL crash checkpoint and its three negative controls.
The [frame containment evidence](../experiments/phase3/document-frame-containment-evidence-2026-11-26.json) preserves the frame containment checkpoint and its three negative controls.
The [pending frame evidence](../experiments/phase3/document-pending-frame-evidence-2026-11-26.json) preserves the first seventy-eight-case checkpoint, before frame containment.
The [cross-site parsing evidence](../experiments/phase3/document-cross-site-parse-evidence-2026-11-26.json) preserves the fifty-three-case checkpoint.
The [synchronous construction evidence](../experiments/phase3/document-synchronous-evidence-2026-11-26.json) preserves the fifty-case checkpoint and its fixture race.
The [partial response evidence](../experiments/phase3/document-partial-evidence-2026-11-26.json) preserves the forty-four-case checkpoint.
The [crash history evidence](../experiments/phase3/document-crash-history-evidence-2026-11-26.json) preserves the fifty-two-case checkpoint and its negative control.
The [crash page evidence](../experiments/phase3/document-crash-page-evidence-2026-11-26.json) preserves the fifty-one-case crash page checkpoint and its two negative controls.
The [recovery evidence](../experiments/phase3/document-recovery-evidence-2026-11-24.json) preserves the first fifty-one-case checkpoint and its negative controls.
The [construction evidence](../experiments/phase3/document-construction-evidence-2026-11-24.json) preserves the forty-six-case checkpoint.
The [shared-process evidence](../experiments/phase3/document-shared-evidence-2026-11-24.json) preserves the forty-four-case checkpoint and its negative control.
The [backend evidence](../experiments/phase3/document-backend-evidence-2026-11-24.json) preserves the forty-case checkpoint and both backend loss controls.
The [kill evidence](../experiments/phase3/document-kill-evidence-2026-11-24.json) preserves the first forty-case multiprocess checkpoint.
The [process evidence](../experiments/phase3/document-process-evidence-2026-11-24.json) preserves the thirty-nine-case multiprocess checkpoint.
The [interruption evidence](../experiments/phase3/document-interruption-evidence-2026-11-24.json) preserves the first thirty-eight-case checkpoint and its earlier cancellation baseline.
The [cancellation evidence](../experiments/phase3/document-cancellation-evidence-2026-11-24.json) preserves the thirty-four-case checkpoint.
The [delegate evidence](../experiments/phase3/document-delegate-evidence-2026-11-24.json) preserves the thirty-one-case checkpoint.
The [callback evidence](../experiments/phase3/document-callback-evidence-2026-11-24.json) preserves the twenty-seven-case checkpoint.
The [cleanup evidence](../experiments/phase3/document-cleanup-evidence-2026-11-23.json) preserves the twenty-two-case checkpoint.
The [audit evidence](../experiments/phase3/document-audit-evidence-2026-11-23.json) preserves the nineteen-case checkpoint.
The [lifecycle evidence](../experiments/phase3/document-lifecycle-evidence-2026-11-23.json) preserves the sixteen-case checkpoint.
The [earlier scope evidence](../experiments/phase3/document-scope-evidence-2026-11-22.json) preserves the nine-case checkpoint.
The [reproduction guide](phase-3-document-scope.md#reproduction) prepares the engine patch without product overrides.
The fifty-nine shared cases passed in three single-process runs.
All eighty-seven cases passed in three multiprocess runs. Both existing Servo controls passed with each mode. The engine patch did not change.
Single-process mode runs two parts: documents with fifty-one cases and frames with eight cases.
Multiprocess mode adds the processes part with twenty-eight cases. Each part runs in its own engine process.
In multiprocess mode, the documents part took 34.1 seconds, the frames part 21.2 seconds, and the processes part 14.9 seconds.
Their margins to the 80-second fixture deadline are about 46, 59, and 65 seconds. The single process had used about 57 of those 80 seconds.
In single-process mode, the documents part took 6.2 to 9.0 seconds, and the frames part took 6.9 to 7.0 seconds. This checkpoint keeps all deadlines.
Three development runs of every part in each mode passed before a fresh final preparation. No final run failed.
The first isolation control keeps a failed frame's healthy pending pipeline open. It fails at `frame-healthy-pending-navigation-failure` in all three runs of each mode.
The second control does not mark a contained frame's history entries for reload. It fails at `frame-history-failure` in all three runs of each mode, at the fixture deadline.
In multiprocess mode, each control also fails at its first process case. Every earlier case passes with each control.
The JavaScript URL and frame containment controls remain in their checkpoints' evidence.
Both backend loss cases passed in three separate engine runs in each mode.
Cleanup completed 1,478 to 1,508 milliseconds after the crash request. The cause of that unwind delay is not established.
These builds retained the previous line-table debug configuration.
Multiprocess evidence uses unsandboxed content processes on macOS arm64.
The actual unwind uses Servo's public panic-reporting setup and `hard_fail = false` in the fixture.
The product keeps its existing engine configuration and dependency versions.
One preliminary repeat timed out at the redirect. The final fixture waits for explicit history-traversal completion and exposes HTTP worker errors.
Five diagnostic repeats passed after that change. The earlier timeout's exact cause remains unproven.
The prior audit also exposed a nonblocking read error on an accepted loopback socket.
The fixture now explicitly selects blocking reads with a timeout. This change does not prove the earlier timeout's cause.

All 61 portable tests, formatting, and portable/Servo-probe Clippy passed locally.
Python syntax checks and twelve runner tests also passed, including part admission, backend markers, per-part markers and summary counts, repeat totals, documented case counts, and child cleanup after a deadline.
The first backend negative control reports the loss but omits revocation. It fails with eight callbacks pending and the document active.
The second control revokes scope but omits local shutdown completion. Its watchdog reports that the destructor waited for the lost constellation.
The ordinary scope cases and both existing Servo controls pass with each control in both modes.
The shared-process negative control removes the closed-view guard. It fails only the race, with crash recovery on the closed view.
The kill evidence retains its negative control, which sends no signal to the blocked child.
The process evidence retains its negative control, which reaps the child without lifecycle recovery.
The prior interruption evidence retains the following two negative controls.
The interrupt negative control ignores closed permits in the interrupt callback.
The interrupted loop then completes, runs its `finally` block, and records `1003` mutations.
The execution-check negative control keeps the watcher but lets the cancelled queued mutation run.
Both existing Servo controls pass with each negative control.
This session started from `origin/main` at `b4318a2`.
The [earlier mechanism evidence](../experiments/phase3/evidence-2026-11-22.json) includes six passing probes, timing samples, and source hashes.
After the [documented builds](phase-3-mechanisms.md#reproduction-and-checks), run `python3 experiments/run_phase3.py` to reproduce the six cases.

## Accepted baseline and prior work

The owner accepted Phase 2 on 2026-11-22 at product checkpoint `cf3e6be`.
The accepted work is integrated into `main` at `b9b0ded`.
The [acceptance review](phase-2-acceptance.md#owner-acceptance-2026-11-22) records the desktop scope, Linux deferral, input limits, and passing keyboard checks.
No required Phase 2 keyboard validation remains. macOS defaults to native Option character input, and Escape immediately cancels prompts.

Rust owns native mechanisms and bounded transport. Clojure owns commands, state, bindings, and browser policy.
The installed inventory retains 25 documented natives. Only one webview is visible per native window. Text and REPL panes support splits.

E5 remains deferred beyond Phase 2. Broader IME, clipboard, display-scale transitions, and interactive Linux behavior remain unverified.

The [Phase 2 handoff history](handoff-2026-11-22.md) preserves the detailed results, measurements, CI links, and investigation limits.
The [earlier handoff](handoff-2026-11-14.md) preserves the Phase 0, Phase 1, and earlier Phase 2 detail.

## Next steps

1. Continue P3.1 from the [required mechanisms](phase-3-mechanisms.md#required-upstream-and-integration-contracts).
   Review the frame containment contract with the owner. A contained view receives no engine notification of its frame's failure.
   A failed send to a frame's pipeline still replaces the whole view through Servo's send-error path. Decide whether that path must also contain the failure.
   A failed frame in an inactive parent, a navigation that the child starts, and a pending traversal inside a frame remain unchecked.
   A failed frame whose healthy pending navigation shares the parent's event loop also remains unchecked.
   The owner decided that failed views at `about:blank` rely on the product's native error state. No engine change is needed for them.
   Their blank crash documents have the origin `null` and run inserted scripts. The product must not treat them as usable pages.
   A `javascript:` URL document in a frame or at `about:blank` has no crash fixture.
   Recovery keeps the result of an inactive `javascript:` URL history entry. The source shows that back to that entry would render it again. Decide whether recovery must drop it too.
   Reload from a crash page uses default target snapshot parameters. Check views that started with inherited sandboxing.
   Frame history after whole-view crash recovery and reload of a frame's error page remain unchecked.
   The owner accepted crash-page replacement for a failed pending navigation.
   Error pages must be recoverable by back, reload, or a new URL. A failed buffer shows a native, platform-styled error until the user recovers or kills it. The [recorded decisions](phase-3-plan.md#recorded-decisions-2026-11-24) give details. No milestone schedules that work yet.
   Extend teardown-race evidence beyond the single ordered close-then-kill case.
   Kills during JavaScript execution and IPC writes also remain untested.
   A hung constellation, a process abort, and panics without an event-loop identity also remain untested.
   Review the queued cancellation, interruption, process-lifeline, and backend loss contracts before integration.
   Decide whether a product contract revokes or tolerates work that an interrupted script already scheduled.
   Cover remaining replacement paths such as `srcdoc`, parser-time resets, and cross-origin history reactivation.
   Review the explicit shutdown contract, callback ownership cycles, and delegate and callback panic policy before product integration.
   Review the proposed history reactivation policy before product integration.
   The current host counter still needs a production registry and handle revocation.
   Protocol-source binding, redirect/rewrite, and evaluator authority mechanisms also remain open.
2. Review the remaining Phase 3 scope decisions and acceptance criteria with the owner before dependent implementation.
   Preserve the accepted single-visible-webview scope until E5 supplies evidence for simultaneous live web splits.
3. Keep the [small Linux investigation](phase-2-small-linux-follow-up.md) deferred under the owner's desktop focus.
   Startup and introspection failures remain unresolved. The latest browser-history repeat passed. Earlier timing variation remains unexplained.
4. Evaluate a fixed Clojurust pin for closed issues [#500](https://github.com/csm/clojurust/issues/500) and [#501](https://github.com/csm/clojurust/issues/501).
   The product pin remains `540e4218`. Remove the two AOT workarounds only after the replacement pin passes their regressions.
