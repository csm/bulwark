# D3: Explicit resource handles

Status: accepted for Phase 1, reviewed 2026-11-07. Implementation remains Phase 1 work.

Rust owns native resources in registries. Clojurust receives opaque identifiers.
An explicit close operation invalidates an identifier.
Subsequent operations on that identifier return a structured error.

Registry identifiers must prevent an old handle from referring to a new resource.
The initial registries use monotonic identifiers from one process-wide allocator.
Identifiers never repeat within a process, including after view or window replacement.
Identifier exhaustion returns a structured error. The allocator never wraps or reuses an identifier.
The registry implementation must cover stale handles and exhaustion without allocating every possible identifier.

Native resource cleanup does not depend on Clojurust GC finalizers.
DOM node handles will also carry a navigation lifetime during Phase 3.
