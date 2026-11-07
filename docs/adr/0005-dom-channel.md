# D5: JavaScript agent and DOM transport

Status: accepted for the initial channel, 2026-11-06. Product security remains Phase 3 work.

The DOM API will call an injected JavaScript agent through Servo's public `evaluate_javascript` API.
The agent returns small values directly and larger data as JSON strings.
The engine receives each result through the associated callback and replies through the runtime bridge.
DOM operations remain asynchronous across the engine boundary.

E4 passed script injection, trivial calls, serialization of 5,000 elements, and a resource interception exchange.
The trivial call had a 49.292 µs median in the first debug run.
The [experiment report](../dom-channel.md) records the measurements and their limits.

Resource interception remains a possible transport for page events.
It does not authenticate a page or permit privileged operations by itself.
The injected agent shares the page's JavaScript world and remains untrusted.
The product must enforce capabilities in Rust.

Node handles will include a document generation and become invalid after navigation.
E4 does not implement those handles or establish a secure privileged bridge.
