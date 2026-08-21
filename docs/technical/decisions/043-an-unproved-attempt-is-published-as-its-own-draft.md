# 043 — An unproved attempt is published as its own draft, and it is what the bound counts

Status: accepted
Cites: capability/cve.rs::land, Landed::CommittedForJudgement, Landed::NothingToLand, UNPROVED_LABEL, UNPROVED_BRANCH_STEM, plan_unproved_pull_request, publish_work, Publication, unproved_summary, JUDGEMENT_BODY, CveMitigate::publish_for_judgement, CveMitigate::counted, bound_reached, Row::UnsafeWithoutDirection, Run::judged, a_repair_whose_check_fails_publishes_a_draft_a_person_has_to_judge, an_unproved_draft_grows_one_commit_a_night_until_the_bound_stops_it, the_check_decides_whether_a_reader_sees_a_repair_or_a_draft, a_needs_work_group_is_committed_and_names_no_id_in_any_commit_body

This record supersedes one sentence of [037](037-the-attempt-bound-is-per-pull-request.md) and keeps its decision. 037 carries the amendment.

## Context

`Row::UnsafeWithoutDirection` published nothing. The first production run reported the row and its rationale, and not the change that failed. An operator cannot direct a change they cannot read, so the row named a state nothing could answer.

`land` reverted an attempt that needed work. So publishing the change means the run keeps a commit it used to throw away.

## Decision

Publish the unproved attempt as a draft pull request of its own, on its own branch, under its own label. Keep the row `unsafe_without_direction` and keep `legacy_label` at `upstream-blocked`. Count the run's attempt against the pull request the run continues: the shared one when it is open, and the unproved draft when it is the only open record.

## Consequences

- The shared pull request never receives an unproved attempt. `UNPROVED_BRANCH_STEM` cuts a second branch under the pushable prefix, and `UNPROVED_LABEL` is the only label the draft carries. `find_labelled_pull_request` selects the shared work by `CVE_LABEL`, so it never reaches the draft, and a repair a person can merge stays mergeable.
- A reader tells the two apart twice. The draft is a draft, and it carries a label the repair does not. `the_check_decides_whether_a_reader_sees_a_repair_or_a_draft` runs one world twice, changes the check alone, and asserts both marks differ.
- The draft asks no human gate. It is opened as a draft and it never asks for the ready transition, so no merge queue and no required check acts on it without a person.
- The revert is gone. `land` sends a group that needs work with a change to `Landed::CommittedForJudgement`, and a group that changed nothing to `Landed::NothingToLand`. Nothing calls `git checkout HEAD --` or `git clean` for a group any more, and the worktree is deleted by `Cleanup::Always` either way.
- The judgement commit names no advisory. `FixedInCommits::read` reads every word of every reachable commit body, so a named advisory reads as a fix. `JUDGEMENT_BODY` says this in the commit it writes, and `a_needs_work_group_is_committed_and_names_no_id_in_any_commit_body` holds it.
- 037's decision stands and its one-sentence reason does not. A failed rework of the shared pull request still leaves nothing in that pull request's log, so its log still cannot count its attempts. The unproved branch does grow one commit for each attempt it carries, which the sentence "the attempts the bound exists to count leave nothing in the commit log" did not allow for.
- The unproved branch grows and is never replaced. `land` moves `HEAD` onto the fetched tip of the draft's branch with `git reset --soft` before it commits, so the index keeps the attempt's tree and the parent is the tip. The push is a fast-forward and is never forced. The commit is written with `--allow-empty`, because an attempt that reproduces the tree already on the branch is a fact to record and not a failure to report.
- An unproved publication does not cover a finding. `Row::AlreadyInProgress` reads `commit_log_dedup` over the attempt tree, the unproved branch is never the attempt tree, and the judgement commit names nothing. So the next run attempts the finding again.
- The bound is what stops that. `CveMitigate::counted` reads the count from the draft when no shared pull request is open, and `bound_reached` still runs after both lookups and before `check_out`, so a run at the bound calls no model and cuts no branch. `an_unproved_draft_grows_one_commit_a_night_until_the_bound_stops_it` drives three nights and then the fourth.
- The count carries across the two artifacts. A run whose count subject is the shared pull request writes the same number into the draft it publishes, so a later run that reads the draft continues the count rather than restarting it.
- The failing check's output goes in the body fiddle owns. `unproved_summary` writes the refusal sentence, the advisories, the exit code, the last 4000 characters of the check's output, the files the attempt declared, and the attempt's own note for each advisory. fiddle rewrites this body every run, and it posts no comment, because it owns no comment stream.
- `Row::UnsafeWithoutDirection` now publishes a branch and a pull request number. `Run::judged` carries them, and an operator reaches the diff from `verdicts.json` and the disposition.
- What the project gave up: one open pull request for one repository. A person watching the forge sees up to two, and has to read the label to know which one fiddle stands behind.
