# Phase 2 small Linux host follow-up

Date: 2026-11-21. This report follows the [repeat checks](phase-2-repeat-checks.md).
Tailscale connectivity returned after the owner enabled it on the Mac.
The product source and key translation did not change.
Phase 2 remains open. Phase 3 has not started.

## Source and method

The existing test host accepted SSH without agent forwarding.
A new detached worktree used exact commit `e4dc2331a87ea1493dfb4799705e135cf8ac79b8`.
Its product source is unchanged from `4d406d6`, which passed GitHub Actions.
The original checkout retained its local changes. The new worktree was clean when the runner started.
The run reused the existing Linux Cargo cache.

The host has two Intel Celeron N4020 cores, about 4 GB RAM, and Linux kernel `5.8.18-glk`.
It was idle before the run, with approximately 2.9 GiB memory available.
The runner used one Cargo build job, one Rust test thread, and disabled debug information and incremental compilation.
The existing bulk Clojure program budget remained 60 seconds per evaluator mode.
The embedded-shell test retained its two-second deadline.
No production bound, assertion, or test deadline changed.

The command was:

```sh
python3 experiments/run_linux_checks.py --suite portable \
  --report-dir target/linux-runs/repeat-e4dc233-20261121
```

## Results

| Check | Result |
|---|---|
| Formatting | Passed. |
| Portable Clippy | Passed with warnings denied. |
| Workspace tests | Failed: 53 passed and two failed among 55 attempted tests. |
| Browser history | Passed in TreeWalk at 23.50 seconds and Tiered at 24.19 seconds. |
| Corrected REPL regression | Passed in TreeWalk at 6.93 seconds and Tiered at 9.14 seconds. |
| Embedded shell | Failed at its two-second deadline before a view creation request reached the engine. |
| Introspection | TreeWalk reached retention checks, then exceeded its 60-second deadline. Tiered did not run after that assertion failed. |
| Other completed worker tests | Buffers, commands, hooks, key reservations, minibuffer, modes, shell model, windows, cancellation, and bootstrap-stop checks passed. |

The workspace command took 383.67 seconds, including compilation.
Its worker test executable finished with 18 passing tests and two failures.
Cargo stopped before the remaining workspace test executable ran.
The portable runner stopped at command three of seventeen. Its remaining fourteen commands did not run.
This result does not establish a passing complete portable suite on the small host.
No Servo or standalone AOT suite ran on this host in this follow-up.

## Failure details and limits

`embedded_shell_opens_a_view_and_displays_native_key_events` reported:

```text
Shell did not create a view: Err(Runtime cancelled)
```

The harness stopped the worker after two seconds without receiving a create request.
The host had no default `.bulwark/init.cljrs` file, so a user init did not supply this failure.
This observation does not establish the exact startup bottleneck or an indefinite stall.

`introspection_messages_and_init_pass_in_both_modes` completed installation, inventory, help, error, and configuration stages.
It printed the retention stage label before reaching its deadline.
The harness then cancelled the worker. Its terminal result was `gas exhausted`.
This result does not establish the cost of an individual retention operation.

The earlier browser-history failure did not reproduce in this run.
That passing repeat does not establish the cause of the earlier timing variation.
The embedded-shell and introspection failures remain reproducible and unresolved.
The passing GitHub Actions run remains separate evidence from this small-host result.

## Evidence and next work

Local evidence resides in `target/small-linux-follow-up/`.
`portable-evidence.tar.gz` preserves the complete remote report directory.
The extracted `repeat-e4dc233-20261121/` contains its JSON summary and all three command logs.
`portable-session.log` retains the runner's terminal failure output.
The summary records the exact revision, clean worktree, runner hash, platform, budgets, commands, durations, and exit statuses.
The existing host configuration remains outside the repository.

Next work is to measure the embedded startup stages and isolate the expensive introspection retention operations.
A diagnosis must preserve the current failed baseline and distinguish test workload cost from product behavior.
This round supplies no fix or owner acceptance.
