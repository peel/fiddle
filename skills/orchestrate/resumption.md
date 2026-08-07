# Orchestrate setup and resumption

Run this setup before selecting a lifecycle phase.

1. Read `orchestrate.json` according to [configuration](configuration.md), parse flags, and compute final settings.
2. Resolve one Beans store shared by every worktree:

   ```bash
   COMMON_GIT_DIR=$(git rev-parse --path-format=absolute --git-common-dir)
   MAIN_CHECKOUT=$(dirname "$COMMON_GIT_DIR")
   MAIN_BEANS_PATH="$MAIN_CHECKOUT/.beans"
   ```

   Pass `--beans-path "$MAIN_BEANS_PATH"` to every Beans command in this lifecycle.
3. Without `--epic`, begin at DISCOVER unless `--skip-discover` requests DEFINE.
4. With `--epic`, write `beans show <id> --json` and `beans list --parent <id> --json` to temporary JSON files. If the epic has one `blocked_by` entry, also load that predecessor into a temporary JSON file. A top-level milestone is never resolved to a child implicitly.
5. Run:

   ```bash
   RESOLVER_ARGS=(--epic "$EPIC_JSON" --children "$CHILDREN_JSON")
   [[ -z "${PREDECESSOR_JSON:-}" ]] || RESOLVER_ARGS+=(--predecessor "$PREDECESSOR_JSON")
   scripts/resolve-orchestrate-phase.sh "${RESOLVER_ARGS[@]}"
   ```

   Add `--delivery-complete` only when the selected epic's delivery evidence proves delivery completed. Read `.state` and `.reason` from the result. Valid states are `SEED`, `DEFINE`, `DEVELOP`, `DELIVER`, `DONE`, `NEEDS_CONTEXT`, and `INVALID`.
6. Stop on `NEEDS_CONTEXT` or `INVALID` and report the resolver reason. Otherwise replace any existing `orchestrate-phase:*` tag with the resolved phase using the canonical Beans path.

The phase tag is visibility only. Tracker state, generation identities, predecessor handoff, Git state, and delivery evidence are the durable source of truth.
