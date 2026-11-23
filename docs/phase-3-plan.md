# Phase 3 implementation plan

Status: P3.1 authorized and started, 2026-11-22. Broader scope decisions remain under owner review.
This document expands [Phase 3 of the development plan](development-plan.md#phase-3-deep-web-integration).
The [vision](bulwark.md) remains authoritative. Proposals here do not change accepted decisions until review.
The [P3.1 mechanism report](phase-3-mechanisms.md) records the first probes, contract drafts, and unresolved upstream requirements.

## Baseline and scope

The source baseline is `main` at `b9b0ded`, equal to fetched `origin/main` on 2026-11-22.
The [Phase 2 acceptance review](phase-2-acceptance.md#owner-acceptance-2026-11-22) records owner acceptance at product checkpoint `cf3e6be`.
Servo remains at `0.7.0`. Clojurust remains at `540e42181ad06046116f0e2007e7f0c449efaa65`.
The two AOT workarounds remain until a replacement pin passes their regressions.

Phase 3 delivers asynchronous DOM access, page hooks, extraction, transformations, custom HTML buffers, restricted extensions, and resource hooks.
Rust owns resources, transport, engine integration, and capability enforcement.
Clojure owns commands, extraction rules, transformations, extension behavior, and browser policy.
Only owned data crosses the engine/runtime boundary.

The accepted display scope remains one visible Servo webview per native window.
Text and REPL panes retain their existing egui rendering.
Custom HTML buffers must share that display scope unless a separate E5 decision changes it.
The small Linux investigation remains deferred. Existing Linux CI coverage remains required.
Profiles, persistence, downloads, packaging, extension registries, and general worker tasks remain later work.
Broader IME, clipboard, and display-scale behavior retain the Phase 2 evidence limits.

## Existing evidence and unresolved contracts

The [E4 report](dom-channel.md) proves injection, JavaScript callbacks, subtree serialization, and a resource interception exchange.
Its first trivial-call p50 was 49.292 µs. Its 5,000-element subtree p50 was 24.741 ms.
Those measurements exclude the runtime bridge and result parsing. They do not establish C6.
The JavaScript agent shares the page world. Its replies and messages remain untrusted data.

The product already has asynchronous string evaluation, retained webviews, bounded requests, and explicit close behavior.
It does not have document-scoped node handles, DOM subscriptions, extension principals, or a privileged page bridge.
The following checks must precede dependent implementation:

| Contract | Required result |
|---|---|
| Document identity | Bind a request and its callback to the actual document. Reject navigation races and late replies. |
| Page events | Deliver bounded subscription data without granting authority to a page or exhausting the ordinary event queue. |
| Principal identity | Preserve an unforgeable extension principal across native calls, awaits, callbacks, commands, and timers. |
| Evaluator reachability | Prevent access to trusted globals, native aliases, filesystem, processes, and unrestricted loaders through alternate paths. |
| Deferred decisions | Establish whether pinned Servo navigation and resource callbacks support bounded asynchronous decisions. |
| Privileged documents | Establish host-authenticated view, document, origin, and frame identity for each bridge message. |
| Custom HTML buffers | Resolve D6's E5 prerequisite while preserving the accepted display scope. |

Namespace prefixes and a restricted Clojure wrapper alone do not establish capability enforcement.
The current native registry installs closures into shared globals without a principal check.
The pinned evaluator also has a transaction policy, but that policy does not establish the required asynchronous extension boundary.
P3.1 must establish a supported mechanism or identify the required upstream work.

## Proposed API and lifecycle contracts

### Requests and node handles

The public `bulwark.dom` functions return the existing promise-like futures, consumed with `await`.
Core.async channels are not a prerequisite for this phase.
The public wrapper returns a value or raises an exception with structured error data.
The native layer retains structured replies and request identifiers.

`query` accepts an explicit web buffer or root handle and a CSS selector.
It returns one opaque node handle, or nil for no match.
`query-all` returns a bounded vector in document order.
`text`, `attr`, and `html` return data from a node handle.
`set-attr!`, `set-text!`, `remove!`, and `click!` perform mutations through the same asynchronous boundary.
The implementation defines missing-attribute, invalid-selector, detached-node, and partial-result behavior before publishing these functions.
No query silently truncates a result without explicit result metadata.

Rust issues non-reused node identifiers and records their view, document generation, and owner principal.
The JavaScript agent maps document-local references to nodes. Page-provided identifiers cannot select native resources.
Rust checks identity and capabilities before admission and before delivery of a reply.
P3.1 must also prove that JavaScript runs in the intended document during a navigation race.
A host counter alone does not prove that property.

Document replacement revokes node handles, subscriptions, and pending DOM requests before replacement content can use them.
Reload, history traversal, redirects, close, crash, and failed loads require explicit fixture coverage.
Same-document navigation preserves handles only where the adapter proves that the document remains the same.
Otherwise, conservative invalidation returns a documented error.
Operations on detached nodes return a stale-node error under the proposed contract.
The initial API targets the top document. Cross-origin frame traversal and shadow-root traversal remain outside this phase.

Each request defines a deadline, payload bounds, and cancellation behavior.
Timeout and cancellation stop result delivery. They cannot undo a mutation that the engine already performed.
Late callbacks cannot resolve another request or allocate handles in a replacement document.
JSON decoding checks schema, nesting, counts, and text size before conversion into Clojure data.
Page data never enters the Clojure source evaluator or an executable reader.

The following limits are proposals for P3.1 measurement and review:

| Resource | Proposed initial limit |
|---|---|
| Existing request transport | Retain 32 queued requests, 64 unfinished requests, 1 MiB payloads, and the five-second request deadline. |
| DOM result | At most 1,024 nodes per query and 1 MiB per reply, including its envelope. Oversized queries return an error. |
| Live node handles | At most 4,096 per document and principal. Explicit release permits long-lived consumers to recover capacity. |
| DOM subscriptions | At most 32 per view and principal, with a separate process-wide admission limit of 256. |
| Subscription batch | At most 64 records and 64 KiB. Buffer exhaustion ends the subscription with an explicit overflow result. |
| Page-data decoding | At most 32 levels of nesting. Malformed data returns a structured error. |

The [contract draft](phase-3-contracts.md) proposes a process-wide limit of 16,384 nodes.
Registry and retained-node memory measurements remain pending before adoption.
These limits do not increase the existing transport capacity or change ordinary input overflow behavior.

### Subscriptions and page lifecycle

`observe` uses MutationObserver. `on` subscribes to named DOM events.
Both return owned subscription handles with explicit unsubscribe operations.
Event data contains only selected fields, sequence information, and the document generation.
It cannot contain arbitrary host operations or executable Clojure forms.
Navigation, buffer close, extension unload, and shutdown cancel the associated subscriptions.
Repeated unsubscribe is safe. Late events cannot restore a cancelled subscription.

The subscription transport uses bounded batches and per-subscription admission limits.
If events exceed those limits, the subscription reports overflow and stops.
A consumer can query a fresh snapshot and create a new subscription.
A MutationObserver burst must not fill the existing 256-event queue and terminate ordinary browsing.
Required lifecycle events retain their explicit delivery and shutdown contract.

`:page/dom-ready` means DOMContentLoaded for a specific document, rather than Servo's existing head-parsed status.
`:page/load-finished` records the engine's completed load for that document.
`:page/unload` records host-side document revocation, including close and crash.
It does not promise delivery of a website's JavaScript unload handler.
Hooks receive generation and sequence data. Duplicate or obsolete notifications do not run hooks for a replacement document.
Host-authenticated lifecycle state remains distinct from untrusted agent notifications.

### Principals and capabilities

Trusted configuration, the internal REPL, and opt-in nREPL retain their existing authority.
Restricted extensions receive host-issued principals and explicit grants.
Untrusted page content receives no host authority from an agent reply, origin string, or message field.
Declared capabilities request authority. A host grant establishes the authority that an extension receives.

The proposed minimum capability vocabulary is:

| Capability | Authority |
|---|---|
| `:dom/read` | Read DOM data and receive bounded subscription data from granted buffers and origins. |
| `:dom/write` | Mutate granted documents. Active mutations also require `:net`. |
| `:net` | Navigate, reload, traverse history, create remote views, accept popups, and register resource hooks within granted scope. |
| `:page/script` | Evaluate raw JavaScript and install page scripts. Also requires `:net` because scripts can initiate requests. |
| `:buffer/render` | Create owned custom buffers and replace their generated content within bounded limits. |

Styles and DOM mutations can initiate network requests or navigation.
Their capability mapping must account for those effects rather than treating them as harmless text changes.
The native inventory must cover every alternate route to navigation and interception, including existing Phase 2 operations.
Any additional capability requires an explicit authority description and denial checks.

Rust records principal identity outside mutable Clojure state and caller-provided maps.
The engine request carries that identity through the owned transport.
Rust checks grants, target ownership, and origin scope at the final native boundary.
Origin scope checks cover redirects and replacement documents.
An asynchronous callback cannot inherit the trusted dispatcher's authority.
Calling a trusted helper cannot increase the caller's authority.
Unloading an extension revokes its grants and cancels owned requests, subscriptions, hooks, and timers.

A denial raises a Clojure exception and records a bounded Messages entry.
The entry identifies the principal, operation, target, and required capability without page content or credentials.
The enforcement proof includes raw native calls, captured aliases, trusted helper calls, delayed callbacks, and interleaved principals.
The proof also covers shared Var mutation, namespace lookup, `eval`, loaders, interop, and standard-library side effects.
If the shared evaluator cannot enforce this contract, an upstream mechanism or separate restricted environment becomes a prerequisite.
Full general-purpose isolate support remains a Phase 5 question. Phase 3 still requires its stated native boundary.

### Navigation, resource hooks, and privileged buffers

Navigation hooks return allow, deny, or redirect data for a host-issued pending request.
Resource hooks return allow, block, or bounded rewrite data for an authenticated resource request.
Clojure defines hook ordering and browser policy. Rust checks authority, request identity, bounds, and deadlines.
Redirects require loop limits and capability checks on each new target.
Restricted code cannot use a forged hook result to complete another principal's request.

The engine thread never waits synchronously for Clojure.
If pinned Servo supports deferred completion, the adapter retains a bounded pending decision with a deadline.
Otherwise, P3.1 records the gap before selecting an upstream change or a reviewed snapshot mechanism.
A snapshot mechanism must preserve the promised hook semantics or require an explicit scope decision.
Timeout behavior distinguishes ordinary web traffic from privileged documents, whose failures must deny access.
The proposed ordinary-traffic timeout policy allows the original request and records the missed hook decision.
That fallback never bypasses a capability denial, origin restriction, or privileged-document rule.
The owner review must settle the deadline and timeout policy before implementation.

Custom types render Clojure hiccup into HTML at host-owned `bulwark://` addresses.
The serializer escapes content and restricts executable markup. Page text cannot become script or privileged bridge markup.
Each type declares a capability ceiling. Each instance receives an owner, document generation, and explicit action table.
Bridge authority is the intersection of the owner's grant, the type's ceiling, and the instance's allowed actions.
Messages select declared actions and supply checked data. They never supply arbitrary Clojure source.

Rust authenticates the actual source view, document, origin, and frame from engine metadata.
A payload field, URL prefix, Referer value, or page-visible token alone does not authenticate a message.
Remote frames, ordinary pages, redirects, copied messages, and obsolete documents receive no privileged authority.
Privileged buffers block undeclared scripts, frames, remote resources, and navigation away from their generated document.
If pinned Servo cannot supply the required provenance, the bridge remains gated pending an upstream mechanism.

## Implementation order

Each milestone produces a reviewable change with retained evidence.
The first milestone resolves mechanisms before the public APIs depend on them.

| Milestone | Work | Required exit evidence |
|---|---|---|
| P3.1: Contracts and mechanism proofs | Extend E4 through the product bridge. Prove document identity, event transport, principal propagation, deferred decisions, and bridge provenance. Resolve the D6/E5 prerequisite. | Local fixtures, adversarial denials, latency samples, proposed native inventory, exact limits, and ADRs for selected mechanisms or upstream gaps. |
| P3.2: Read-only DOM slice | Add node registry, `query`, `query-all`, `text`, `attr`, and `html`. Add scoped native authorization before exposing restricted callers. | Real Servo reads in both evaluator modes, native bypass denials, navigation-race checks, cleanup, and initial C6 measurements. |
| P3.3: Mutations, subscriptions, and lifecycle | Add mutations, `observe`, `on`, unsubscribe, and the three lifecycle hooks. | Fixture mutations, event ordering, overload behavior, stale generations, cancellation, and close during pending work. |
| P3.4: Restricted extension loading | Add a local directory manifest, grants, restricted evaluation, and owned commands, hooks, subscriptions, and timers. | Empty-capability extension denials, alternate-call denials, interleaved async principals, unload/reload cleanup, and exceptions plus Messages entries. |
| P3.5: Navigation and resource hooks | Expose bounded allow/deny/redirect and block/rewrite decisions. Add origin scope checks and redirect limits. | Page links, forms, script navigation, history, redirects, denied interception, decision timeout, runtime stalls, and shutdown with pending decisions. |
| P3.6: Extraction and transformations | Implement article, table, link, and metadata helpers in Clojure. Add domain-scoped user content and reader mode. | Local extraction assertions, idempotent installation, reload behavior, and physical reader tasks on five agreed article sites. |
| P3.7: Custom buffer types | Add hiccup serialization, `bulwark://` delivery, the authenticated action bridge, and the title dashboard. | C5 adversarial origin/frame checks, escaped content, instance capability limits, retained hidden views, focus, resize, and close. |
| P3.8: Reference extension and acceptance | Build the HN extension through public APIs. Run C1–C6, regressions, performance measurements, and owner review. | Reproducible reports, physical Mac evidence, Linux CI, native policy review, and owner acceptance. |

P3.1 precedes all milestones. P3.2 precedes P3.3 and P3.6.
P3.4 precedes restricted hook registration and the reference extension.
P3.7 depends on the privilege and display decisions from P3.1, plus P3.4 cleanup and grants.
An unresolved security mechanism blocks the dependent milestone. Trusted-only behavior does not satisfy restricted-extension acceptance.

The manifest proposal uses `extension.edn` and `.cljrs` entry files.
It declares an identifier, entry path, capability requests, and target scopes.
The loader checks paths and symlinks against the extension directory before evaluation.
The initial loader reads local extensions only. Registry discovery and package installation remain Phase 5 work.
The owner must resolve Q6 before the source format becomes a public contract.

## Acceptance evidence

| Criterion | Required evidence |
|---|---|
| C1: HN dashboard | One Clojure-only extension under 200 source lines extracts the front page into EDN records, renders a custom dashboard, and refreshes every five minutes. Its bindings open stories in new web buffers. |
| C2: Reader mode | The command works on five owner-approved mainstream article sites. Each result retains the article title, body, and links without duplicate transformation artifacts. |
| C3: Native enforcement | An extension with no `:net` grant cannot navigate or intercept through direct native calls. Alternate operations, aliases, trusted helpers, and delayed callbacks also fail. |
| C4: Stale DOM handles | Handles fail after document replacement, including calls queued before navigation and replies received afterward. Reload and history fixtures cover generation changes. |
| C5: Privileged bridge | Ordinary pages, remote frames, forged origin fields, copied requests, and obsolete privileged documents cannot run privileged actions. Denial occurs in Rust. |
| C6: DOM latency | Release p50 for the complete sequential query-then-text operation is less than 10 ms on a simple local page. |
| Other deliverables | Local fixtures cover tables, links, metadata, user content, subscriptions, hooks, resource rewrite, custom types, and extension cleanup. |
| Accepted regressions | Relevant Phase 1 and 2 checks pass, including browsing, keyboard routing, Option/Escape policy, REPL, init reload, AOT, and bounded shutdown. |

The HN example also retains the title dashboard required by the custom-type deliverable.
The line count includes the extension's Clojure source and excludes only its EDN manifest and shared public library code.
No extension-specific logic can move into Rust or an uncounted helper to meet the limit.
A local HN fixture proves extraction and refresh deterministically. A live task records the actual URL, time, and result.
The acceptance run includes one real five-minute refresh and a separate bounded clock check for repeated scheduling.
Unloading the extension cancels the refresh timer.

The proposed reader-site set is Wikipedia, BBC News, The Guardian, Ars Technica, and MDN.
The owner review must settle that set and exact article tasks before live acceptance.
Fixtures cover missing metadata, duplicate table headers, relative links, malformed markup, and large content.
Reader extraction and site rules remain Clojure code over `bulwark.dom`.
JavaScript supplies DOM mechanisms rather than hidden article-selection policy.

C6 timing starts before the Clojure query call and ends after the awaited text result returns to Clojure.
It includes both engine round trips, scheduling, serialization, and conversion.
The proposed method uses 20 warmup operations and 200 measured operations in the normal Tiered release shell.
The report retains every sample, p50, p95, maximum, hardware, pins, fixture size, and profile state.
A separate TreeWalk run records evaluator costs. E4-only timing cannot substitute for C6.
Phase exit also records release startup and memory with ten retained web buffers under the existing performance practice.

Pure Clojure assertions run under the interpreter and standalone pinned AOT.
Real Servo fixtures run in both supported evaluator modes.
Linux CI and physical macOS evidence remain separate. Live-site checks never become network-dependent CI requirements.
The native inventory includes signatures, docstrings, grants, error data, ownership, limits, and cleanup behavior.
The phase exit review records every Rust addition and its mechanism-only purpose.

## Owner review for dependent implementation

The owner authorized P3.1. The draft retains the full Phase 3 deliverables and C1–C6.
The review needs these decisions:

1. Accept or revise the milestone order and capability model.
2. Resolve D6's E5 prerequisite for custom HTML buffers.
   The proposal proves safe coexistence and switching under one visible webview, while simultaneous web splits remain deferred.
   If D6 retains a full E5 requirement, P3.7 must wait for that experiment and its scope decision.
3. Select `.cljrs` only or `.cljc` support for local extensions, as required by Q6.
4. Agree on the five reader sites and article tasks.
5. Set ordinary navigation/resource timeout behavior and the initial manifest grant procedure.

The contract review produces concrete ADRs and bounded schemas in P3.1.
No new extension trust claim, engine pin, display scope, or failed acceptance criterion changes without a recorded decision.
