# 083 — Toil is a document and a gate

Status: accepted
Cites: qualify, recheck, Qualification, Eligibility, Eligible, Refusal, RULES, RuleState, Standing, EvidenceClass, TicketFacts, ModelReview, Scope, Change, OutOfScope, Step, Ready, Workflow, WorkflowFile, WorkflowRefusal, WorkflowCapability, WORKFLOW, WORKFLOW_VERSION, CAPABILITIES, TOIL, StepOutputs, StepParams, Offer, Verdict, RunOutcome, LinkPullRequest, TransitionIssue, AddComment, AskOnIssue, FromStepParams, moved_since_qualifying, refusal_note, selected_workflow, within_scope, neither_effect_is_buildable_from_a_step_alone, crates/fiddle-runtime/src/orchestration.rs, workflows/toil.toml, crates/fiddle-cli/src/main.rs, crates/fiddle-runtime/src/toil/qualify.rs, crates/fiddle-runtime/src/capability/workflow.rs, crates/fiddle-acceptance/tests/toil.rs, crates/fiddle-acceptance/tests/capability_selection.rs, crates/fiddle-runtime/tests/toil_document.rs, an_eligible_ticket_produces_one_pull_request_and_one_jira_link, a_refused_ticket_is_told_why_on_its_own_issue, a_ticket_that_moves_after_it_is_qualified_opens_no_pull_request, a_second_run_over_the_same_ticket_adds_no_second_pull_request_and_no_second_link, a_rerun_whose_branch_is_gone_finds_its_own_pull_request_and_its_own_link, a_workflow_the_judge_rejects_exits_twelve_and_a_workflow_it_accepts_does_not, the_command_line_toil_route_enforces_the_bound_config_check_reports, a_deployment_that_denies_the_comment_effect_publishes_nothing_and_still_refuses, a_site_that_refuses_the_comment_still_refuses_the_ticket, an_absent_toil_table_resolves_to_the_documented_bounds, the_plain_rendering_names_the_label_and_the_bounds_it_resolved, crates/fiddle-cli/src/config.rs, crates/fiddle-acceptance/tests/config_check.rs, assess, correlation_key, write_atomically, StubChangePort, crates/fiddle-core/src/assessment.rs, crates/fiddle-runtime/tests/workflow_capability.rs, a_run_reported_retryable_reaches_a_terminal_state_when_it_is_retried, a_retry_over_a_branch_this_invocation_already_published_reaches_the_effect_tail, record_change_set, the_marker_a_workflow_writes_is_the_marker_the_change_port_observes, the_summary_names_the_marker_the_change_set_carries_and_never_one_nobody_wrote
Retired: a_retry_over_a_branch_this_invocation_already_published_does_not_converge

## Context

Requirement 21 of the product requirements document asks for a toil agent. That document is not in this tree; `docs/specs/` is gitignored and the bean body carries the contract. A labelled Jira ticket becomes one pull request, linked back onto the ticket, with no person in the loop.

ADR 074 had already settled the workflow format. A file is read and never evaluated, it holds no condition, and a capability that wants a condition belongs in Rust. `WorkflowCapability` existed under that rule, and nothing reached it from a command line.

Two questions were left. Where does the judgement "is this work fiddle takes on?" live. And what does the workflow document have to gain before it can carry the toil route at all.

This record grades each claim it makes. MEASURED means a test in this repository observes the behaviour. ARGUED means the claim is read off the source and no test fails if it is wrong. STILL NOT REACHED means no run has done the thing.

Nothing in this record is measured against real infrastructure. Every toil measurement below runs the compiled binary against a loopback model gateway, a loopback Jira server, a compiled stand-in for `gh`, and a local bare git repository as the remote. No toil run has reached a real forge or a real Jira site.

ARGUED, read off ADR 074 and off the source at `77b82f6`: the context above. No test asserts what ADR 074 settled, and none asserts that nothing reached `WorkflowCapability` from a command line before this milestone.

## Decision

**Eligibility is an outer Rust gate. It is not a step.**

ARGUED, read off the source at `77b82f6`: `toil::qualify` runs in `crates/fiddle-cli/src/main.rs`, in `qualified`, before the document is loaded. `RULES` holds the thirteen rule names it applies. It answers `Qualification::Eligible` or `Qualification::Refused`, and a `Refusal` carries the rule that failed, what was found and the remedy that would change the answer.

Three reasons hold the gate outside the document. That placement, all three reasons, and the three paragraphs that state them are ARGUED, read off the source at `77b82f6` and off ADR 074, except where a sentence names its own measurement. ADR 074 gives the first. Eligibility is nothing but conditions, and a condition in a file means the capability belongs in Rust.

