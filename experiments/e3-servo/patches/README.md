# Radio validation evaluation patch

`servo-0.7-radio-unrooted.patch` adapts [Servo PR #49855](https://github.com/servo/servo/pull/49855) to the published `servo-script` 0.7.0 package.
The upstream merge is `d4067b91beceeac11172c4ed1401c602d8338041`.
The modified engine source retains its MPL-2.0 license.

The patch replaces the inner radio-group validation walk with an unrooted iterator under `NoGC`.
It preserves the group filtering and missing-value calculation.
The iterator API is named `traverse_preorder_non_rooting` in 0.7.0.
That version lacks the upstream patch's additional `update_indeterminate_state` call site, so this adaptation does not add it.

This patch is an experiment. Product Cargo manifests do not reference it.
The [engine evaluation](../../../docs/phase-2-engine-optimization.md) records the builds, controls, and limitations.
