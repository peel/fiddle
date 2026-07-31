# Evaluator Context Loading Order

Every evaluator, claude or external, receives its context in this order.

Positions 1, 2, 3, and 8 are static: they come from files named in `orchestrate.json` and are assembled by `scripts/assemble-evaluator-context.sh --domain <name>`, which develop-loop 1f runs for both provider paths. Positions 4 through 7 depend on run state and are the caller's to append after that output.

1. **Evaluation protocol** — `skills/evaluate/SKILL.md`.
2. **Domain template** — `skills/evaluate/evaluator-<domain>.md`, from the resolved domain's `template` field, falling back to `evaluator-general` so an unconfigured domain is still scored against something.
3. **Project calibration** — `evaluators.domains.<domain>.calibration`, or the default `docs/evaluator-calibration-<domain>.md` that the attended gate (1i) and the blind spot-check (deliver 5.0) write. Position 3 is load-bearing: the anchors have to be in context before the evaluator forms its own scale, or the project's corrections do not apply to the scores it produces.
4. **Runtime evidence** (runtime-configured domains) — `skills/runtime-evidence/SKILL.md` plus runtime state (port, domain).
5. **Runtime and stack agents** (if configured) — the files named by the domain's `runtime_agent` or `stack_agents`.
6. **Task criteria** — the bean's acceptance criteria and the domain template's scoring dimensions.
7. **Prior scorecards** (iteration 2+) — the diff since BASE_SHA (`git diff {BASE_SHA}...HEAD`), the previous scorecard, and its guidance.
8. **Antipatterns** — `evaluators.domains.<domain>.antipatterns`, or the default `docs/antipatterns-<domain>.md`. These fill the `{ANTIPATTERNS}` placeholder inside the protocol rather than trailing the pack, so they are read last but land at position 1's placeholder.

Calibration and antipattern content is truncated at any `## Retired` heading. Retired entries are kept for audit by deliver 5g and encode judgment against an evaluator and model version that have since moved, so they must not reach a live evaluator.
