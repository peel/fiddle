# 089 — A member's direction widens what a steered run may change, and the run works on the pull request's head

Status: accepted
Cites: COMMITTER, a_hook_installed_in_the_clone_does_not_stop_fiddle_committing, commit_described, run_dated, submitted_at, marked_comment_or_holding, read_marked_or_holding, a_link_fiddle_wrote_at_another_revision_is_found_by_what_it_links, a_link_a_person_pasted_is_not_one_fiddle_wrote, the_revision_a_run_observes_is_the_revision_the_link_builds_its_identity_from, STEERED_SCOPE, STEERED_EVALUATION, standing_on, NO_CHANGE, widened, CHANGED, changed, stand_on, move_to, InWorktree, crates/fiddle-runtime/src/capability/workflow.rs, crates/fiddle-runtime/src/capability/cve.rs, crates/fiddle-runtime/src/github/answer.rs, crates/fiddle-runtime/src/workspace/mod.rs, workflows/prompts/toil.md, workflows/prompts/change_evaluate.md, crates/fiddle-acceptance/tests/toil.rs, a_member_review_widens_the_change_and_the_change_it_earns_is_published_and_answered, a_workspace_moves_to_a_commit_and_refuses_to_move_over_changes, a_retry_over_a_branch_this_invocation_already_published_reaches_the_effect_tail, a_rerun_whose_tree_changed_is_not_forced_over_the_branch_the_first_run_published

## Context

OBSERVED on 2026-10-01, live run 14 of `jira:ISP-263`. The brief carried both Claude comments from #270, and the review on #275 asked `Claude's comment 1 and 2 seems legit ones. Should we do them ?`. The answer the run posted did not mention them. `workflows/prompts/toil.md` tells the agent to make nothing the ticket did not ask for, and `workflows/prompts/change_evaluate.md` rejects a change that holds anything the ticket did not ask for. A review that asks for more had no way to be acted on.

The operator decided that a review from somebody who speaks for the project widens the scope.

Building it exposed a second fault. MEASURED by the row below before the fix: a rerun's workspace was made at the fixture's `HEAD`, not at the pull request's head, so a steered change was built on the base and `ensure_branch_published` refused it, `fiddle/ISP-42 exists on the remote and is not an ancestor of HEAD; not forced`. The live runs did not meet it only because the local clone was checked out on the pull request's branch. A runner that clones afresh would.

## Decision

**When direction on the pull request steers a run, what it asks is part of the work, the evaluation judges against the ticket and the direction together, and the run builds on the pull request's head.**

- The agent step of a steered run carries `STEERED_SCOPE` after the direction. It says the direction's asks are work alongside the ticket, that the rule becomes make nothing that neither the ticket nor the direction asked for, and that every ask is answered, with a reason for one the agent does not act on. It holds for direction on the pull request and not for a comment on the ticket, whose rule in the preamble is unchanged.
- The evaluation step of a steered run carries `STEERED_EVALUATION`: a part the direction asked for is part of the change asked for, and what neither asked for is still more than was asked.
- Before a steered run reaches the agent, `stand_on` fetches the pull request's branch and `Workspace::move_to` moves the workspace to the head the steer step read. `move_to` refuses a workspace that already holds changes.
- A steered run that publishes a change answers on the pull request with `CHANGED` and the agent's summary, under the marker. An ask the agent did not act on is answered there.

A first run carries neither sentence. The size bound in `[orchestration.toil]` still holds over the widened change.

## A steered run that changes nothing still runs the publish steps

Standing on the pull request's head means a steered retry that rebuilds the same change finds nothing to commit. Under ADR 087 that run answered and settled at the commit step, so `a_retry_over_a_branch_this_invocation_already_published_reaches_the_effect_tail` stopped reaching the link and the transition: a run that failed after the pull request opened and before those steps would never have them finished.

The operator decided that such a run finishes them. When the commit step finds nothing to commit on a steered run, it records the head the steer step read as the commit the run earned. The branch step finds the branch already there, the pull request step finds it open, the link and transition steps run as they do on any run, and the run answers last. On `jira:ISP-263` that moves the ticket to In Review on its next run.

## What run 15 showed

OBSERVED on 2026-10-01, live run 15. The run completed, answered every ask with a reason, linked #275 on the ticket and moved ISP-263 to In Review. It did not act on the Claude comments, because the review asks `Should we do them ?` and decides nothing. Its answer opened on a headline that claimed the change asked for was already here, which was not true of the two asks it declined, and its text called the checkout `origin/main`.

