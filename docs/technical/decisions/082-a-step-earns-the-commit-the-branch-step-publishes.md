# 082 — A step earns the commit the branch step publishes

Status: accepted
Cites: Workspace, base_date, stamp, GIT_AUTHOR_DATE, GIT_COMMITTER_DATE, env_clear, ensure_branch_published, NonFastForward, InWorktree, land, CveMitigate, a_workspace_command_reads_the_dates_of_the_revision_the_workspace_was_cut_at, two_workspaces_at_one_revision_commit_one_tree_as_one_sha, a_workspace_command_inherits_no_credential, a_retry_over_a_branch_this_invocation_already_published_reaches_the_effect_tail, a_rerun_whose_tree_changed_publishes_the_commit_that_tree_makes, a_rerun_whose_tree_changed_is_not_forced_over_the_branch_the_first_run_published, a_second_attempt_over_the_same_tree_publishes_the_commit_the_first_published, the_same_bump_landed_twice_in_two_workspaces_is_the_same_commit, crates/fiddle-runtime/src/workspace/mod.rs, crates/fiddle-runtime/src/workspace/command.rs, crates/fiddle-runtime/tests/workspace.rs, Step, Ready, StepOutputs, OutputRefusal, StepParams, WorkflowCapability, EnsureBranchPublished, FromStepParams, ProposeChange, WorkflowRefusal, EnsurePullRequestReady, workflows/toil.toml, crates/fiddle-runtime/tests/toil_document.rs, the_branch_step_publishes_the_commit_the_commit_step_made_from_the_agents_work, a_run_whose_agent_wrote_nothing_refuses_at_the_branch_step_and_publishes_no_sha, a_commit_step_is_spelt_by_its_kind_alone_and_carries_no_other_field, a_sha_no_object_in_the_workspace_matches_records_because_the_check_is_spelling_alone, one_run_that_records_two_different_shas_refuses_and_recording_one_twice_does_not, record_head_sha, earned_head_sha, commit_changed, crates/fiddle-runtime/src/capability/commit.rs
Retired: no_step_earns_the_commit_the_branch_step_publishes

## Context

`EnsureBranchPublished::from_params` read `head_sha` from `StepParams`. A run receives its `StepParams` before its first step. A document whose agent step writes the tree could therefore not name the commit the agent made.

`StepOutputs` carried two fields, `pull_request` and `verdict`. No step yielded a commit. `ProposeChange` solves the same problem in Rust: it runs `git commit` in the workspace and passes the object name to `EnsureBranchPublished::new`. The document had no equivalent.

## Decision

**A `commit` step commits the workspace and earns the commit it made.**

`Step::Commit` is the fifth step kind. It carries no field. The step reads `Workspace::changed_files`. It then calls `commit_changed`, which runs `git add` and `git commit` in the workspace and reads `git rev-parse HEAD`. The step records that object name with `StepOutputs::record_head_sha`.

`EnsureBranchPublished::from_params` reads `StepParams::earned_head_sha`. It no longer reads `params.head_sha`.

**A clean workspace earns nothing, and the branch step refuses.**

The commit step makes no empty commit. The run continues to the branch step. The branch step then refuses when it is built, and the reason names `ensure_branch_published`. The refusal lands where the consequence is, and not one step earlier.

**The recorded commit is checked for spelling, and one run earns one commit.**

`OutputRefusal::Misspelt` refuses an answer that is not 40 hexadecimal characters. That is the whole check. It asks no repository whether an object of that name exists, and `a_sha_no_object_in_the_workspace_matches_records_because_the_check_is_spelling_alone` measures the gap: a well-spelt sha that `git cat-file -t` reports absent from a workspace whose own head that same command does resolve records without complaint.

What makes an earned sha a commit is therefore where it comes from, not this check. Outside the tests, `record_head_sha` has one caller: the commit step, which reads `git rev-parse HEAD` after its own `git commit`. `record_head_sha` is public, so a later caller could record a well-spelt name of nothing and this check would not notice. A build that wants the stronger property must resolve the name against a repository and say so here.

`OutputRefusal::Recommitted` refuses a second, different well-spelt sha in the same run. Its check is the same spelling test and an inequality, so its message names what the commit step answered and calls neither value a commit. A later step is given no guess in place of either.

**We rejected: the agent step earns the commit.**

Three reasons. An agent step would then commit whether or not the document asks for a commit, so a document that runs an agent to read a project would also write to it. An evaluation that rejects the change would arrive after the commit was already made, because the evaluation step follows the agent step. And the document could not say where the commit happens, which ADR 074 requires of a format a reader reads.

**We rejected: a `sha` field on the commit step.**

A document that names a commit configures the answer the step is supposed to earn. `deny_unknown_fields` refuses such a field. `a_commit_step_is_spelt_by_its_kind_alone_and_carries_no_other_field` holds both halves.

**A commit carries the dates of the revision the workspace was cut at.**

`commit_changed` set no date, so git stamped the wall clock and one unchanged tree produced a different object name on every run. `ensure_branch_published` then saw a branch that was not an ancestor of its head and refused, correctly, for ever: the guard could not recognise its own work. `fiddle-buu6` carries the finding.

`Workspace::create_at` now reads `git log -1 --format=%cI` at the revision it checked out and keeps that instant. `Workspace::run` passes it to every command it starts as `GIT_AUTHOR_DATE` and `GIT_COMMITTER_DATE`. A commit made in a workspace is therefore a function of its base, its tree, its message and its author, and not of the second it was made in. A rerun over an unchanged tree rebuilds the same object name, so the push is a no-op the remote accepts rather than a divergence it rejects.

