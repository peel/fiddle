# 059 — Forty turns is enough, and the brief was the bound

Status: accepted
Cites: AgentBudget, default_max_turns, run_command, MIGRATION_PREAMBLE

## Context

The step fiddle replaced ran a skill with a procedure at `--max-turns 120`. fiddle sends general advice at a default of 40. `fiddle-1z63` asked whether the capability owes a procedure and whether 40 turns is enough, and required the answer to be measured after the brief and the edit tool landed. Both have.

Two production repairs of the same advisory, runs 32648667532 and 32650396920:

| run | turns used | read_file | run_command | edit_file | run_check | largest turn |
| --- | --- | --- | --- | --- | --- | --- |
| A | 16 | 3 | 10 | 1 | 1 | 320 tokens |
| B | 12 | 2 | 7 | 1 | 1 | 349 tokens |

Neither run spent a return or a retry. The largest single answer was 349 tokens against a ceiling of 8192.

## Decision

Leave `default_max_turns` at 40 and add no procedure.

## Consequences

- A repair costs 12 to 16 turns, so the budget is spent to at most 40 percent. Raising it would buy nothing that the measurement shows a need for, and it would pay for a looping model.
- `run_command` is the most-used tool at 7 to 10 calls. It had **zero** calls across the three runs before ADR 057, because the brief said the agent could do nothing else before offering it. The scarce resource was never the turn count; it was a brief that contradicted the tool set.
- The agent found the procedure without being given one: read the manifest, run the ecosystem's own command, edit what it left, run the check. A procedure in Rust would have named `go.mod` and put Go back on the wrong side of ADR 025.
- The old 120 belonged to a different tool surface. It is not evidence about this one.
- What remains unmeasured is a repair that fails its check and has to read the failure and try again. Both measured runs passed their checks first time. A budget for that path should be measured on that path.
