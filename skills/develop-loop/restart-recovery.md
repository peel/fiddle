# Restart Recovery

If a bean is already `in-progress` (session restart or crash recovery), re-derive its state from the scripts rather than from memory or context — the session that wrote the state is gone, and a guess restarts work that already converged or skips work that did not:

```bash
scripts/parse-eval-log.sh --bean-id {id}
scripts/assess-git-state.sh --base-sha {sha}
```

Resume based on their output.

**Interpreting restart state:**

- `parse-eval-log.sh` returns `{base_sha, total_dispatches, iteration_count, last_verdict, last_guidance, iterations, unchanged_tree_reevaluations}`.
  - `iterations[]` carries each logged iteration's `dispatches`, `tree` and `convergence` verdict, and `unchanged_tree_reevaluations` counts those whose tree matched the iteration before them. Both are empty or zero for a log written without `--tree-sha`, which means the log did not record it, not that the tree changed every time. The bean's own `unchanged_tree_reevaluations:` header line is absent entirely on a log initialised before the field existed, which is likewise not a zero.
  - `last_verdict` is `PASS | FAIL | UNGRADED | UNKNOWN`. **UNGRADED is not a verdict** — it means the last entry contains a dimension the log could not compare (no threshold, a non-numeric score, or a scorecard shape it could not read), so that iteration establishes nothing either way. Treat it as needing a fresh evaluation, never as a pass. `UNKNOWN` means no iteration has been logged at all.
- `assess-git-state.sh` returns `{state: CLEAN|DIRTY|CORRUPTED}`.
  - **CLEAN:** Code is committed. Resume from domain resolution and evaluation (step 1c) if last verdict was not CONVERGED, or skip to next task if CONVERGED.
  - **DIRTY:** Uncommitted changes exist. Set them aside as described below, then resume from evaluation.
  - **CORRUPTED:** Merge conflict or broken state. Escalate to human — mark bean `needs-attention`.

## Setting work aside, and two ways of doing it that do not work

Before a rebase, a revert, or anything that rewrites the tree, the instinct is
to take a snapshot first. Two of the obvious ways produce something that looks
like a snapshot and is not.

**`git diff > file` is not necessarily a patch.** Under a shell wrapper that
summarizes command output — several exist, and the one on this project's
machines does — the redirect captures the *summary*, and `git apply` rejects
the result. The failure arrives only when you try to restore, which is the
moment the snapshot was for. `git stash create` writes a real commit object,
prints its sha, and touches no stack:

```bash
SNAP=$(git stash create)   # empty output means the tree was clean
git stash store -m "pre-<operation> <bean-id>" "$SNAP"   # only if you want it findable
```

Restore with `git checkout "$SNAP" -- .` or inspect with `git show "$SNAP"`.

**Bare `git stash` / `git stash pop` is unsafe in a worktree.** The stack is
shared with the main checkout and every other worktree, and other sessions may
be pushing and popping it concurrently — a bare `pop` can restore someone
else's work over yours. Prefer a temporary WIP commit. If you must stash, push
with a unique tag, capture your own entry's sha, and `apply` it rather than
`pop`:

```bash
git stash push -u -m "<bean-id>-<unique-tag>"
SHA=$(git stash list --format='%H %gs' | grep "<bean-id>-<unique-tag>" | cut -d' ' -f1)
git stash apply "$SHA"
```

Then drop the entry by re-finding its current `stash@{n}` by tag — the index
will have moved if another session touched the stack in between.

Whichever you use, verify the snapshot exists before relying on it. A safety
net you did not check is one you will find out about at the worst time.
