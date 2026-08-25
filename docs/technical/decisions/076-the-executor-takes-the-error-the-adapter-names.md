# 076 — The executor takes the error the adapter names

Status: accepted

Cites: AdapterError, EffectPhase, IntegrationOperation::Error, EffectError::Adapter, adapter_source, RetryAdvice, crates/fiddle-runtime/tests/adapter_boundary.rs

## Context

`IntegrationOperation::inspect` and `apply` returned `Result<_, GhError>`, so
every effect reported a GitHub error whatever it talked to. M5 adds a Jira
effect. The milestone's purpose is that a non-GitHub capability costs one
struct, an async inspect, an async apply, one outcome classification and one
registry line, with no edit to `fiddle-core`.

A first attempt added `type Error: AdapterError` to the trait and then pinned
`Error = GhError` at every consumer: the executor's three bounds,
`read_until_settled`, and the two capability helpers. The associated type was
declared and made inert in the same change, and holistic review caught it.

## Decision

`IntegrationOperation` carries `type Error: AdapterError`, and the executor and
the capability helpers are generic over it rather than pinning it.

`AdapterError::outcome` takes an `EffectPhase` of `Inspect` or `Apply` and has
no default. An error type that answered by omission would opt itself out of the
classification that decides the run's exit row.

The three facts the executor read out of the concrete enum became `AdapterError`
methods with safe defaults: `advice()` for backoff, `is_worth_reading_again()`
for the settling read, and `duplicates()` for the duplicate-state mapping.

`EffectError::Adapter` carries `Box<dyn AdapterError>`, so `EffectError` stays a
concrete enum, and `EffectError::adapter_source::<E>()` reads the adapter's own
error back. `RetryAdvice` moved from `github/cli.rs` to `effect/mod.rs`, so an
adapter outside GitHub imports nothing from the github module.

An unclassified failure is `Unknown`, never `NotCommitted`. `NotCommitted` is
the claim that permits a retry, and a landed write reported as not committed is
retried into a duplicate.

## Consequences

`tests/adapter_boundary.rs` drives an operation whose `Error` is `JiraError`
through `Executor::execute`, so the contract is executable rather than asserted.
Restoring the pin is `error[E0271]` at nine sites.

`EffectError` no longer names the concrete adapter error at the type level. A
caller that wants it downcasts through `adapter_source::<E>()` and handles the
`None` case. That is the cost of keeping `EffectError` concrete while the
adapter error varies, and it was preferred to making `EffectError` generic,
which would have rippled through every capability and every receipt.

Three of the four new `AdapterError` methods carry defaults, which is the same
hazard the no-default rule on `outcome` exists to prevent, at lower stakes: a
new adapter that ignores them gets conservative behaviour rather than a wrong
classification.
