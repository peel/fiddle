---
name: discover
description: Use before defining a feature or epic to gather context, research options, and challenge scope assumptions.
---

# Discover


## Usage

Invoke as `fiddle:discover <topic> [--skip-docs] [--skip-challenge]`.

Gather project context, research the ecosystem, and challenge scope assumptions before defining a solution.

ARGUMENTS: {ARGS}

## Configuration

Parse from `{ARGS}`:

| Flag | Default | Description |
|---|---|---|
| `--skip-docs` | false | Skip discover-docs — go straight to research and challenge |
| `--skip-challenge` | false | Skip the challenge step after scope confirmation |

### Config File

Config: see `skills/orchestrate/SKILL.md` for the schema. This skill reads `providers.phases.discover` (the provider list for this phase), the `providers.<name>.command` and `.flags` declarations for each provider named there, and `providers.timeout`.

## Steps

### Step 1: Docs Discovery

Skip if `--skip-docs` was set.

Invoke discover-docs to gather project context and identify gaps:
Use the `fiddle:discover-docs` skill with `<topic>`.

This reads existing docs, CLAUDE.md, beans, and relevant source files. It produces a structured summary of what exists, what's relevant, and what gaps remain.

#### User Research Context

After discover-docs, check for user research artifacts:
- Persona files in `docs/product/personas/` — if they exist, load them. Note which personas are relevant to the topic being discovered.
- Latest insight summary in `docs/product/insights/` — if it exists, load it. Surface any themes or open questions relevant to the topic.

If these artifacts exist, include relevant findings when presenting the discovery summary. If they don't exist, skip silently.

### Step 2: External Research

If providers are configured (default: codex):

Read `hooks/dispatch-provider.sh` for collection rules. For each provider, dispatch via:

```bash
hooks/dispatch-provider.sh <provider> \
  --role "Research analyst" \
  --topic "<topic>" \
  --instructions "Research: ecosystem patterns, prior art, implementation approaches, potential pitfalls. Be specific and cite concrete examples."
```

Run provider dispatches in parallel when the harness supports it; otherwise run them sequentially. Collect results in **attended** mode.

If no provider CLI is available, skip and proceed with the current harness's internal knowledge only.

### Step 3: Challenge Scope

Skip if `--skip-challenge` was set.

Invoke the challenge skill to confirm scope and stress-test assumptions:
Use the `fiddle:challenge` skill with `--phase discover`.

This opens by synthesizing findings and confirming scope with the user, then walks the decision tree on assumptions and constraints — resolving every branch before moving forward. It self-serves answers from the codebase and only asks the user about genuine ambiguity.
