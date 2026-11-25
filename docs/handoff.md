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
Its thirty-eight shared cases cover JavaScript URLs, script-created initial documents, an actual script-thread unwind, and callback panics.
Multiprocess mode adds six cases: content-process exit and kill, and four shared-process cases.
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
Another session started this audit and stopped before its final run. This checkpoint completes and verifies it.
Failures before document construction and remaining replacement paths stay open.
The product pins and installed natives remain unchanged.

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

The [shared-process evidence](../experiments/phase3/document-shared-evidence-2026-11-24.json) records the current S1 runs, engine controls, and local checks.
Its baseline is the backend checkpoint at `4001e29`, whose three final runs in each mode passed.
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
The thirty-eight shared cases passed in three single-process runs.
All forty-four cases passed in three multiprocess runs. Both existing Servo controls passed with each mode.
Both backend loss cases passed in three separate engine runs in each mode.
Cleanup completed 1,543 to 1,627 milliseconds after the crash request. The cause of that unwind delay is not established.
These builds retained the previous line-table debug configuration.
Multiprocess evidence uses unsandboxed content processes on macOS arm64.
The actual unwind uses Servo's public panic-reporting setup and `hard_fail = false` in the fixture.
The product keeps its existing engine configuration and dependency versions.
One preliminary repeat timed out at the redirect. The final fixture waits for explicit history-traversal completion and exposes HTTP worker errors.
Five diagnostic repeats passed after that change. The earlier timeout's exact cause remains unproven.
The prior audit also exposed a nonblocking read error on an accepted loopback socket.
The fixture now explicitly selects blocking reads with a timeout. This change does not prove the earlier timeout's cause.

All 61 portable tests, formatting, and portable/Servo-probe Clippy passed locally.
Python syntax checks and eight runner tests also passed, including mode admission, backend markers, and child cleanup after a deadline.
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
The starting worktree matched freshly fetched `origin/main` at `8104604`.
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
   Continue the S1 lifecycle audit with failures before document construction.
   A source review found that the panic handler revokes a view without a top browsing context, then returns early.
   Evaluation without that context fails immediately with `InternalError`. No fixture checks either path yet.
   Extend teardown-race evidence beyond the single ordered close-then-kill case.
   Kills during JavaScript execution, IPC writes, and document construction also remain untested.
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
