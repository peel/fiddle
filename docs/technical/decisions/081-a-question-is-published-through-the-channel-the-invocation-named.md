# 081 — A question is published through the channel the invocation named

Status: accepted

Cites: DecisionChannel, DecisionChannel::named_by, DecisionChannel::asked_by, authoritative, publish, PublishedAsk, PublishError, ChannelError, CapabilityError::Unasked, ProposeChange, HumanInteractionPort, JiraConversation, GitHubConversation, AskOnIssue, AskedOnIssue, PublishDecisionRequest, asked_already, Decider, resolve, DecisionResolution, WorkItemState, InvocationScheme, JIRA_COMMENT_ADDED, PUBLISH_DECISION_REQUEST, a_jira_run_asks_on_the_issue_and_leaves_the_pull_request_unwritten, a_pull_request_run_asks_on_the_pull_request_and_leaves_the_issue_unwritten, a_jira_run_that_observed_no_revision_asks_nobody_and_names_the_rule, a_jira_run_whose_revision_is_not_a_time_asks_nobody_and_names_the_issue, the_two_refusals_the_channel_rule_gives_are_not_one_refusal, no_invocation_names_two_channels, the_effect_name_the_evidence_line_spells_follows_the_channel, a_pull_request_run_asks_on_the_pull_request_although_it_observed_an_issue, a_second_run_carrying_the_snapshot_it_started_with_recognises_its_own_question, a_run_that_re_reads_the_issue_after_the_write_asks_no_second_time, the_port_and_the_channel_router_name_one_comment_and_write_it_once, the_port_reads_back_every_reply_beside_the_account_that_wrote_it, the_question_the_issue_is_asked_is_identified_by_the_request_and_not_by_the_revision, a_jira_run_reads_the_reply_on_its_own_question_and_proceeds, a_jira_reply_from_an_account_this_deployment_did_not_nominate_decides_nothing, a_jira_account_id_equal_to_an_allowed_github_id_is_not_that_decider, a_jira_account_id_spelled_like_an_allowed_github_id_is_not_that_decider, a_github_author_id_spelled_like_an_allowed_jira_account_is_not_that_decider, JiraDecision, deciders, a_jira_account_the_document_names_reaches_the_allowlist_as_a_jira_decider, one_number_written_in_both_decision_tables_resolves_to_two_deciders, a_jira_decision_table_that_names_nobody_is_refused, an_email_address_is_not_a_jira_account_id, a_mistyped_key_in_the_jira_decision_table_is_refused, config_check_reports_the_jira_accounts_that_may_decide, an_ignored_reply_is_visible_in_what_the_run_published, every_registered_descriptor_builds_the_operation_its_name_means_or_refuses_in_its_name, WorkflowCapability, StepParams, DecisionWalk, orchestration::observe, crates/fiddle-cli/src/config.rs, crates/fiddle-cli/src/render.rs, crates/fiddle-acceptance/tests/config_check.rs, crates/fiddle-runtime/src/human/mod.rs, crates/fiddle-runtime/src/capability/propose.rs, crates/fiddle-runtime/tests/propose_capability.rs, crates/fiddle-runtime/tests/jira_conversation.rs, crates/fiddle-runtime/tests/registry_resolution.rs, crates/fiddle-runtime/tests/workflow_capability.rs, authorized_commenters, authorized_comments, quoted_ticket, within_scope, Eligible, jira.decision, the_gate_and_the_implementer_read_one_text, an_authorized_comment_directs_the_change_and_without_one_the_description_directs_it

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
enumerates seven invocation references against three observations against two
pull-request states, which is 42 combinations, and asserts the distribution those
combinations produce: 4 name the issue, 15 name the pull request, and 23 name nobody.
The bound alone is satisfied by a derivation that answers nothing for every input, so
the sweep counts what it names rather than only what it refuses.

## Decision two — the two unreachable arms are now reachable, and are proved

`PublishError::Channel` and `PublishError::Unaddressable` were arms a single
GitHub channel could not reach. Deriving the channel from the invocation makes
both reachable from a run, so neither is dead weight the GitHub caller carries
for the Jira caller.

