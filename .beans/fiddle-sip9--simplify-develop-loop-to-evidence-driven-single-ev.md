---
# fiddle-sip9
title: Simplify develop-loop to evidence-driven single evaluator per domain
status: todo
type: epic
priority: normal
tags:
    - orchestrate-phase:DEFINE
created_at: 2026-07-28T12:39:50Z
updated_at: 2026-07-28T12:52:01Z
parent: fiddle-rwyx
---

## Context

Observation from real usage: adversarial multi-provider code reviews in develop-loop are frequently a waste of time and rarely change the outcome within 1-2 runs. Research into Dex Horthy's software factory pattern (ace-fca, Pragmatic Engineer interview, "Why Software Factories Fail") and the LLM self-correction literature explains why: judge-model errors are correlated with generator errors ("if the model knew good code, it would write it"), so same-class adversarial review adds little. Review iterations pay off only when they inject new evidence (test runs, runtime probes, invariant checks, plan-conformance diffs), not when another model re-reads the same diff and emits opinions.

## Design Decisions

1. **Keep the cycle.** Loop until it does not fail: fresh implementer -> gather evidence per domain -> single evaluator per domain interprets evidence -> thresholds/convergence -> on FAIL re-dispatch fresh implementer with failing evidence as guidance. Dispatch budget and escalation exits (SPEC_DEFECT, BLOCKED, DISPATCHES_EXCEEDED) unchanged.
2. **One evaluator per domain per iteration** (was: per domain x per provider). Per-task domain focus is retained and is the important bit: the bean's eval block `domains:` list still resolves via resolve-domains.sh to template, calibration, antipatterns, and runtime config, so the domain keeps determining which evidence is gathered.
3. **Cross-provider preference.** The domain `providers` array changes semantics from "dispatch all" to an ordered preference list. Pick the first available provider that differs from the provider that ran the implementer; if none available, fall back to the implementer's provider with a fresh context. Provider availability comes from the existing session-start detection hook.
4. **Evaluator role repositioned:** plan-conformance and evidence interpretation on verifiable criteria (tests, invariants, runtime behavior), not open-ended quality judgment. Maintainability/architecture judgment lives upstream in DEFINE plan review and at finish-branch, not in the loop's scorecard.
5. **Drop the provider-reconciliation machinery:** per-provider scorecard merge, conservative min-across-providers scoring, and disagreement tracking are no longer needed. Cross-domain merge stays.
6. **Consider relaxing two-consecutive-passes:** double-pass exists to damp judge noise; with evidence-backed verdicts, re-evaluating unchanged code re-measures the same facts. Option: require double-pass only for judgment-scored dimensions, single pass for evidence-backed criteria (saves one dispatch per bean in the common case).

## Files

- Modify: skills/develop-loop/SKILL.md (steps 1f, 1g-1h; dispatch accounting text "2 providers x 2 domains = 4 dispatches")
- Modify: skills/develop-loop/scorecard-merge.md (remove provider merge, keep cross-domain merge and spec-defect check)
- Modify: scripts/merge-scorecards.sh + tests (single-provider input path)
- Modify: scripts/resolve-domains.sh docs/semantics if needed (providers array meaning)
- Modify: skills/develop/SKILL.md and skills/orchestrate/SKILL.md (evaluators config contract, examples)
- Modify: README.md (multi-provider scoring section)
- Test: scripts/test-merge-scorecards.sh, scripts/test-multi-provider.sh (rework to preference-list selection)

## Open Questions

- Does holistic review (develop-holistic) keep multi-provider dispatch? It runs once per epic, so the cost argument is weaker there.
- Does the freed adversarial budget move upstream to DEFINE panel (where critique of a small plan artifact has more headroom)?
- Backward compatibility for existing orchestrate.json files that list multiple providers per domain (interpret as preference order, likely safe).

## Steps

- [ ] Rework develop-loop 1f to single evaluator per domain with cross-provider preference selection
- [ ] Simplify scorecard-merge.md and merge-scorecards.sh to cross-domain-only merge
- [ ] Reposition evaluator templates toward evidence interpretation and plan conformance
- [ ] Update dispatch accounting and budget examples throughout
- [ ] Decide and implement single-pass convergence for evidence-backed criteria
- [ ] Update orchestrate/develop/README docs for the new providers semantics
- [ ] Rework provider-related test scripts

## Harness Enforcement (Claude Code)

The iterate-until-pass guarantee should move from prompt discipline to the harness where available:

- **Stop hook (preferred, automatic):** ship a Stop hook in hooks/hooks.json that reads the active bean's eval-log verdict (parse-eval-log.sh / check-convergence.sh output) and refuses to end the turn unless a terminal state is recorded: CONVERGED, or needs-attention via SPEC_DEFECT / BLOCKED / DISPATCHES_EXCEEDED. Deterministic, no judge model. 12-factor #8 applied at the harness layer.
- **/goal (user-typed alternative):** condition phrased against recorded verdicts, e.g. "bean <id> eval log records CONVERGED or the bean is marked needs-attention". Note: the goal evaluator is a small model judging the transcript only (no tools), so it is grounded only because HARD-GATEs paste script output into the turn. Must include the escalation exits in the condition or it fights the dispatch budget. /goal is a built-in command; skills cannot set it programmatically.
- **/loop (outer watchdog only):** time-driven and session-scoped (7-day expiry, restored on --resume), wrong driver for the condition-driven inner cycle; useful as a self-healing supervisor re-firing fiddle:develop --epic <id>, which is idempotent via restart recovery.

Codex/Pi harnesses keep the skill-encoded loop as fallback via the using-fiddle harness mapping.

- [ ] Add Stop-hook verdict gate for develop-loop (Claude harness)
