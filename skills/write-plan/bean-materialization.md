# Bean materialization

After saving and reviewing the plan, use `fiddle:define-beans` for sizing: one or two TDD cycles is a task; three or more becomes a feature with child task beans.

If `--epic <id>` is set, verify it with `beans show <id> --json`. Otherwise create a todo epic using the plan's goal and architecture plus any design contracts and hard constraints.

For every `### Task N:` in document order:

1. Count its failing-test cycles and choose task or feature plus child tasks.
2. Create each bean under its parent with `## Context`, exact `## Files`, the copied checkbox `## Steps`, and the copied fenced `## Evaluation` block.
3. Set `--blocked-by` for sequential child behavior and feature-level external dependencies.

Never invent evaluation criteria while materializing: return to the plan when its eval block is absent. Verify every non-container task with `scripts/validate-bean-body.sh --body <body-file>`; fix failures before continuing. Finally report the epic ID and task/feature counts.

When `--from-orchestrate` is absent, offer execution only after every bean passes its gate; otherwise return control to the caller.
