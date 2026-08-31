# 078 — A Jira identity names a state of an issue

Status: accepted

Cites: TransitionIssue, AddComment, LinkPullRequest, FileVerdict, TransitionedIssue, MarkedComment, FiledIssue, canonical_revision, canonical_updated, effect_id, EffectName, EffectError::IdentityDiverged, JIRA_ISSUE_TRANSITIONED, JIRA_COMMENT_ADDED, JIRA_PULL_REQUEST_LINKED, JIRA_ISSUE_FILED, ProjectedStatus, ConfiguredNames, WorkState, the_target_names_the_issue_and_the_state_it_was_read_in, the_target_changes_when_the_issue_changes, the_target_names_the_project_and_the_marker_the_search_selects_on, an_issue_whose_updated_field_is_not_a_time_is_refused_rather_than_read_as_a_state, the_receipt_names_the_state_the_issue_was_read_in_and_not_the_one_it_reached, a_typed_state_is_reported_beside_the_status_the_site_named, every_committed_write_moves_the_updated_field_the_identity_is_built_from, the_colonless_offset_jira_cloud_sends_is_read_and_is_never_carried_through_raw, crates/fiddle-runtime/src/jira/transition.rs, crates/fiddle-runtime/src/jira/comment.rs, crates/fiddle-runtime/src/jira/link.rs, crates/fiddle-runtime/src/jira/file_verdict.rs, crates/fiddle-runtime/src/jira/revision.rs, crates/fiddle-runtime/tests/compile_fail/jira_target_without_revision.rs, crates/fiddle-runtime/tests/jira_effects.rs, docs/technical/effects-repository.md

## Context

`docs/technical/effects-repository.md` lists eight cases that re-derivation does
not cover. Case 8 is the wrong object or the wrong revision. A postcondition can
hold for the right issue at a revision the effect never read. The state matches
and the meaning does not.

The document states the Jira consequence as a review rule. A Jira target must
carry the issue key and a revision-like fact. Without the second part a person
approves what they read, the issue changes, and the effect applies the approved
transition to a state nobody approved.

A review rule holds only while a reviewer reads every new operation. M5b adds
four operations at once.

## Decision

**A Jira operation that acts on an existing issue spells its target
`{issue_key}@{issue_updated}`.** `TransitionIssue`, `AddComment` and
`LinkPullRequest` each spell it. `issue_updated` holds `fields.updated`
canonicalised to UTC RFC 3339.

**The derive checks the spelling at compile time.** `target` interpolates struct
fields by name. A placeholder that names no field is a compile error, not a
review finding. `crates/fiddle-runtime/tests/compile_fail/jira_target_without_revision.rs`
pins the error for a struct that carries `issue_key` and no `issue_updated`.

**One canonicaliser answers for every operation.** `canonical_revision` in
`crates/fiddle-runtime/src/jira/revision.rs` is the only one.
`canonical_updated` and `revision_of` wrap it and differ only in the refusal
text. Two canonicalisers that disagree would spell one state two ways and derive
two identities for it.

**A field that is not a time refuses the operation.** `TransitionIssue::new`
answers `JiraError::Malformed` and builds no identity.
`an_issue_whose_updated_field_is_not_a_time_is_refused_rather_than_read_as_a_state`
pins it.

**`FileVerdict` is exempt, because it creates an issue.** It holds no issue key
and no revision when it runs. Its target is `{project_key}/{marker}`. ADR 079
records what that exemption costs.

## The evidence class of each claim

This record grades each claim as **measured**, **argued** or **inferred**, and
names what measured it.

- **Measured.** Jira Cloud's issue resource carries no `version` at the response
  root, so `fields.updated` is the only revision-like fact the site offers. ADR
  077 records the read of `ISP-267` on `snplow.atlassian.net` on 2026-08-26.
- **Measured.** Jira Cloud sends a colonless offset, which RFC 3339 cannot
  spell. `the_colonless_offset_jira_cloud_sends_is_read_and_is_never_carried_through_raw`
  carries the measured value. The target therefore interpolates the
  canonicalised form and never the raw field.
- **Argued, not measured.** `fields.updated` moves when the issue changes. The
  whole value of the revision component rests on this, and no live run has shown
  it. `every_committed_write_moves_the_updated_field_the_identity_is_built_from`
  is a property of the loopback stub in
  `crates/fiddle-runtime/tests/support/stub_jira.rs`, which this milestone also
  wrote. A site that updates the field late, or not for every kind of change,
  would leave the identity stable across a change it is built to notice.
- **Argued.** A stale approval cannot be acted on. The argument is sound and
  rests on the previous item, so it is no stronger than that item.
- **Measured, hermetically.** The target changes when the revision changes.
  `the_target_changes_when_the_issue_changes` and
  `the_target_names_the_issue_and_the_state_it_was_read_in` measure the
  interpolation, not the site.

## Consequences

**An approval expires when anybody touches the issue.** The identity is built to
change on every change, so a busy issue can invalidate an approval between the
read and the person's answer. `Executor::walk` refuses the mismatch as
`EffectError::IdentityDiverged`, which is the correct answer and not a
convenient one. No lane measures how often this happens on a real project.

**A receipt names the state the issue was read in.**
`the_receipt_names_the_state_the_issue_was_read_in_and_not_the_one_it_reached`
pins it. A reader who wants the state the issue reached must read the issue
again.

**`TransitionIssue::inspect` compares the site's own status name, not the
projection.** The receipt value is a `ProjectedStatus`, and
`a_typed_state_is_reported_beside_the_status_the_site_named` pins that. The
branch that decides whether to send a transition compares
`status.jira_status_name` with the requested name. The read also builds
`ConfiguredNames::new(None, None, None, None, None)`, so `[jira.workflow]`
reaches this operation nowhere. ADR 077 measured that `Blocked` and
`In Progress` share the category `indeterminate`, so an unconfigured projection
answers one `WorkState` for two situations. That costs nothing here only because
nothing branches on the `WorkState`. It would cost immediately if a later reader
did.

**The rule is now a compile error and not a review rule.** A fifth Jira
operation that acts on an existing issue cannot omit the revision and still
build.

**What the derive cannot check.** It checks that a field named `issue_updated`
exists. It does not check that the field holds a revision. A struct that names
the field and fills it with a constant compiles.
