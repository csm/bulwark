# Phase 3 native contract draft

Status: proposed during P3.1, 2026-11-22. None of these new natives is installed in the product.
The [mechanism report](phase-3-mechanisms.md) identifies the unresolved engine and runtime requirements.
The [trust ADR](adr/0007-phase-3-trust-boundaries.md) records the proposed authority boundary.
The [event ADR](adr/0008-phase-3-events-and-decisions.md) records subscription and responder ownership.

## Envelope and authority

All calls use the existing asynchronous native response envelope:

```clojure
{:request/id 123 :ok true :value ...}
{:request/id 123 :ok false :error :stale-document}
```

Admission failures can retain the existing nil request identifier.
Public Clojure wrappers return the value or raise `ex-info` with the structured error data.
Rust supplies principal identity from the evaluation context. No caller-provided identity or capability field grants authority.
The request transport carries only owned data and the host-issued identity.
The engine checks grants and document scope again at execution and result delivery.

Document, node, subscription, and decision identifiers share D3's non-reused process identity rule.
An identifier's registry entry determines its kind, owner, target, and lifetime.
Callers cannot change those attributes by reconstructing a map.
The native boundary rejects an identifier of the wrong resource kind.

## Proposed native inventory

The operation names remain provisional under D11.
Each installed operation will require a signature, docstring, introspection tags, and checks in both evaluator modes.

| Operation | Request and result | Authority and ownership |
|---|---|---|
| `dom-query` | View, expected document, optional root, selector, and single/all selection return nil, one node id, or a vector. | `:dom/read`, granted target. Rust issues node ids only after a matching document reply. |
| `dom-read` | Node id, field `:text`, `:attr`, or `:html`, and optional attribute name return a string or nil. | `:dom/read`, node owner, and current document. |
| `dom-write!` | Node id, operation, and bounded arguments return nil. | `:dom/write` and `:net`, current document, and granted origin. `click!` also uses this path. |
| `dom-release!` | Owned node ids return nil. | Owner or trusted host. Repeated release succeeds without restoring an identifier. |
| `dom-subscribe!` | Node/root, named DOM event or observer fields, and selected result fields return a subscription id. | `:dom/read`, granted target, and owner. Arbitrary executable handlers are excluded. |
| `dom-drain!` | Subscription id returns a bounded batch and terminal status. | Subscription owner, current document, and current grants. |
| `dom-unsubscribe!` | Subscription id returns nil. | Owner or trusted host. Repeated cancellation succeeds. |
| `navigation-decide!` | Decision id and allow/deny/redirect data return nil. | Registered hook owner with `:net`. Redirect remains gated by frame-aware engine support. |
| `resource-decide!` | Decision id and allow/block/rewrite data return nil. | Registered hook owner with `:net`. Outbound rewriting remains gated by engine support. |
| `user-content-install!` | Granted origins, script/style data, and a registration name return an owned id. | `:net`, plus `:page/script` for scripts. Installation documents next-load versus live behavior. |
| `user-content-remove!` | Registration id returns nil. | Owner or trusted host. Existing document effects require separate documented cleanup. |
| `buffer-render!` | Owned instance id, revision, escaped HTML, and declared action data return nil. | `:buffer/render`, within the type ceiling and owner grant. Bridge activation remains gated. |
| `buffer-action-complete!` | Action request id and checked result data return nil. | Matching instance owner and declared action. Source provenance must pass before action dispatch. |

Type and hook registration belong to owned host services, with Clojure public wrappers.
Their callbacks retain the registering principal across asynchronous dispatch.
They cannot install arbitrary Clojure closures into trusted global registries before evaluator authority exists.
Extension load, unload, and grant changes are host operations. An extension cannot grant itself additional authority.

## Existing native audit

Phase 2's native inventory remains installed for trusted code.
The proposed restricted entry points need the following treatment:

