# Phase 2 engine and preference decision

Date: 2026-11-21. Source baseline: `c8a3bdc`, equal to fetched `origin/main`.
This work continues the [engine evaluation](phase-2-engine-optimization.md).
Phase 2 remains open. Phase 3 has not started.

## Engine decision

The product retains published Servo `0.7.0` and its current lockfile.
The correction path uses a future tagged upstream release that contains the radio optimization.
The experimental backport remains a diagnostic candidate.

[Servo PR #49855](https://github.com/servo/servo/pull/49855) merged on November 12 at `d4067b91beceeac11172c4ed1401c602d8338041`.
The latest [Servo release, `0.8.0`](https://github.com/servo/servo/releases/tag/v0.8.0), dates from November 9.
The release tag does not contain that merge.
Thus, a change to `0.8.0` alone does not supply this optimization.

The earlier evaluation shows a partial latency improvement without new failures in its focused comparison.
The optimization retains nested document scans and two existing validity failures per round.
It does not correct large-group complexity or establish full engine correctness.
A local engine fork adds maintenance cost without resolving those limits.

Before an engine update, repeat the radio workload, validity comparison, matched banner comparison, product suites, and required physical tasks.
Any correction to repeated group scans needs its own measurements and correctness evidence.
If large-group latency remains unacceptable, pursue an upstream correction that avoids repeated document scans.
The current preference decision does not claim an engine performance correction.

## Preference comparison

The comparison uses the retained baseline release probe from the engine evaluation.
Its SHA-256 is `4f3d848582e4910668ed43dd2961b164ef0b430f21b0afd9099409c8232581e5`.
The binary uses the published product engine, WebCrypto, and the default user agent.
Each process starts without saved browser state or a page script.

The `default` configuration retains both disabled preferences.
The `compatibility` configuration enables only `dom_intersection_observer_enabled` and `dom_adoptedstylesheet_enabled`.
Each process runs for thirty seconds and inspects the document after twenty seconds.
The runner retains commands, binary hashes, CPU time, heartbeats, document observations, and raw logs.
No compilation or other browser evaluation overlaps these samples.

The comparison covers all five required sites and both upstream performance URLs.
All fourteen processes returned zero, with 29 heartbeats and one document inspection each.

| URL | Default CPU | Both interfaces CPU | Maximum heartbeat delay, default / both |
|---|---:|---:|---:|
| Wikipedia Rust | 22.44 s | 22.75 s | 12 / 12 ms |
| Hacker News | 1.68 s | 1.69 s | 12 / 12 ms |
| GitHub Servo | 2.95 s | 3.79 s | 12 / 12 ms |
| MDN Array.map | 4.20 s | 4.23 s | 12 / 12 ms |
| BBC News | 2.89 s | 3.44 s | 15 / 25 ms |
| MDN issue URL | 3.10 s | 3.50 s | 15 / 15 ms |
| Amazon issue URL | 2.52 s | 2.49 s | 15 / 15 ms |

GitHub retained 83 skeleton elements under default preferences and zero with both interfaces enabled.
BBC returned homepage content in both configurations.
The MDN issue URL also returned content in both configurations.
Each Amazon process displayed the same “Continue shopping” gate.
These Amazon samples do not exercise the storefront or resolve its reported CPU behavior.

These single samples do not establish small CPU regressions or improvements.
Short diagnostic loads do not establish physical site acceptance or stable long-term resource use.
The [Amazon observer issue](https://github.com/servo/servo/issues/44907) remains open.

Reproduction:

```sh
python3 experiments/compare_intersection_observer.py \
  --binary /absolute/path/to/baseline-release \
  --configuration default --configuration compatibility \
  --out target/preference-evaluation/release-comparison
```

The original command still compares default preferences against IntersectionObserver alone.
Repeated comparisons alternate configuration order.

## Named Wikipedia banner repeats

Three fresh release processes loaded the same banner preview for 45 seconds each.
All enabled both interfaces on the unchanged published engine.
The read-only observation script recorded the banner name, population, and radio count.
Every process displayed `B2627_111914_en6C_dsk_p1_lg_amt_cnt` with 26 radio inputs.
Every process returned zero, completed 44 heartbeats, and returned three document inspections.
Maximum heartbeat delays were 302, 766, and 753 ms.

These repeats did not reproduce the earlier 34-second debug pause.
They establish responsiveness for these release loads, with visible latency variation.
They do not establish a speedup, full banner coverage, physical Wikipedia acceptance, or corrected radio complexity.
The existing debug bottleneck and validity failures remain visible in the engine report.

Reproduction:

```sh
python3 experiments/compare_intersection_observer.py \
  --binary /absolute/path/to/baseline-release \
  --configuration compatibility --repeat 3 --seconds 45 \
  --url 'https://en.wikipedia.org/wiki/Rust_%28programming_language%29?banner=B2627_111914_en6C_dsk_p1_lg_amt_cnt&force=1&uselang=en' \
  --user-script target/preference-evaluation/banner-observation.js \
  --out target/preference-evaluation/banner-release
```

The observation script is `OBSERVE` from `experiments/compare_radio_banner.py`.

## Product correction and local regressions

The product correction enables these two real engine interfaces.
All other experimental preferences retain their defaults.
The product adds no JavaScript API shim, site replacement, or user-agent override.
Rust configures engine interfaces. Clojure retains commands, browser state, and browser policy.

The local Servo fixture exercises stylesheet adoption in both a document and a shadow root.
It requires actual computed colors and the adopted sheet identities.
It also requires an IntersectionObserver callback for the visible heading.
The fixture disconnects its observer and removes its temporary stylesheet elements.
These assertions run in both evaluator modes through the existing Servo suite.

## Physical BBC follow-up

The physical round used the verified debug binary in `/tmp/BulwarkPreferenceEvaluation.app`.
The isolated profile started without saved site state or an init file.
Both executable hashes were `9be06763a467e9ff6294464512c8bd01b3b7df67a72a305e2b1a373a0eec15cc`.
The separate release build passed before the consented article tasks.

Computer Use supplied address entry, pointer clicks, wheel input, reload shortcuts, and native close.
The keyboard layout remained Dvorak.
Loopback nREPL supplied read-only observations. No dispatcher call or JavaScript page action supplied a physical result.
The page had no diagnostic script, API shim, or user-agent override.
These tasks supply no latency measurement.

Physical address entry loaded BBC News.
The owner explicitly approved its Terms of Use button in this chat before the physical click.
The click dismissed the overlay.
A physical headline click opened the current lead article at `https://www.bbc.com/news/articles/cq7827z2841zo`.
Physical address entry later loaded the original failing URL, `https://www.bbc.com/news/articles/cwvg2l312ylqo`.
That prompt required a second Enter in this round.

| Article | First wheel scroll | Physical reload | Wheel scroll after reload |
|---|---:|---|---:|
| Current lead | `scrollY=1584` | Readable article retained | `scrollY=792` |
| Original failing URL | `scrollY=1584` | Readable article retained | `scrollY=792` |

The lead article time origin changed from `2810825538.83` to `2810850291.60` ms after reload.
The original article time origin changed from `2810900259.60` to `2810938442.40` ms.
These observations establish new documents for both physical reloads.
Both final documents retained article content without the application-error screen.
Both interfaces remained available.

Third-party console errors remain, including privacy-management and advertising initialization errors.
Final reload observations reported `readyState=interactive`.
The result establishes these article, scroll, and reload tasks. It does not establish complete site compatibility or completed resource loading.
The required five-site scope remains unchanged.

The native close button stopped the process with status zero and joined its runtime worker.
The profile port file disappeared. The final composed frame retains the readable original article.
This report supplies no owner acceptance for Phase 2.

## Required checks and evidence

| Check | Result |
|---|---|
| Portable workspace | All 58 tests passed. |
| Native unit tests and Servo contract | All 17 unit tests and the contract passed. |
| Local Servo fixture | Passed both evaluator modes, including actual observer callbacks and adopted stylesheet rendering. |
| Native process suites | All 16 cases passed in each evaluator mode. |
| Clippy | Portable and native checks passed with warnings denied. |
| Formatting, whitespace, Python compilation | Passed. |
| Native release build | Passed. |
| Physical BBC | Both consented article workflows passed. Native close returned zero and removed the port file. |
Current Linux CI and the additional small-host deadline failures remain open.
This report supplies no owner acceptance.

Local evidence resides in `target/preference-evaluation/`.
`release-comparison/` retains all fourteen site loads and its summary.
`banner-release/` retains the three named banner loads and the observation script hash.
`checks/` retains every command, status, test log, and native fixture frame.
`physical/` retains binary hashes, read-only observations, and native process evidence.
Saved browser state remains outside the repository.