- `NO_CHANGE` is now `**fiddle made no change for the direction above.**`, which is true whether the work was already there or an ask was declined. The agent's text says which.
- The steered agent step carries `standing_on`, naming the pull request's head commit and saying it is what the pull request holds, not its base branch.

## What run 18 showed

OBSERVED on 2026-10-02, live run 18. The operator's reply `Yes, do Claude's comments 1 and 2.` steered the run with the review it replied to as context. The agent made both changes, the check passed, the evaluation accepted, commit `e04749c` was published onto #275, and the reply was answered by its id.

The same run linked #275 on ISP-263 a second time, comment 183792 beside 183759. A link's identity is the issue at its revision, and run 15's transition had moved the revision. `inspect` now reads through `read_marked_or_holding`: when no comment carries this run's marker, a comment that carries a fiddle marker and the pull request's URL is the link already written. A link a person pasted carries no fiddle marker and does not count. The identity is unchanged, and `the_revision_a_run_observes_is_the_revision_the_link_builds_its_identity_from` now holds that a second revision builds a second identity and writes nothing. Comment 183792 was deleted by hand.

## The rows

`a_member_review_widens_the_change_and_the_change_it_earns_is_published_and_answered`: a steered rerun's agent writes a further change, the evaluation accepts it, the run exits 0, the branch moves to a commit holding that change, and one answer opens on `CHANGED` and names the review. Both sentences reach their briefs, and the first run's briefs carry neither. Without `stand_on` the push is refused and the row fails. Without the answer it fails.

`a_workspace_moves_to_a_commit_and_refuses_to_move_over_changes` holds `move_to`.

`a_rerun_whose_tree_changed_is_not_forced_over_the_branch_the_first_run_published` now holds that a steered rerun's change is published on top of the first run's commit, which is its parent. The guard that refuses a diverged push without force is still held by `a_diverged_push_is_refused_not_forced`. `a_retry_over_a_branch_this_invocation_already_published_reaches_the_effect_tail` reaches the branch, pull request, link and transition steps and then the answer.

## Consequences

- A steered run makes one more fetch.
- A member's review can now make fiddle push a commit onto the pull request and move the ticket through the same effect steps a first run takes.
- A first run's commit is still stamped with the workspace's base date, so a retry rebuilds one commit. A steered commit is not: see below.

## Amended 2026-10-02: a commit says what it is, and a steered one is dated at its direction

ADR 091 replaces the messages below for a report that carries a `commit_message`. They remain for a run whose report carries none. The date rule is unchanged.

OBSERVED on 2026-10-02: the operator read #275 as holding no commit from run 18. `e04749c` was there, but it read exactly as `3acb655`: `fiddle <fiddle@invalid> 2026-09-01T14:09:11Z identities: jira:ISP-263`. Every commit carried the fixed subject `<project>: <invocation>` and the workspace's base date.

- A first run's commit is `<project>: [<ticket>] <ticket summary>` with `Refs: <invocation>` as its body, built from the ticket and nothing the model wrote, so a retry still rebuilds one commit. `a_retry_over_a_branch_this_invocation_already_published_reaches_the_effect_tail` still holds that.
- A steered commit is `<project>: <invocation>, answering the direction on <repo>#<pr>`, with the agent's summary as its body. It lands on the pull request's head, so a retry of it finds nothing to commit and its text need not repeat.
- A steered commit is dated at the newest direction it answers: a review's `submitted_at` or a comment's `created_at`. `commit_described` passes that to `run_dated`. A retry of the same direction reads the same date.

The widened-change row reads both messages and the steered commit's date. Without the date, or with the old subject, it fails.

## Amended 2026-10-02: a hook on the machine does not refuse fiddle's commit

OBSERVED on 2026-10-02, live run 24. The agent fixed the two `planOperations` calls and its check passed, but the commit failed: a `prek` pre-commit hook in the clone's `.git/hooks` asked for a `.pre-commit-config.yaml` that the project does not have. The hook came from fiddle's own development shell, which installs its hooks into the repository the shell starts in. The project's checks are the checks fiddle runs. A hook is the machine's, so `COMMITTER` now sets `core.hooksPath=/dev/null` on every commit fiddle makes.

`a_hook_installed_in_the_clone_does_not_stop_fiddle_committing` installs a hook that refuses every commit in the fixture. The run completes and publishes. Without the setting it fails.
