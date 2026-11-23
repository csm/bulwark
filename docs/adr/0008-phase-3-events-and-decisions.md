# D13: Bounded page events and deferred decisions

Status: proposed after P3.1 probes, 2026-11-22. Product integration remains pending.

## Proposed decision

DOM subscriptions use explicit bounded drains over the existing asynchronous request/reply transport.
An observer or event handler appends selected data to its own bounded page-side batch.
A drain returns records, sequence data, document identity, and terminal overflow state.
Page data cannot directly enqueue host lifecycle events or select host operations.
The host checks all returned data, including text size, record count, nesting, principal, and document identity.

Batch exhaustion disconnects the subscription and records terminal overflow.
The overflow result remains available until the owner consumes it or cancels the subscription.
No silent truncation or automatic observer restart occurs.
The consumer can obtain a fresh snapshot and subscribe again.
The existing fatal input-event overflow contract remains unchanged.

Drains use fair bounded admission, with at most one pending drain per subscription.
There is no continuously runnable polling loop and no direct observer feed into the ordinary event queue.
The initial proposed active-drain interval is 50 ms, with a process-wide rate limit of 100 drains per second.
Idle subscriptions retain no pending drain request. Activation requires a bounded owner request or scheduled service turn.
The production scheduler must demonstrate fair progress and idle CPU behavior before adoption.
The P3.1 JavaScript probe proves only a 64-record batch and terminal overflow.
It does not establish the production subscription scheduler or hostile-page memory bounds.

Servo's navigation and resource responders remain on the engine thread.
Owned decision data crosses the runtime bridge. Engine callbacks never await Clojure synchronously.
A bounded registry retains each responder until a decision, deadline, cancellation, or shutdown consumes it.
Cleanup explicitly denies a navigation that requires denial. Dropping a Servo navigation responder implicitly allows it.
Dropping an unintercepted resource permits the original request to continue.
Those drop defaults cannot implement a privileged denial policy.

Allow and deny retain the original target and responder.
Frame-aware navigation redirection and outbound request rewriting require additional mechanisms.
Replacing a resource response does not rewrite its outbound request.
Denying an iframe navigation and loading its URL in the top view does not preserve the original target.
Neither replacement counts as the requested API without a recorded scope decision.

## Display scope

The P3.1 probe serves HTML through a registered `bulwark` protocol using Servo's public reexports.
The probe switches between that view and a retained web document under one visible view.
It does not prove native input, simultaneous web splits, or a privileged bridge.

The proposed D6 clarification permits custom HTML buffers under the accepted one-visible-view scope.
Full E5 remains mandatory for simultaneous visible web content.
Owner review of this clarification remains pending. The probe does not change D6 by itself.
