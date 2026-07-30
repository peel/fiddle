---
# fiddle-50c5
title: Give codex evaluator sandbox a writable TMPDIR for artifact execution
status: todo
type: task
created_at: 2026-07-23T16:15:45Z
updated_at: 2026-07-23T16:15:45Z
---

Holistic remediation candidate (source: runtime_health, codex), converted to a standalone harness follow-up because it concerns provider configuration, not epic code. The codex CLI runs with "-s read-only" (orchestrate.json providers.codex.flags), so mktemp fails and codex evaluators cannot execute test scripts or the trend script; during epic fiddle-4ask's holistic review codex had to score runtime_health without execution evidence (scored 5 on a suite the lead and claude verified green twice).

Options to evaluate: a per-role flags override for evaluator/holistic dispatches (e.g. workspace-write against a disposable worktree copy with TMPDIR inside it), or a documented precondition in holistic-review.md that sandboxed providers receive a lead-verified execution transcript as runtime evidence.

- [ ] Decide the mechanism (flags override vs documented transcript precondition)
- [ ] Implement in dispatch-provider.sh and/or holistic-review.md
- [ ] Verify codex can execute (or consume transcripts for) test scripts in a holistic run