| Existing surface | Restricted behavior |
|---|---|
| Create, navigate, reload, back, forward, accept popup | Require `:net`, target scope, and redirect enforcement. |
| Raw `eval-js` | Require `:page/script` and `:net`. Structured reads cannot delegate to an unrestricted script helper. |
| Info, select, close, zoom, deny popup | Require appropriate target ownership or an explicit host grant. Numeric knowledge of a handle gives no authority. |
| UI publication and key-route publication | Remain trusted shell operations. Extensions use owned command and binding services. |
| Global event retrieval | Remains a trusted dispatcher operation. Extensions receive scoped event data. |
| Internal evaluation jobs and application quit | Remain trusted-only. These operations cannot provide a restricted caller with a trusted evaluation context. |
| Log and echo | Use bounded owner-scoped services. They cannot bypass payload, rate, or retention bounds. |
| Identifier allocation and viewport reads | Require host admission limits and an explicit public use before restricted exposure. |

The runtime audit also covers standard-library file access, process execution, environment access, loaders, interop, and concurrency constructors.
The product's complete native table is not a restricted extension table.

## Result and lifetime semantics

A single query with no match returns nil. An all-query with no matches returns an empty vector.
A missing attribute returns nil. Present empty text or an empty attribute returns an empty string.
An invalid selector returns `:invalid-selector`.
An oversized result returns `:payload-too-large` or `:too-many-nodes`, without a partial success or leaked handles.
Detached nodes return `:stale-node`.
Document replacement returns `:stale-document`. A closed view retains the existing `:stale-handle` behavior.
Capability denial returns `:capability-denied` with the operation and required capability.
Cancelled, timed-out, or revoked requests cannot allocate nodes or subscriptions through late callbacks.
Mutation cancellation does not promise rollback after execution.
A failure that stays inside a child frame does not return `:stale-document` for the top document.
The engine's frame failure notice names the view's document, each failed frame, and the reason.
The host keeps that document's handles and subscriptions. It may show an error state for the frame.
The notice is not a lifecycle event and grants no authority. The [frame failure notice](phase-3-document-scope.md#frame-failure-notice) records the prototype contract.

An event batch uses this proposed form:

```clojure
{:subscription 71 :document 19 :sequence 4
 :records [{:type :child-list :target 62}]
 :status :active}
```

Terminal statuses are `:overflow`, `:cancelled`, and `:stale-document`.
A terminal result remains available through bounded host state until consumption or owner cleanup.
Node references inside events require the same owner, bounds, and lifetime checks as query results.
The host never treats a page-supplied document or sequence field as authenticated metadata.

## Bounds for implementation review

These bounds are proposed engineering budgets. Only the observer record cap has a live P3.1 implementation.
Other bounds require native and adversarial checks before adoption.

| Resource | Proposed bound |
|---|---|
| Ordinary transport | Existing 32 queued requests, 64 unfinished requests, 1 MiB payloads, and five-second deadline. |
| Query | 1,024 node results. No silent truncation. |
| Node registry | 4,096 live records per document/principal and 16,384 process-wide. Explicit release recovers capacity. |
| Subscription registry | 32 per view/principal and 256 process-wide, including terminal results. |
| Batch | 64 records, 64 KiB UTF-8, and 32 levels of decoded nesting. |
| Drain requests | One pending drain per subscription and 100 per second process-wide. |
| Pending decisions | Eight per view and 64 process-wide. Each has an explicit deadline. |
| Redirect chain | At most eight redirects, with a grant check at every target. |
| Generated HTML | At most 1 MiB per instance revision, including the action description. |

The 16,384-node limit is provisional, rather than a measured memory ceiling.
Measurement must include native registry overhead and retained JavaScript nodes on representative documents.
The engine retains only bounded responder registries. No Servo responder crosses into the runtime thread.
Origin scopes use parsed scheme, host, and port tuples. URL-prefix matching does not establish a scope.
Opaque origins and custom protocols require explicit host identity rules.

The proposed ordinary web decision deadline is 200 ms.
On timeout, ordinary traffic continues with the original request and a diagnostic entry.
Privileged traffic, capability denials, and origin restrictions always fail closed.
This policy remains subject to owner review and does not change the current product.
