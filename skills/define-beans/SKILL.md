---
name: fiddle:define-beans
description: Bean sizing rules for writing-plans. Determines when a plan task should become a feature with child task beans based on TDD cycle budget.
---

# Define Beans

## When to Use

Called by writing-plans during "Create Beans from Plan" step. Evaluates each `### Task N:` in the plan and determines whether it should be a single task bean or a feature bean with child tasks.

## Sizing Rule

An automated implementer agent gets ~50 turns per bean. Each TDD cycle (write failing test, implement, verify, commit) costs ~8-10 turns including codebase exploration and build issues.

| TDD cycles in task | Bean type | Structure |
|---|---|---|
| 1-2 | task | Single task bean under the epic |
| 3+ | feature | Feature bean under the epic, with child task beans (1 per behavior) |

## How to Count TDD Cycles

Each distinct testable behavior is one TDD cycle. Count the number of "write a failing test for X" steps in the plan task. If the task says "write tests for X, Y, and Z" — that's 3 cycles, not 1.

## Decomposing into Feature + Tasks

When a plan task needs 3+ cycles:

1. Create a **feature** bean for the group:
   ```bash
   beans create "Task N: <group title>" --json -t feature -s todo -p <priority> --parent <epic-id> --tag branch -d "Plan: <plan-path> Task N

   <overall goal from the plan task>"
   ```

2. Create a **task** bean per behavior under the feature. The body must pass the completeness gate:
   ```bash
   beans create "Task Na: <specific behavior>" --json -t task -s todo --parent <feature-id> --tag branch -d "Plan: <plan-path> Task N, step group a

   ## Context

   Repo: <absolute path to repo>
   <1-2 sentences explaining what this is and why, enough for an agent with no prior context>

   ## Files

   - Create/Modify: <exact paths relative to repo root>
   - Test: <exact test file path>

   ## Steps

   - [ ] Write failing test for <behavior> — <what the test asserts, expected failure message>
   - [ ] Run test, verify it fails: <exact command>
   - [ ] Implement: <what to add/change, key logic, code snippets from plan>
   - [ ] Run tests, verify pass: <exact command>
   - [ ] Commit

   ## Acceptance Criteria

   - <observable, verifiable criterion 1>
   - <observable, verifiable criterion 2>"
   ```

3. Chain children with `--blocked-by` where one behavior builds on another. Independent behaviors need no ordering.

4. Set the feature's own `--blocked-by` to external dependencies (other tasks/features from the plan that must complete first).

## Bean Body Completeness Gate

After creating each task bean, verify its body passes this gate before moving on. If it fails, fix the body inline — do not proceed to the next bean.

**Gate: An agent with zero context can implement this bean by reading only its body.**

Check each requirement. All must pass:

| # | Check | Fail if |
|---|---|---|
| 1 | **Steps exist** | Body has no `## Steps` section or no `- [ ]` checkboxes |
| 2 | **Steps are actionable** | Any step says "see plan", "as above", "similar to Task N", or lacks concrete instructions |
| 3 | **Acceptance criteria exist** | Body has no `## Acceptance Criteria` section |
| 4 | **Acceptance criteria are verifiable** | Any criterion is vague ("works correctly") rather than observable ("returns 200 on /health/db") |
| 5 | **Files are specified** | Body references code changes but has no `## Files` section listing exact paths |
| 6 | **Repo is specified** | Work spans multiple repos and the body doesn't say which repo to work in |
| 7 | **Context is sufficient** | Body references concepts, modules, or patterns the implementing agent won't know without explanation |

The bean body should reference the plan path for additional context (`Plan: <path> Task N`), but must not **depend** on the plan to be implementable. The plan is supplementary — the bean is the contract.

## Shared Contracts (for parallel beans)

When an epic has multiple features/tasks that will run in parallel worktrees and touch related code, define shared contracts upfront in the **epic bean body** before creating children:

- **Types and interfaces:** Function signatures, struct definitions, interface contracts that multiple beans will implement or call
- **Integration points:** Which package exports what, expected function names, shared constants

Include a `## Contracts` section in the epic bean body. Each child bean's description should reference it: `"See parent epic contracts for shared types."` This prevents parallel workers from making incompatible implementation choices.

## Dependencies

- **Between children of the same feature:** Use `--blocked-by` between task beans when one behavior depends on another's code.
- **Between features:** The feature bean itself carries `--blocked-by` to external dependencies. When the feature is activated, its ready children become workable.
- **Cross-feature child dependencies:** Avoid. If task 3a depends on task 2c, make feature 3 depend on feature 2 instead.

## Example

Plan task with 6 TDD cycles:
> ### Task 2: Union-Find TTL, Cleanup, and Concurrency
> (write test for cleanup, implement cleanup, write test for Close, implement Close, write test for StartCleanup, implement StartCleanup, write test for ExtendTTL, implement ExtendTTL, write test for MemoryUsage, implement MemoryUsage, write test for memory cap, implement memory cap)

Becomes:

```
Feature: "Task 2: TTL, Cleanup, and Concurrency"  (parent: epic)
  Task: "Task 2a: Forest.Cleanup mark-and-sweep"   (parent: feature)
  Task: "Task 2b: Forest.Close stops goroutine"     (parent: feature)
  Task: "Task 2c: Forest.StartCleanup periodic"     (parent: feature, blocked-by: 2a)
  Task: "Task 2d: Forest.ExtendTTL per shard"       (parent: feature)
  Task: "Task 2e: Forest.MemoryUsage estimate"      (parent: feature)
  Task: "Task 2f: Memory cap with LRA eviction"     (parent: feature, blocked-by: 2e)
```

Plan task with 1 TDD cycle stays as-is:
> ### Task 1: Core Union-Find Node struct

Becomes a single task bean — no feature wrapper needed.
