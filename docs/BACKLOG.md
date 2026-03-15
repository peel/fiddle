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

### 2026-03-15 — Document AGENTS.md symlink setup in README
SYSTEM.md notes the AGENTS.md symlink for shared provider context, but README is more discoverable for new users. Add setup instructions there.
Origin: code review of async provider coordination epic
Tags: #docs

### 2026-03-15 — Clarify provider-context.md path resolution in panel/SKILL.md
The dispatch procedure references `roles/provider-context.md` by relative path. orchestrate/SKILL.md resolves both paths explicitly, but panel/SKILL.md only references the dispatch procedure without explicitly mentioning the template. Adding the explicit reference would improve clarity.
Origin: code review of async provider coordination epic
Tags: #debt #docs
