# Restart Recovery

If a bean is already `in-progress` (session restart or crash recovery), re-derive its state from the scripts rather than from memory or context — the session that wrote the state is gone, and a guess restarts work that already converged or skips work that did not:

```bash
scripts/parse-eval-log.sh --bean-id {id}
scripts/assess-git-state.sh --base-sha {sha}
```

Resume based on their output.

**Interpreting restart state:**

- `parse-eval-log.sh` returns `{base_sha, total_dispatches, iteration_count, last_verdict, last_guidance}`.
- `assess-git-state.sh` returns `{state: CLEAN|DIRTY|CORRUPTED}`.
  - **CLEAN:** Code is committed. Resume from domain resolution and evaluation (step 1c) if last verdict was not CONVERGED, or skip to next task if CONVERGED.
  - **DIRTY:** Uncommitted changes exist. Commit or stash them, then resume from evaluation.
  - **CORRUPTED:** Merge conflict or broken state. Escalate to human — mark bean `needs-attention`.
