# 072 — The only documentation is for the agents building this

Status: accepted
Cites: config_check_refuses_a_sweep_document_that_run_cve_would_refuse, config_check_accepts_a_document_that_run_cve_can_run, unrunnable_cve

## Context

`docs/fiddle-agentic-factory-prd.md` ran to 1780 lines and called itself the contract: "when implementation and this document disagree, this document is the contract". Six acceptance lanes read TOML out of it and asserted the schema matched.

`fiddle-ue6x` then found that the document the manual offers as **copyable** declares `[orchestration.cve]` and omits `workspace.fixture`. Copying it gives a deployment that passes `config check` and refuses at `run cve`. The contract was wrong about the one thing a reader would do with it.

Fixing that would have been one line. The question it raised is why a document nobody outside this project reads was being kept in step with the schema by six tests.

## Decision

Keep documentation for the agents building this, and nothing else.

Removed: the manual, `docs/product/` (VISION, MARKET, PRICING, GTM, FEEDBACK), and the six lanes that policed the manual, with their helpers.

Kept: the ADRs, SYSTEM.md, RUNBOOKS.md, style.md, BACKLOG.md, the evaluator calibration, and the milestone plans. Each of those is read by something that acts on it.

## Consequences

- The ADRs are now the only record of why. There is no second place for a decision to disagree with them.
- `config_check.rs` loses six lanes and thirteen helpers, and gains two that ask the question the workflow actually relies on: can `run cve` run this document. That is a smaller file testing a truer thing.
- ADR 021 still refers to a lane that read the manual, and to the manual. That record stands as written; this one says where the manual went.
- Nothing now checks that a configuration example outside the repository is loadable, because there is no such example. A deployment's own `fiddle.toml` is checked by `config check --capability cve`, which is the check that matters.
- If this project ever needs a reader outside it, the manual is in the history.
