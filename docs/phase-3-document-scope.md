# P3.1 document scope prototype

Status: experimental S1 slice. The product keeps Servo 0.7.0 and its current lockfile.
The [mechanism contract](phase-3-mechanisms.md#s1-engine-document-identity-and-scoped-evaluation) remains open.

## Mechanism

The [engine patch](../experiments/phase3/patches/servo-0.7-document-scope.patch) adds a document identity to the existing evaluation channel.
The identity contains a pipeline and an engine-issued incarnation.
A checked counter allocates each incarnation without reuse within its process.
The pipeline distinguishes different processes.
Page data does not supply either field.

`request_document_identity` returns the active top document's identity through an empty script evaluation.
`evaluate_javascript_in_document` carries the expected identity through the constellation to the script thread.
The constellation rejects a different active pipeline.
Immediately before execution, the script thread rejects a different incarnation or an inactive document.
A successful result includes the identity that executed the script.

Document construction allocates an incarnation.
A successful `document.open()` reset allocates another incarnation before the engine clears the DOM.
Fragments and `pushState` preserve the incarnation.
The script thread checks the identity again after result serialization because getters can reset the document.
The constellation also rejects successful replies from pipelines that it no longer considers active.

The original public evaluation signature remains available for the negative control.
The prototype changes its internal result channel and can reject replies after replacement.
This patch is an experiment against published sources, not an upstream submission or a product dependency override.
The modified engine files retain their MPL-2.0 license headers.

## Regression scope

The local fixture supplies every resource through interception.
The probe requires no network service or remote website.
Fixture markers establish test order, without authentication claims.

| Case | Required assertion |
|---|---|
| Initial document and result | Navigation replaces the initial `about:blank` identity. A successful result carries the expected identity. |
| Same-document navigation | Fragment changes and `pushState` preserve the identity. |
| Held request | A request for A enters the engine after B activates. The engine rejects it before any mutation in B. |
| Negative control | The original unscoped API executes that same mutation in B. |
| Queued reset | Two queued requests name one incarnation. The first resets the document. Both replies fail, and the second mutation never runs. |
| Pipeline collision | `document.open()` changes the incarnation while its pipeline remains equal. |
| Serialization reset | A result getter resets the document. The engine rejects the result. |
| Delayed host delivery | A successful callback retains its obsolete identity after a later reset. The host still needs a revocation check. |
| Reload and history | Reload replaces the identity. History selects an identity distinct from the departed document and rejects the departed identity. |
| Different views | Each view rejects the other view's document identity. |

The probe groups these assertions into nine cases.
The runner repeats them three times and retains the existing Servo contract and E4 baseline results.

## Local evidence

All nine cases passed in three runs on macOS arm64 with software rendering.
The existing Servo contract probe and E4 baseline also passed against the patched engine.
History restored the previous document identity in all three runs.
The delayed-delivery case retained a successful reply from the document before its reset.
That result demonstrates the remaining need for a host revocation check.

The [retained evidence](../experiments/phase3/document-scope-evidence-2026-11-22.json) includes the results, logs, source hashes, and binary hash.
All 61 portable tests, portable Clippy, formatting, and Python syntax checks passed.
The patched E4 probe also passed Clippy with warnings denied for the probe.
The initial sandboxed graphics run failed during macOS surface initialization.
The successful graphics runs used normal macOS graphics services outside that sandbox.
Servo emitted its known GL texture warning and debug linker warning.
The local engine copies also emitted warnings from the published source under disabled features.
No remote CI, physical input, Linux, or release latency result applies to this checkpoint.

## Remaining requirements

This prototype does not close S1 or permit a restricted DOM API.
Activation and revocation notifications remain absent.
The host still needs identity-tagged DOM-ready and load-complete events.
A reply can become obsolete after the constellation check but before the host consumes it.
Document resets also need revocation messages, including resets after serialization.
The host must reject those replies before it allocates node handles.

The next S1 slice must cover delayed callbacks, redirects, failed loads, close, crash, and cancellation.
History activation needs an explicit revocation policy for retained documents.
The initial `about:blank` regression covers replacement, not every script-created initial document.
Other replacement paths, including JavaScript URLs, need an audit and dedicated fixtures.

S2 still needs the complete current document identity on protocol messages.
The reset regression proves that a matching protocol pipeline alone cannot distinguish two incarnations.
This patch does not authorize a privileged action bridge.
Evaluator authority, redirect/rewrite mechanisms, and owner scope decisions also remain open.

## Reproduction

Prepare copies of the six published Servo packages and an isolated Bulwark workspace:

```sh
python3 experiments/prepare_document_scope.py --out target/phase3-document-scope
```

Build the candidate:

```sh
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="$PWD/target" cargo build --offline --locked \
  --manifest-path target/phase3-document-scope/candidate/Cargo.toml \
  -p bulwark-experiment-e4 --features document-scope
```

Run the candidate with independent 35-second process deadlines:

```sh
python3 experiments/run_document_scope.py \
  --binary target/debug/bulwark-experiment-e4 \
  --preparation target/phase3-document-scope \
  --output target/phase3-document-scope-evidence
```

The preparation refuses an existing output directory.
It records the patch hash, original source hashes, and product manifest hashes in `provenance.json`.
Only the candidate manifests reference the copied packages.
The runner records the binary hash, probe hashes, assertion markers, exit statuses, and log hashes.
The `document-scope` feature requires the patched candidate and cannot compile against the product engine.
The normal E4 feature and the portable workspace retain their existing build commands.
