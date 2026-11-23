# D12: Phase 3 document and extension authority

Status: proposed after P3.1 probes, 2026-11-22. Product integration remains gated.
The [mechanism report](../phase-3-mechanisms.md) records evidence and reproduction commands.
The [contract draft](../phase-3-contracts.md) records proposed native operations and bounds.

## Evidence

The injected agent shares the page world. The page can replace its operations and forge its data.
Top-document and same-origin iframe fetches can produce identical public resource metadata in the same view.
The public custom-protocol request exposes richer source metadata, which the final probe distinguishes across four frame contexts.
That alternative still needs a binding to the active privileged document and rejection of obsolete messages.
Servo's public evaluation request takes a view identifier without an expected document identifier.
Its callback returns a value without the executing document's identity.
A successful navigation request does not mean that the replacement document reached the complete state.

The current product runtime exposes native functions and trusted globals across namespaces.
The P3.1 counterexample reproduces navigation, an alias after await, trusted-state mutation, and file access from an extension namespace.
A declaration of an empty capability set changes none of those results.
These are counterexamples to a proposed restriction, rather than regressions in the accepted trusted-only shell.

## Proposed decision

Document identities must originate in the engine, rather than the JavaScript agent.
Scoped evaluation must compare the expected identity at execution, before any mutation occurs.
Each reply must identify its executing document. Obsolete replies cannot allocate handles or deliver results.
Close, crash, replacement, and history activation require explicit identity transitions.
Same-document changes require an explicit identity-preservation contract.

Page-origin messages remain data until the host authenticates the source document and frame.
A request URL, claimed origin, referrer, shared-world secret, or agent identifier cannot establish authority.
The privileged action bridge remains unavailable until the engine supplies source provenance.
Generated `bulwark://` HTML alone grants no authority.
The custom protocol retains an empty `privileged_paths` list and receives no Servo internals API.

Extension authority must originate in Rust and survive every asynchronous continuation.
The native boundary must evaluate the caller's authority, including calls through trusted helpers or captured aliases.
An extension cannot mutate trusted globals or reach unrestricted standard-library effects through alternate evaluation paths.
The existing namespace-prefix proposal in D8 does not establish those properties on the current pin.
Restricted loading requires a supported evaluator authority mechanism or an independently restricted environment.
The implementation must prove that alternative before selecting it. A second default runtime alone does not establish restriction.

## Consequences

The ordinary JavaScript callback channel remains the D5 transport choice.
It does not satisfy the document-identity or privileged-bridge contract without additional engine support.
Trusted diagnostics can continue through existing APIs. They cannot count as restricted-extension acceptance.
Public restricted DOM operations and extension loading remain gated by their relevant proofs.
The current engine and runtime pins remain unchanged.

The required interfaces are specified in the [mechanism report](../phase-3-mechanisms.md#required-upstream-and-integration-contracts).
No upstream issue, patch, or release adoption is claimed by this ADR.
