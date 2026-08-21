# 040 — The endpoint may be named, and a credential still cannot be written

Status: accepted
Cites: WrittenOrNamed, named_variable, EnvRef::visit_str, NamedValueAbsent, resolve_named, model_client, OrchestrationCve::image, crates/fiddle-acceptance/tests/binary_repair.rs::a_named_endpoint_that_resolves_to_nothing_refuses_and_reaches_no_gateway, config_check_reports_the_variable_that_names_the_endpoint

## Context

`snowplow-incubator/snowplow-identities` must commit a `fiddle.toml`. The user decided that the configuration is a versioned file. A committed document is reviewable in a pull request, it is diffable, and `config check` runs against the bytes that ship. All four credential fields already take `{ env = "NAME" }`, and `EnvRef::visit_str` refuses a written credential. So a committed document exposes no credential.

`[agent] base_url` accepted only a written string. The host cannot publish its gateway URL, so that one field blocked the whole document. The endpoint is not a credential, and 43 places in this repository write one.

## Decision

Add a second form for `[agent] base_url` alone, as `WrittenOrNamed`. A written string stays legal, and `{ env = "NAME" }` names a variable instead. Leave `EnvRef` exactly as it is, so a credential still cannot be written. Refuse a named endpoint that is unset, empty or blank as `NamedValueAbsent`, at the point where a capability builds its model client. Report the variable in `config check`, and never the resolved value.

## Consequences

- The change is additive. Every written `base_url` still loads, and the manual's own bytes still pass the `config_check` lane.
- The refusal is at the point of use, not at load. `config check` accepts a document whose variable nothing exports, exactly as it accepts one whose credential nothing exports. The schema check reads the document, and the run reads the environment.
- `WrittenOrNamed` and `EnvRef` share `named_variable`. Both forms name a variable by one rule, and only the written case differs between them.
- The `config check` payload's `base_url` is now a string or a table. A reader must ask which form the document used. The plain rendering pays the same price as `agent.base_url.env = NAME`, which is the key an operator goes and sets.
- What the project gave up: one form for one field. A field with two forms is a field a later reader can get wrong, and no other field gains the second form here.
- The refusal on an empty value applies ADR 039's rule rather than making a new one. A named value that resolves to nothing is a document the build cannot act on.

## Why `EnvRef` was not widened

ADR 012's "The credential and the configuration" states the promise: `api_key` deserializes only from `{ env = "NAME" }`, and a document carrying a literal value fails to load. `EnvRef` serves four fields. Widening it would make a written credential legal in all four, to make one field that is not a credential nameable. The refusal is the whole value of that type, and the new type carries no such refusal because it guards no secret.

## `[orchestration.cve] image` keeps one form

Recorded because the question was asked and answered, and no code changed.

`image` is the near neighbour. It has no default, it has no guess, and the host's value is a tag the host's own workflow builds. It keeps a written value, for three reasons.

- A tag is committable in a way a gateway URL is not. `ghcr.io/<org>/<repo>:<tag>` names the publishing repository, which that repository already knows about itself.
- ADR 020 already owes this field a stronger change, and a second form is not it. That record says "The record carries the digest, not the tag", and its losing alternative was a field declaring the revision the image was built from, checked against the checkout. `docs/BACKLOG.md`'s 2026-08-18 entry owns that half. A second form added now would grow the schema sideways where a stronger design is already owed.
- No caller asks for it. ADR 020 warns that "A field no caller sets is either off by default, asserting nothing, or refuses every existing run".

One case changes this answer: a host workflow that builds a per-run tag, such as a tag keyed by the commit. Then the document cannot carry the tag, and the choice is between this same second form and the declared-revision field ADR 020 describes. That choice needs the host's workflow in hand. Guessing it is how a schema grows sideways.
