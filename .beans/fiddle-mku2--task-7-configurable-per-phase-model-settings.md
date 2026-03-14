---
# fiddle-mku2
title: 'Task 7: Configurable per-phase model settings'
status: todo
type: task
priority: high
tags:
    - worktree
created_at: 2026-03-14T19:22:02Z
updated_at: 2026-03-14T19:22:02Z
parent: fiddle-9qn1
---

Plan: docs/plans/2026-03-14-orchestrate-panel-integration.md Task 7

Files:
- Modify: skills/orchestrate/SKILL.md (Configuration section + DEVELOP ralph spawn)
- Modify: skills/panel/SKILL.md (3 model references)
- Modify: skills/ralph-subs-implement/SKILL.md (2 model references)
- Modify: skills/ralph-subs-implement/roles/review-coordinator.md (2 model references)
- Modify: skills/ralph-beans-implement/SKILL.md (2 model references)
- Modify: skills/ralph-beans-implement/roles/review-coordinator.md (2 model references)

Steps:
1. Add models {} block to orchestrate.conf HCL schema with defaults table:
   - models.discover = "default" (session model)
   - models.define = "default"
   - models.develop.standard = "default"
   - models.develop.lite = "sonnet"
   - models.deliver = "default"
   Only develop.lite = "sonnet" is active; rest commented out.
   "default" keyword = inherit session model. Omitted = same as "default".
2. Update orchestrate SETUP config parsing to include models {} block
3. Update orchestrate DEVELOP ralph spawn: replace model: "sonnet" with models.develop.standard
4. Update panel skill: replace 3x model: "haiku" with config-read for models.define
5. Update ralph-subs-implement: replace 2x model: "sonnet" with models.develop.standard
6. Update ralph-subs-implement/roles/review-coordinator.md:
   - tier-1: replace model: "haiku" with models.develop.lite (default: "sonnet")
   - tier-2: replace model: "sonnet" with models.develop.standard (default: inherit)
7. Update ralph-beans-implement: same changes as steps 5-6
8. Verify: grep for hardcoded model refs should find zero
9. Commit

Acceptance criteria:
- Zero hardcoded model: "haiku" or model: "sonnet" in skills/
- orchestrate.conf schema has models {} block with defaults table
- All 12 model references use config values
- Standalone skills (no config file) use correct defaults
