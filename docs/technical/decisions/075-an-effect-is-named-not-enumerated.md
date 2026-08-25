# 075 — An effect is named, not enumerated

Status: accepted
Cites: EffectName, EffectDescriptor, BUILT_IN, RegistryError, ENSURE_BRANCH_PUBLISHED, PolicyTable, PolicyDocument, UnknownEffect, effect_id, branch_name, ProposedEffect, IntegrationOperation, AdapterError, EffectPhase

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

**The six wire spellings are frozen.** `ensure_branch_published`, `ensure_pull_request`, `ensure_check_requested`, `publish_decision_request`, `ensure_pull_request_ready` and `ensure_pull_request_body` are constants in `fiddle-core`.

## Consequences

**Renaming a frozen spelling is a breaking identity migration, and it can cause a duplicate external mutation.**

`effect_id` hashes the project, the invocation reference, the kind spelling and the target. The kind is an input, so a new spelling gives a new identity for the same real-world object. Two failures follow, and both are concrete.

The identity is written into the world. `branch_name` builds the published branch from a fixed `fiddle/` prefix and the identity derived with `ENSURE_BRANCH_PUBLISHED`. Rename that constant and the branch name changes. A resumed run then inspects for a branch that does not exist under the new name, sees nothing, and publishes a second branch. The first branch is orphaned, and the pull request that points at it is stale.

A person's approval stops answering. `walk` compares `binding.effect` to the derived identity and refuses a decision that does not match. An approval issued before a rename answers the old identity, so it no longer authorizes the effect it was granted for.

A rename therefore needs a migration that maps old identities to new ones, or it needs every in-flight run to be drained first. Neither is free, and neither is implemented. Treat the six spellings as a wire format.

**An adapter reports the outcome, and it knows the phase.** `IntegrationOperation` carries an associated `Error: AdapterError`, and `AdapterError::outcome` takes an `EffectPhase` of `Inspect` or `Apply`. The same transport failure means different things in the two phases. A failure while inspecting mutated nothing. A failure while applying may have mutated the world, and it reports `Unknown` rather than a guess.

**A downstream effect gets the same treatment as a built-in one.** `install` validates a name before the registry accepts it, and both rejection moments read the registry rather than a compiled list. What the two removed comments protected is now enforced by code that runs, not by a type that cannot be extended.
