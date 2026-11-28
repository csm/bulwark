# Development handoff

## Current task and test scope

This repository's current work is defensive reliability and authorization engineering on locally owned Servo fixtures.
The current task is ordinary crash-recovery and document-lifecycle correctness in the P3.1 document-scope prototype.

- Every fixture uses local files, loopback HTTP, or intercepted local responses.
  There are no third-party targets, credentials, persistence mechanisms, or external services.
- Product dependency pins and the product's security configuration remain unchanged.
  The experimental engine patch applies only to isolated copies of the published Servo packages, never to the installed product configuration.
- Agents must not alter, disable, or circumvent their own execution sandbox.
- Native macOS graphical tests require native graphics and window-server access.
  Run them on the host. If the agent environment lacks that access, complete static checks and leave the graphical execution commands for the owner.
- Process-termination and fault-injection suites are separate, targeted validation tasks.
  Documentation and unrelated edits do not require them. Recovery-logic changes do; see [Focused acceptance checks](#focused-acceptance-checks).

The word "sandbox" has three unrelated meanings in these documents. Do not confuse them:

1. The coding agent's execution environment. Agents must leave it unchanged.
2. Servo's content-process sandbox, an engine process-isolation feature.
   It is disabled only in this local experimental fixture configuration, because the fixture runs unsandboxed content processes on macOS arm64.
   The product's content-process configuration is unchanged.
3. HTML document and iframe sandboxing flags, which restrict a page's origin and scripts.
   The engine patch works to preserve these flags correctly across crash recovery and reload.

## Current baseline

Work started from `2a0a682` on `main` (merge of pull request 16). This checkpoint is the commit that follows it; its engine patch hash is `05c5b2da…` and matches the frame group evidence.
Servo remains at `0.7.0`. Clojurust remains at `540e42181ad06046116f0e2007e7f0c449efaa65`.
The two AOT workarounds remain until a replacement pin passes their regressions.
The [frame group evidence](../experiments/phase3/document-frame-groups-evidence-2026-11-27.json) records the current runs.
Its engine patch differs from the previous checkpoint's patch only by one read-only probe, which lists a view's child frames.
The [owner decisions evidence](../experiments/phase3/document-owner-decisions-evidence-2026-11-27.json) records the previous checkpoint, whose patch hash matches `2a0a682`.

## Current engineering task

Crash-recovery and document-lifecycle correctness for the S1 document-scope prototype.
The owner required three behaviors. Each is implemented in the experimental engine patch and covered by fixture evidence at this checkpoint:

| Required behavior | Status |
|---|---|
| Contain a nested frame's failure to that frame, preserving a healthy top-level document and unrelated views | Implemented. The parent keeps its document, scope, and pending callbacks, and receives one frame failure notice. Only the frame shows Servo's error page. Several frames that fail together, and a frame inside a failed frame, also receive one notice that names only the outermost failed frames. See [frame failure containment](phase-3-document-scope.md#frame-failure-containment), [frame failure notice](phase-3-document-scope.md#frame-failure-notice), and [frame group failure](phase-3-document-scope.md#frame-group-failure). |
| Prevent crash recovery from rendering a `javascript:` URL document's result again | Implemented. The crash document is Servo's error page, reload fetches the document's URL, and recovery removes an inactive `javascript:` URL history entry. See [JavaScript URL crash page](phase-3-document-scope.md#javascript-url-crash-page) and [JavaScript URL history entry](phase-3-document-scope.md#javascript-url-history-entry). |
| Preserve the correct original document sandboxing state during recovery and reload | Implemented. The crash page's load data carries the original sandboxing flags, and reload restores them even after history rebuilds the crash page. See [crash error page audit](phase-3-document-scope.md#crash-error-page-audit). |

The other documented P3.1 prerequisites stay explicitly open; see [Open requirements](#open-requirements).
The next concrete work is the open fixture gaps and contract reviews under [Next steps](#next-steps). No new milestone is authorized.

## Files expected to change

Continued work in this area touches only:

- `experiments/e4-dom-channel/src/document_scope.rs`: the S1 fixture and probes.
- `experiments/phase3/patches/servo-0.7-document-scope.patch`: the experimental engine patch, applied only by the preparation script to isolated package copies.
- `experiments/run_document_scope.py` and `experiments/test_document_scope_runner.py`: the runner and its tests.
- `docs/phase-3-document-scope.md`, `docs/phase-3-mechanisms.md`, and this handoff.
- New evidence JSON under `experiments/phase3/`.

Do not change product manifests, `Cargo.lock` pins, installed natives, product security configuration, or the accepted display scope.
Do not rewrite retained evidence JSON or historical logs.
Do not rename code identifiers, test markers, or output strings. Propose any such rename separately.

## Required behavioral outcome

- A child frame's failure (script-thread failure, content-process exit, forced termination of the fixture-owned child process, or a failed send) stays inside that frame when the parent's own top-level document runs in another event loop.
  The parent keeps its document, identity, scope, and pending callbacks. It receives exactly one frame failure notice and no crash notification.
  Each outermost failed frame navigates to Servo's error page in a new event loop. The error page has every sandboxing flag, an opaque origin, and no scripts.
- A view whose own top-level document ran in the failed event loop still fails as a whole view, with revocation and its own crash document.
  One failure can therefore notify several views.
- Crash recovery of a `javascript:` URL document shows Servo's error page with the failure reason, the origin `null`, and no running scripts.
  Reload fetches the document's URL without the result's `Content-Type` header. Back and forward cannot reach a removed `javascript:` URL history entry.
- Reload from a crash page restores the failed load's original sandboxing flags, origin, and script behavior, including after history rebuilds the crash page in a new pipeline.
- Every revocation reaches the host before its terminal callbacks. Retained replies from failed documents never pass the host allocation check.
  Views outside the failed event loop keep their scope and accept evaluation.

## Focused acceptance checks

Prepare, build, and run the fixture as described in [Reproduction](phase-3-document-scope.md#reproduction):

```sh
python3 experiments/prepare_document_scope.py --out target/phase3-document-backend
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_TARGET_DIR="$PWD/target" cargo build --offline --locked \
  --manifest-path target/phase3-document-backend/candidate/Cargo.toml \
  -p bulwark-experiment-e4 --features document-scope-multiprocess
python3 experiments/run_document_scope.py \
  --binary target/debug/bulwark-experiment-e4 \
  --preparation target/phase3-document-backend \
  --output target/phase3-document-backend-single-evidence
python3 experiments/run_document_scope.py \
  --binary target/debug/bulwark-experiment-e4 \
  --preparation target/phase3-document-backend \
  --output target/phase3-document-backend-multiprocess-evidence --multiprocess
python3 -m unittest discover -s experiments -p test_document_scope_runner.py -v
```

Expected results at this checkpoint:

- Sixty-three shared cases pass in single-process mode (documents part: 52, frames part: 11). Multiprocess mode adds the processes part (32 cases) for ninety-five total.
- Each part runs in its own engine process, prints exactly its own markers, and reports its own case count. Each repeat covers every case of its mode once.
- Both backend loss cases pass in their own engine processes in each mode. Both existing Servo controls pass.
- The two current negative controls each fail at their first dependent case: one notice for each failed frame instead of one for each view fails at `frame-siblings-failure`, and removing both guards that exclude a frame inside a failed frame fails at `frame-nested-failure`.
  See [Local evidence](phase-3-document-scope.md#local-evidence) for their exact failure points.
  The three controls of the previous checkpoint (no notice, previous send-error path, no history drop) were not rerun.
- All 61 portable tests, formatting, and portable/probe Clippy (warnings denied) pass. Python syntax checks and twelve runner tests pass.

These fixture runs need native macOS graphics and window-server access.
Run them on the host. Do not change or attempt to circumvent the agent's execution environment.
If that access is unavailable, complete static checks and leave the graphical execution commands for the owner.

## Explicitly excluded or deferred work

- No product dependency pin, product engine configuration, installed native, or product security configuration changes.
  The experimental patch never enters the product build.
- Process-termination cases (child exit, host SIGKILL of a fixture-owned child) and fault-injection cases (intentional test-triggered engine failure, backend loss) are targeted validation.
  Run them when recovery logic changes. They are not required for documentation or unrelated edits.
- Servo content-process sandboxing stays disabled only in this local experimental fixture configuration. Do not generalize that to the product.
- E5 (simultaneous live web splits) remains deferred. The accepted display scope is one visible webview per native window.
- The small Linux investigation remains deferred. No remote CI, native-input, Linux, or release-latency result applies to this checkpoint.
- Owner review remains pending for the D6/E5 prerequisite, the extension format, the reader-site tasks, and the decision timeout policy.

## Implementation summary

The [mechanism report](phase-3-mechanisms.md) records the P3.1 probes.
Deferred navigation, deferred resource responses, bounded observer drains, and local `bulwark://` rendering passed under one visible view.
The final release DOM prototype used two requests, with p50 values of 0.095333 ms in Tiered and 0.090500 ms in TreeWalk.
These measurements exclude the future node registry and JSON schema conversion. C6 remains pending.

The [document scope report](phase-3-document-scope.md) records the S1 prototype in detail:

- The engine patch assigns each document a pipeline plus an engine-issued incarnation, checked at execution and after serialization.
  Lifecycle notifications activate and revoke top documents. History reactivation receives a fresh incarnation.
  A host allocation check rejects obsolete replies before any host state changes.
- Pending callbacks close execution permits on revocation, cancellation, release, and shutdown.
  A per-script-thread watcher interrupts running JavaScript when a permit closes. Interruption cannot undo earlier effects or revoke already-scheduled promise reactions and timers.
- The process lifeline connects child exit and forced termination to the same recovery path as an intentional test-triggered engine failure.
  Constellation loss revokes every view and finishes shutdown locally.
- Construction, partial-response, synchronous-construction, cross-site parsing, pending-frame, and frame-path audits characterize failure behavior at each lifecycle point.
  Frame containment, the frame failure notice, the send-error rule, and the crash-page fixes implement the owner decisions.

The callback and delegate audits established that one panicking callback or delegate must not skip terminal delivery to the rest of a revoked batch.
The prototype attempts every delegate notification and terminal callback before it resumes the first panic.
The host must revoke local scope before any fallible work in those notifications.

## Open requirements

P3.1 remains open on these requirements:

- Engine document identity and protocol-source binding to the current privileged document.
  Custom-protocol metadata distinguishes four frame contexts, but the binding remains unproven.
- Navigation redirect and outbound request rewrite mechanisms.
- Evaluator authority across asynchronous work.
  Both evaluator modes reproduce missing authorization checks under namespace-only restriction. No restricted extension loader is exposed.
- Production subscription scheduling and registry memory measurements.

The [contract draft](phase-3-contracts.md) and two proposed ADRs record intended boundaries without changes to product behavior or pins.
The namespace-only counterexamples demonstrate missing restriction. They do not establish capability enforcement or authorize restricted extension loading.
Page-controlled agent data is not authenticated host metadata. A custom protocol alone does not establish a privileged bridge.

## Owner decisions

The [recorded decisions](phase-3-plan.md#recorded-decisions-2026-11-24) in the Phase 3 plan are authoritative.
They cover crash-page replacement for a failed pending navigation, recoverable error pages with a native error state, frame failure containment,
the `javascript:` URL crash rule, failed views at `about:blank`, the `servo-net` patch approval, the frame failure notice, the send-error rule, and the history drop.

## Limitations of current evidence

- Servo content-process sandboxing is disabled only in this local experimental fixture configuration (macOS arm64, software rendering).
  The product keeps its existing single-process configuration.
- The actual-unwind fixture uses Servo's public panic-reporting setup with `hard_fail = false`. The product keeps its existing engine configuration.
- These builds retained the previous line-table debug configuration. Backend cleanup completed 1,472 to 1,501 milliseconds after the crash request; the unwind delay's cause is not established.
- The send case closes the constellation's channel with a probe. A real dead script thread, a serialization error to a live event loop, and the other send-error callers have no fixture.
- A frame inside a healthy frame, a nested failure whose inner frame runs in another event loop than its failed parent, and a failed send to a group of frames have no notice fixture.
  The group fixtures cover script-thread unwind, content-process exit, and host SIGKILL for sibling frames and for a frame inside a failed frame.
- The group fixtures do not read the error page's sandboxing flags.
  The content-process id probe (`frame_content_process_id_for_document_scope_probe`) reports only the newest child frame's process, so the nested inner frame's process is never read.
  Its membership in the failed event loop rests on the event loop identifier that the child frames probe reports before the failure, which equals the outer frame's.
- A `javascript:` URL document in a frame or at `about:blank`, a `javascript:` URL entry in a frame's history, an entry that no diff enters, and an evicted entry have no fixture.
- One single-process run of the first negative control printed its error and then exited with signal 11 during teardown.
  In multiprocess mode, the send-error control reached the runner deadline because the engine destructor waited for shutdown completion after the fixture returned its error.
- One preliminary repeat timed out at the redirect. The final fixture waits for explicit history-traversal completion and exposes HTTP worker errors.
  Five diagnostic repeats passed after that change. The earlier timeout's exact cause remains unproven.
- The prior audit exposed a nonblocking read error on an accepted loopback socket. The fixture now explicitly selects blocking reads with a timeout.
  This change does not prove the earlier timeout's cause.

## Evidence

The [reproduction guide](phase-3-document-scope.md#reproduction) prepares the engine patch without product overrides.
Each evidence file records runs, source and binary hashes, negative controls, and local checks for its checkpoint.

| Evidence | Checkpoint |
|---|---|
| [Frame groups, 2026-11-27](../experiments/phase3/document-frame-groups-evidence-2026-11-27.json) | Current: sibling frames and a nested frame in one failure, sixty-three and ninety-five cases, two negative controls |
| [Owner decisions, 2026-11-27](../experiments/phase3/document-owner-decisions-evidence-2026-11-27.json) | Frame failure notice, send-error rule, history drop; three negative controls |
| [Frame paths, 2026-11-27](../experiments/phase3/document-frame-paths-evidence-2026-11-27.json) | Frame path checkpoint and two isolation controls |
| [JavaScript URL crash, 2026-11-26](../experiments/phase3/document-javascript-crash-evidence-2026-11-26.json) | JavaScript URL crash page and three negative controls |
| [Frame containment, 2026-11-26](../experiments/phase3/document-frame-containment-evidence-2026-11-26.json) | Frame containment checkpoint and three negative controls |
| [Pending frame, 2026-11-26](../experiments/phase3/document-pending-frame-evidence-2026-11-26.json) | First seventy-eight-case checkpoint, before frame containment |
| [Cross-site parsing, 2026-11-26](../experiments/phase3/document-cross-site-parse-evidence-2026-11-26.json) | Fifty-three-case checkpoint |
| [Synchronous construction, 2026-11-26](../experiments/phase3/document-synchronous-evidence-2026-11-26.json) | Fifty-case checkpoint and its fixture race |
| [Partial response, 2026-11-26](../experiments/phase3/document-partial-evidence-2026-11-26.json) | Forty-four-case checkpoint |
| [Crash history, 2026-11-26](../experiments/phase3/document-crash-history-evidence-2026-11-26.json) | Fifty-two-case checkpoint and its negative control |
| [Crash page, 2026-11-26](../experiments/phase3/document-crash-page-evidence-2026-11-26.json) | Fifty-one-case crash page checkpoint and two negative controls |
| [Recovery, 2026-11-24](../experiments/phase3/document-recovery-evidence-2026-11-24.json) | First fifty-one-case checkpoint and its negative controls |
| [Construction, 2026-11-24](../experiments/phase3/document-construction-evidence-2026-11-24.json) | Forty-six-case checkpoint |
| [Shared process, 2026-11-24](../experiments/phase3/document-shared-evidence-2026-11-24.json) | Forty-four-case checkpoint and its negative control |
| [Backend loss, 2026-11-24](../experiments/phase3/document-backend-evidence-2026-11-24.json) | Forty-case checkpoint and both backend loss controls |
| [Content-process termination, 2026-11-24](../experiments/phase3/document-kill-evidence-2026-11-24.json) | First forty-case multiprocess checkpoint |
| [Process lifeline, 2026-11-24](../experiments/phase3/document-process-evidence-2026-11-24.json) | Thirty-nine-case multiprocess checkpoint |
| [Interruption, 2026-11-24](../experiments/phase3/document-interruption-evidence-2026-11-24.json) | First thirty-eight-case checkpoint and earlier cancellation baseline |
| [Cancellation, 2026-11-24](../experiments/phase3/document-cancellation-evidence-2026-11-24.json) | Thirty-four-case checkpoint |
| [Delegate panics, 2026-11-24](../experiments/phase3/document-delegate-evidence-2026-11-24.json) | Thirty-one-case checkpoint |
| [Callback panics, 2026-11-24](../experiments/phase3/document-callback-evidence-2026-11-24.json) | Twenty-seven-case checkpoint |
| [Cleanup, 2026-11-23](../experiments/phase3/document-cleanup-evidence-2026-11-23.json) | Twenty-two-case checkpoint |
| [Audit, 2026-11-23](../experiments/phase3/document-audit-evidence-2026-11-23.json) | Nineteen-case checkpoint |
| [Lifecycle, 2026-11-23](../experiments/phase3/document-lifecycle-evidence-2026-11-23.json) | Sixteen-case checkpoint |
| [Earlier scope, 2026-11-22](../experiments/phase3/document-scope-evidence-2026-11-22.json) | Nine-case checkpoint before lifecycle notifications |
| [Mechanism evidence, 2026-11-22](../experiments/phase3/evidence-2026-11-22.json) | Six passing probes, timing samples, and source hashes |

After the [documented builds](phase-3-mechanisms.md#reproduction-and-checks), run `python3 experiments/run_phase3.py` to reproduce the six mechanism cases.

## Next steps

1. Continue P3.1 from the [required mechanisms](phase-3-mechanisms.md#required-upstream-and-integration-contracts).
   - Review the frame failure notice contract before product integration. The fixtures now cover a view with several failed frames in one failure and a failed frame inside a failed frame.
     A frame inside a healthy frame, a nested failure across event loops, and a failed send to a group remain unchecked.
   - The send case closes the constellation's channel with a probe. A real dead script thread and a serialization error to a live event loop remain unchecked.
   - A failed frame in an inactive parent, a navigation that the child starts, and a pending traversal inside a frame remain unchecked.
   - A failed frame whose healthy pending navigation shares the parent's event loop also remains unchecked.
   - Failed views at `about:blank` rely on the product's native error state. Their blank crash documents have the origin `null` and run inserted scripts.
     The product must not treat them as usable pages.
   - A `javascript:` URL document in a frame or at `about:blank` has no crash fixture.
   - Recovery removes an inactive `javascript:` URL entry from a frame's history too. No fixture covers that path, a first entry that no diff enters, or an evicted entry.
   - Reload from a crash page uses default target snapshot parameters. Check views that started with inherited sandboxing.
   - Frame history after whole-view crash recovery and reload of a frame's error page remain unchecked.
   - Error pages must be recoverable by back, reload, or a new URL. A failed buffer shows a native, platform-styled error until the user recovers or closes it.
     The [recorded decisions](phase-3-plan.md#recorded-decisions-2026-11-24) give details. No milestone schedules that work yet.
   - Extend teardown-race evidence beyond the single ordered close-then-terminate case.
   - Forced termination during JavaScript execution and IPC writes also remains untested.
   - A hung constellation, a process abort, and panics without an event-loop identity also remain untested.
   - Review the queued cancellation, interruption, process-lifeline, and backend loss contracts before integration.
   - Decide whether a product contract revokes or tolerates work that an interrupted script already scheduled.
   - Cover remaining replacement paths such as `srcdoc`, parser-time resets, and cross-origin history reactivation.
   - Review the explicit shutdown contract, callback ownership cycles, and delegate and callback panic policy before product integration.
   - Review the proposed history reactivation policy before product integration.
   - The current host counter still needs a production registry and handle revocation.
   - Protocol-source binding, redirect/rewrite, and evaluator authority mechanisms also remain open.
2. Review the remaining Phase 3 scope decisions and acceptance criteria with the owner before dependent implementation.
   Preserve the accepted single-visible-webview scope until E5 supplies evidence for simultaneous live web splits.
3. Keep the [small Linux investigation](phase-2-small-linux-follow-up.md) deferred under the owner's desktop focus.
   Startup and introspection failures remain unresolved. The latest browser-history repeat passed. Earlier timing variation remains unexplained.
4. Evaluate a fixed Clojurust pin for closed issues [#500](https://github.com/csm/clojurust/issues/500) and [#501](https://github.com/csm/clojurust/issues/501).
   The product pin remains `540e4218`. Remove the two AOT workarounds only after the replacement pin passes their regressions.

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
