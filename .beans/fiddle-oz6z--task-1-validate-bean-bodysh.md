---
# fiddle-oz6z
title: 'Task 1: validate-bean-body.sh'
status: todo
type: task
tags:
    - branch
created_at: 2026-07-30T11:20:24Z
updated_at: 2026-07-30T11:20:24Z
parent: fiddle-85jh
---

Plan: docs/plans/2026-07-30-claude5-slimdown.md Task 1

## Context

Repo: /Users/peel/wrk/fiddle. Part of epic fiddle-85jh (Claude-5 skill slim-down). See the epic body for shared Contracts and the spec docs/specs/2026-07-29-claude5-slimdown-design.md for the House Style and Prompt-Side Invariant Set that govern every rewrite.

## Files

- Create: `scripts/validate-bean-body.sh`
- Test: `scripts/test-validate-bean-body.sh`

## Steps

- [ ] Write failing test (assert_exit/assert_json harness, mktemp fixtures): (1) complete body (fenced ```eval with `domains:` and `criteria:`, a `## Files` section with a `- Create:`/`- Modify:`/`- Test:` line, a `- [ ]` checklist) → exit 0; (2) missing eval block → exit 2, JSON error on stderr naming "eval block"; (3) eval block without `criteria:` → exit 2; (4) missing files section → exit 2 naming "files"; (5) no checkbox steps → exit 2 naming "steps"; (6) container feature bean flag `--container` → exit 0 regardless (exempt); (7) missing --body file → exit 2.
- [ ] Run test, verify failure (script missing).
- [ ] Implement: `validate-bean-body.sh --body <file> [--container]`. Greps: fenced eval block containing `domains:` and `criteria:`; `## Files`-or-`Files:` with at least one Create/Modify/Test line; at least one `- [ ]`. All failures collected into one JSON error array on stderr, exit 2. Header comment documents exit codes 0/2. chmod +x.
- [ ] Run test green; full sweep `for t in scripts/test-*.sh; do bash "$t" >/dev/null 2>&1 || echo "FAIL: $t"; done` clean.
- [ ] Commit (PREK_ALLOW_NO_CONFIG=1; imperative title, Previously/Now body).

## Evaluation

```eval
domains: [infrastructure]
criteria:
  infrastructure:
    - id: complete-body-passes
      check: "A body with fenced eval block, files section, and checklist exits 0"
    - id: each-gap-named
      check: "Missing eval block, criteria key, files section, or steps each exit 2 with a JSON error naming the gap"
    - id: container-exempt
      check: "--container exits 0 regardless of body content"
thresholds: {}
```
