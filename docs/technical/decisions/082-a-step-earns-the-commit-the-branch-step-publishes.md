# 082 — A step earns the commit the branch step publishes

Status: accepted
Cites: Step, Ready, StepOutputs, OutputRefusal, StepParams, WorkflowCapability, EnsureBranchPublished, FromStepParams, ProposeChange, WorkflowRefusal, EnsurePullRequestReady, workflows/toil.toml, crates/fiddle-runtime/tests/toil_document.rs, the_branch_step_publishes_the_commit_the_commit_step_made_from_the_agents_work, a_run_whose_agent_wrote_nothing_refuses_at_the_branch_step_and_publishes_no_sha, a_commit_step_is_spelt_by_its_kind_alone_and_carries_no_other_field, a_sha_no_object_in_the_workspace_matches_records_because_the_check_is_spelling_alone, record_head_sha, earned_head_sha, commit_changed, crates/fiddle-runtime/src/capability/commit.rs
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

`OutputRefusal::Misspelt` refuses an answer that is not 40 hexadecimal characters. That is the whole check. It asks no repository whether an object of that name exists, and `a_sha_no_object_in_the_workspace_matches_records_because_the_check_is_spelling_alone` measures the gap: a well-spelt sha that `git cat-file -e` reports absent from the workspace records without complaint.

What makes an earned sha a commit is therefore where it comes from, not this check. Outside the tests, `record_head_sha` has one caller: the commit step, which reads `git rev-parse HEAD` after its own `git commit`. `record_head_sha` is public, so a later caller could record a well-spelt name of nothing and this check would not notice. A build that wants the stronger property must resolve the name against a repository and say so here.

`OutputRefusal::Recommitted` refuses a second, different commit in the same run. A later step is given no guess in place of either.

**We rejected: the agent step earns the commit.**

Three reasons. An agent step would then commit whether or not the document asks for a commit, so a document that runs an agent to read a project would also write to it. An evaluation that rejects the change would arrive after the commit was already made, because the evaluation step follows the agent step. And the document could not say where the commit happens, which ADR 074 requires of a format a reader reads.

**We rejected: a `sha` field on the commit step.**

A document that names a commit configures the answer the step is supposed to earn. `deny_unknown_fields` refuses such a field. `a_commit_step_is_spelt_by_its_kind_alone_and_carries_no_other_field` holds both halves.

## Consequences

The shipped `workflows/toil.toml` names six steps: agent, evaluate, commit, `ensure_branch_published`, `ensure_pull_request` and `jira.pull_request_linked`. `the_branch_step_publishes_the_commit_the_commit_step_made_from_the_agents_work` runs that document against a bare remote and compares the pushed object name with the workspace head. It is not compared with a step parameter.

`no_step_earns_the_commit_the_branch_step_publishes` held the gap as a negative assertion. Two positive tests replace it. `a_run_whose_agent_wrote_nothing_refuses_at_the_branch_step_and_publishes_no_sha` holds the other direction: the run refuses, the workspace head does not move, and the remote holds no branch.

`StepParams::head_sha` stays. `EnsurePullRequestReady::from_params` reads it. That effect has a `Human` minimum, so `WorkflowRefusal::Gated` refuses any document that names it. No workflow step reads `params.head_sha` now.

`StepOutputs` is state that passes between steps. ADR 074 says that variables between steps force a scope, then interpolation, then expressions. `StepOutputs` avoids that because it is closed and typed. A document cannot name a field of it, and no step parameter interpolates one. Adding a third earned value means adding a field and a refusal, and it does not widen the document format.

The commit message is `<project>: <invocation ref>`. `ProposeChange` wrote the same message from its own copy of the same three git calls. Both now call `commit_changed` and `message` in `capability/commit.rs`, so one change moves both. `CveMitigate` still commits through its own `Git` trait, which takes a different message, and this record does not merge that third path.

A document that commits twice over two different trees is refused. A future document that genuinely needs two commits must change this rule, and it must record why here.
