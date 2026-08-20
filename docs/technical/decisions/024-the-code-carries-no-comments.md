# 024 — The code carries no comments

**Date:** 2026-08-20
**Status:** accepted
Cites: none

## Context

Comment prose had grown to 42,873 lines across `.rs`, `.sh`, `.toml`, `.nix`, `.yml`
and the Go fixtures — a maintenance surface of its own, drifting from the code it
narrated. Evaluation of M4c Task 3a spent most of its findings on stale comments:
a verdict census stating four false things, doc comments naming producers that no
longer existed, a fixture doc justifying a deleted refusal. The census at
`cve_mitigation.rs` was the stated reason for not building a lane the design
required, and a reader who trusted it would not have built it.

## Decision

This project carries no comments. Not why-comments, not rejected-alternative
notes, not caller invariants. Every form — `//`, `///`, `//!`, `/* */`, `#` in
shell, TOML, Nix and YAML, `<!-- -->` in HTML — is absent from production code,
tests, fixtures, manifests, hooks, scripts and workflow definitions.

Meaning goes into names: a function name, a type, a test name. When something
seems to need prose, that is a signal to restructure the code or rename the test.

Three things are not comments and stay:

- **Attributes and directives.** `#[allow(...)]`, `#![deny(...)]`, `#[cfg(...)]`,
  rustfmt and clippy directives. Removing them changes compilation.
- **Shebangs.** `#!/usr/bin/env bash` on line 1 is how the kernel picks an
  interpreter.
- **Program output that happens to be spelled as a doc comment.** `clap` derives
  `--help` text from `///`; that text is what an operator reads at the terminal,
  so it lives in `#[arg(help = ...)]` and `#[command(about = ...)]` instead.

Documentation under `docs/` and `.docs/` is out of scope: it is content addressed
to a reader, not commentary attached to a line of code. Anything durable that a
deleted comment was carrying belongs there, or in an ADR like this one.

## Consequences

Prose can no longer drift out of step with code it describes, because there is
none to drift. A stale explanation cannot disarm the next reader.

What gets harder: a non-obvious invariant now has to be expressed as a name, a
type, or a test that fails when it is violated. That is more work than writing a
sentence, and it is the trade being made — a test that enforces an invariant
cannot go stale without going red, and a sentence can.

Two doctests were the only executable prose in the tree. They became
`the_authorization_envelope_type_is_nameable_from_another_crate` and
`a_struct_literal_cannot_forge_an_authorization_envelope_from_another_crate` in
`crates/fiddle-runtime/tests/effect_protocol.rs`, which drive `rustc` over a
probe crate directly. `cargo test --doc` now reports zero tests, and it should
stay at zero: a code fence in a doc comment is a comment.

Setup instructions that lived only in `.env.example` and
`.github/workflows/github-effects.yml` headers are gone from those files. The
durable record is `docs/technical/effects-repository.md`.
