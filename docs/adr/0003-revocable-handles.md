# D3: Explicit resource handles

Status: accepted contract, 2026-10-27. Implementation remains Phase 1 work.

Rust owns native resources in registries. Clojurust receives opaque identifiers.
An explicit close operation invalidates an identifier.
Subsequent operations on that identifier return a structured error.

Registry identifiers must prevent an old handle from referring to a new resource.
Generation counters or identifiers that never repeat can satisfy this requirement.
The first registry implementation will select one mechanism and include a stale-handle test.

Native resource cleanup does not depend on Clojurust GC finalizers.
DOM node handles will also carry a navigation lifetime during Phase 3.