`base_date` refuses a revision whose date does not read as an instant, rather than falling back to the clock. A silent fallback would restore the defect in the one case a reader would not look at.

**We rejected: a branch guard that compares trees rather than object names.**

Three reasons. It leaves every commit irreproducible and only moves the comparison. The guard would have to hold its own rule about what counts as the same work, which an object name already answers. And the refusal would still be reached on every rerun, so the cost would be paid on the path that is meant to be ordinary.

**We rejected: widening the push to `--force`.**

A guard that refuses to overwrite a branch it does not recognise is the behaviour we want. The defect was that it could not recognise its own work, and a force flag answers a different question.

## Consequences

The shipped `workflows/toil.toml` names six steps: agent, evaluate, commit, `ensure_branch_published`, `ensure_pull_request` and `jira.pull_request_linked`. `the_branch_step_publishes_the_commit_the_commit_step_made_from_the_agents_work` runs that document against a bare remote and compares the pushed object name with the workspace head. It is not compared with a step parameter.

`no_step_earns_the_commit_the_branch_step_publishes` held the gap as a negative assertion. Two positive tests replace it. `a_run_whose_agent_wrote_nothing_refuses_at_the_branch_step_and_publishes_no_sha` holds the other direction: the run refuses, the workspace head does not move, and the remote holds no branch.

`StepParams::head_sha` stays. `EnsurePullRequestReady::from_params` reads it. That effect has a `Human` minimum, so `WorkflowRefusal::Gated` refuses any document that names it. No workflow step reads `params.head_sha` now.

`StepOutputs` is state that passes between steps. ADR 074 says that variables between steps force a scope, then interpolation, then expressions. `StepOutputs` avoids that because it is closed and typed. A document cannot name a field of it, and no step parameter interpolates one. Adding a third earned value means adding a field and a refusal, and it does not widen the document format.

The commit message is `<project>: <invocation ref>`. `ProposeChange` wrote the same message from its own copy of the same three git calls. Both now call `commit_changed` and `message` in `capability/commit.rs`, so one change moves both. `CveMitigate` still commits through its own `Git` trait, which takes a different message, and this record does not merge that third path. The dates are shared even so, because the trait implementation that reaches a worktree runs through `Workspace::run`.

**`Workspace::run` passes two named variables through `env_clear()`, and both are values the workspace computed.**

A workspace command used to see `HOME`, `LANG` and `PATH`, and `RUSTUP_HOME` when the parent named one. It now sees those and the two dates. Neither date is inherited from the parent process, so no credential reaches a command that could not reach one before. `a_workspace_command_inherits_no_credential` holds the whole list on both arms, and `a_workspace_command_reads_the_dates_of_the_revision_the_workspace_was_cut_at` holds the values. Every command a document runs sees them, not only `git commit`. A project check that reads a commit date therefore reads the base revision's date. No shipped check does, and that last sentence is ARGUED.

**All three committing capabilities are stamped, because all three commit through `Workspace::run`, and each is measured separately.**

`WorkflowCapability` and `ProposeChange` share `commit_changed`. `CveMitigate` still commits through its own `Git` trait, but the implementation that reaches a worktree is `InWorktree`, whose `run` is `Workspace::run`. MEASURED, one row per capability: `a_retry_over_a_branch_this_invocation_already_published_reaches_the_effect_tail` in `crates/fiddle-acceptance/tests/toil.rs` drives two toil runs over one unchanged tree without deleting the branch, and the second reaches `ensure_pull_request` and `jira.pull_request_linked`; `a_second_attempt_over_the_same_tree_publishes_the_commit_the_first_published` drives `ProposeChange` twice and reads the published commit's dates back off the remote; `the_same_bump_landed_twice_in_two_workspaces_is_the_same_commit` lands one bump in two worktrees at one revision through `land` and compares the object names. Each of the three pins the base revision's date rather than only the equality, because two runs inside one second are equal whether or not anything is stamped. The fixture each row runs over takes an empty base commit dated `2021-02-03T04:05:06+02:00`, and the row reads `%cI` and `%aI` off the published commit and compares them with that date. No wall clock in a run produces a 2021 date, so the comparison cannot pass by coincidence. `two_workspaces_at_one_revision_commit_one_tree_as_one_sha` pins the same date for the same reason, because two commits made inside one second are one sha whether or not anything is stamped.

**The guard still refuses a tree it did not publish.**

MEASURED: `a_rerun_whose_tree_changed_is_not_forced_over_the_branch_the_first_run_published` drives a second run whose agent writes different contents, and the run exits 11 at the branch step with the branch unmoved and no second pull request. MEASURED: `a_rerun_whose_tree_changed_publishes_the_commit_that_tree_makes` drives the same pair over a deleted branch and the published object name differs from the first. A fix that made the guard admit everything would pass the convergence row and fail these two.

A run that records two different shas is refused, and `one_run_that_records_two_different_shas_refuses_and_recording_one_twice_does_not` holds that rule on `StepOutputs` directly. A document that writes, commits, writes again and commits again therefore publishes neither commit. That second sentence is an argument from the rule, not a measurement: no test runs a document with two commit steps. A future document that genuinely needs two commits must change this rule, and it must record why here.
