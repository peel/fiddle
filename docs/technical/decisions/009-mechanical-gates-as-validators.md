# 009 — Enforce mechanical gates with exit-2 scripts, not prose gates

**Date:** 2026-07-31
**Status:** accepted

Partially supersedes [005]: the bean-body validation HARD-GATE it introduced, and the Iron Laws duplication it accepted as a cost. ADR 005's split of develop into an orchestrator plus sub-skills remains in force.

## Context

Two of fiddle's gates were mechanical checks written as prose: develop Step 1 asked the orchestrator to eyeball every bean body for an eval block, a files section, and a steps checklist, and nothing checked evaluator scorecards at all, so a truncated or mis-keyed scorecard could reach the merge and converge a bean on an unusable evaluation. ADR 008 removes the emphatic framing those prose gates leaned on, which makes the question of what actually enforces them unavoidable rather than optional.

## Decision

A check that can be expressed as a rule about text belongs in a script with an exit-code contract, not in prose asking an agent to be careful. Two validators, both exit 0 when valid and exit 2 with a JSON error array on stderr:

- `scripts/validate-bean-body.sh --body FILE [--container]` checks for a fenced eval block containing `domains:` and `criteria:`, a files section with at least one Create/Modify/Test/Delete line, and at least one checkbox step. Container feature beans are exempt. develop Step 1 runs it per bean and stops on exit 2.
- `scripts/validate-scorecard.sh --scorecard FILE --criteria-ids LIST` checks that criteria ids exactly match the bean's eval block in both directions, that every criterion and scored dimension carries non-empty evidence, that `provider` is present, that `dimensions` is an object (an explicitly empty one being valid evidence-only), and that `spec_defect` has a reason when detected. develop-loop 1f runs it before the merge; an invalid scorecard costs one re-dispatch, then the bean goes to needs-attention. It also checks the fields `check-thresholds.sh` will grade on — numeric `score` and `threshold`, string `id`, boolean `pass` — so the two validators want the same envelope and a mis-shaped card is refused where it was produced rather than three steps later. The envelope is written once in `skills/develop/scorecard-envelope.md`; both scripts name it on exit 2, because a checker that reports only the missing field leaves the reader to guess which of two conventions is canonical, and guessing produced a milestone of hand-translated scorecards.

Criteria ids arrive as an argument rather than being parsed out of YAML, matching how `resolve-domains.sh` receives `--domains`.

## Consequences

- The gates now hold on every harness. This matters most for Codex and Pi, where no Stop hook exists to catch a skipped step, and it is strictly stronger than the prose it replaces on Claude too.
- The invalid-scorecard re-dispatch policy is written down for the first time. It existed in the design spec as "existing behavior" but appeared in no skill file, so nothing implemented it.
- `validate-scorecard.sh` accepts `comment` as an alias for dimension `evidence`, because `provider-context.md` documents `comment` to external evaluators while `evaluate/SKILL.md` requires `evidence`. Discovered by running the validator against a real codex scorecard. Fixing it in the validator rather than in prose keeps one enforcement point, but the underlying field-name split between those two files is still there and should be reconciled. `evidence`/`comment` is the only alias either validator tolerates: `criterion` for `id`, `met` for `pass`, and `min` for `threshold` are named in the error and refused, because normalising them silently is the same act as hand-translating them, one layer down.
- Two costs, both real. `validate-bean-body.sh` has no documented way to derive its `--body` argument from `beans show`, so a literal-minded reader on a hook-less harness has to invent that step, and an uninvokable gate is no gate. Its files check also accepts only `## Files` or `Files:`, rejecting the bold `**Files:**` form fiddle's own plan template emits, so a bean body drafted verbatim from a plan task fails.
- `define-beans` still asks the bean author to hand-check some of what the validator now owns. Four of its seven rows are irreducibly human judgment, so the overlap is partial, but the mechanical half of that table is now duplicated against a script that can drift from it.
