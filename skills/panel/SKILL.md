---
name: panel
description: Use when evaluating architectural approaches or design decisions — runs structured multi-model adversarial analysis with cross-review rounds and synthesis
disable-model-invocation: true
argument-hint: <topic> [--providers codex,gemini] [--rounds 2] [--context file1 file2]
---

# Panel

Structured multi-model adversarial analysis. Each participant argues an independent position, cross-reviews the others, and Claude synthesizes into a verdict.

ARGUMENTS: {ARGS}

## Configuration

Parse from `{ARGS}`:

| Flag | Default | Description |
|---|---|---|
| `--providers <list>` | codex,gemini | Comma-separated external providers |
| `--rounds <N>` | 2 | Number of cross-review rounds |
| `--context <files>` | none | Space-separated file paths to read for context |

If `--context` files are provided, read each with the Read tool. Include their contents as context for all participants.

## Provider Roles

Roles are fixed — each provider argues from an assigned perspective:

| Provider | Perspective | Focus |
|---|---|---|
| **Claude** | Synthesis + codebase/domain | Final judgment, project-specific context |
| **Codex** | Implementation depth | Code patterns, technical feasibility, performance |
| **Gemini** | Ecosystem breadth | Alternatives, prior art, industry patterns |

Claude is always present and always does synthesis. External providers participate in debate rounds.

## Mode Detection

### Provider Availability

Check if multi_mcp tools are available by attempting to list MCP tools. If `multi_mcp` tools (`debate`, `chat`) are NOT available, enter **degraded mode**.

For each provider in `--providers`:
- If multi_mcp is available → full mode with that provider
- If multi_mcp is NOT available → degraded mode (Claude subagents only)

### Invocation Context

**Standalone** (user typed `/peel:panel <topic>`):
- The topic is the user's question or decision
- Before Phase 1: generate 2-3 candidate approaches from the topic. These become the positions to debate.

**From orchestrate** (called programmatically after brainstorming):
- Approaches are already provided in the arguments
- Skip candidate generation, proceed directly to Phase 1

## Protocol

### Phase 1 — Independent Positions (parallel)

Each participant receives: the topic, context files, and their assigned perspective. Each produces an independent position arguing for their assigned approach.

**Full mode:** Spawn all in parallel in ONE message:

1. **Claude position** — spawn via Agent tool:
```
Agent(
  subagent_type: "general-purpose",
  model: "haiku",
  mode: "bypassPermissions",
  run_in_background: true,
  prompt: "You are arguing from a codebase/domain perspective. Topic: {topic}. Context: {context}. Produce a position paper: what you recommend, why, key tradeoffs, risks. Be specific and cite concrete patterns from the codebase context."
)
```

2. **Codex position** — call multi_mcp debate:
```
multi_mcp debate(
  provider: "codex",
  prompt: "Topic: {topic}. Context: {context}. Argue from an implementation depth perspective: code patterns, technical feasibility, performance implications. Produce a position paper."
)
```

3. **Gemini position** — call multi_mcp debate:
```
multi_mcp debate(
  provider: "gemini",
  prompt: "Topic: {topic}. Context: {context}. Argue from an ecosystem breadth perspective: alternatives, prior art, industry patterns. Produce a position paper."
)
```

Collect all results before proceeding.

**Degraded mode** (no external providers):

Spawn exactly 2 Claude subagents as assigned advocates:

```
Agent A(
  subagent_type: "general-purpose",
  model: "haiku",
  mode: "bypassPermissions",
  run_in_background: true,
  prompt: "You are an assigned advocate for Approach 1: {approach_1}. Argue FOR this approach with full conviction. Topic: {topic}. Context: {context}. Produce a position paper."
)

Agent B(
  subagent_type: "general-purpose",
  model: "haiku",
  mode: "bypassPermissions",
  run_in_background: true,
  prompt: "You are an assigned advocate for Approach 2: {approach_2}. Argue FOR this approach with full conviction. Topic: {topic}. Context: {context}. Produce a position paper."
)
```

### Phase 2 — Cross-Review (parallel)

Send ALL positions to each participant. Each critiques the others: agreements, disagreements, new concerns raised.

**Full mode:** Send to each in parallel:
- Claude subagent: receives Codex + Gemini positions, critiques both
- Codex via multi_mcp debate: receives Claude + Gemini positions, critiques both
- Gemini via multi_mcp debate: receives Claude + Codex positions, critiques both

**Degraded mode:**
- Agent A: receives Agent B's position, critiques it
- Agent B: receives Agent A's position, critiques it

### Phase 3+ — Additional Rounds

If `--rounds > 1`, repeat cross-review: each participant receives all critiques from the previous round and responds. Arguments should sharpen, not repeat.

Run `--rounds - 1` additional cross-review rounds (Phase 2 counts as round 1).

### Synthesis

After all rounds complete, Claude (the lead — you) reads ALL positions and ALL cross-review responses. Produce the output in the exact format below.

Do NOT delegate synthesis. You do this yourself.

## Output Format

Produce exactly this structure:

```markdown
## Debate: <topic>

### Consensus
- [Points where all participants agree]

### Majority (2/3)
- [Points where 2 of 3 agree, with noted dissent]
- "Claude and Codex prefer X. Gemini argues Y because..."

### No consensus
- [Points of unresolved disagreement]
- Include tradeoffs for each position

### Recommendation
Based on the above, I recommend [approach] because [reasoning].
[Note key dissents and why they don't apply / do apply here.]
```

In degraded mode (2 participants), use "Both agree" / "Disagreement" instead of Majority.

## After Synthesis

**From orchestrate:** If full consensus → report "consensus" and proceed automatically. If disagreement → present the output to the user and ask them to pick.

**Standalone:** Present the output to the user. Done.

## Anti-Rationalization Checks

Before outputting your synthesis, verify each of these. If any check fails, go back and fix it.

1. **Did I skip cross-review?** Each participant MUST see and critique the others' positions. If you produced synthesis from only Phase 1, go back and run Phase 2.

2. **Did I dump raw output instead of synthesizing?** The synthesis must be in YOUR voice, not copy-pasted provider responses. Rewrite if needed.

3. **Did I skip degraded mode when providers were unavailable?** If multi_mcp tools were not available, you MUST have spawned 2 Claude subagents. If you just wrote a pros/cons list yourself, go back and spawn the agents.

4. **Does the output have all four sections?** Consensus, Majority, No consensus, Recommendation — all four MUST be present. Use "None" for empty sections.

5. **Did I run the requested number of rounds?** Count the actual cross-review rounds. If fewer than `--rounds`, run the missing rounds.
