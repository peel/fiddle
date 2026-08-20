# 011 — Constrain an invocation reference value at the parse boundary

Status: accepted
Cites: InvocationRef::slug, InvocationRefError::IllegalValueCharacter, InvocationRefError::Malformed, InvocationRefError::UnknownScheme, InvocationRefError::EmptyValue, crates/fiddle-acceptance/tests/inspect_ref.rs

## Context

`InvocationRef::slug` promised a path-safe rendering, and `FromStr` checked only the separator and the scheme. Every path fiddle derives interpolates that slug: the report bundle, the attempt journal and the stub port reads. So `fiddle run 'beans:../../../pwned'` wrote `report.json` outside the report directory, and the stub ports read outside the stub root.

## Decision

Validate the value once, at the parse boundary in `fiddle-core`, against ASCII letters, digits, `-`, `_` and `:`. Reject anything else as `InvocationRefError::IllegalValueCharacter`, and exit 2 before any filesystem access. Admit `:`, so that `jira:ICE-1:sub` keeps parsing.

## Consequences

- Path containment is a property of the type rather than of each call site. Holding an `InvocationRef` proves the value is safe to interpolate, and a later derived path inherits that.
- The project gave up sanitising at each use site. There were three derived paths already, and per-site sanitising makes every new one a fresh vulnerability.
- The class admits ASCII only. An external source may produce a non-ASCII identifier, which now fails at parse. Somebody should confirm the real jira and scanner formats before those adapters land.
- The four rejection diagnostics stay pairwise distinct, which `inspect_ref.rs` asserts.

The slug is `<scheme>-<value>`, so the `beans-` prefix absorbs one `..` and the two-dot form stays inside the directory. Escaping takes three, which is why a shallower reproduction looks harmless. The vector was harmless while a local operator was the only source of a reference. It became live at M1, when a `jira`, `scheduled` or `scanner` reference arrives from an external system.
