---
name: develop
description: Use when implementing an epic's task beans through the evaluator loop — after plan and beans exist
---

# Develop — Evaluator Loop


## Usage

Invoke as `fiddle:develop --epic <id>`.

Execute an implementation plan by iterating: validate → implement per-task → holistic review → finish.

**Announce:** "I'm using fiddle:develop to implement this epic via the evaluator loop."

ARGUMENTS: {ARGS}

## Iron Laws

Read and internalize: `skills/develop/iron-laws.md`

## Rationalization Prevention

| Rationalization | Reality |
|---|---|
| "Only one task, skip holistic" | Holistic catches integration issues invisible to per-task eval |
| "All beans passed, holistic will too" | Per-task scores say nothing about cross-domain coherence |
| "Worktree setup is overhead" | Worktree protects main branch. Non-negotiable. |
| "Bean bodies look fine, skip validation" | Thin bodies produce thin implementations. Validate. |

## Step 0: Validate and Setup

### 0a. Validate Epic

```bash
beans show <epic-id> --json
```

Confirm the epic exists and has child task beans. If no child beans → stop: "No task beans found for this epic. Run `fiddle:define` first."

### 0b. Worktree Setup

Use the `fiddle:worktrees` skill.

Creates an isolated worktree for the epic. All subsequent work happens in this worktree.

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

Store `max_dispatches_per_task` for the convergence budget. Store `domains` for evaluator dispatch: each domain's `providers` array is an ordered preference list for selecting the single evaluator for that domain (the first available provider differing from the implementer wins; implementers are always claude), not a dispatch fan-out. Store `evaluators.holistic.providers` for holistic review dispatch (default: `["claude"]`); holistic review dispatches to all listed providers.

## Step 1: Bean Body Validation

<HARD-GATE>
Before entering the per-task loop, validate EVERY task/bug bean under the epic.
For each bean, the body MUST contain:
  1. An eval block (fenced ```eval block, or `domains:` + `criteria:` pattern)
  2. A files section (at least one line matching `- Create:`, `- Modify:`, `- Test:`, or `Files:`)
  3. A steps checklist (at least one `- [ ]` checkbox item)

If ANY bean fails validation, STOP. Report which beans failed and what is missing:
  "Bean <id> has an incomplete body. Implementer agents work from the bean body alone —
  thin bodies produce thin implementations.
  Missing: [eval block | files section | steps checklist]
  Fix the bean body before proceeding."

Do NOT enter the per-task loop with incomplete beans.
Do NOT silently skip validation.
Feature beans that are purely containers for child task beans are exempt.
</HARD-GATE>

## Step 2: Per-Task Loop

Process each task bean sequentially. For each bean:

Use the `fiddle:develop-loop` skill with `--bean <bean-id> --epic <epic-id>`.

The develop-loop sub-skill handles the full evaluation cycle for one bean: dispatch implementer, dispatch evaluators, merge scorecards, check convergence, iterate until converged or budget exceeded.

Each bean returns as either `completed` or `needs-attention` (escalated). Skip beans already marked `completed`.

## Step 3: Holistic Review

After all task beans are processed (completed or escalated):

Use the `fiddle:develop-holistic` skill with `--epic <epic-id>`.

The develop-holistic sub-skill assesses the full system as an integrated whole, creates remediation beans if needed, and iterates until the holistic review converges or is escalated.

<HARD-GATE>
Holistic review is mandatory. Do NOT skip to Step 4.
Do NOT invoke finish-branch before holistic review has CONVERGED or been escalated.
</HARD-GATE>

## Step 4: Completion

Use the `fiddle:finish-branch` skill.

User picks: merge, PR, keep, or discard. Worktree cleanup happens here.

## Restart Resilience

On session restart, develop re-derives state entirely from beans:

1. List epic's task beans via `beans list --parent <epic-id> --json`
2. Find any bean with `in-progress` status
3. For in-progress beans: use `fiddle:develop-loop` with `--bean <id> --epic <epic-id>` — the loop handles its own restart detection via parse-eval-log.sh + assess-git-state.sh
4. Skip already-`completed` beans
5. Process remaining `todo` beans normally
6. After all task beans are processed, check if holistic review already ran by looking for `scorecard-holistic.json` and holistic history file. If in progress or not started, use `fiddle:develop-holistic` with `--epic <epic-id>`

No session-scoped state to lose. All evaluation history lives on bean bodies.

## Harness Enforcement (Claude Code)

The skill-encoded loop above is the cross-harness baseline. On Claude Code, harness mechanisms additionally enforce it:

- **Stop hook (preferred, automatic).** Ships in `hooks/hooks.json` (`develop-verdict-gate.sh`). While the `.fiddle/active-bean` marker names a bean without a recorded terminal verdict, the hook blocks turn-end so the loop continues. Terminal states: CONVERGED, or needs-attention via SPEC_DEFECT / BLOCKED / DISPATCHES_EXCEEDED. Deterministic, no judge model; fails open when no marker exists. The marker lifecycle (arming and clearing) is owned by develop-loop; see `skills/develop-loop/SKILL.md`.
- **/goal (manual equivalent).** When the Stop hook is unavailable, set a goal whose condition is phrased against recorded verdicts and includes the escalation exits: "the active bean has a recorded terminal verdict: CONVERGED, or needs-attention via SPEC_DEFECT / BLOCKED / DISPATCHES_EXCEEDED". A goal phrased only as "converged" fights the dispatch budget: it keeps pushing iterations after the loop has legitimately escalated.
- **/loop (optional outer watchdog).** `/loop` re-firing `Skill("fiddle:develop", args: "--epic <epic-id>")` on an interval guards against a stalled or dead session. It is idempotent via Restart Resilience above: each firing re-derives state from beans, skips completed work, and resumes in-progress work. It is NOT a driver for the inner cycle. The watchdog is time-based and session-scoped, while the per-bean cycle is verdict-driven and enforced by the Stop hook or /goal.

Codex and Pi harnesses have none of these mechanisms; they keep the skill-encoded loop via the `fiddle:using-fiddle` harness mapping.