The second is cost. An ineligible ticket must pay for no worktree, no forge client and no model call. Placing the judgement inside the document would spend all three to reach the step that declines the work. MEASURED: `a_refused_ticket_is_told_why_on_its_own_issue` in `crates/fiddle-acceptance/tests/toil.rs` counts zero model calls, zero pull requests and zero branches for a ticket without the trigger label, and the run exits 2.

The third is the refusal itself. A refusal is a write on the ticket, and the run that must not start cannot be the run that performs it. So the gate publishes the refusal, and the document never sees the ticket. ADR 084 records what that required of the effect executor.

**A qualification is rechecked against the revision the run observes.**

ARGUED, read off the source at `77b82f6`: `toil::recheck` compares the `fields.updated` the ticket was qualified at with the revision the pre-execution observation read. `moved_since_qualifying` in `crates/fiddle-runtime/src/orchestration.rs` calls it and stops the run when the two differ. MEASURED: `a_ticket_that_moves_after_it_is_qualified_opens_no_pull_request`.

**The gate grades its own rules, and no reader sees the grades.**

ARGUED, read off the source at `77b82f6`: `RuleState` has three variants: `Held`, `Failed` and `NotReached`, and the first two each carry an `EvidenceClass` of `Measured` or `Argued`. A rule decided from a field the tracker returned is therefore distinguishable from one decided by `ModelReview` reading the ticket text. `Refusal` carries that ledger.

ARGUED, read off the source at `77b82f6`: the ledger reaches no surface. `refusal_note` publishes the failed rule, what was found, the remedy and the quoted ticket text, and no more. Nothing outside `crates/fiddle-runtime/src/toil/qualify.rs` reads `Standing`, `rules_held` or `rules_not_reached`. So a person reading the comment on their own ticket is told which rule failed and is not told which of the other twelve were reached, nor on what class of evidence any of them stood.

**`toil` is the selectable name, and `WORKFLOW` stays out of the registry.**

ARGUED, read off the source at `77b82f6`: `CAPABILITIES` holds six ids, and `fiddle_core::TOIL` is one of them. `WORKFLOW` is not, and ADR 074's reason still holds: every name in that array must be selectable on a command line, and a workflow needs a document. What changed is that `toil` names a document at a fixed path. `selected_workflow` maps `Selection::Toil` to `workflows/toil.toml` beside the deployment document, and refuses a file whose `stage` is not `toil`.

**A workflow earns typed outputs, and one step is read-only.**

ARGUED, read off the source at `77b82f6`: `StepOutputs` holds three values: a pull request number, a `Verdict` and a head sha. It is closed and typed. A document names none of the three, so the format gained no variable and no interpolation, which is what ADR 074 forbids. ADR 082 records the head sha half.

ARGUED, read off the source at `77b82f6`: `Step::Evaluate` runs the agent under `Offer::Judge`. That offer returns `READING` alone, so the judge is given `read_file`, `list_files` and `search_files` and is given neither `edit_file` nor `write_file` nor `run_check`. Its answer is a `Verdict` through `output_schema` with a required tool choice, so it is parsed and not read out of prose. A `Verdict::Rejected` breaks the step loop and the run reports `RunOutcome::Rejected`, which exits 12.

MEASURED through the binary: `a_workflow_the_judge_rejects_exits_twelve_and_a_workflow_it_accepts_does_not` in `crates/fiddle-acceptance/tests/capability_selection.rs` runs one document twice and changes only the verdict the gateway stub serves. The rejecting run exits 12 and the accepting run exits 0, so a build that exits 12 whatever the judge said fails the second half. The accepting run also carries the correlation marker its own summary names, and the rejecting run carries none.

**Scope is enforced in Rust, after the agent step and before any effect.**

ARGUED, read off the source at `77b82f6`: `Scope` carries `max_files_changed` and `max_diff_lines`. `WorkflowCapability::within_scope` measures the workspace after each agent step and refuses through `OutOfScope`. The document names no bound; `[orchestration.toil]` does, and an absent table resolves to 10 and 500 rather than to no bound.

MEASURED through the binary: `the_command_line_toil_route_enforces_the_bound_config_check_reports` in `crates/fiddle-acceptance/tests/capability_selection.rs`, where each bound refuses on its own and admits the same change when that one number is loosened. That lane names no effect step, so it measures the bound and not the ordering. The ordering is measured one level down, against the shipped document rather than through the binary: `a_change_beyond_the_bounds_stops_before_any_effect_and_each_bound_bites_on_its_own` and `a_change_inside_both_bounds_runs_to_the_effect_tail` in `crates/fiddle-runtime/tests/toil_document.rs` are the pair.

**The document has no `ask` step, and that is the requirement rather than an omission.**

