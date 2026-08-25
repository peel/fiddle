# Orchestrate configuration

`orchestrate.json` in the project root is the live configuration and authority on every value. Phase skills link here instead of duplicating schema so configuration cannot drift.

## CLI flags

| Flag | Default | Description |
|---|---|---|
| `--epic <id>` | none | Resume one explicit epic. A seed-aware epic reconstructs context and executes its planning seed before development. A milestone is rejected. |
| `--no-triage` | false | Skip quick-path triage. |
| `--skip-discover` | false | Start at DEFINE. |
| `--skip-docs` | false | Pass through to DISCOVER. |
| `--skip-challenge` | false | Pass through to DISCOVER and DEFINE. |
| `--skip-panel` | false | Pass through to DEFINE. |

Provider configuration comes from `orchestrate.json`; CLI flags never override it. Provider availability is detected at session start by `hooks/session-start-check-providers.sh`.

## Configuration schema

```json
{
  "providers": {
    "codex": { "command": "codex exec", "flags": "--json -s read-only" },
    "gemini": { "command": "gemini", "flags": "-o json --approval-mode auto_edit" },
    "phases": {
      "discover": ["codex"],
      "define": ["codex", "gemini"],
      "develop": [],
      "develop_holistic": ["codex"],
      "deliver": ["codex"]
    },
    "timeout": { "attended": 120, "unattended": 90 }
  },
  "evaluators": {
    "attended": false,
    "max_dispatches_per_task": 16,
    "domains": {
      "general": {
        "template": "evaluator-general",
        "providers": ["claude", "codex"],
        "calibration": "docs/evaluator-calibration-general.md",
        "antipatterns": "docs/antipatterns-general.md",
        "thresholds": {}
      }
    },
    "holistic": { "providers": ["claude"], "max_iterations": 4 },
    "spot_check": { "rate": 5 },
    "aging": { "window_days": 90, "quiet_epics": 3 }
  },
  "acceptance": {
    "live": {
      "command": "scripts/live-cve-steering.sh",
      "env": ["FIDDLE_GITHUB_TOKEN", "FIDDLE_CVE_TAG"],
      "required": true
    }
  },
  "deliver": {
    "product_artifacts": {
      "templates_path": "docs/product/templates",
      "output_path": "docs/releases",
      "artifacts": ["release-notes", "social"]
    }
  },
  "models": {
    "phases": {
      "discover": "default",
      "define": "default",
      "develop": "default",
      "deliver": "default"
    },
    "roles": {
      "brainstorm": "default",
      "panel": "default",
      "implementer": "default",
      "evaluator": "default",
      "holistic": "default",
      "deliver": "default"
    }
  },
  "plans": {}
}
```

Fallbacks apply only when a key is absent: `max_dispatches_per_task` 16, `holistic.max_iterations` 3, `holistic.providers` `["claude"]`, `spot_check.rate` 5 (0 or less disables it), `aging.window_days` 90, and `aging.quiet_epics` 3. Read the live file rather than relying on fallbacks.

A domain provider array is an ordered preference for the one per-task evaluator: select the first available provider differing from the implementer. `evaluators.holistic.providers` is a fan-out and dispatches every listed provider.

## Internal subagent models

`models.phases` configures a phase default; `models.roles` configures an individual internal role. Valid values are `default`, `smol`, and `slow`.

```json
"models": {
  "phases": {
    "discover": "default",
    "define": "default",
    "develop": "default",
    "deliver": "default"
  },
  "roles": {
    "brainstorm": "default",
    "panel": "default",
    "implementer": "default",
    "evaluator": "default",
    "holistic": "default",
    "deliver": "default"
  }
}
```

Resolve every internal subagent with `scripts/resolve-subagent-model.sh --phase <phase> --role <role> --config orchestrate.json`. A role value wins over its phase value; a missing value falls back to `default`. The resolver emits a `model` field only for `smol` or `slow`; omit the harness model parameter when it is absent so the subagent inherits the session model. External provider CLIs remain controlled only by `providers` and never use this resolver.

## Provider defaults

| Phase | Default providers | Rationale |
|---|---|---|
| DISCOVER | codex | Research depth from a code-oriented model. |
| DEFINE | codex, gemini | Diverse architectural perspectives. |
| DEVELOP | none | The domain-expert loop provides review. |
| DEVELOP holistic | codex | An outside system perspective. |
| DELIVER | codex | Drift and documentation review. |

Claude is implicit and never listed. A configured `codex` participant means the current harness plus Codex participate.

## Merge order

Defaults → config file → CLI flags. Later values override earlier ones; provider assignments remain config-only. Orchestrate reads the configuration once during setup, while standalone phase skills read the keys they need.

## Live acceptance

`acceptance.live` names the command that exercises a milestone against the real
system it acts on. Develop step 3 runs it before holistic review and deliver
step 2 refuses to deliver an epic whose body does not carry its result.

| Key | Meaning |
|---|---|
| `command` | The command to run, relative to the project root. The project provides it. |
| `env` | Variables the command requires. Their absence is the command's own error to report, not the lifecycle's to guess. |
| `required` | When true, an epic without a recorded result is not delivered. |

The lifecycle owns the gate and the project owns what it runs, because what
counts as the real system differs per project. A project that omits the key
declares no live gate; develop records that fact and what is therefore
unverified, rather than skipping silently.

The command must fail rather than skip. A gate that reports success without
exercising anything is the failure that most resembles a pass.
