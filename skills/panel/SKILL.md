---
name: panel
description: Use when evaluating architectural approaches or design decisions — runs structured multi-model adversarial analysis with cross-review and synthesis
---

# Panel


## Usage

Invoke as `fiddle:panel <topic> [--rounds 2] [--context file1 file2]`.

Structured multi-model adversarial analysis. Participants argue independent positions, cross-review the others, and the lead synthesizes into a verdict.

ARGUMENTS: {ARGS}

## Configuration

Parse from `{ARGS}`:

| Flag | Default | Description |
|---|---|---|
| `--rounds <N>` | 2 | Number of cross-review rounds |
| `--context <files>` | none | Space-separated file paths to read for context |

If `--context` files are provided, read each file. Include their contents as context for all participants.

## Participants — Provider Gate (MUST execute before Phase 1)

You MUST perform these steps in order. Do NOT skip to degraded mode without completing them.

**Step 1.** Run the dispatch script in check mode for each provider:
```bash
hooks/dispatch-provider.sh codex --check; hooks/dispatch-provider.sh gemini --check
```
Each outputs JSON: `{"provider":"<name>","available":true/false,"command":"..."}`. Collect the results.

**Step 2.** Select mode based on results:

**Full mode** (at least one provider has `"available":true`):

| Participant | Perspective | Dispatch |
|---|---|---|
| **Lead harness** | Codebase/domain context | harness subagent or inline analysis |
| **Codex** | Implementation depth: code patterns, feasibility, performance | `hooks/dispatch-provider.sh codex ...` |
| **Gemini** | Ecosystem breadth: alternatives, prior art, industry patterns | `hooks/dispatch-provider.sh gemini ...` |

Only include providers that returned `"available":true`. The current harness is always present.

**Degraded mode** (all providers returned `"available":false` or dispatch script not found):

| Participant | Perspective | Dispatch |
|---|---|---|
| **Advocate A** | Assigned advocate for Approach 1 | harness subagent, or inline sequential pass |
| **Advocate B** | Assigned advocate for Approach 2 | harness subagent, or inline sequential pass |

Read `models.define` from `orchestrate.json` (project root). If "default" or not set, omit the `model:` parameter to inherit session model.

## Invocation Context

**Standalone** (user typed `fiddle:panel <topic>`):
- Generate 2-3 candidate approaches from the topic before Phase 1

**From orchestrate** (called after brainstorming):
- Approaches are already provided in the arguments — proceed directly to Phase 1

## Protocol

### Phase 1 — Independent Positions (parallel)

Spawn ALL participants in one message. Each receives the topic, context, and their assigned perspective. Each produces a position paper: what they recommend, why, key tradeoffs, risks.

**Harness participants:** start a subagent if the current harness supports one and the session permits it; otherwise run the position inline as a separate labeled pass. Prompt: "You are arguing from <perspective>. Topic: <topic>. Context: <context>. Produce a position paper: what you recommend, why, key tradeoffs, risks."

**External providers:**
```bash
hooks/dispatch-provider.sh <provider> \
  --role "<perspective>" \
  --topic "<topic>" \
  --approaches "<candidate approaches>" \
  --instructions "Produce a position paper: what you recommend, why, key tradeoffs, risks."
```

Run independent participants in parallel when the harness supports it; otherwise run them sequentially. Read `hooks/dispatch-provider.sh` for collection rules. Collect in **attended** mode. Wait for all results before proceeding.

### Phase 2 — Cross-Review (parallel)

Each participant receives ALL positions from Phase 1 and critiques them: agreements, disagreements, new concerns.

**Harness participants:** start a subagent if available, or run the review inline as a labeled pass. Prompt: "Review these positions on <topic>:\n\n<all positions>\n\nCritique: agreements, disagreements, new concerns raised."

**External providers:**
```bash
FEEDBACK_FILE=$(mktemp /tmp/feedback-XXXX.md)
# <write all positions from Phase 1 to $FEEDBACK_FILE>

hooks/dispatch-provider.sh <provider> \
  --role "<same perspective as Phase 1>" \
  --topic "<topic>" \
  --previous-feedback-file "$FEEDBACK_FILE" \
  --instructions "Critique the other positions: agreements, disagreements, new concerns."
```

Fire all in one message. Collect in **attended** mode.

### Additional Rounds

If `--rounds > 1`, repeat cross-review: each participant receives all critiques from the previous round and responds. Arguments should sharpen, not repeat. Run `--rounds - 1` additional rounds (Phase 2 counts as round 1).

### Synthesis

After all rounds, Claude (the lead — you) reads ALL positions and ALL cross-review responses. Produce the output below.

Do NOT delegate synthesis. You do this yourself.

## Output Format

```markdown
## Debate: <topic>

### Approaches

**Approach 1: <name>**
<full approach text>

**Approach 2: <name>**
<full approach text>

[...for each approach]

### Consensus
- [Points where all participants agree]

### Disagreement
- [Points of unresolved disagreement with tradeoffs for each position]
- Note which participants hold each position

### Recommendation
Based on the above, I recommend [approach] because [reasoning].
[Note key dissents and why they don't apply / do apply here.]
```

Use "None" for empty sections. All four sections (Approaches, Consensus, Disagreement, Recommendation) MUST be present.

## After Synthesis

**From orchestrate:** If full consensus → report "consensus" and proceed automatically. If disagreement → present the output to the user and ask them to pick.

**Standalone:** Present the output to the user. Done.