ARGUED, read off the source at `77b82f6`: `Step` has five shapes: `Agent`, `Evaluate`, `Check`, `Effect` and `Commit`. It is a serde enum tagged by `kind` with `deny_unknown_fields`, so a document naming a sixth kind is refused when it loads. `WorkflowRefusal::Gated` separately refuses a document that names an effect whose minimum is `Human`.

ARGUED: a toil run therefore cannot suspend on a question by construction. Requirement 21 wants that: toil declines decision-heavy work at the gate instead of asking about it. The channel that asks a person a question belongs to `propose_change`, and ADR 081 records it.

## Consequences

**The epic's outcome, both halves, measured through the binary against loopback stubs.**

An eligible ticket produces one pull request and one link comment on the ticket. MEASURED: `an_eligible_ticket_produces_one_pull_request_and_one_jira_link` counts one pull request and one branch off the `gh` stand-in, one link comment off the tracker stub, and the three effect steps in the order the shipped document names them. A second run over the same ticket adds neither, and three tests hold that for three different reasons. `a_second_run_over_the_same_ticket_adds_no_second_pull_request_and_no_second_link` pins the second run reading the correlation marker the first run recorded, deriving `complete`, and never executing the document at all; it pays for the eligibility review and for no agent turn. `a_retry_over_a_branch_this_invocation_already_published_reaches_the_effect_tail` removes that record first, so the run works the ticket again, rebuilds the commit the first run published and reaches all three effect steps, where the pull request step and the link step each recognise their own prior work. It used to refuse at the branch step instead, because the rerun's sha moved; `fiddle-buu6` fixed that and ADR 082 records how. `a_rerun_whose_branch_is_gone_finds_its_own_pull_request_and_its_own_link` removes the record and the remote branch, so the second run reaches all three effect steps, and the pull request step and the link step each name what the first run made. Inspect-before-write on those two effects is therefore measured.

An ineligible ticket is refused on its own issue with the reason on it, and the refusal is performed through the effect executor under an effect identity. MEASURED: `a_refused_ticket_is_told_why_on_its_own_issue`, named above, extracts the effect id from the marker inside the published comment and requires the run to print the same id. `a_deployment_that_denies_the_comment_effect_publishes_nothing_and_still_refuses` holds that deployment policy still governs the write, and `a_site_that_refuses_the_comment_still_refuses_the_ticket` holds that a tracker error does not turn the refusal into something else.

STILL NOT REACHED: no toil run has reached a real forge or a real Jira site. The agreement between these stubs and the real services is argued from M5b's four live Jira lanes against one project on one site, and from nothing on the forge side.

**A successful toil run exited 11, and `fiddle-l4ls` made the workflow write the marker it is judged by.**

Of the two answers this decision left open — the workflow writes the marker, or a workflow capability is judged by something else — `fiddle-l4ls` took the first. `assess` reads one input, the correlation marker in the change set, and five capabilities record it. `WorkflowCapability` was the sixth and recorded none. It now records the change set at the end of a document that ran to its end, through the same `write_atomically` the other five use, so `crates/fiddle-core/src/assessment.rs` is unchanged and nothing moved for `stub_mark`, `fixture_repair`, `publish_change`, `propose_change` or `cve_mitigate`. The second answer was refused for a reason beyond uniformity: a marker is the run's memory, and a workflow judged only by having run to its end would work the ticket again on every rerun.

MEASURED through the binary: `an_eligible_ticket_produces_one_pull_request_and_one_jira_link` reads the marker this invocation expects out of the post-execution observation, reads the same marker off the change set on disk, and pins exit 0 beside the one pull request and the one link. MEASURED at the capability: `the_marker_a_workflow_writes_is_the_marker_the_change_port_observes` in `crates/fiddle-runtime/tests/workflow_capability.rs` reads the marker back through `StubChangePort`, and a run a judge rejected records none.

MEASURED, where it was argued before: a retry converges in both cases. `a_run_reported_retryable_reaches_a_terminal_state_when_it_is_retried` drives a run the pre-execution recheck refused, retries it against a settled ticket, and the retry completes. `a_retry_over_a_branch_this_invocation_already_published_reaches_the_effect_tail` drives the harder case: a run that published a branch and recorded no completion is retried without the branch being deleted, and the retry reaches `ensure_pull_request` and `jira.pull_request_linked`, adds no second pull request and no second link, and records the completion. It converges because `fiddle-buu6` made the commit a function of the tree rather than of the clock; ADR 082 holds that decision. The row that pinned the old behaviour, `a_retry_over_a_branch_this_invocation_already_published_does_not_converge`, said in its own message that it would red when `buu6` landed. It did, and this row replaced it.

