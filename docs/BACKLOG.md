# Backlog

<!-- Ideas, technical debt, someday/maybe items. Cross-cutting — covers both
     product and technical concerns. Append-only with dates.
     
     Periodically review: promote to beans, delete, or leave.
     Agents can suggest additions here during brainstorming.
     Agents should read this before planning to avoid re-discovering known items. -->

<!-- ### YYYY-MM-DD — Title
     Description of the idea or debt item.
     Origin: brainstorm session / feedback / code review / noticed during work
     Tags: #idea #debt #optimization #feature #experiment #infrastructure
-->

### 2026-04-02 — Skill size estimates are unreliable for verbatim extraction
Design spec predicted 12KB for develop-loop; actual was 20.5KB (71% over). JSON examples, code blocks, and HARD-GATE blocks are denser than prose and harder to estimate. Future specs involving skill extraction should measure the source line range directly rather than estimating.
Origin: implementation (develop modularization epic fiddle-wdg0)
Tags: #debt #optimization


### 2026-07-28 — PR-review feedback channel into calibration/antipattern memory
Harvest human review comments from finish-branch PRs back into calibration and antipattern files, /iterate-style, closing the reviewer-to-memory loop that only the attended gate feeds today. Deferred from epic fiddle-sip9 (one loop at a time).
Origin: brainstorm (fiddle-sip9), research: humanlayer/skills design-control-loop
Tags: #idea #feature

### 2026-07-28 — Scheduled antipattern-eradication maintenance loop
Scheduled CI workflow that consumes deliver's antipattern files: a script scans the codebase for occurrences, one is picked per run, a coding agent fixes it and opens a PR, at most one PR open at a time, human merges. Continuous cleanup between epics, complementary to develop-loop. Deferred from epic fiddle-sip9.
Origin: brainstorm (fiddle-sip9), research: humanlayer/skills design-control-loop
Tags: #idea #infrastructure #experiment
