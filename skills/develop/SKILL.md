---
name: develop
description: Use when implementing an epic's task beans through the evaluator loop — after plan and beans exist
---

# Develop — Evaluator Loop


## Usage

Invoke as `fiddle:develop --epic <id>`.

Execute an implementation plan by iterating: validate → implement per-task → holistic review → finish.

ARGUMENTS: {ARGS}

## Step 0: Validate and Setup

### 0a. Validate Epic

```bash
beans show <epic-id> --json
```

Confirm the epic exists and has child task beans. If no child beans, stop: "No task beans found for this epic. Run `fiddle:define` first."

### 0b. Worktree Setup

Use the `fiddle:worktrees` skill.

Creates an isolated worktree for the epic. All subsequent work happens in this worktree, which keeps the main branch clean.

### 0c. Read Evaluator Config

Read `orchestrate.json` from project root. Extract the `evaluators` block:

```json
{
  "evaluators": {
    "attended": false,
    "max_dispatches_per_task": 60,
    "domains": {
      "general": { "template": "evaluator-general", "providers": ["claude"] }
    },
    "holistic": {
      "providers": ["claude"],
      "max_iterations": 3
    }
  }
}
```

Store `max_dispatches_per_task` for the convergence budget. Each domain's `providers` array is an ordered preference list for selecting that domain's single evaluator (the first available provider differing from the implementer wins; implementers are always claude), not a dispatch fan-out. `evaluators.holistic.providers` (default `["claude"]`) behaves differently: holistic review dispatches to all listed providers.

## Step 1: Bean Body Validation

For each task/bug bean under the epic, run `scripts/validate-bean-body.sh --body <body-file>` (add `--container` for feature beans that are pure containers). On exit 2, stop and report the JSON errors; do not enter the loop with an incomplete bean. Implementer agents work from the bean body alone, so a thin body produces a thin implementation.

## Step 2: Per-Task Loop

Process each task bean sequentially, skipping any already marked `completed`:

Use the `fiddle:develop-loop` skill with `--bean <bean-id> --epic <epic-id>`.

The develop-loop sub-skill runs the full cycle for one bean — implementer, evaluators, scorecard merge, convergence — and returns the bean as either `completed` or `needs-attention`.

## Step 3: Holistic Review

Once every task bean is processed (completed or escalated), run holistic review, and do not invoke finish-branch until it has converged or been escalated. Per-task scores say nothing about cross-domain coherence; only a whole-system pass catches it.

Use the `fiddle:develop-holistic` skill with `--epic <epic-id>`.

The develop-holistic sub-skill assesses the system as an integrated whole, creates remediation beans if needed, and iterates until it converges or escalates.

## Step 4: Completion

Use the `fiddle:finish-branch` skill.

User picks: merge, PR, keep, or discard. Worktree cleanup happens here.

## Restart Resilience

On session restart, develop re-derives state entirely from beans — all evaluation history lives on bean bodies, so there is no session-scoped state to lose:

1. List the epic's task beans via `beans list --parent <epic-id> --json`
2. For any bean with `in-progress` status, use `fiddle:develop-loop` with `--bean <id> --epic <epic-id>` — the loop detects its own restart state via parse-eval-log.sh + assess-git-state.sh
3. Skip `completed` beans; process remaining `todo` beans normally
4. Once all task beans are processed, check whether holistic review already ran by looking for `scorecard-holistic.json` and the holistic history file. If it is unfinished or never started, use `fiddle:develop-holistic` with `--epic <epic-id>`

## Harness Enforcement (Claude Code)

The skill-encoded loop above is the cross-harness baseline. On Claude Code, harness mechanisms additionally enforce it:

- **Stop hook (preferred, automatic).** Ships in `hooks/hooks.json` (`develop-verdict-gate.sh`). While the `.fiddle/active-bean` marker names a bean without a recorded terminal verdict, the hook blocks turn-end so the loop continues. Terminal states: CONVERGED, or needs-attention via SPEC_DEFECT / BLOCKED / DISPATCHES_EXCEEDED. Deterministic, no judge model; fails open when no marker exists. The marker lifecycle (arming and clearing) is owned by develop-loop; see `skills/develop-loop/SKILL.md`.
- **/goal (manual equivalent).** When the Stop hook is unavailable, phrase the goal condition against recorded verdicts, including the escalation exits: "the active bean has a recorded terminal verdict: CONVERGED, or needs-attention via SPEC_DEFECT / BLOCKED / DISPATCHES_EXCEEDED". A goal phrased only as "converged" fights the dispatch budget, pushing iterations after the loop has legitimately escalated.
- **/loop (optional outer watchdog).** `/loop` re-firing `Skill("fiddle:develop", args: "--epic <epic-id>")` on an interval guards against a stalled or dead session, and is idempotent via Restart Resilience above. It is a time-based session guard, not a driver for the inner cycle, which is verdict-driven and enforced by the Stop hook or /goal.

Codex and Pi harnesses have none of these mechanisms; they keep the skill-encoded loop via the `fiddle:using-fiddle` harness mapping.