MEASURED: `progress[].summary` is now read off the post-run observation of the change set rather than formatted from the expected marker. `the_summary_names_the_marker_the_change_set_carries_and_never_one_nobody_wrote` in `crates/fiddle-runtime/src/orchestration.rs` runs three worlds: a capability that records the expected marker, one that records nothing, and one that records another invocation's marker. Only the first says a marker was written. The second names no marker at all, because naming the expected one is the sentence that read like evidence. The third names both markers and reports that the post-condition is unsatisfied. It names no author, because a read of the change set after the run shows which marker is present and never who put it there; the fixture capability in that world writes the observed marker during the run, so "this run wrote it" and "this run did not write it" are both unsupported. The test refuses an authorship verb in the summary in either direction.

**`jira.pull_request_linked` gained its first caller, and one registered name still has none.**

ARGUED, read off the source at `77b82f6`: `LinkPullRequest::from_params` reads the issue key and the revision out of `StepParams`, which `StepParams::observing` fills from the work item the run observed, and the pull request number out of `StepOutputs`, which the effect step before it earned. ADR 078 requires that identity to come from a read of the issue rather than from a document, and it still does: the document names the effect, and the run supplies the two facts. What changed is that the run now carries them to the step. `fiddle-jgnc` recorded `jira.pull_request_linked` as having no caller; that is out of date for this one name.

The refusal is narrower than it was, not gone. MEASURED: `neither_effect_is_buildable_from_a_step_alone` still passes, because the `StepParams` it builds names no issue key and no revision, and both `AddComment` and `LinkPullRequest` refuse such a set. ARGUED: a document is admitted with a `jira.pull_request_linked` step whatever the run will observe, and a run whose observation carries no revision fails at that step. No test drives that arm.

ARGUED, read off the source at `77b82f6`: `AddComment` and `TransitionIssue` refuse `from_params` unconditionally. `jira.comment_added` is reached instead by two operation types in Rust: `AskOnIssue`, on the decision channel, and `AddComment`, on the toil refusal path. `jira.issue_transitioned` has no caller at all. Requirement 21 also sets the ticket to In Review, and `workflows/toil.toml` records in its own text why no step does: the name is registered, its operation refuses every set of step parameters, so a step naming it would load and then fail after the pull request was already open.

**A rejected evaluation is now reachable from a command line, and the suite that exists for outcomes does not cover it.**

ARGUED, read off `crates/fiddle-acceptance/tests/run_outcome.rs` at `77b82f6`: that file covers outcomes end to end across twelve tests and has no exit-12 case. The coverage sits in `crates/fiddle-acceptance/tests/capability_selection.rs` instead, beside the capability that reaches it.

**An absent `[orchestration.toil]` table resolves to a bound and not to none.** ARGUED, read off the source at `77b82f6`: `WorkflowCapability` holds its scope as an option, and the command-line route was wired before it was bounded, because `fiddle-tikb` reached the capability and `fiddle-vphp` gave it the bound. MEASURED: `config::toil_bounds` resolves a missing table to 10 files and 500 diff lines, so a deployment that writes no such table is bounded rather than unbounded, which `an_absent_toil_table_resolves_to_the_documented_bounds` in `crates/fiddle-cli/src/config.rs` holds. MEASURED through the binary: `config check` reports both resolved values, which is where an operator sees a number they did not write, and `the_plain_rendering_names_the_label_and_the_bounds_it_resolved` in `crates/fiddle-acceptance/tests/config_check.rs` requires the defaults 10 and 500 on that surface.

**Re-labelling one ticket cannot start a second round of work.**

The marker `assess` reads is `correlation_key(project, invocation_ref)`, and a toil invocation names the issue. A ticket that was worked, had its trigger label removed and had it added again therefore carries the marker of the run that already completed. The next run reads that marker, judges the work done and executes nothing, whatever the ticket now says. ARGUED, read off `correlation_key`, `assess` and `WorkflowCapability::record_change_set`: no test drives a re-labelled ticket.

This is the cost of the answer this record took. A marker is the run's memory, and memory keyed by the invocation cannot tell a second request from a repeat of the first. A build that wants a second round needs either a new invocation identity — a marker that carries something the second request changed — or an explicit reset that clears the completion. Neither exists. Deleting the change-set record is what the tests do to force a second round, and it is not an operator surface.

**What the document still cannot express.**

ARGUED, read off `workflows/toil.toml` and the source at `77b82f6`: it cannot branch, because there is no condition, no loop and no variable, and ADR 074 gives the reason. It cannot ask a person anything. It cannot name a bound, an eligibility rule or a commit — those are `fiddle.toml`, Rust and an earned output. It cannot name an effect that acts on an existing Jira issue other than the link, because `FromStepParams` refuses the other two. It cannot set a ticket's status. A route that needs any of those is a Rust capability, which is the escape hatch ADR 074 named and not a widening of this format.
