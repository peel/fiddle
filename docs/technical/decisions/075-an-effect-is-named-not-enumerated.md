# 075 — An effect is named, not enumerated

Status: accepted
Cites: EffectName, EffectDescriptor, BUILT_IN, RegistryError, ENSURE_BRANCH_PUBLISHED, JIRA_ISSUE_FILED, JIRA_COMMENT_ADDED, JIRA_ISSUE_TRANSITIONED, JIRA_PULL_REQUEST_LINKED, PolicyTable, PolicyDocument, UnknownEffect, effect_id, branch_name, ProposedEffect, IntegrationOperation, AdapterError, EffectPhase, FromStepParams, resolve, describe

## Context

Until this milestone the set of effects was a Rust type. Two comments said why, and both are now removed from the tree. They are quotable from commit `a7fdf2f`.

`EffectKind`, in `fiddle-core`, was a closed enum. Its comment said it was **"Closed rather than a free string … an unrecognised kind is a rejected effect, not one fiddle will attempt on the strength of a name it does not know."** It added that the enum **"makes the identity's kind component a spelling this build controls."**

`PolicyTable`, in `fiddle-cli`, was a struct with one field for each kind. Its comment said it was **"Exhaustive rather than a map, and that is the point of the type: a map would admit a key for an effect kind this build has never heard of and accept it silently, which is precisely how a rule an operator believed they had written comes to apply to nothing."**

Both comments rest on the same argument. The type rejects an unknown name, so no other code has to. That argument holds only while every effect is compiled into this binary. A downstream binary that adds an effect cannot add a variant to an enum in this crate.

## Decision

**An effect is identified by a parsed name, not by an enum variant.**

`EffectKind` is gone. `EffectName` replaces it, and `BUILT_IN` in `fiddle-runtime` holds one `EffectDescriptor` for each effect this build performs. `install` adds a further slice for a downstream binary. `RegistryError` refuses a duplicate name and a name that `EffectName::parse` rejects. Nothing loads at run time.

`PolicyTable` is now a map from `EffectName` to `DeploymentRule`.

**The registry rejects an unknown name, and it does so at two moments.**

1. **Config load.** `TryFrom<PolicyDocument> for PolicyTable` reads each key. A key that `EffectName::parse` rejects, or that `registry::describe` does not find, is collected. Any such key refuses the whole document, and the refusal names every effect the build does perform. This replaces what the exhaustive struct did, and it keeps the property the old comment asked for: a rule that governs nothing is refused, never accepted in silence.

2. **Execution.** `Executor::walk` calls `registry::describe` on the proposed name. An unregistered name returns `EffectError::UnknownEffect`. The check runs before the first traced step, so it is earlier than capability validation and earlier than identity derivation.

**The execution check is not redundant with the config check.** `ProposedEffect` has four public fields, and a capability constructs its own. No constructor and no type stands between a capability and a proposal. The executor check is therefore the only guard at that moment, and it must stay even though the config check exists.

**The ten wire spellings are frozen.** `ensure_branch_published`, `ensure_pull_request`, `ensure_check_requested`, `publish_decision_request`, `ensure_pull_request_ready`, `ensure_pull_request_body`, `jira.issue_filed`, `jira.comment_added`, `jira.issue_transitioned` and `jira.pull_request_linked` are constants in `fiddle-core`.

M5b added the last four. They were written as `pub const` in their own operation modules while three lanes built them in parallel, so that no two lanes edited `fiddle-core/src/effect.rs` at once. That reason expired when the lanes merged. A shipped spelling belongs in `fiddle-core` because the consequence below is about the spelling and not about the module that happens to hold the operation.

**`jira.transition` is not a spelling this build ships.** It is the suite's example of a name no descriptor holds. Registering it as an alias was measured: it reds six tests that depend on the name staying unregistered, in four files.

