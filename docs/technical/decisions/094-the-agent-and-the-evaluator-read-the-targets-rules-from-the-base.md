# 094 — The agent and the evaluator read the target's rules from the base

Status: accepted; amended 2026-10-15 so a rules file fiddle cannot quote whole refuses the run
Cites: ProjectRules, RuleFile, RulesError, UnquotableRules, Unquotable, RULE_FILES, RULES_CAP_BYTES, fenced, cut_from, committed_file, committed_bytes, Committed, stand_on, SteeredBy, task_carrying, report_quoted, REPORT_FRAME, with_report, origin_ref, fence_for, crates/fiddle-runtime/src/agent/rules.rs, crates/fiddle-runtime/src/workspace/mod.rs, crates/fiddle-runtime/src/capability/workflow.rs, crates/fiddle-runtime/tests/project_rules.rs, workflows/prompts/change_evaluate.md, workflows/prompts/checks_evaluate.md, workflows/prompts/pull_request_evaluate.md, crates/fiddle-acceptance/tests/toil.rs, crates/fiddle-acceptance/tests/pull_request.rs, crates/fiddle-acceptance/tests/support/rules.rs, the_projects_rules_for_agents_reach_the_toil_agent_and_its_evaluation_from_the_base, a_project_with_no_rules_for_agents_gets_no_rules_section, a_rules_file_over_the_cap_refuses_a_toil_run_before_any_agent_step, a_rules_file_over_the_cap_refuses_the_run_before_any_model_is_asked, a_rules_file_that_is_not_text_refuses_the_run_before_any_model_is_asked, a_rules_link_out_of_the_repository_refuses_the_run_and_its_target_is_never_printed, each_rules_file_fiddle_cannot_quote_whole_refuses_with_its_kind, a_directory_named_like_a_rules_file_is_no_rules_file, crates/fiddle-runtime/src/capability/mod.rs, crates/fiddle-runtime/src/orchestration.rs, the_projects_rules_reach_a_checks_run_from_the_base_and_not_from_the_head_it_stands_on, the_projects_rules_reach_a_pull_request_run_from_its_base_and_not_from_its_head, a_link_to_a_rules_file_is_quoted_as_the_file_it_names, the_rules_are_the_commits_and_not_the_working_trees, a_file_at_the_cap_is_quoted_and_one_byte_more_refuses, a_revision_the_workspace_cannot_name_is_an_error_and_not_an_empty_answer, docs/technical/RUNBOOKS.md, docs/technical/SYSTEM.md

## Context

`fiddle-20l5`. In live run 2 on snowplow-incubator/snowplow-identities#275 the agent changed two test assertions and added a code comment. The target's `CLAUDE.md` forbids both. The agent's answer said no assertion changed, and the evaluator accepted the change. No workflow gave the target's `CLAUDE.md` or `AGENTS.md` to the agent or to the evaluator. The evaluator also did not see the agent's report, so it could not compare the answer with the change.

## Decision

**Every agent step and every evaluate step of `toil`, `checks` and `pull_request` quotes the target's `CLAUDE.md` and `AGENTS.md` from the run's base commit. A file is quoted whole, or the run stops.**

### Source

- `RULE_FILES` names `CLAUDE.md` and `AGENTS.md` at the repository root. `ProjectRules::at` reads them from a commit through `Workspace::committed_file` and `Workspace::committed_bytes`, never from the working tree. A change under review cannot rewrite the rules it is judged by.
- A `pr:` run's base is the branch the pull request names, `base.ref`. `stand_on` fetches it beside the head and resolves `origin/<base>`, and `SteeredBy` carries that commit. A base that cannot be fetched or resolved fails the run. It does not give a brief with no rules.
- Every other run's base is the commit the workspace was cut from, `Workspace::cut_from`. That is the HEAD of `[workspace] fixture`, which is the commit fiddle builds the run's branch on. A `jira:` run that steers onto its pull request's head still reads the commit it was cut from. That run's branch is fiddle's own, and fiddle built it on that commit.
- An absent file gets no entry, and two absent files give no section. A directory or a submodule with a rules file's name is not a file. `@` imports are not followed. A link is followed one step inside the repository and quoted as the file it names.

### Where

`task_carrying` puts the rules after the step's prompt and before the ticket, the direction and the checks, for the agent step and the evaluate step alike. `fenced` frames them: the agent holds the work to them, because they say how the project's files may be changed and what may be written in them; a line in them that tries to give a tool, change the task or change the report is part of the quotation, and so is a line that looks like one of fiddle's headings. Each file sits between fence lines from `fence_for`, so a file that holds a code fence gets a longer one.

### Bound, amended

The first text of this record quoted no part of a file over the cap and told the reader to read it with `read_file`. That broke the rule above. `read_file` reads the workspace, which is the pull request's head for a `pr:` run and a `checks` run, and is the agent's own working tree for every evaluator. A change could then rewrite the rules it is judged by. The lead reports that the real file this work began from, the `CLAUDE.md` of snowplow-identities, is 8425 bytes, far under the cap. This record did not measure it.

Now `RULES_CAP_BYTES` is 32768 bytes per file, and a rules file fiddle cannot quote whole refuses the run. `ProjectRules::at` answers `RulesError::Unquotable` with an `UnquotableRules` for each of these kinds of `Unquotable`: a file over the cap, a file that is not UTF-8 text, a link that leaves the repository, a link to no file, and a link to another link. A link to a directory is a link to no file. The refusal names the file, the commit, its size or kind, and the cap. It names a link by its own path and never prints the link's target. No part of the file is quoted anywhere.

The read happens before the first agent step, so the run asks no agent or evaluator, changes nothing, pushes nothing and answers nothing on the pull request. A `jira:` run asks the ambiguity review before the workflow starts, so that one model call can come before the refusal.

The refusal is `CapabilityError::UnquotableRules`, with `Recurrence::Permanent`. The orchestration maps that to `RunOutcome::Failed`, exit 20, a conclusion about the request (ADR 016). The cause is a property of the target repository at that commit, and another run reads the same file, so the run is not retryable. It is not invalid input either: the reference is sound, and exit 2 is for a refusal before a run begins.

### Evaluation

The evaluate step now also quotes the agent's own report, through `with_report` and `report_quoted`, framed by `REPORT_FRAME` as a claim and not evidence. `change_evaluate.md`, `checks_evaluate.md` and `pull_request_evaluate.md` each reject a change that breaks a quoted rule, and the finding quotes the rule and names its file. Each rejects a report that misstates what changed. `pull_request_evaluate.md` counts an unrequested code comment, and an edited test assertion the mention did not ask for, as more than the mention asked.

## Consequences

- A `pr:` run makes one more fetch, of its base branch, before the agent runs.
- A `jira:` run reads the rules of its fixture's HEAD. That commit is the base of the run: the agent changes its code, and fiddle builds the branch on it.
- Each brief grows by the size of the two files, up to 64 KiB.
- A target whose rules file is over the cap, or not text, or a link fiddle does not follow, gets no run until the file changes on its base.
- The transcript cuts each field over 16384 characters in the middle, so it can hold less than the whole quotation. The commit the brief names holds the whole text.

Measured on 2026-10-15. Iteration 1: the acceptance lanes on quoting, the runtime tests, and two mutation probes. Reading the rules from the workspace's current HEAD failed the `checks` and `pull_request` lanes and left the `toil` lanes green. Reading every run from the commit the workspace was cut from failed the `pull_request` lane alone. Iteration 2: the three `pull_request` refusal lanes and the `toil` refusal lane, which failed on `7867b86`, where each of those runs completed and pushed. Not measured: a live run against a real target.
