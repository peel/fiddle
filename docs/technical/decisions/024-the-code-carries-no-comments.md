# 024 — The code carries no comments

Date: 2026-08-20
Status: accepted
Cites: crates/fiddle-runtime/src/effect/mod.rs::AuthorizedEffect, crates/fiddle-runtime/tests/effect_protocol.rs::the_authorization_envelope_type_is_nameable_from_another_crate, crates/fiddle-runtime/tests/effect_protocol.rs::a_struct_literal_cannot_forge_an_authorization_envelope_from_another_crate, docs/technical/effects-repository.md

## Context

Comment prose had grown to 42,873 lines across the sources and the Go fixtures. It was a maintenance surface of its own, drifting from the code it narrated. Evaluation of M4c Task 3a spent most of its findings on stale comments, including a census stating four false things.

## Decision

Carry no comments in this project. Remove every form from production code, tests, fixtures, manifests, hooks, scripts and workflow definitions. Put meaning into a function name, a type or a test name instead.

## Consequences

- Prose can no longer drift out of step with the code it describes, because there is none to drift. A stale explanation cannot disarm the next reader.
- The project gave up the cheap sentence. A non-obvious invariant now has to be a name, a type or a test that goes red. That is more work, and a test enforcing an invariant cannot go stale silently.
- Two doctests moved into `effect_protocol.rs`, where they drive `rustc` over a probe crate directly.
- Setup instructions that lived only in `.env.example` and the `github-effects.yml` header are gone from those files. `docs/technical/effects-repository.md` is the durable record.
- The removal is incomplete. `AuthorizedEffect` still carries the two `///` fences the tests above replaced. So `cargo test --doc` reports 2 tests rather than 0.

## Which forms, and what replaces them

Every form goes: `//`, `///`, `//!`, `/* */`, `#` in shell, TOML, Nix and YAML, and `<!-- -->` in HTML. When something seems to need prose, that is a signal to restructure the code or rename the test.

Three things are not comments and stay.

**Attributes and directives.** `#[allow(...)]`, `#![deny(...)]`, `#[cfg(...)]`, and rustfmt and clippy directives. Removing them changes compilation.

**Shebangs.** `#!/usr/bin/env bash` on line 1 is how the kernel picks an interpreter.

**Program output that happens to be spelled as a doc comment.** `clap` derives `--help` text from `///`, and an operator reads that text at the terminal, so it lives in `#[arg(help = ...)]` and `#[command(about = ...)]` instead.

Documentation under `docs/` and `.docs/` is out of scope. It is content addressed to a reader, not commentary attached to a line of code. Anything durable that a deleted comment carried belongs there, or in an ADR like this one.

## What the removal missed

This ADR asserted that `cargo test --doc` reports zero tests and should stay at zero, because a code fence in a doc comment is a comment. Measured at this head, it reports two. `crates/fiddle-runtime/src/effect/mod.rs` still carries a `///` block above `AuthorizedEffect`, holding the plain fence and the `compile_fail` fence that the two named tests were written to replace. The replacements landed and the originals did not leave, so the same property is now asserted twice, once by a test and once by a comment this decision forbids.

One further hit is not a comment. `crates/fiddle-acceptance/tests/support/mod.rs` holds a `//` sequence inside a string literal, which is fixture text a test writes to disk rather than commentary on a line of code. A reader grepping for `//` should expect it.