Four of the six reach the registry through `describe`: `lookup_refuses_a_name_no_descriptor_holds`, `an_unregistered_proposal_is_refused_before_an_identity_is_derived` and `an_unregistered_name_is_refused_ahead_of_the_capability_it_names` through `Executor::walk`, and `an_effect_this_build_does_not_perform_is_refused_when_the_workflow_is_built` through `WorkflowCapability::new`. One reaches it through `resolve`: `a_name_no_descriptor_holds_resolves_to_no_constructor`. One reads `BUILT_IN` directly: `an_admissible_extension_is_answered_beside_the_built_ins` installs a test extension that claims the name, so a built-in of the same name makes `admissible` answer `Duplicate`.

**Five further tests spell the name and are not evidence for this.** They stay green whether or not the name is registered, so a reader must not count them. `a_name_no_rule_key_spells_is_left_ungated` (`fiddle-cli/src/config.rs`) tests that `rule_for` allows a row no document wrote, which holds for a registered name too. `a_name_outside_the_grammar_is_refused` (`fiddle-core/src/effect.rs`) tests the grammar. `every_effect_failure_declares_which_exit_row_it_belongs_in`, `no_other_permanent_refusal_became_a_wait` and `no_effect_failure_a_workflow_can_meet_is_a_wait` build an `EffectError` value and ask the registry nothing.

Bean `fiddle-cphb` adds a seventh dependent case that has not landed: a toil document naming `jira.transition` must refuse at load.

## Consequences

**Renaming a frozen spelling is a breaking identity migration, and it can cause a duplicate external mutation.**

`effect_id` hashes the project, the invocation reference, the kind spelling and the target. The kind is an input, so a new spelling gives a new identity for the same real-world object. Two failures follow, and both are concrete.

The identity is written into the world. `branch_name` builds the published branch from a fixed `fiddle/` prefix and the identity derived with `ENSURE_BRANCH_PUBLISHED`. Rename that constant and the branch name changes. A resumed run then inspects for a branch that does not exist under the new name, sees nothing, and publishes a second branch. The first branch is orphaned, and the pull request that points at it is stale.

A person's approval stops answering. `walk` compares `binding.effect` to the derived identity and refuses a decision that does not match. An approval issued before a rename answers the old identity, so it no longer authorizes the effect it was granted for.

A rename therefore needs a migration that maps old identities to new ones, or it needs every in-flight run to be drained first. Neither is free, and neither is implemented. Treat the ten spellings as a wire format.

**An adapter reports the outcome, and it knows the phase.** `IntegrationOperation` carries an associated `Error: AdapterError`, and `AdapterError::outcome` takes an `EffectPhase` of `Inspect` or `Apply`. The same transport failure means different things in the two phases. A failure while inspecting mutated nothing. A failure while applying may have mutated the world, and it reports `Unknown` rather than a guess.

**Registration says this build performs the effect. It does not say a step can name it.**

`resolve` is `describe(name).map(|descriptor| descriptor.construct)`, so every descriptor carries a `Construct`. Six built-ins build an operation from a `StepParams`. The four Jira effects refuse one, and the refusal is the correct answer rather than an unfinished constructor.

Each Jira target carries an observed revision of an issue, or a scan verdict. `TransitionIssue`, `AddComment` and `LinkPullRequest` put the issue's `fields.updated` into the target, so the identity names one state of one issue. `FromStepParams::from_params` is synchronous and takes no adapter, so it cannot read that field. Putting a snapshot of the issue into `StepParams` would not help: a snapshot taken earlier derives an identity for a state the issue has left, which is the failure the revision was added to prevent. `FileVerdict` needs an advisory, a package and a rationale, which exist in a scan verdict and not in a workflow step; a constructor built from defaults would file a real ticket made of them.

So the four are registered because `Executor::walk` refuses an unregistered name before its first traced step, and because `PolicyTable` gates only a registered name. They are constructed by the capability that holds the observation. `every_registered_descriptor_builds_the_operation_its_name_means_or_refuses_in_its_name` measures both lists by asking every descriptor, and a Jira effect that moved into the building list gained a constructor made of defaults.

**A downstream effect gets the same treatment as a built-in one.** `install` validates a name before the registry accepts it, and both rejection moments read the registry rather than a compiled list. What the two removed comments protected is now enforced by code that runs, not by a type that cannot be extended.
