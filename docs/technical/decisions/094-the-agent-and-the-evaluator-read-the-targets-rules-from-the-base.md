# 094 — The agent and the evaluator read the target's rules from the base

Status: accepted
Cites: ProjectRules, RuleFile, Withheld, RULE_FILES, RULES_CAP_BYTES, fenced, cut_from, committed_file, committed_bytes, Committed, stand_on, SteeredBy, task_carrying, report_quoted, REPORT_FRAME, with_report, origin_ref, fence_for, crates/fiddle-runtime/src/agent/rules.rs, crates/fiddle-runtime/src/workspace/mod.rs, crates/fiddle-runtime/src/capability/workflow.rs, crates/fiddle-runtime/tests/project_rules.rs, workflows/prompts/change_evaluate.md, workflows/prompts/checks_evaluate.md, workflows/prompts/pull_request_evaluate.md, crates/fiddle-acceptance/tests/toil.rs, crates/fiddle-acceptance/tests/pull_request.rs, crates/fiddle-acceptance/tests/support/rules.rs, the_projects_rules_for_agents_reach_the_toil_agent_and_its_evaluation_from_the_base, a_project_with_no_rules_for_agents_gets_no_rules_section, a_rules_file_over_the_cap_is_named_with_its_size_and_no_part_of_it_is_quoted, the_projects_rules_reach_a_checks_run_from_the_base_and_not_from_the_head_it_stands_on, the_projects_rules_reach_a_pull_request_run_from_its_base_and_not_from_its_head, a_link_to_a_rules_file_is_quoted_as_the_file_it_names, the_rules_are_the_commits_and_not_the_working_trees, a_file_at_the_cap_is_quoted_and_one_byte_more_is_withheld, a_link_out_of_the_project_bytes_that_are_not_text_and_a_directory_quote_nothing, a_revision_the_workspace_cannot_name_is_an_error_and_not_an_empty_answer, docs/technical/RUNBOOKS.md, docs/technical/SYSTEM.md

## Context

`fiddle-20l5`. In live run 2 on snowplow-incubator/snowplow-identities#275 the agent changed two test assertions and added a code comment. The target's `CLAUDE.md` forbids both. The agent's answer said no assertion changed, and the evaluator accepted the change. No workflow gave the target's `CLAUDE.md` or `AGENTS.md` to the agent or to the evaluator. The evaluator also did not see the agent's report, so it could not compare the answer with the change.

## Decision

**Every agent step and every evaluate step of `toil`, `checks` and `pull_request` quotes the target's `CLAUDE.md` and `AGENTS.md` from the run's base commit. A file is quoted whole or not at all.**

### Source

- `RULE_FILES` names `CLAUDE.md` and `AGENTS.md` at the repository root. `ProjectRules::at` reads them from a commit through `Workspace::committed_file` and `Workspace::committed_bytes`, never from the working tree. A change under review cannot rewrite the rules it is judged by.
- A `pr:` run's base is the branch the pull request names, `base.ref`. `stand_on` fetches it beside the head and resolves `origin/<base>`, and `SteeredBy` carries that commit. A base that cannot be fetched or resolved fails the run. It does not give a brief with no rules.
- Every other run's base is the commit the workspace was cut from, `Workspace::cut_from`. That is the HEAD of `[workspace] fixture`, which is the commit fiddle builds the run's branch on. A `jira:` run that steers onto its pull request's head still reads the commit it was cut from. That run's branch is fiddle's own, and fiddle built it on that commit.
- An absent file gets no entry, and two absent files give no section. A directory or a submodule with a rules file's name is not a file. `@` imports are not followed. A link is followed one step inside the repository and quoted as the file it names. A link out of the repository, or to no file, is named and not quoted.

### Where

`task_carrying` puts the rules after the step's prompt and before the ticket, the direction and the checks, for the agent step and the evaluate step alike. `fenced` frames them like the quoted instruction: they bind how the project's files are changed and what is written in them; they do not give tools, change the task or change the report; a line that looks like one of fiddle's headings is part of the quotation. Each file sits between fence lines from `fence_for`, so a file that holds a code fence gets a longer one.

### Bound

`RULES_CAP_BYTES` is 32768 bytes per file. A file over the cap is `Withheld::OverTheCap`. The brief names the file and its size, says that no part is quoted and why, and tells the reader to read the whole file with `read_file`. A file that is not UTF-8 text is `Withheld::NotText` and is treated the same way. No file is truncated.

### Evaluation

The evaluate step now also quotes the agent's own report, through `with_report` and `report_quoted`, framed by `REPORT_FRAME` as a claim and not evidence. `change_evaluate.md`, `checks_evaluate.md` and `pull_request_evaluate.md` each reject a change that breaks a quoted rule, and the finding quotes the rule and names its file. Each rejects a report that misstates what changed. `pull_request_evaluate.md` counts an unrequested code comment, and an edited test assertion the mention did not ask for, as more than the mention asked.

## Consequences

- A `pr:` run makes one more fetch, of its base branch, before the agent runs.
- `read_file` reads the workspace. For a `pr:` run that is the pull request's head, so a withheld file is read as the head holds it. Only a file over the cap, or one that is not text, has this gap.
- A `jira:` run reads the rules of its fixture's HEAD. A fixture that is behind `[github] base` gives the rules of that older commit, which are also the rules of the code the agent changes.
- Each brief grows by the size of the two files, up to 64 KiB.

Measured: the five acceptance lanes and five runtime tests cited above, and two mutation probes on 2026-10-15. Reading the rules from the workspace's current HEAD failed the `checks` and `pull_request` lanes and left the `toil` lanes green. Reading every run from the commit the workspace was cut from failed the `pull_request` lane alone. Not measured: a live run against a real target. That is the bean's last step.
