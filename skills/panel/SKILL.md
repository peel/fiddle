---
name: fiddle:panel
description: Use when evaluating architectural approaches or design decisions — runs structured multi-model adversarial analysis with cross-review rounds and synthesis
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

1. Read `orchestrate.conf` → `providers` block. For each provider listed in the `define` phase list, read its `command` field.
2. Check each provider's CLI binary on PATH:
   ```bash
   which <first-word-of-command>   # e.g. "codex" for "codex exec", "gemini" for "gemini"
   ```
3. At least one external provider binary found → **full mode**. None found → **degraded mode** (Claude subagents only).

### Invocation Context

**Standalone** (user typed `/fiddle:panel <topic>`):
- The topic is the user's question or decision
- Before Phase 1: generate 2-3 candidate approaches from the topic. These become the positions to debate.

**From orchestrate** (called programmatically after brainstorming):
- Approaches are already provided in the arguments
- Skip candidate generation, proceed directly to Phase 1

## Protocol

### Phase 1 — Independent Positions (parallel)

Each participant receives: the topic, context files, and their assigned perspective. Each produces an independent position arguing for their assigned approach.

**Full mode:** Read `skills/ralph-subs-implement/roles/provider-dispatch.md`. Also read `skills/ralph-subs-implement/roles/provider-context.md` for prompt construction. Follow **Read Config** and **Build Prompt** for each available provider. Spawn ALL in one message — true parallelism.

1. **Claude position** — spawn via Agent tool:

Read `models.define` from `.claude/orchestrate.conf` if it exists. If the value is "default" or not set, omit the `model:` parameter to inherit the session model. Otherwise use the configured model.

```
Agent(
  subagent_type: "general-purpose",
  model: <models.define>,  # omit if "default" to inherit session model
  mode: "bypassPermissions",
  run_in_background: true,
  prompt: "You are arguing from a codebase/domain perspective. Topic: {topic}. Context: {context}. Produce a position paper: what you recommend, why, key tradeoffs, risks. Be specific and cite concrete patterns from the codebase context."
)
```

2. **External providers** — for each available provider, follow provider-dispatch **Build Prompt** then **Dispatch (Background)**:

Template placeholders:
- `{PROVIDER_ROLE}`: Codex → "Implementation depth: code patterns, technical feasibility, performance" / Gemini → "Ecosystem breadth: alternatives, prior art, industry patterns"
- `{TOPIC}`: the debate topic
- `{APPROACHES}`: the candidate approaches
- `{INSTRUCTIONS}`: "Produce a position paper: what you recommend, why, key tradeoffs, risks."
- Leave `{DESIGN_DOC}`, `{DIFF}`, `{PREVIOUS_FEEDBACK}` empty (stripped by Build Prompt)

Fire each provider via provider-dispatch **Dispatch (Background)** — one `Bash(run_in_background: true)` per provider, all in the same message as the Claude Agent call.

3. **Collect** — follow provider-dispatch **Collect Results** with **attended mode**. Collect all results before proceeding.

**Degraded mode** (no external providers):

Spawn exactly 2 Claude subagents as assigned advocates:

```
Agent A(
  subagent_type: "general-purpose",
  model: <models.define>,  # omit if "default" to inherit session model
  mode: "bypassPermissions",
  run_in_background: true,
  prompt: "You are an assigned advocate for Approach 1: {approach_1}. Argue FOR this approach with full conviction. Topic: {topic}. Context: {context}. Produce a position paper."
)

Agent B(
  subagent_type: "general-purpose",
  model: <models.define>,  # omit if "default" to inherit session model
  mode: "bypassPermissions",
  run_in_background: true,
  prompt: "You are an assigned advocate for Approach 2: {approach_2}. Argue FOR this approach with full conviction. Topic: {topic}. Context: {context}. Produce a position paper."
)
```

### Phase 2 — Cross-Review (parallel)

Send ALL positions to each participant. Each critiques the others: agreements, disagreements, new concerns raised.

**Full mode:** Read `skills/ralph-subs-implement/roles/provider-dispatch.md`. Also read `skills/ralph-subs-implement/roles/provider-context.md` for prompt construction. Follow **Read Config** and **Build Prompt** for each available provider. Spawn ALL in one message.

- **Claude subagent**: receives all other positions via `Agent(run_in_background: true)`, critiques them
- **External providers**: for each, follow provider-dispatch **Build Prompt** then **Dispatch (Background)**:
  - `{PROVIDER_ROLE}`: same as Phase 1 for each provider
  - `{TOPIC}`: the debate topic
  - `{PREVIOUS_FEEDBACK}`: all positions from the previous round
  - `{INSTRUCTIONS}`: "Critique the other positions: agreements, disagreements, new concerns."
  - Leave `{APPROACHES}`, `{DESIGN_DOC}`, `{DIFF}` empty (stripped by Build Prompt)

Fire all in one message. Collect via provider-dispatch **Collect Results** with **attended mode**.

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

3. **Did I skip degraded mode when providers were unavailable?** If no external provider binaries were found on PATH, you MUST have spawned 2 Claude subagents. If you just wrote a pros/cons list yourself, go back and spawn the agents.

4. **Does the output have all four sections?** Consensus, Majority, No consensus, Recommendation — all four MUST be present. Use "None" for empty sections.

5. **Did I run the requested number of rounds?** Count the actual cross-review rounds. If fewer than `--rounds`, run the missing rounds.