- `Channel(ChannelError::NoneNamed)` is what a `jira` run gets when nothing
  observed a revision for the issue. A run that cannot say which snapshot of the
  issue it read did not observe the issue, so an unrevised observation addresses
  nothing.
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

## Decision four — the reply is read through the channel the question was published on

`publish` answers an `InteractionRef`, and that reference is what `resolve`
reads. `DecisionWalk` carries it as `asked_on`. A pull-request comment reaches
`GitHubConversation::responses`; an issue comment reaches
`JiraConversation::responses`. Neither port will read the other channel's
reference, and each refuses by name rather than reading nothing.

The allowlist is widened to match. `Decider` is `GitHubAuthor(u64)` or
`JiraAccount(String)`. A Jira account id is a string the site mints and a GitHub
author id is a number, and the two namespaces are unrelated, so one entry cannot
stand for both. The refusal is a property of the type and not of a comparison a
future caller can loosen.

A deployment names each channel's deciders in that channel's own table:
`[github.decision].authorized` holds numeric user ids and
`[jira.decision].authorized` holds Jira account ids. The Jira table has since
gained a second reader outside this record: the toil eligibility gate weighs a
comment on a ticket against it, because a comment that decides a question the
ticket left open is deciding something. ADR 083 records that, and nothing about
the table's shape or its namespace changed for it. What that one reader now
causes is larger than this record described, and the section below states it. One table per channel, under
the channel's table, mirrors the enum and mirrors `[jira.labels]`, which is where
this document already puts a Jira-only setting. The alternative considered was one
heterogeneous list of tagged entries. It was declined because it makes the GitHub
table carry Jira identities and because it changes the type of a key deployments
already write. `deciders` reads both tables and answers one `Vec<Decider>`, which
is the only list a propose run is given, so a key with no reader cannot appear
here. A propose run whose document names nobody in either table is refused before
it starts, because a run that can ask and can never accept an answer suspends for
ever. An account id is checked for shape at load: an email address or a display
name written where an `accountId` belongs matches no reply, so it is refused at
the table it was written in rather than at the first suspension.

`ProposeChange::walk` looks for a standing question through `asked_already`,
which takes the same channel list `publish` takes and holds the same
`authoritative` rule. Before this, `walk` inspected GitHub unconditionally, so a
Jira-steered run looked for its question where it had never asked it, found
nothing, and asked again.

`AskOnIssue` and `AddComment` both perform `jira.comment_added`, and both post a
comment on an issue, so one deployment policy rule governs both. The registry
holds one constructor for that name, `AddComment`, which refuses to be built from
a step at all, and `AskOnIssue` is constructed only by `publish` and
`asked_already`. The identity each writes into the world is the identity each
reads back: `AddComment` derives its marker from its own target and refuses when
the executor authorized another, and `AskOnIssue` writes the request id it was
given and searches for the same value, so the two cannot diverge.

## What `[jira.decision] authorized` grants

This record introduced the table as the accounts whose reply to a question a run
asked is an answer. It now grants three things, and the third arrived with
`fiddle-aicn` on 2026-09-03.

- **Answering a question a run asked.** `resolve` weighs a reply on the issue
  against the list, and a reply from anybody else decides nothing. This is what
  the table was introduced for.
- **Deciding whether work proceeds.** `authorized_comments` at the toil
  eligibility gate admits a comment from an account in the list, and a comment
  that settles a question the description left open moves the gate from refused
  to admitted. `fiddle-v6pu` did this; ADR 083 records it.
- **Directing the change an agent makes.** The text the gate judged is the text
  the implementer is given, so an authorized comment reaches an agent holding
  `edit_file`, `write_file` and `run_check`, and its choice between the options a
  description weighed is what that agent is told to build. `fiddle-aicn` did
  this; ADR 083 records it.

The table stays one table, and it keeps this name. Four reasons hold that, and
the first is the one that decides it.

The three grants are three consequences of one relationship, not three
relationships. Every reader asks the list the same question — did an account this
deployment trusts to decide questions about its tickets write this? — and each
acts on the same answer. `Decider` is unchanged, `deciders` still builds the list
from both decision tables, and no reader compares anything but a `Decider` value.

