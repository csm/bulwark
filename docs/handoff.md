# Development handoff

## Current work: P3.1 mechanisms

The owner authorized P3.1. The [mechanism report](phase-3-mechanisms.md) records the probes and unresolved prerequisites.
The [Phase 3 draft plan](phase-3-plan.md) proposes the remaining milestones and acceptance evidence.
Owner review remains pending for the D6/E5 prerequisite, extension format, reader-site tasks, and decision timeout policy.

P3.1 passed deferred navigation, deferred resource responses, bounded observer drains, and local `bulwark://` rendering under one visible view.
The final release DOM prototype used two requests, with p50 values of 0.095333 ms in Tiered and 0.090500 ms in TreeWalk.
These measurements exclude the future node registry and JSON schema conversion. C6 remains pending.

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

All six final probe cases, 61 portable tests, formatting, and portable/Servo-probe Clippy passed locally.
The [retained evidence](../experiments/phase3/evidence-2026-11-22.json) includes timing samples, source hashes, and final probe results.
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
   Prepare document identity, protocol-source binding, redirect/rewrite, and evaluator authority mechanisms with their required regressions.
2. Review the remaining Phase 3 scope decisions and acceptance criteria with the owner before dependent implementation.
   Preserve the accepted single-visible-webview scope until E5 supplies evidence for simultaneous live web splits.
3. Keep the [small Linux investigation](phase-2-small-linux-follow-up.md) deferred under the owner's desktop focus.
   Startup and introspection failures remain unresolved. The latest browser-history repeat passed. Earlier timing variation remains unexplained.
4. Evaluate a fixed Clojurust pin for closed issues [#500](https://github.com/csm/clojurust/issues/500) and [#501](https://github.com/csm/clojurust/issues/501).
   The product pin remains `540e4218`. Remove the two AOT workarounds only after the replacement pin passes their regressions.
