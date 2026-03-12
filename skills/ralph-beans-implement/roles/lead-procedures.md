# Lead Procedures

On-demand procedures for the team lead. Read this file only when a specific procedure is needed.

## Worktree Setup

When `--workers > 1`:
1. Create `.worktrees/` directory if it doesn't exist
2. **Integration worktree:**
   - Epic: `git worktree add .worktrees/{epic-id}-integration -b epic/{epic-id} HEAD` then `direnv allow .worktrees/{epic-id}-integration`
   - Non-epic: `git worktree add .worktrees/integration -b work/{first-bean-id} HEAD` then `direnv allow .worktrees/integration`
3. Worktree base ref: the integration branch (`epic/{epic-id}` or `work/{first-bean-id}`)
4. Worktree name prefix: `{epic-id}-worker` when `--epic` is set, `worker` otherwise
5. For each worker slot (1..N): `git worktree add .worktrees/{prefix}-{N} -b {prefix}-{N}/scratch {base-ref}` then `direnv allow .worktrees/{prefix}-{N}`
6. Record worktree paths for assignment — each `worktree`-tagged bean gets a slot path
7. `branch`-tagged beans skip worktree assignment and run in the main checkout serially

## Lead Verification

Runs **once** per review transition, before sending a review request to the coordinator. The lead only checks exit codes — full output stays on disk for reviewers.

1. Determine worktree path from bean's `worktree-slot:*` tag (or main checkout for `branch`-tagged beans)
2. Run via Bash (discard output):
   ```bash
   cd {worktree_path} && echo "VERIFIED_AT:$(git rev-parse HEAD) BEAN:{bean_id} TS:$(date -u +%Y-%m-%dT%H:%M:%SZ)" > .verification-output.txt && direnv exec . sh -c 'go test -short ./... 2>&1; echo "EXIT:$?"' >> .verification-output.txt && direnv exec . sh -c 'go build ./... 2>&1; echo "EXIT:$?"' >> .verification-output.txt
   ```
3. Check only exit codes (grep for `EXIT:` lines). Do NOT read the full file.

**If all EXIT:0:** Proceed to send review request to coordinator.

**If any fail:** Do NOT send review request. Instead:
- `beans update {id} --tag role:review-fix-{cycle}` (increment cycle)
- Check review cycle. If >= max_review_cycles → "Abandon Bean"
- Otherwise: spawn fix implementer teammate. Tell it to read `{WORKTREE_PATH}/.verification-output.txt` for failure details.

## Epic Holistic Review

When all child beans of an epic are completed and only the epic bean remains.

**Detection:** After "Assess and Act" finds no ready or active beans. When `--epic` is set, check the epic bean with `beans show <epic-id> --json`. Otherwise, check `BEANS_LIST` for `type: epic` beans that are `in-progress`/`todo` with all children `completed`.

**Loop:**
1. Tag the epic: `beans update {id} --tag epic-review-cycle:{N}` (starting at 1)
2. Send review request to the coordinator with epic scope:
   - Content: epic description + list of all child bean IDs and titles
   - Instruction: review `git diff main...epic/{epic-id}` (or `git diff main...HEAD` without `--epic`)
   - Reviewers look for: inconsistencies between beans, integration issues, naming conflicts, missed edge cases, dead code
3. Wait for coordinator verdict.

**CLEAN:** `beans update {id} --status completed`, mark team task completed → Cleanup.

**Issues:** Present findings to user. Spawn fix implementer(s). When fixes done, increment cycle and send new review request (step 2). Max cycles = max_review_cycles. If exceeded → present remaining issues, complete anyway.

## Abandon Bean

When a bean must be abandoned (max review cycles exceeded, implementer hit max_turns):

1. `beans update {id} --status draft --tag abandoned`
2. Revert uncommitted changes: `git checkout -- .` (safe because committed work is in git)
3. Check `BEANS_LIST` for beans whose `blocked-by` includes the abandoned bean
4. For each dependent: `beans update {dep-id} --status draft --tag abandoned-upstream:{id}`
5. Cascade: repeat 3-4 for each newly-abandoned dependent (depth-first)
6. Report to user: list the abandoned bean and all cascaded dependents

## Cleanup

When no incomplete beans remain:

1. Send `shutdown_request` to all remaining teammates (implementers and review coordinator).
2. **Merge workers into integration worktree:**
   - Integration worktree: `.worktrees/{epic-id}-integration` (epic) or `.worktrees/integration` (non-epic)
   - `cd {integration-worktree-path}`
   - For each completed bean with `worktree-slot:*` tag: `git merge {prefix}-{N}/scratch`, then `git worktree remove .worktrees/{prefix}-{N}` and `git branch -d {prefix}-{N}/scratch`
3. `TeamDelete`
4. Report final status and integration branch name to user.
5. Ask user: "Work is on branch `{integration-branch}`. What would you like to do?"
   - **Checkout in main worktree** → in main worktree: `git checkout {integration-branch}`. Remove integration worktree: `git worktree remove {integration-worktree-path}`.
   - **Rebase onto main** → in integration worktree: `git rebase main`. Then in main worktree: `git checkout main && git merge --ff-only {integration-branch}`. Remove integration worktree: `git worktree remove {integration-worktree-path}`.
   - **Leave as-is** → keep integration worktree, report path.
