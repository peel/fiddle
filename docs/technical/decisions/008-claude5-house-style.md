# 008 — Write skills as judgment plus rationale, not emphatic rules

**Date:** 2026-07-31
**Status:** accepted

## Context

Fiddle's skills accumulated the guardrail style that earlier models needed: 47 HARD-GATE blocks, 44 capitalized MUSTs, 45 "Do NOT" directives, six rationalization tables, ten Red Flags sections, and an iron-laws file loaded by two skills. Anthropic's Claude 5 context-engineering guidance identifies this as counterproductive overconstraint and reports removing over 80% of Claude Code's system prompt with no measurable loss. Fiddle also has to keep working on Codex and Pi, which have no hook parity and so cannot fall back on mechanical enforcement when prose stops shouting.

## Decision

Skills state an invariant once, plainly, followed by the reason it exists, and then get out of the way. Removed tree-wide: gate markup, capitalized emphasis, rationalization tables, red-flag lists, announcement lines, and restatements of a rule already stated in the same file. Preserved verbatim as interface rather than instruction: frontmatter `description` fields (they are the trigger selector, and slimming them risks non-activation), JSON schemas, script invocations with their exit-code handling, cross-skill handoff pointers, and quoted external content.

One text serves every harness. There are no per-model or per-harness density variants, on the reasoning that a second copy tuned for weaker models is a second copy to keep true.

## Consequences

- The tree dropped from 38549 to 34270 words. The cut is uneven and that is expected: prose-heavy skills fell hard (tdd -48%, debug -30%, brainstorm -29%) while `orchestrate` and `using-fiddle` grew, absorbing the single schema home and the authoring note respectively.
- Rationale replaces repetition, so a reader who disagrees with an invariant can now see what it is protecting against and argue with the reason rather than the volume.
- The ordering and honesty invariants are the exposed surface of this bet. They survive as declarative sentences paired with an imperative still in the flow; if a future edit drops the imperative and keeps only the declarative headline, the invariant degrades to trivia. `docs/technical/SYSTEM.md` carries an invariant against exactly that, and `skills/using-fiddle/SKILL.md` carries the authoring note for contributors, which is what makes the anti-regression load-bearing rather than decorative.
- No behavioral eval harness was built, so the evidence that slimming preserved compliance is the test suites, a two-iteration holistic review, and live usage. That is weaker than an A/B and was accepted deliberately.
- Two skills were left unconverted, `deliver-docs` and `discover-docs`, which keep `## Rules` appendices restating their own flow. The convention is established and these are the visible remainder of it.