Splitting the table would make the incoherence configurable. An account named as
able to decide whether work proceeds and not named as able to direct it is
exactly the state `fiddle-aicn` removed: the gate admits a ticket on a decision
the implementer must then ignore, and the run opens a plausible pull request
implementing the option nobody chose. A second table re-creates that as an option
an operator can select by accident, and a divergence between two lists is silent
in the way a wrong number is silent.

The third grant widens who is trusted by nobody and what a comment can cause by
nothing. The description already reaches the implementer unconditionally and
anybody with edit permission on the ticket can write it, which is a wider set
than this list. What bounds an implementer that was misled is `within_scope`,
which runs after the agent step and before any effect, and deployment policy,
which governs every effect. Blindness never bounded it.

`decision` is the right word in the key, and the narrow reading was in this
record's prose rather than in the name. The alternative names considered were
`[jira.trusted]` and `[jira.authority]`, and both were declined for the reason
this record already declined one heterogeneous list: it changes a key deployments
already write, and it buys nothing the sentence above does not.

What `config check` reports is unchanged, and is still accurate.
`[jira.decision]` reports as `enforced-by-propose-change-and-by-the-toil-gate`,
because the gate is still the only reader in the toil route. The implementer reads
the gate's own output and never the table.

## The evidence class of each claim

A stub measurement is not a live measurement, and a behaviour no run reaches
is neither. Each claim below carries its class.

- **Measured against stubs.** A run invoked as `jira:IDENT-1` posts one comment
  on the issue and zero on the pull request, and a run invoked as `beans:w-1`
  posts one on the pull request and zero on the issue, both against stub sites
  that count requests. The two tests observe the same Jira issue, so the zero in
  each is the counter-case to the one in the other.
- **Measured against stubs, and re-graded.** A second invocation that carries the
  revision the site holds after the first write posts **no** second comment, for
  one comment on one issue.
  `a_run_that_re_reads_the_issue_after_the_write_asks_no_second_time` runs it.
  This row read "posts a second comment, for two comments on one issue" until
  `AskOnIssue` replaced `AddComment` as the question a `jira` channel publishes.
  The earlier reading was a correct measurement of the identity in the tree at
  the time: `AddComment` spells its target `{issue}@{fields.updated}`, and
  `StubJira` advances `fields.updated` on a write, so the second invocation
  derived a marker the first had never written. `AskOnIssue` spells its target
  `{issue}#{request}`, which the revision does not move, so the second
  invocation's `inspect` finds the comment the first one wrote.
  `the_question_the_issue_is_asked_is_identified_by_the_request_and_not_by_the_revision`
  holds the identity apart from the revision and holds it apart from a constant:
  a question about another commit is another request, so another target.
- **Unmeasured, and its class is unchanged.** That Jira Cloud advances
  `fields.updated` when a comment is added. No test in this tree reads a live
  site's `fields.updated` after a comment is added, so this record still observes
  that behaviour on the stub only, and no live probe has been run since. It is
  expected because `fields.updated` names the time the issue last changed and a
  comment changes the issue. What has changed is the consequence, not the class:
  the advance no longer produces a duplicate, because the question is no longer
  identified by the revision. A live site that advanced `fields.updated` and one
  that did not would both leave the question asked once.
- **Counted on the tree.** One of six capabilities asks a person anything, and
  it reaches `publish`. Counted by `impl Capability for` under `crates/*/src`.
  That search answers ten: six production capabilities, and four test doubles in
  the `#[cfg(test)]` module `crates/fiddle-runtime/src/orchestration.rs` opens
  at line 418. The six are the denominator. Of the six, only `ProposeChange`
  constructs a `HumanDecisionRequest`, at
  `crates/fiddle-runtime/src/capability/propose.rs:215`. The instrument is a
  search of the source. Nothing runs.
- **Measured by an executing test.** `named_by` names one channel or none across
  all 42 combinations `no_invocation_names_two_channels` enumerates, distributed
  4, 15 and 23. The test builds every input and reads every answer.
- **Argued.** `NotOne` earns its place although no derivation reaches it.
- **Not reached.** A workflow document that spells a `publish_decision_request`
  step reaches `PublishDecisionRequest` without passing through `publish`. A
  registry test builds it. No run does.
