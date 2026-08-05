# Bean materialization

## Create Beans from Plan

After the plan is saved and self-reviewed, materialize it as beans. `fiddle:develop` runs `scripts/validate-bean-body.sh` on every bean and stops on exit 2, so beans created outside this shape fail validation before the loop starts.

### Step 1: Load the Bean Sizing Rules

Use the `fiddle:define-beans` skill.

This loads the sizing heuristic (1–2 TDD cycles → task bean; 3+ cycles → feature bean with child task beans), the bean body template (Files / Steps / Evaluation sections), and the Bean Body Completeness Gate. Apply these rules to every plan task.

### Step 2: Resolve the Epic

If `--epic <id>` was provided, reuse that epic. Verify it exists and is type `epic`:

```bash
beans show <id> --json
```

Otherwise, create a new epic from the plan's header (Goal, Architecture) and any `## Contracts` / hard-constraints sections from the design doc:

```bash
beans create --json "<feature-name from plan title>" -t epic -s todo -d "$(cat <<'EOF'
Plan: <plan-path>

<Goal sentence from plan header>

## Architecture
<Architecture paragraph from plan header>

## Contracts
<contracts captured from design doc, if any>

## Hard constraints
<constraints captured from spec, if any>
EOF
)"
```

Capture the epic ID for the next step.

### Step 3: Materialize Each `### Task N:` as Beans

Iterate through every `### Task N:` heading in the plan in document order. For each:

1. Count the TDD cycles in the task's checklist (each "Write the failing test" step is one cycle). Apply the sizing rule from `fiddle:define-beans` to choose **task** vs. **feature + children**.
2. Create the bean(s) under the epic with `--parent <epic-id>`. The body takes this exact shape:
   - `## Context` — repo path + a sentence on what/why
   - `## Files` — the `Files:` block from the plan task copied verbatim (paths only, one per line, prefixed `- Create:` / `- Modify:` / `- Test:`)
   - `## Steps` — the plan task's `- [ ]` checklist copied verbatim, including code blocks
   - `## Evaluation` — the fenced ` ```eval ` block copied verbatim from the plan task
3. Wire `--blocked-by` for any sequential dependencies between behaviors of a feature, and feature-level `--blocked-by` for cross-task dependencies (per `fiddle:define-beans` rules).

If a plan task has no fenced ` ```eval ` block, stop and add one to the plan first. Eval criteria invented during bean creation are criteria the spec never agreed to.

### Step 4: Run the Completeness Gate

After all beans are created, list children of the epic and verify each task bean body passes the Bean Body Completeness Gate from `fiddle:define-beans` (steps exist and are actionable; eval block present; files specified; sufficient context). Feature beans that are pure containers are exempt.

Fix any failing body inline. Do not exit Step 4 until every task bean passes.

### Step 5: Report

Print a summary line:

```
Epic <epic-id> ready: <N> task beans, <M> feature beans, all gate-checked.
```

## Execution Handoff

If `--from-orchestrate` was set, return control to the caller without prompting the user.

Otherwise, once the plan is saved, self-reviewed, and critiqued, offer execution:

**"Plan complete and saved to `docs/plans/<filename>.md`. Epic `<epic-id>` ready with `<N>` beans. Ready to execute?"**

When execution begins:

- Use fiddle:develop to implement the plan task-by-task
- Fresh subagent per task + evaluation between tasks
