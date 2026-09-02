# 084 — A deployment may hold a tracker and no forge

Status: accepted
Cites: EffectContext, GhCli, GitCli, JiraHttp, GhError, GitError, JiraError, Executor, AuthorizedEffect, EffectReceipt, AdapterError, EffectOutcome, Recurrence, CapabilityError, DeploymentPolicy, PolicyTable, AddComment, publish_refusal, tracker_client, qualifies_a_ticket, crates/fiddle-runtime/src/effect/mod.rs, crates/fiddle-cli/src/main.rs, crates/fiddle-runtime/tests/jira_effect_credential.rs, crates/fiddle-acceptance/tests/toil.rs, a_tracker_only_deployment_comments_through_the_executor_and_reaches_no_forge, a_refused_ticket_is_told_why_on_its_own_issue, a_deployment_that_denies_the_comment_effect_publishes_nothing_and_still_refuses, a_site_that_refuses_the_comment_still_refuses_the_ticket

## Context

`EffectContext` is the one value every effect adapter reads its client from. Until `a5ca6f6` it held `gh: GhCli` and `git: GitCli` outright, and `jira: Option<JiraHttp>` beside them. A forge client was therefore a precondition of performing any effect at all, including one that touches no forge.

ADR 083 made that a problem. The toil eligibility gate refuses an ineligible ticket by writing a comment on the ticket, and it does so before any run exists. A deployment that only reads and writes a tracker has no `[github]` table, no forge credential and no attempt worktree. Under the old type it could not build an `EffectContext` without inventing a `GhCli` and a `GitCli` it holds no credential and no worktree for. So it could not reach `Executor` honestly, and its one write would have been a raw adapter call outside the seven-step order that ADR 033 requires of every mutation.

This is a decision about the effect executor and not about toil. It is recorded separately for that reason.

This record grades each claim as ADR 083 does. MEASURED means a test in this repository observes the behaviour. ARGUED means the claim is read off the source and no test fails if it is wrong. STILL NOT REACHED means no run has done the thing.

ARGUED, read off the history at `a5ca6f6` and the source at `77b82f6`: the two context paragraphs above, which state the shape `EffectContext` held and what that shape prevented.

## Decision

**All three clients are optional, and an absent one is an adapter error.**

ARGUED, read off the source at `77b82f6`: `EffectContext` now holds `gh: Option<GhCli>`, `git: Option<GitCli>` and `jira: Option<JiraHttp>`. Three accessors replace the field reads: `gh_client`, `git_client` and `jira_client`. Each answers the client or the adapter's own `Unconfigured` error, whose text names the table the deployment did not write.

ARGUED, read off the source at `77b82f6`: `EffectContext::new` still takes a forge and a local git, so a forge deployment is constructed exactly as before. `EffectContext::tracking` is the new constructor and takes a `JiraHttp` alone.

**An absent client is permanent, not correctable.**

ARGUED, read off the source at `77b82f6`: `GhError::Unconfigured` and `GitError::Unconfigured` classify `EffectOutcome::NotCommitted` in both phases, because no request was sent. `CapabilityError::recurrence` maps both to `Recurrence::Permanent`: a missing table is a fact about the deployment document, and running the same command again cannot change it. `JiraError::Unconfigured` already worked this way. ADR 076 records the rule these three follow: the executor takes the outcome the adapter names, and an unclassified failure is `Unknown` rather than `NotCommitted`.

**A tracker-only write keeps the whole protocol.**

MEASURED: `a_tracker_only_deployment_comments_through_the_executor_and_reaches_no_forge` in `crates/fiddle-runtime/tests/jira_effect_credential.rs` builds a context with `EffectContext::tracking`, asserts that `gh_client` and `git_client` both refuse, and then performs an `AddComment` against a loopback tracker stub. It reads the recorded trace and requires the steps `inspect_postcondition`, `combine_policy`, `authorize`, `apply` and `observe_postcondition`, which is the order a raw adapter call would skip. The receipt names an external reference and the published comment carries the effect marker.

MEASURED through the binary: `a_refused_ticket_is_told_why_on_its_own_issue` in `crates/fiddle-acceptance/tests/toil.rs` runs a deployment document with no `[github]` table, and the refusal reaches the ticket with the effect identity the receipt names printed on the operator's line. Deployment policy still governs the write: `a_deployment_that_denies_the_comment_effect_publishes_nothing_and_still_refuses` denies the effect and the ticket receives nothing while the run still refuses, and `a_site_that_refuses_the_comment_still_refuses_the_ticket` holds that a tracker error leaves the refusal a refusal rather than turning it into something else.

STILL NOT REACHED: no tracker-only deployment has written to a real Jira site. Every measurement above is against a loopback stub.

## Consequences

**No type says which clients an effect needs.** ARGUED, read off the source at `77b82f6`: `EnsurePullRequest` asks for a forge client at the moment it runs, and nothing before that moment refuses a deployment that names a forge effect and holds no forge. The refusal is late and it is clear, but it is a runtime error rather than a load-time one. This is the same shape ADR 075 records for effect names, and it is closed by review.

**The policy table a tracker-only deployment needs lives under `[github]`.** ARGUED, read off the source at `77b82f6`: `qualified` in `crates/fiddle-cli/src/main.rs` falls back to `config::PolicyTable::default()` when no `[github]` table exists, so such a deployment cannot strengthen the minimum on the one effect it performs. `fiddle-xhgd` carries that.

**An effect performed outside a run has nowhere to file its receipt.** ARGUED, read off the source at `77b82f6`: the eligibility gate runs before any bundle exists, so the `EvidenceRef` for the refusal comment lands in no report and the attempt trace at that moment has no journal behind it. The effect id is printed on the operator's line instead, which is durable in captured output and in nothing else. Every other effect this system performs leaves a receipt in a bundle. `fiddle-7jo5` carries that, and it is a consequence of this decision rather than of the refusal path.
