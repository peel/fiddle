# 009 — Enforce a mechanical gate with an exit-2 script, not with prose

Date: 2026-07-31
Status: accepted
Partially supersedes 005: the bean-body prose gate and the Iron Laws duplication. Its split of develop into an orchestrator and sub-skills stands.
Cites: scripts/validate-bean-body.sh, scripts/validate-scorecard.sh, scripts/check-thresholds.sh, skills/develop/scorecard-envelope.md, skills/develop/provider-context.md, scripts/resolve-domains.sh

## Context

Two of fiddle's gates were mechanical checks written as prose. Develop step 1 asked the orchestrator to read every bean body, and nothing checked a scorecard at all. So a truncated or mis-keyed scorecard could reach the merge and converge a bean on an unusable evaluation.

## Decision

Put a check that can be stated as a rule about text into a script with an exit-code contract. Both validators exit 0 when the document is valid, and exit 2 with a JSON error array on stderr. Name `skills/develop/scorecard-envelope.md` on every exit 2, so no reader has to guess which convention is canonical.

## Consequences

- The gates hold on every harness. This matters most on Codex and Pi, which have no Stop hook to catch a skipped step.
- The invalid-scorecard policy is written down for the first time. An invalid scorecard costs one re-dispatch, and then the bean goes to needs-attention.
- The project gave up one field name to keep one enforcement point. `validate-scorecard.sh` accepts `comment` as an alias for a dimension's `evidence`. `provider-context.md` names `comment` as the required one, and `evaluate/SKILL.md` names neither. Both files now document both names, and the validator is still what decides.
- `validate-bean-body.sh` has no documented way to derive its `--body` argument from `beans show`. Its files check accepts `## Files` or `Files:` and refuses the `**Files:**` form fiddle's own plan template emits.
- `define-beans` still asks its author to hand-check some of what the validator owns. Four of its seven rows are human judgment. The mechanical half is now duplicated against a script that can drift from it.

`validate-bean-body.sh --body FILE [--container]` requires three things. It wants a fenced eval block carrying `domains:` and `criteria:`. It wants a files section carrying at least one Create, Modify, Test or Delete line. It wants at least one checkbox step. A container feature bean is exempt.

`validate-scorecard.sh --scorecard FILE --criteria-ids LIST` requires the criteria ids to match the bean's eval block in both directions. It requires every criterion and scored dimension to carry evidence. It requires `provider` to be present, `dimensions` to be an object, and `spec_defect` to carry a reason. It also checks the fields `check-thresholds.sh` grades on, so a mis-shaped card is refused where it was produced. It names and refuses `criterion` for `id`, `met` for `pass` and `min` for `threshold`. Normalising them silently is hand-translation one layer down. Criteria ids arrive as an argument rather than parsed from YAML, which matches how `resolve-domains.sh` receives `--domains`.