- **Measured against stubs, and re-graded from Not reached.** A Jira run reads
  the reply to its own question and proceeds.
  `a_jira_run_reads_the_reply_on_its_own_question_and_proceeds` drives
  `ProposeChange` twice against `StubJira` and a GitHub stub. The first
  invocation asks on the issue and suspends. A nominated account then replies on
  the issue. The second invocation, carrying the later revision the first write
  left behind, reads that reply, interprets it, and marks the pull request ready:
  one GraphQL mutation, one comment on the issue across both invocations, and
  zero comments on the pull request. `resolve` reads through the channel the
  question was published on, which it takes from the `InteractionRef` the ask
  earned, so `JiraConversation::responses` is now called by a capability and not
  only by a test.
- **Measured against stubs.** A reply from an account this deployment did not
  nominate leaves the question standing.
  `a_jira_reply_from_an_account_this_deployment_did_not_nominate_decides_nothing`
  runs the same two invocations with the reply written by a stranger and gets
  `AwaitingDecision` and zero mutations. It is the counter-case to the row above:
  without it, a run that can read no reply at all would satisfy neither and both
  would look alike.
- **Measured against stubs.** A Jira account id and a GitHub author id cannot
  satisfy one allowlist entry. `Decider` is
  `GitHubAuthor(u64) | JiraAccount(String)`, and the allowlist is compared
  against that value rather than against a number.
  `a_jira_account_id_equal_to_an_allowed_github_id_is_not_that_decider` seeds a
  Jira reply from account id `505401` against an allowlist naming GitHub author
  `505401`, gets `AwaitingDecision` and zero mutations, then names the same
  string as a Jira account and gets the mutation. Two unit cases,
  `a_jira_account_id_spelled_like_an_allowed_github_id_is_not_that_decider` and
  `a_github_author_id_spelled_like_an_allowed_jira_account_is_not_that_decider`,
  hold the refusal in both directions and pair each with the case that corrects
  only the channel.
- **Measured at the configuration boundary, and re-graded from Counted on the
  tree, and a gap.** A deployment names a Jira decider under `[jira.decision]`,
  and the account it writes there reaches the allowlist a propose run is given as
  a `Decider::JiraAccount`. `deciders` is the one place that builds that list,
  from both decision tables, and `crates/fiddle-cli/src/main.rs` passes what it
  answers.
  `a_jira_account_the_document_names_reaches_the_allowlist_as_a_jira_decider`
  parses a document and compares the resolved list, and pairs that with the same
  document minus the table, so the row cannot pass on a resolver that appends an
  account to every document.
  `one_number_written_in_both_decision_tables_resolves_to_two_deciders` writes
  `70121` in both tables and asserts the two entries are unequal, so the type
  refusal above holds at the document as well as in the walk.
  `config_check_reports_the_jira_accounts_that_may_decide` reads the same key back
  through the shipped binary.
  This row read "No deployment can name a Jira decider yet" until `[jira.decision]`
  was admitted, and the earlier reading was a correct measurement: the allowlist
  was built from `[github.decision].authorized` alone, so a Jira-steered
  deployment read its reply and declined it as `ActorNotAuthorized`.
  The class is a document measurement joined to a stub measurement, and it is not
  a site measurement. Nothing here reads a live Jira account id. The run half is
  the stub-measured row above, and the join between the two halves is the one
  account string written in the document in `crates/fiddle-cli/src/config.rs` and
  supplied to the capability in
  `crates/fiddle-runtime/tests/propose_capability.rs`. No test drives the binary
  through two Jira invocations end to end.
- **Measured against stubs, and re-graded twice.** `config check` reports
  `[github.decision]` as `enforced-by-propose-change` and `[jira.decision]` as
  `enforced-by-propose-change-and-by-the-toil-gate`. It reported
  `accepted-not-enforced` for both with the phrase "no capability in this build
  reads it", and that reading was already false when written: `main.rs` fed
  `[github.decision].authorized` into the propose configuration,
  `ProposeChange::walk` passes it to `resolve` as the allowlist, and
  `an_ignored_reply_is_visible_in_what_the_run_published` drives the shipped
  binary against a document naming one authorized id and records the reply the
  allowlist declined. It then reported both tables as `enforced-by-propose-change`,
  and the Jira half of that reading stopped being complete when `fiddle-v6pu` made
  the toil eligibility gate read the same table: `[jira.decision].authorized` now
  also names the accounts whose comment on a ticket is context the ambiguity
  review reads, and a comment from anybody else is not. `fiddle-aicn` then carried
  the text the gate judged to the implementer, so a comment from one of those
  accounts also directs the change an agent makes; the section above states all
  three grants and argues why one table carries them. Neither change added a
  reader of the table, so the status word did not move again. ADR 083 records both
  decisions and grades them. The GitHub status word is still scoped to
  `propose-change`, because no other capability reads that table.

