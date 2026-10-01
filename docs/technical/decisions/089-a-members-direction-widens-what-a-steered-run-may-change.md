# 089 — A member's direction widens what a steered run may change, and the run works on the pull request's head

Status: accepted
Cites: STEERED_SCOPE, STEERED_EVALUATION, standing_on, NO_CHANGE, widened, CHANGED, changed, stand_on, move_to, InWorktree, crates/fiddle-runtime/src/capability/workflow.rs, crates/fiddle-runtime/src/capability/cve.rs, crates/fiddle-runtime/src/github/answer.rs, crates/fiddle-runtime/src/workspace/mod.rs, workflows/prompts/toil.md, workflows/prompts/change_evaluate.md, crates/fiddle-acceptance/tests/toil.rs, a_member_review_widens_the_change_and_the_change_it_earns_is_published_and_answered, a_workspace_moves_to_a_commit_and_refuses_to_move_over_changes, a_retry_over_a_branch_this_invocation_already_published_reaches_the_effect_tail, a_rerun_whose_tree_changed_is_not_forced_over_the_branch_the_first_run_published

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

## The rows

`a_member_review_widens_the_change_and_the_change_it_earns_is_published_and_answered`: a steered rerun's agent writes a further change, the evaluation accepts it, the run exits 0, the branch moves to a commit holding that change, and one answer opens on `CHANGED` and names the review. Both sentences reach their briefs, and the first run's briefs carry neither. Without `stand_on` the push is refused and the row fails. Without the answer it fails.

`a_workspace_moves_to_a_commit_and_refuses_to_move_over_changes` holds `move_to`.

`a_rerun_whose_tree_changed_is_not_forced_over_the_branch_the_first_run_published` now holds that a steered rerun's change is published on top of the first run's commit, which is its parent. The guard that refuses a diverged push without force is still held by `a_diverged_push_is_refused_not_forced`. `a_retry_over_a_branch_this_invocation_already_published_reaches_the_effect_tail` reaches the branch, pull request, link and transition steps and then the answer.

## Consequences

- A steered run makes one more fetch.
- A member's review can now make fiddle push a commit onto the pull request and move the ticket through the same effect steps a first run takes.
- The workspace's base date, which fiddle stamps on its commits, is still the date of the commit it was made at, not of the head it moved to.
