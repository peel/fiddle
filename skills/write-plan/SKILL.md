---
name: write-plan
description: Use when you have a spec or requirements for a multi-step task, before touching code.
---

# Writing Plans

Invoke as `fiddle:write-plan [--from-orchestrate] [--epic <id>]`.

Turn an approved design into a self-contained implementation plan and validated beans. Use a dedicated worktree. `--from-orchestrate` suppresses the interactive handoff; `--epic <id>` reuses an existing parent epic.

## 1. Scope and structure

Split independent subsystems before planning. Map file responsibilities and plan only changes that can be independently understood and tested. Follow [plan document format](plan-format.md) for the required header, task structure, evaluation blocks, self-review, and external critique.

## 2. Materialize beans

After the plan is saved, self-reviewed, and critiqued, follow [bean materialization](bean-materialization.md). Every task bean must pass its completeness gate before execution begins.

## 3. Handoff

With `--from-orchestrate`, return after validated bean creation. Otherwise offer execution only after the plan and all beans are ready, then invoke `fiddle:develop` to implement task-by-task with fresh implementers and evaluation between tasks.
