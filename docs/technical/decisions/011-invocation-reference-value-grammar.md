# 011 — An invocation reference value is constrained at the parse boundary

Status: accepted

## Context

`InvocationRef::slug()` documented itself as "a path- and filename-safe rendering", but `FromStr` validated only that a `:` separator was present and the scheme was known, accepting the value verbatim. Every path `fiddle` derives — the report bundle, the attempt journal added by ADR 010, and the stub port reads — interpolates that slug.

`fiddle run 'beans:../../../pwned'` therefore wrote `report.json` outside `<report.dir>`, and the stub ports read through `<stub.root>/work/../../../pwned.json`. Note the arithmetic: the slug is `<scheme>-<value>`, so the `beans-` prefix absorbs one `..` and the two-dot form stays inside — it takes three to escape, which is why a shallower reproduction looks harmless.

Harmless while a local operator is the only source of references. A live arbitrary-write vector from M1, when `jira`, `scheduled` and `scanner` references arrive from external systems.

## Decision

The value is validated once, at the parse boundary in `fiddle-core`, against a documented character class: ASCII letters and digits, `-`, `_`, and `:`. Anything else is rejected with a fourth `InvocationRefError` variant, `IllegalValueCharacter`, rendered through miette like its three siblings and exiting 2 before any filesystem access.

`:` is admitted deliberately so `jira:ICE-1:sub` — a pre-existing accepted form whose value itself contains a separator — keeps parsing.

Sanitising at each use site was rejected. There were already three derived paths and M1 adds more, so per-site sanitising makes every new derived path a fresh vulnerability.

## Consequences

Path containment is now a property of the type rather than of each call site: holding an `InvocationRef` is proof the value is safe to interpolate, and a future derived path inherits that without its author having to know.

The class is ASCII-only. M1's external sources may produce identifiers containing non-ASCII characters, which would now be rejected at parse. That is the safe default for path derivation, but it needs confirming against real jira and scanner identifier formats before those adapters land, rather than being discovered as a field failure.

The four rejection diagnostics remain pairwise distinct, which `crates/fiddle-acceptance/tests/inspect_ref.rs` asserts.