## Consequences

**A `jira` run reads the answer on the issue it asked on.** `DecisionWalk`
carries the `InteractionRef` the ask earned, and `resolve` reads through the
channel that reference names: `GitHubConversation` for a pull-request comment,
`JiraConversation` for an issue comment. `ProposeChange::walk` looks for a
standing question on the channel `DecisionChannel::named_by` chose, through
`asked_already`, so a Jira-steered run no longer inspects a GitHub pull request
for a question it never asked there.

One deterministic order serves both channels, and two steps read differently
inside it.

- `select_candidates` weighs one `Decider` value for both channels. On GitHub a
  comment is not a person when the site says `type: Bot` or names an app. Jira
  says neither, so on Jira a comment is not a person when it comes from the
  account that asked the question, which is the account this run writes as.
- `re_read_candidates` re-reads each GitHub comment by id, because the listing
  and the body acted on are two reads and something can change between them. One
  Jira read answers the listing and the bodies together, so there is no window
  and no second read. `DecisionError::ReplyEdited` is therefore a GitHub-only
  refusal. The request comment is still held to `created == updated` on both
  channels, and `DecisionError::RequestEdited` is reachable from both.

**Idempotence holds for a retry and for a fresh observation.**

*Measured against stubs.* A run that carries the revision it started with finds
the comment it already wrote and posts nothing further.
`a_second_run_carrying_the_snapshot_it_started_with_recognises_its_own_question`
runs that case against `StubJira` and counts one comment on the issue.

A fresh invocation carrying a later revision is now the same case.
`a_run_that_re_reads_the_issue_after_the_write_asks_no_second_time` calls
`publish` twice in one process and gives the second call the revision `StubJira`
holds after the first write. The issue carries one comment, and both calls name
the same effect id and the same comment.
`a_jira_run_reads_the_reply_on_its_own_question_and_proceeds` runs the same shape
through the capability rather than through `publish` alone, and counts one
comment across two invocations.

*Argued from source, and now inconsequential.* `run` calls `ctx.observe`, which
is `orchestration::observe`, before it derives the next action, at
`crates/fiddle-runtime/src/orchestration.rs:146`. A second run therefore reads
the issue again instead of carrying the first run's snapshot, and where that read
answers a later revision the run holds a later revision. It no longer builds a
different question from it. This link is still read from the source; no test
executes it, and nothing now depends on it.

**The revision is a precondition on the ask and not part of its identity.**
`AskOnIssue::new` refuses a `fields.updated` it cannot read, so a `jira` channel
naming an unreadable revision publishes nothing and
`PublishError::Unaddressable` stays reachable from a run. The canonical revision
is carried into `AskedOnIssue` and printed in the postcondition sentence, which
tells a reader which snapshot of the issue the run held when it asked. It is not
in the target, so it cannot move the question.

**A second asking path exists, is unreached by a run, and is exercised by a
test.** A workflow document can spell a `publish_decision_request` step, which
the registry builds from `StepParams` without passing through `publish`.
`WorkflowCapability::new` has 19 call sites and every one is a test:
`tests/workflow_capability.rs` holds 18 and `tests/registry_resolution.rs` holds
the nineteenth. That same file holds the only `decision_request: Some(...)` in
the tree, and
`every_registered_descriptor_builds_the_operation_its_name_means_or_refuses_in_its_name`
asserts `publish_decision_request` is among the six descriptors that build from
`StepParams`. So the bypass is not merely spellable; it is built and asserted
today, and only the absence of a production caller keeps a run off it. When a
document is given that step, it has to be routed through the selector or it will
ask on the wrong channel.
