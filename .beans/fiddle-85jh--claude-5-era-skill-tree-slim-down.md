---
# fiddle-85jh
title: Claude-5-era skill tree slim-down
status: todo
type: epic
priority: normal
tags:
    - orchestrate-phase:DEVELOP
created_at: 2026-07-30T11:19:41Z
updated_at: 2026-07-30T14:07:23Z
---

Plan: docs/plans/2026-07-30-claude5-slimdown.md
Spec: docs/specs/2026-07-29-claude5-slimdown-design.md

Rewrite all 51 skill files to the Claude-5 judgment-plus-rationale house style: strip HARD-GATE markup, caps emphasis, rationalization tables, Red Flags, Announce lines; keep ordering and honesty invariants as single plain statements with rationale; move the two remaining mechanical gates into exit-2 validators; dedup the config schema.

## Architecture

Two validators (validate-bean-body.sh, validate-scorecard.sh) with test suites land first, then staged family rewrites gated by the full test sweep.

## Contracts

- validate-bean-body.sh --body FILE [--container]: exit 0 valid, exit 2 with JSON error array on stderr. Container features exempt.
- validate-scorecard.sh --scorecard FILE --criteria-ids LIST: exit 0 valid, exit 2 with JSON error array. Criteria ids passed as arg, no YAML parsing, mirroring resolve-domains.sh --domains.
- House Style and Prompt-Side Invariant Set are defined in the spec and are authoritative for the rewrite.

## Hard constraints

- Frontmatter description fields preserved verbatim, the trigger selector.
- Every cross-skill handoff pointer preserved.
- All test-*.sh suites and portability checks green at every family checkpoint.

## Holistic Review Record (2026-07-30)

CONVERGED in 2 iterations, both PASS with no regressions and no remediation beans: integration 8/7, coherence 7/7, holistic_spec_fidelity 8/8, polish 7/6, runtime_health 9/9.

Verified: every enumerated House Style removal is at zero tree-wide (HARD-GATE 47 to 0, GATE 49 to 0, Red Flags 10 to 0, Announce 5 to 0, Do NOT 45 to 0, MUST 44 to 1), the single MUST survivor being brainstorm's frontmatter description which the spec preserves as the trigger selector. All 29 description fields byte-identical to main. Handoff pointer targets equal by set on both sides. All ten Prompt-Side Invariants survive an adversarial hook-less-weak-model read: each pairs a declarative invariant plus rationale with an imperative in the flow, so none degraded from instruction to trivia. Tree 38549 to 34270 words (-11.1%), with the only two growths being the two the spec ordered to grow. 17/17 suites green; both validators driven live across 9 input cases and portable to bash 3.2.

Weak rows accepted rather than remediated (flagged for human judgment): deliver-docs and discover-docs retain ## Rules appendices restating their own flow, the visible remainder of a convention the epic established by removing four of six such blocks; and -11.1% sits at the low end of substantial, mitigated by the remainder being largely non-compressible interface text.

Follow-ups worth a cheap pass if reopened: deliver-docs:24 and :34 name a non-existent evolve skill as the authority on doc structure while the doc schema is duplicated with discover-docs:15-47; the stale HARD-GATE comment at scripts/test-multi-domain-holistic.sh:456 (its exact parallel at :247 was updated); a superseding ADR for 005, which deliver-docs produces naturally when it runs; validate-bean-body.sh's <body-file> has no documented derivation from beans show, and its files check rejects the bold **Files:** form the plan format uses.

Correction to iteration 1: its 25-vs-25 pointer-target count does not reproduce under any interpretation; parity holds by set equality instead (19/19, 33/33, 26/26 depending on reading). Recorded because the number was unverifiable evidence for a claim that is in fact true.
