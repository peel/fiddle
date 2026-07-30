---
name: init
description: Use when starting fiddle on a new project — scaffolds docs, orchestrate.json, and beans from templates.
---

# Init

Initialize a project for use with fiddle.

## Process

Run the init script:

```bash
bash scripts/init.sh .
```

The script:
- Copies doc templates from `.docs/` to `docs/` (skips if docs/ already has content)
- Copies `orchestrate.json` (skips if it exists)
- Runs `beans init` with prefix derived from directory name (skips if `.beans.yml` exists)

Report what was created vs what already existed based on script output.

Suggest next steps:
```
"Project docs are in docs/. Start by filling in:
- docs/product/VISION.md — what you're building, who for, and why
- docs/technical/SYSTEM.md — how the project works today

Then run fiddle:discover-docs to review gaps and fill in the rest.
Edit orchestrate.json to customize evaluator domains.
When ready: fiddle:orchestrate <topic>"
```

The script is idempotent and overwrites nothing: re-running it skips every component that is already initialized, so it is safe to run against a partially set up project.
