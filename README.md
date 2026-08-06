# Fiddle

Portable Agent Skills library for orchestrating a four-phase development lifecycle with a calibrated evaluator loop and multi-model support across Claude, Codex, and Pi.

## Orchestrate

`fiddle:orchestrate <topic>` chains four phases. Each phase is also a standalone skill.

**DISCOVER** [`fiddle:discover`](skills/discover/SKILL.md) — Scan project docs, research the ecosystem via external providers, and challenge scope assumptions until every branch is resolved.

**DEFINE** [`fiddle:define`](skills/define/SKILL.md) — Brainstorm approaches, run a multi-model adversarial panel, challenge the chosen design, then produce an implementation plan with sized beans.

**DEVELOP** [`fiddle:develop`](skills/develop/SKILL.md) — Execute beans via the evaluator loop: dispatch implementer → dispatch one evaluator per domain → check thresholds → converge or iterate. Multi-domain evaluation with holistic cross-domain review when all tasks complete.

**DELIVER** [`fiddle:deliver`](skills/deliver/SKILL.md) — Drift analysis via external providers, evaluator evolve step (calibration updates, antipattern capture, threshold tuning), update technical docs, close the epic.

> [!NOTE]
> Any CLI that accepts a prompt on stdin works as a provider (Codex, Gemini, Copilot, etc). Configure per phase in [`orchestrate.json`](orchestrate.json).

## Evaluator Loop

The develop phase uses a calibrated evaluator loop: dispatch implementer → dispatch evaluators → check thresholds → converge or iterate. Script-enforced convergence (two consecutive passes required) with a dispatch budget that prevents infinite iteration.

**Per-domain evaluation.** Tasks spanning multiple domains (frontend + backend) get independent per-domain scoring. `resolve-domains.sh` maps task domains to evaluator templates. Domain-specific templates (frontend, backend) replace the general template for typed projects.

**Runtime verification.** Runtimes are started before evaluation and probe transcripts are captured into a per-domain evidence pack (tests, checks, runtime probes); evaluators interpret what the pack records rather than launching or probing the app themselves.

**Provider selection.** Each domain's `providers` list is an ordered
preference; the evaluator runs on the first available provider that differs
from the implementer's, falling back to the implementer's provider in a
fresh context. Evidence (tests, invariant checks, runtime probes) is
gathered before dispatch and handed to the evaluator as an artifact.
Holistic review still scores across all configured providers.

**Holistic review.** After all tasks complete, a cross-domain integration review produces a spec coverage matrix and remediation loop for any gaps.

**Calibration + evolve.** Attended mode shows scorecards to humans before acting — corrections become calibration anchors in project-specific files. Antipattern files loaded by implementer and evaluator. The deliver evolve step encodes improvements for future runs.

## Skills

| Skill | Description |
|-------|-------------|
| [`fiddle:using-fiddle`](skills/using-fiddle/SKILL.md) | Bootstrap routing and harness tool mappings. |
| [`fiddle:brainstorm`](skills/brainstorm/SKILL.md) | Collaborative design dialogue with calibration anchor extraction. |
| [`fiddle:write-plan`](skills/write-plan/SKILL.md) | Generate implementation plan from a design spec. |
| [`fiddle:evaluate`](skills/evaluate/SKILL.md) | Evaluator protocol — score an implementation against domain template and criteria. |
| [`fiddle:challenge`](skills/challenge/SKILL.md) | Walk the decision tree on any plan or design until shared understanding. |
| [`fiddle:panel`](skills/panel/SKILL.md) | Multi-model adversarial debate with provider/harness cross-review. |
| [`fiddle:tdd`](skills/tdd/SKILL.md) | Test-driven development workflow. |
| [`fiddle:verify`](skills/verify/SKILL.md) | Run checks and verify implementation. |
| [`fiddle:discover-docs`](skills/discover-docs/SKILL.md) | Socratic dialogue to bootstrap or review project docs. |
| [`fiddle:deliver-docs`](skills/deliver-docs/SKILL.md) | Post-ship doc updates — SYSTEM.md, ADRs, BACKLOG. |
| [`fiddle:define-beans`](skills/define-beans/SKILL.md) | Task sizing rules for decomposing plans into beans. |
| [`fiddle:adr`](skills/adr/SKILL.md) | Create an architecture decision record. |
| [`fiddle:feedback`](skills/feedback/SKILL.md) | Append a user feedback signal. |
| [`fiddle:backlog`](skills/backlog/SKILL.md) | Append an idea or tech debt item. |
| [`fiddle:debug`](skills/debug/SKILL.md) | Structured debugging workflow. |

## Configuration

Orchestrate reads [`orchestrate.json`](orchestrate.json) from the project root. All keys optional — defaults apply when omitted. See [`fiddle:orchestrate`](skills/orchestrate/SKILL.md) for the full reference.

```jsonc
{
  "providers": {
    "codex": { "command": "codex exec", "flags": "--json -s read-only" },
    "gemini": { "command": "gemini", "flags": "-o json --approval-mode auto_edit" },
    "phases": { "discover": ["codex"], "define": ["codex", "gemini"], ... }
  },
  "evaluators": {
    "attended": false,
    "max_dispatches_per_task": 10,
    "domains": {
      "general": {
        "template": "evaluator-general",
        "providers": ["claude"],
        "calibration": "docs/evaluator-calibration-general.md",
        "antipatterns": "docs/antipatterns-general.md"
      }
    }
  }
}
```

## Install

```bash
# Claude — from marketplace
/plugin marketplace add github:peel/peel-marketplace
/plugin install fiddle

# Claude — from source
claude --plugin-dir /path/to/fiddle

# Codex — from source
# Register the repo as a local marketplace, enable the plugin,
# and link this checkout into Codex's local plugin cache.
bash /path/to/fiddle/scripts/install-codex-local.sh
```

If editing `~/.codex/config.toml` directly, use:

```toml
[marketplaces.fiddle-dev]
source_type = "local"
source = "/path/to/fiddle"

[plugins."fiddle@fiddle-dev"]
enabled = true
```

The repo must contain `.agents/plugins/marketplace.json`; that file points Codex at `plugins/fiddle`, a thin local wrapper around this repo's `.codex-plugin/plugin.json` and `skills/` tree.

```bash
# Pi — from source
pi install /path/to/fiddle

# Maki — from source
bash /path/to/fiddle/scripts/install-maki.sh
```

The Maki installer maintains one marked Fiddle block in the global `~/.config/maki/init.lua`. Re-running it updates that block and the discovered skill list without duplicating commands or replacing unrelated personal configuration. If an older unmanaged Fiddle-only init already exists, use `scripts/install-maki.sh --replace-unmanaged`; it creates an `.pre-fiddle-installer.bak` backup first. Do not also add a project-local `.maki/init.lua` that registers Fiddle commands. Run `/reload` after installation.

Providers are auto-detected on session start.

### Optional: Clash (conflict detection)

When running parallel workers in worktrees, fiddle includes a PreToolUse hook that warns agents before writing to files that conflict with another worktree. This requires [clash](https://github.com/clash-sh/clash):

```bash
# via cargo
cargo install clash-sh

# via nix
nix profile install github:clash-sh/clash
```

The hook is advisory (never blocks) and silently skips if clash is not installed.
