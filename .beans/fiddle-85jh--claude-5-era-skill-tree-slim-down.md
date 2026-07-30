---
# fiddle-85jh
title: Claude-5-era skill tree slim-down
status: todo
type: epic
created_at: 2026-07-30T11:19:41Z
updated_at: 2026-07-30T11:19:41Z
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
