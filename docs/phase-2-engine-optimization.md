# Radio validation engine evaluation

Date: 2026-11-21. Baseline: `1ef0723` with the current compatibility changes.
This evaluation continues the [Wikipedia pause investigation](phase-2-wikipedia-investigation.md).

## Finding

The upstream iterator optimization substantially reduces the retained radio fixture's insertion time.
The median improvement is 2.25–2.28 times in debug and 1.49–1.63 times in release.
Release latency remains 91–94 ms for 40 radio inputs in a document with 10,000 filler nodes.
The optimization reduces iterator overhead but retains the nested document scans.
It does not establish a complete correction for large radio groups or live fundraising banners.

The product still uses published Servo `0.7.0` without this patch.
The product manifest, lockfile, adopted-stylesheet preference, and IntersectionObserver preference remain unchanged.

## Candidate and isolation

[Servo PR #49855](https://github.com/servo/servo/pull/49855) introduces an unrooted radio-group iterator guarded by `NoGC`.
Its upstream merge is `d4067b91beceeac11172c4ed1401c602d8338041`.
The [retained patch](../experiments/e3-servo/patches/servo-0.7-radio-unrooted.patch) adapts that change to the published `servo-script` 0.7.0 package.
That package names the traversal API `traverse_preorder_non_rooting`.
It lacks the upstream `update_indeterminate_state` call site, which this adaptation therefore omits.

Exactly five engine source files change.
Four thread the `NoGC` argument through missing-value validation; the radio implementation supplies the new iterator.
The group filter and missing-value calculation remain unchanged. The modified source retains MPL-2.0 licensing.

Both harnesses copy the same workspace and probe source.
Only the candidate adds a local `servo-script` Cargo patch.
Its lockfile removes that package's registry source and checksum without changing dependency versions.
The preparation helper applies the patch with zero fuzz and records provenance.
A fresh preparation reproduced all five modified files exactly.
The registry cache and product source are not modified.

## Method and measurements

The host was an Apple M3 Max running macOS 26.5.2, build `25F84`.
The compiler was Rust 1.97.1, commit `8bab26f4f`.
Builds used the normal Cargo debug and release profiles with the same Servo features, including WebCrypto and JIT.
All four builds finished before measurements began. No other evaluation process ran concurrently.
Filesystem caches were warm; each sample used a fresh process and document.
The ordinary desktop environment remained active.

The fixture builds 10,000 filler nodes, then times synchronous insertion of 40 same-group radio inputs with `performance.now()`.
It uses no network request, observer instance, or fundraising script.
Each configuration has three samples. Baseline and candidate order alternates between pairs.
Each probe has a sixty-second internal deadline and a seventy-five-second external deadline.
The completion mode exits after exactly one fixture result; process startup is outside the insertion timer.

| Profile | Observer preference | Baseline median | Candidate median | Speedup | Reduction |
|---|---|---:|---:|---:|---:|
| Debug | Disabled | 5,421.69 ms | 2,407.96 ms | 2.25× | 55.6% |
| Debug | Enabled | 5,369.27 ms | 2,357.84 ms | 2.28× | 56.1% |
| Release | Disabled | 135.46 ms | 91.07 ms | 1.49× | 32.8% |
| Release | Enabled | 153.31 ms | 93.88 ms | 1.63× | 38.8% |

All final insertion samples, in milliseconds:

| Profile and preference | Baseline samples | Candidate samples |
|---|---|---|
| Debug, disabled | 5368.90, 5431.94, 5421.69 | 2360.86, 2436.07, 2407.96 |
| Debug, enabled | 5441.91, 5366.82, 5369.27 | 2394.98, 2336.88, 2357.84 |
| Release, disabled | 135.46, 127.82, 138.28 | 91.79, 85.79, 91.07 |
| Release, enabled | 161.26, 136.17, 153.31 | 93.88, 95.65, 93.04 |

The same text-input control took 19.23 ms baseline and 20.88 ms candidate in debug.
Release controls took 2.56 ms baseline and 2.22 ms candidate.
Each control has one sample and does not establish a small regression or improvement.
The large debug pause must not be presented as measured shipping latency.
The release fixture remains a synchronous task above a normal frame budget.
These measurements do not predict the earlier 34-second live debug pause directly.

## Correctness controls

The [validity fixture](../experiments/e3-servo/fixtures/radio-validity.html) runs 22 checks across ten rounds, with allocation pressure between rounds.
It covers checked-state changes, requiredness, different names, form ownership, explicit form attributes, detached trees, and shadow trees.
Required checkbox, file, and text inputs exercise the other changed validation signatures.

Both profiles and both engines produced identical results: 200 passing checks and 20 failures.
Required unnamed and empty-name radios each fail once per round because Servo reports `valueMissing=false`.
The [HTML radio rules](https://html.spec.whatwg.org/multipage/input.html#radio-button-state-(type=radio)) require nonempty matching names for group membership.
The fixture preserves these failures as existing behavior; it does not change expectations to produce a passing result.
There are no newly observed validity regressions in this fixture.
This is a focused equivalence check, not a complete Web Platform Tests run or garbage-collection safety proof.

## Live Wikipedia comparison

Four fresh release processes loaded the original Rust article for 45 seconds each.
Their order was baseline, candidate, candidate, baseline.
All enabled both adopted stylesheets and IntersectionObserver, retained WebCrypto, and used the same read-only banner observation script.
They used the same final release binaries as the deterministic comparison.
No compilation or other browser probe ran concurrently with the live probes.

| Run | Fundraising banner observed | Maximum heartbeat delay | Document radio count |
|---|---|---:|---:|
| Baseline 01 | No | 12 ms | 8 |
| Candidate 02 | No | 12 ms | 8 |
| Candidate 03 | Yes | 477 ms | 26 |
| Baseline 04 | No | 12 ms | 8 |

Every run returned zero and completed three document inspections.
The banner run identified `B2627_111914_en6C_dsk_p1_lg_amt_cnt`, from campaign `WMF_FR_FY2627_en6C_dsk_0701`.
This uneven assignment cannot establish a baseline-to-candidate live improvement.

The [CentralNotice documentation](https://www.mediawiki.org/wiki/Extension:CentralNotice#Overriding_selection) provides a `banner` preview parameter and a `force` override.
The retained runner selected that observed banner with `force=1` and `uselang=en` for one further baseline/candidate pair.
Its user script only records banner identity, population, and radio count; it does not change page behavior.
These runs use the same preferences and 45-second duration, without stack sampling.

| Matched preview | Banner verified | Heartbeats | Inspections | Maximum heartbeat delay |
|---|---|---:|---:|---:|
| Baseline release | Yes, 26 radios | 44 | 3 | 497 ms |
| Candidate release | Yes, 26 radios | 44 | 3 | 22 ms |

Both processes returned zero without an external timeout.
Neither reproduced the earlier 34-second debug pause.
One live pair does not establish a stable speedup: heartbeat placement, network timing, and page work can differ.
The natural candidate banner run's 477 ms delay also limits a blanket responsiveness claim.
These are diagnostic loads, not physical interaction or donation-flow acceptance.
They verify this named banner in both release engines, without proving an exact match to the earlier unidentified debug banner.

## Reproduction

Prepare new isolated workspaces from the cached published engine source:

```sh
python3 experiments/prepare_radio_optimization.py --out target/radio-evaluation
```

Build each harness in debug and release. Preserve each executable before another build replaces it in a shared target directory.
For example, build a baseline release executable:

```sh
cargo build --locked --manifest-path target/radio-evaluation/baseline/Cargo.toml \
  -p bulwark-experiment-e3 --features servo --example bbc_probe --release
```

Run the comparison with the corresponding preserved binaries:

```sh
python3 experiments/evaluate_radio_optimization.py \
  --baseline /absolute/path/to/baseline-release \
  --candidate /absolute/path/to/candidate-release \
  --profile release --repeat 3 --out target/radio-release-results
```

Repeat with debug binaries and `--profile debug`.
The runner rejects changed validity results, incomplete workloads, missing or duplicate results, invalid timing values, and overdue processes.
It retains every completed case and raw log, including diagnostics from failed runs.

Repeat the matched live preview:

```sh
python3 experiments/compare_radio_banner.py \
  --baseline /absolute/path/to/baseline-release \
  --candidate /absolute/path/to/candidate-release \
  --banner B2627_111914_en6C_dsk_p1_lg_amt_cnt \
  --out target/radio-banner-results
```

The live runner rejects an absent or different banner and retains each command, binary hash, observation, and raw log.
`--repeat` adds pairs and alternates their order. Future server changes can make this banner unavailable.

## Checks and evidence

The final probe passed focused Clippy with warnings denied.
Workspace formatting, Python compilation, and whitespace checks passed.
Preparation reproduced the exact candidate engine source and lockfile without resolving unrelated dependencies.
Completion checks accepted one result and rejected duplicate, missing, and invalid-timing results.
The retained debug and release results also passed the final workload and validity-count guards.
The local-path engine build exposed 17 warnings in the published upstream source; those unrelated warnings remain outside this patch.
No complete engine test suite, physical acceptance round, or current Linux CI run supplied this evaluation.

Local evidence resides in `target/engine-optimization/`.
`final-debug-results/` and `final-release-results/` contain all final fixture results, commands, and binary hashes.
`live-release-final/` retains the four natural loads; `forced-banner-release/` retains the matched preview and its observation script.
The final build logs, `probe-clippy-final.log`, and `preparation-check-v2/` retain the focused validation evidence.
The preliminary release series stopped after a duplicate completion marker; its samples do not enter the final medians.
The callback now permits only one pending completion query and one published result.
The minimal engine patch also excludes formatting-only changes caught during preparation.

Final executable SHA-256 values:

| Executable | SHA-256 |
|---|---|
| Baseline debug | `31f9685dd99493c0b946cadf507fa7da7e93f02264a018d7591aa74334ab1fa2` |
| Candidate debug | `f0ee22893dbf3e38b661c24426f211939e37b1a4525c23746879b590ae2e0b58` |
| Baseline release | `4f3d848582e4910668ed43dd2961b164ef0b430f21b0afd9099409c8232581e5` |
| Candidate release | `790c113903dddbd4b31bf98789aabfc39655d6d1d08a709fea5e287d25b0bb24` |

Product `Cargo.toml` SHA-256 remains `7b5b598976d830e481ff8fbb7a2695face27633fabb49d5bb41bf4caaf52cf41`.
Product `Cargo.lock` SHA-256 remains `9c6198751af6d38f9bae7a0d9cb4a01bfd60ec2a35857f101ff99c89bbfa4438`.

## Disposition

Retain this upstream candidate as a measured partial optimization.
Prefer an upstream release containing the correction over promoting this experimental backport without broader validation.
Reducing repeated group scans remains relevant if large-group latency stays unacceptable after this change.
BBC acceptance and the five-site preference evaluation remain separate Phase 2 requirements.
The later [preference decision](phase-2-preference-evaluation.md) selects the upstream release path and records those compatibility checks.
