# Orchestrate setup and resumption

Run this setup before selecting a lifecycle phase.

1. Read `orchestrate.json` according to [configuration](configuration.md), then parse the invocation flags and compute the final settings.
2. If `--epic <id>` is supplied, run `beans show <id> --json`. The bean must exist and be an `epic` or `milestone`; otherwise stop and report the error.
3. With `--epic`, run `beans list --parent <epic-id> --json` and determine the resumption phase:
   - No child beans: DEFINE.
   - Any child in `todo` or `in-progress`: DEVELOP.
   - All children completed or tagged `needs-attention`, and no `deliver-docs` commit: DELIVER.
   - Documentation already evolved (`git log --oneline --grep="deliver-docs"`): DONE; report completion.
4. Without `--epic`, begin at DISCOVER unless `--skip-discover` requests DEFINE.
5. When an epic exists, set `orchestrate-phase:<phase>` on it with `beans update <epic-id> --tag orchestrate-phase:<phase>`.

The phase tag makes restart state visible; bean state remains the source of truth.
