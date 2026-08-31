# 081 — A question is published through the channel the invocation named

Status: accepted

Cites: DecisionChannel, DecisionChannel::named_by, DecisionChannel::asked_by, authoritative, publish, PublishedAsk, PublishError, ChannelError, CapabilityError::Unasked, ProposeChange, HumanInteractionPort, JiraConversation, GitHubConversation, AddComment, PublishDecisionRequest, WorkItemState, InvocationScheme, JIRA_COMMENT_ADDED, PUBLISH_DECISION_REQUEST, a_jira_run_asks_on_the_issue_and_leaves_the_pull_request_unwritten, a_pull_request_run_asks_on_the_pull_request_and_leaves_the_issue_unwritten, a_jira_run_that_observed_no_revision_asks_nobody_and_names_the_rule, a_jira_run_whose_revision_is_not_a_time_asks_nobody_and_names_the_issue, the_two_refusals_the_channel_rule_gives_are_not_one_refusal, no_invocation_names_two_channels, the_effect_name_the_evidence_line_spells_follows_the_channel, a_pull_request_run_asks_on_the_pull_request_although_it_observed_an_issue, crates/fiddle-runtime/src/human/mod.rs, crates/fiddle-runtime/src/capability/propose.rs, crates/fiddle-runtime/tests/propose_capability.rs, crates/fiddle-runtime/tests/jira_conversation.rs

## Context

A person steers a run through the channel the work came from. A run started as
`jira:ISP-42` that stops for a decision has to ask on that issue. A run started
from a pull request has to ask there.

`DecisionChannel`, `authoritative` and `publish` were built for that choice and
nothing reached them. Measured at `32f16ef`: no file outside
`crates/fiddle-runtime/src/human/mod.rs` constructed a `DecisionChannel`.
`ProposeChange` asked a person by building a `PublishDecisionRequest` itself.

`fiddle-jgnc` recorded two costs that made `ProposeChange` decline the selector.
Both are real, and this record answers each.

1. `publish` widens the error with two arms one GitHub channel cannot reach.
2. `publish` hides the effect name the receipt evidence line spells.

## Decision one — the channel follows the invocation, not the observation

`DecisionChannel::named_by` takes the invocation reference, the work item the
run observed, and the pull request the run holds. A `jira` scheme names the
issue and the revision it was observed at. Every other scheme, and a reference
that does not parse, names the pull request.

The rule is the invocation and never the observation. A pull-request run that
observed a Jira issue for another reason still asks on the pull request.
`a_pull_request_run_asks_on_the_pull_request_although_it_observed_an_issue`
holds that direction.

`named_by` answers zero channels or one, never two. `no_invocation_names_two_channels`
enumerates every scheme against every observation and pins it.

## Decision two — the two unreachable arms are now reachable, and are proved

`PublishError::Channel` and `PublishError::Unaddressable` were arms a single
GitHub channel could not reach. Deriving the channel from the invocation makes
both reachable from a run, so neither is dead weight the GitHub caller carries
for the Jira caller.

- `Channel(ChannelError::NoneNamed)` is what a `jira` run gets when nothing
  observed a revision for the issue. A comment on an issue builds its identity
  from the revision the issue was read at, so an unrevised observation
  addresses nothing.
  `a_jira_run_that_observed_no_revision_asks_nobody_and_names_the_rule` runs it.
- `Unaddressable` is what a `jira` run gets when the observed revision is not a
  `fields.updated` a target can be spelled from, per ADR 078.
  `a_jira_run_whose_revision_is_not_a_time_asks_nobody_and_names_the_issue`
  runs it.

`ChannelError::NotOne` stays unreachable from a run, because `named_by` answers
at most one channel. **That arm is accepted, not removed.** It guards the list
`publish` takes, which is a slice a future caller can fill from more than one
source, and one question answered twice on two channels is worse than a refusal.
`the_two_refusals_the_channel_rule_gives_are_not_one_refusal` runs the empty
list and the crowded list and holds that each carries its own reason.

Both refusals reach a capability as `CapabilityError::Unasked`, whose
recurrence is permanent for both: a run that observed no revision observes none
on a retry either.

## Decision three — the effect name is returned, not hidden

`publish` answers `PublishedAsk`, which carries the `EffectName` the chosen
channel performed beside the receipt. `DecisionChannel::asked_by` spells
`publish_decision_request` for a pull request and `jira.comment_added` for an
issue.

This is what removes the second cost. The caller writes its receipt evidence
line from the name the selector answered, so routing through the selector
changes no evidence line on the GitHub path and gives the Jira path a truthful
one. `the_effect_name_the_evidence_line_spells_follows_the_channel` pins the
mapping, and the two run tests pin the whole evidence sequence each channel
earns.

## The evidence class of each claim

- **Measured.** A run invoked as `jira:IDENT-1` posts one comment on the issue
  and zero on the pull request, and a run invoked as `beans:w-1` posts one on
  the pull request and zero on the issue, both against stub sites that count
  requests. The two tests observe the same Jira issue, so the zero in each is
  the counter-case to the one in the other.
- **Measured.** One of six capabilities asks a person anything, and it reaches
  `publish`. Counted by `impl Capability for` under `crates/*/src` and by which
  of those files construct a `HumanDecisionRequest`.
- **Argued.** `NotOne` earns its place although no derivation reaches it.

## Consequences

**A `jira` run asks on the issue and cannot yet read the answer there.**
`ProposeChange::continue_from` resolves a reply through `DecisionWalk`, which
names a repository, a pull request and GitHub author identifiers. A Jira run
therefore suspends on every invocation rather than interpreting a comment. The
question is not asked twice: `AddComment` recognises its own marker at the
revision it was written for. Reading the reply from the issue is the next step
and is not in this record.

**A second asking path exists and is unreached.** A workflow document can spell
a `publish_decision_request` step, which the registry builds from `StepParams`
without passing through `publish`. `StepParams` carries `decision_request: None`
everywhere outside one registry unit test, and `WorkflowCapability` has no
caller outside tests, so no run reaches it. When a document is given that step,
it has to be routed through the selector or it will ask on the wrong channel.
