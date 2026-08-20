# 008 — Write a skill as judgment plus rationale, not as an emphatic rule

Date: 2026-07-31
Status: accepted
Cites: skills/using-fiddle/SKILL.md, docs/technical/SYSTEM.md, skills/deliver-docs/SKILL.md, skills/discover-docs/SKILL.md

## Context

Fiddle's skills carried the guardrail style an earlier model needed: 47 HARD-GATE blocks and 44 capitalized MUSTs. Anthropic's Claude 5 guidance calls this overconstraint, reporting an 80% cut to Claude Code's system prompt with no measured loss. Fiddle also runs on Codex and Pi, which have no hooks to fall back on.

## Decision

State an invariant once, plainly, give the reason it exists, and stop. Remove gate markup, capitalized emphasis, rationalization tables, red-flag lists, announcement lines and self-restatement. Keep one text for every harness, with no per-model variant.

## Consequences

- The skills tree fell from 38549 to 34270 words. The cut was uneven, as expected. `tdd` fell 48%, `debug` 30% and `brainstorm` 29%, while `orchestrate` and `using-fiddle` grew.
- A reader who disagrees with an invariant can now see what it protects and argue with the reason.
- The ordering and honesty invariants are the exposed part of this bet. A later edit that keeps the declarative headline and drops the imperative degrades the invariant to trivia.
- The project gave up an eval harness. The evidence is the test suites, one holistic review and live usage. That is weaker than an A/B test.
- `deliver-docs` and `discover-docs` still carry a `## Rules` appendix restating their own flow. They are the visible remainder of the old convention.

The census was 47 HARD-GATE blocks, 44 capitalized MUSTs and 45 "Do NOT" directives. It was also six rationalization tables, ten Red Flags sections and an iron-laws file loaded by two skills. Codex and Pi have no hook parity, so neither can fall back on mechanical enforcement.

Five things are interface rather than instruction, and stay verbatim. A frontmatter `description` field stays, because it selects the trigger and a shorter one risks non-activation. So do a JSON schema, a script invocation with its exit codes, a cross-skill pointer and quoted external content. `docs/technical/SYSTEM.md` carries the invariant against dropping an imperative, and `skills/using-fiddle/SKILL.md` carries the authoring note.
