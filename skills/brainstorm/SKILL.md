---
name: brainstorm
description: Use before creative work or behavior changes to clarify intent, explore tradeoffs, and produce an approved design.
---

# Brainstorming Ideas Into Designs

Turn an idea into a fully formed design through collaborative dialogue, then hand off to planning.

A design is presented and the user approves it before any implementation action — no code, no scaffolding, no other skill invoked. Unexamined assumptions are cheapest to fix while no code exists yet, so this holds for every project however simple it looks; for a truly small change the design can be a few sentences, but it is still presented and approved.

## Flow

Create a task for each item and complete them in order:

1. **Explore project context** — files, docs, recent commits, and user research artifacts (personas in `docs/product/personas/`, the latest insight summary in `docs/product/insights/`) if they exist.
2. **Offer the visual companion** when the topic will involve visual questions. Its own message, with no other content. See Visual Companion below.
3. **Ask clarifying questions** — one at a time, covering purpose, constraints, success criteria.
4. **Propose 2-3 approaches** with trade-offs and your recommendation.
5. **Present the design** in sections scaled to their complexity, getting approval after each.
6. **Write the design doc** to `docs/specs/YYYY-MM-DD-<topic>-design.md` as a local lifecycle artifact; do not commit it (user preferences for spec location override this default).
7. **Extract initial calibration anchors** when the spec describes visual or behavioral output — save to `docs/evaluator-calibration-<domain>.md` and register the path in `orchestrate.json`.
8. **Spec self-review** — inline check for placeholders, contradictions, ambiguity, scope.
9. **Ask the user to review the written spec** before proceeding.
10. **Invoke the `fiddle:write-plan` skill** to create the implementation plan.

Step 10 is the terminal state. `fiddle:write-plan` is the only skill invoked after brainstorming — not frontend-design, mcp-builder, or any other implementation skill.

## Understanding the Idea

Check the current project state first (files, docs, recent commits). If `docs/product/personas/` or `docs/product/insights/` exist, load the relevant personas and the latest insight summary, and use them to ground questions and design decisions in real user signal.

Assess scope before asking detailed questions. If the request describes multiple independent subsystems ("build a platform with chat, file storage, billing, and analytics"), flag that immediately rather than spending questions on the details of a project that needs decomposing first. Help the user split it: what are the independent pieces, how do they relate, what order should they be built? Then brainstorm the first sub-project through the normal flow. Each sub-project gets its own spec → plan → implementation cycle.

For appropriately-scoped projects, ask one question per message, preferring multiple choice where it fits. If a topic needs more exploration, break it into several questions rather than stacking them into one.

When discovery stalls on an agent-side uncertainty, ask one optional diagnostic rather than guessing: “What missing context, access, or tool would make this decision reliable?” or “What did the previous run reveal that this design should improve?” Do not ask either when the answer is already available from the repository or conversation.

## Exploring Approaches

Propose 2-3 approaches with trade-offs, leading with your recommendation and the reasoning behind it. Apply YAGNI ruthlessly: features nobody asked for come out of the design here, not in review. Where persona files or insight summaries are available, reference them — which approach serves the personas with the highest needs, and does any approach conflict with a known feedback theme?

## Presenting the Design

Present the design once you believe you understand what is being built. Scale each section to its complexity: a few sentences when straightforward, up to 200-300 words when nuanced. Ask after each section whether it looks right so far, and go back to clarify when something does not make sense. Cover architecture, components, data flow, error handling, testing.

**Design for isolation and clarity.** Break the system into units that each have one clear purpose, communicate through well-defined interfaces, and can be understood and tested independently. For each unit you should be able to say what it does, how it is used, and what it depends on. If someone cannot understand a unit without reading its internals, or the internals cannot change without breaking consumers, the boundaries need work. Smaller units also suit you: you reason better about code you can hold in context at once, your edits are more reliable in focused files, and a file that has grown large is usually doing too much.

**In existing codebases,** explore the current structure before proposing changes and follow the established patterns. Where existing code has problems that affect the work — a file that has grown too large, unclear boundaries, tangled responsibilities — include targeted improvements in the design, the way a good developer improves code they are working in. Leave unrelated refactoring out.

## Calibration Anchor Extraction

If the design spec describes what the output should look like (visual designs, API contracts, behavioral descriptions), extract calibration anchors for each relevant evaluator dimension (`correctness`, `domain_spec_fidelity`, `code_quality` — see the domain template for the exact dimension names). For each dimension, describe what poor, acceptable, and excellent look like for this specific project:

```markdown
## [dimension] — Initial Anchor (YYYY-MM-DD)
**Poor (3-4):** [What a poor implementation of this dimension looks like for this project]
**Acceptable (6-7):** [What an acceptable implementation looks like]
**Excellent (9-10):** [What an excellent implementation looks like]
```

Save to `docs/evaluator-calibration-<domain>.md`, where `<domain>` matches the evaluator domain (`general`, `frontend`, ...). Evaluators load these anchors during implementation to calibrate their scoring, so add the path to `orchestrate.json` for discovery:

```json
"evaluators": {
  "domains": {
    "<domain>": {
      "calibration": "docs/evaluator-calibration-<domain>.md"
    }
  }
}
```

Skip this step for purely structural specs (scripts, configuration, tooling) with no visible output.

## Spec Self-Review

After writing the spec, read it with fresh eyes and fix what you find inline. No re-review pass is needed.

1. **Placeholders:** any "TBD", "TODO", incomplete section, or vague requirement.
2. **Internal consistency:** sections that contradict each other, or an architecture that does not match the feature descriptions.
3. **Scope:** focused enough for a single implementation plan, or in need of decomposition.
4. **Ambiguity:** any requirement open to two readings — pick one and make it explicit.

## User Review Gate

After self-review, ask the user to review the written spec:

> "Spec written locally to `<path>`. Please review it and let me know if you want to make any changes before we start writing out the implementation plan."

Wait for their response. If they request changes, make them and re-run the self-review. Proceed only once they approve, then invoke the `fiddle:write-plan` skill.

## Visual Companion

A browser-based companion for showing mockups, diagrams, and visual options during brainstorming. It is a tool, not a mode: accepting it makes it available for questions that benefit from visual treatment, and does not route every question through the browser.

Offer it once, in a message that contains only the offer, and wait for the response before continuing. Bundling the offer with a clarifying question asks for two decisions at once and usually loses one of them.

> "Some of what we're working on might be easier to explain if I can show it to you in a web browser. I can put together mockups, diagrams, comparisons, and other visuals as we go. This feature is still new and can be token-intensive. Want to try it? (Requires opening a local URL)"

If the user declines, continue with text-only brainstorming.

Once they accept, decide per question whether to use the browser or the terminal, on one test: would the user understand this better by seeing it than by reading it? The browser is for content that is itself visual — mockups, wireframes, layout comparisons, architecture diagrams, side-by-side visual designs. The terminal is for content that is text: requirements questions, conceptual choices, trade-off lists, A/B/C/D options, scope decisions. A question about a UI topic is not automatically a visual question: "what does personality mean in this context?" is conceptual, "which wizard layout works better?" is visual.

If they agree to the companion, read the detailed guide before proceeding: `skills/brainstorm/visual-companion.md`
