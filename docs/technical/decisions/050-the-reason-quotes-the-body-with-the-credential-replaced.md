# 050 — The reason quotes the provider's body, with the resolved credential replaced

Status: accepted
Amends 012, which stands. Its gateway choice and its budget gap are untouched.
Cites: crates/fiddle-runtime/src/gateway.rs, Redaction, Redaction::excerpt, Gateway, completion_model, REDACTED, agent::provider_fault, agent::classify, scripts/tier2.sh, crates/fiddle-acceptance/tests/binary_repair.rs, a_preserved_body_that_echoes_the_credential_is_quoted_with_it_replaced, a_preserved_body_that_echoes_no_credential_is_quoted_whole, a_preserved_body_is_withheld_when_the_credential_is_unknown, a_gateway_that_echoes_the_credential_is_quoted_with_the_credential_replaced, a_gateway_that_echoes_no_credential_is_quoted_whole

## Context

`provider_fault` received the provider's response body and dropped it. Every provider failure reported a status and nothing else.

Run 32595349852 failed after six minutes of work. Its whole record was "the gateway answered 400 Bad Request". Five probe rounds then guessed what the body said. Each probe returned 200 against the real gateway, so no probe named the cause. The request shape is not the fault, and the discarded sentence was the only remaining evidence.

ADR 012 states the reason for the drop: "an error body is where a credential echo surfaces". The hazard is real. A gateway answers a bad key with the key. Two tests pinned the rule. This record replaces both, and the replacements keep the same hazard closed.

The rule withheld the body because part of it might be a secret. fiddle resolves `[agent] api_key`, so it holds the exact string. An exact replacement is available, and a guess is not needed.

## Decision

The reason quotes the body. `Redaction` holds the credential, and it does three things in order.

1. It replaces every occurrence of the credential with `[redacted]`.
2. It bounds the result to 240 characters and marks a cut with `…`.
3. It quotes the result with Rust's debug escape, so a body cannot forge a second line.

The order matters. A cut before a replacement can leave a prefix of the credential in the text.

`completion_model` returns `Gateway`, which carries the model and the redaction together. One read of the credential builds both. A caller cannot build the client and forget the redaction.

The status stays. It is useful without a body, and `tier2.sh` and an operator both read it.

## The unknown credential prints nothing

`Redaction::excerpt` returns `None` when the redaction holds no credential. `provider_fault` then prints the status and the sentence "fiddle holds no credential to redact, so it withholds the body".

This project chose to print nothing rather than to print an unredacted body. A path with no credential cannot tell whether the body carries one. It also cannot tell an operator that it checked. The sentence says which of the two happened, so a missing excerpt is never read as an empty body.

An empty credential is treated as unknown. An empty string matches every position, so a replacement over it marks nothing and hides nothing.

Every call site that holds no credential today is a test that drives a mock model. The production path always holds one, because `model_client` resolves the credential before it builds the client.

## Why the replacement is exact and not a pattern

A pattern over text an adversary chooses is not a guarantee. A pattern that misses one credential reads in the log exactly like a guard that held.

`a_gateway_that_echoes_no_credential_is_quoted_whole` is the test for this. Its gateway echoes `sk-a-key-this-run-does-not-hold-71bd`, and the test asserts that the string reaches the reason whole. A guard written as `sk-[a-z0-9-]+` would replace it and redden that test.

`GitCli::redact` and `GhCli::redact` use the same exact replacement for their own tokens. This record adds a third site and no new technique.

## Consequences

- The reason for run 32595349852's failure shape now reads:
  `the gateway answered 401 Unauthorized: "{\"error\":{\"code\":\"invalid_api_key\",\"message\":\"Incorrect API key provided: [redacted]. …\"}}"`.
- `tier2.sh` printed `reason[:300]`. The prefix of a provider reason is 109 characters, so 300 cut the excerpt it exists to show. It now prints `reason[:2048]`, which is `PUBLISHED_TEXT_LIMIT`, the bound the payload already carries. The JSON record always held the whole reason.
- The bound is 240 characters, and `snippet` in `scanner/wizcli.rs` uses 120 for a scanner's stderr. A gateway wraps its message in a JSON envelope, and 120 characters can end inside the envelope. 240 is a second number for the same job, and this record accepts that cost.
- `Redaction` does not derive `Debug`. A derived `Debug` would print the credential, which is the one thing the type exists to stop. `a_redaction_never_renders_the_credential_it_holds` pins it.
- The credential now reaches one more struct. Four capability configs carry a `Redaction`, and `ToolHost` carries none. A model's declared command never travels beside it, which is the separation ADR 046 argued for `WorkspaceCommand`.
- `attempt` and `attempt_briefed` take one more argument. A caller must name a redaction, so a new call site cannot inherit silence by default.

## The rejected option

`docs/BACKLOG.md` proposed "a scrubber registered with the resolved credential at the one place it is read", and called it a process-wide mutable registry.

A registry needs no new argument and no new field. It also hides the dependency. A reader of `provider_fault` could not tell whether a credential was registered, and a test could not set two different states in one process. A registry that nothing set would withhold every body while looking like it worked.

The explicit argument costs thirteen call sites and one field on four configs. It buys a compiler error for every path that has no answer, which is what the unknown-credential arm needs to be honest about.

## What the tests prove, and what they do not

`a_gateway_that_echoes_the_credential_is_quoted_with_the_credential_replaced` and `a_gateway_that_echoes_no_credential_is_quoted_whole` differ by one input: the key the stub gateway echoes. The first asserts the marker in the reason, the raw credential absent from stdout, from stderr, and from every published file. The second asserts the echoed key reaches the reason whole.

Both drive the compiled binary through a real HTTP client, so they measure the production path for a status and a body together.

The unknown-credential arm has a unit test and no acceptance test. No production path reaches it, so no acceptance scenario can express it without a stub model. `a_preserved_body_is_withheld_when_the_credential_is_unknown` is the whole coverage.

No test proves that a real gateway echoes a credential. The stub does it because a real one can.
