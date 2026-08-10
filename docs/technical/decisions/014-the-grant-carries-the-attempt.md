# 014 — A capability learns its attempt id from its grant, not from its configuration

**Date:** 2026-08-09
**Status:** accepted

## Context

`FixtureRepair` publishes `repair:<changed>:<attempt>` as the evidence a completed repair earns. The last field is a cross-reference: `crates/fiddle-runtime/src/capability/repair.rs` states that it lets a reader tie the reference back to the record of the same attempt.

It did not. Two attempt ids existed per run, both real and both unique, and they did not name each other:

- `fiddle_runtime::attempt` mints the id the attempt journal and the published bundle are filed under. It mints it itself, deliberately, so that no caller can hand in a duplicate and collide two bundles on one path.
- `main.rs` builds the capability *before* that call — it has to, because `attempt` takes a `&dyn Capability` — and so minted a second id into `RepairConfig.attempt`, which named the ephemeral worktree and became the suffix above.

So the published reference named an attempt that appeared in no bundle and on no disk. Nothing was malformed: the worktree was uniquely named and the evidence was well-formed. What was wrong is worse than a missing field — the *format* implied a tie that did not hold, so a reader who followed it correctly would find nothing and have no way to tell that from a bundle that had been deleted. No test asserted the two agreed, which is why it survived.

`docs/BACKLOG.md`'s 2026-08-09 entry records the gap and names the two ways out: passing an id into `AttemptContext`, which gives up the "minted once, here" property, or handing the id to the capability at `execute` time, which it describes as changing the `Capability` trait.

## Decision

**The `ExecutionGrant` carries the attempt id.** `ExecutionGrant::authorise` takes the derivation *and* the attempt it is being issued under; `RunContext` gains an `attempt` field that `fiddle_runtime::attempt` fills with the id it already minted; `RepairConfig.attempt` is deleted, and `FixtureRepair::execute` reads `grant.attempt_id()` for both the worktree name and the evidence suffix. `fiddle_runtime::mint_attempt_id` is no longer re-exported at the crate root, so the front door offers no way to mint one at the edge.

This is the second of the two options the backlog entry named, taken through the grant rather than through a new parameter — so `Capability::execute`'s signature is unchanged and only the type of an argument it already took is wider.

**Why the grant is the right home.** A grant is not "you may do this"; it is "*this attempt* authorises you to do this". It already carries the capability id for exactly that reason — an implementation must refuse a grant naming a different capability rather than doing that capability's work — and the attempt is the other half of the same sentence. Putting the id anywhere else means a capability holding two claims about which run it is part of, which is the defect restated rather than removed.

**Where an attempt id is minted does not move.** It is still minted once, in `attempt`, before anything is recorded, so the collision property the original design bought is untouched. What changed is that the id now *travels* to the capability, along the one channel that already means "you are authorised, as part of this run", instead of being minted a second time by whoever assembled the capability.

**Why not the other closure.** Dropping the suffix — publishing `repair:<changed>` and carrying no identifier — would also have made the reference honest, and it is a one-line change against this one's eight files. It was rejected because the tie is worth having and was already promised in three places (the module's own documentation, the field's, and the reference's shape), and because the cost is paid once here while the alternative pays a small tax forever: an operator holding a repair's evidence and a directory of bundles would have no way to pair them, which is precisely the question a receipt exists to answer.

## Consequences

**`ExecutionGrant` is no longer `Copy`.** `AttemptId` owns a `String`. The grant is passed by value into `execute` once per execution, so the clone is per-attempt rather than per-call, and `capability_id()` now takes `&self`.

**Every construction site of a grant or a `RunContext` names an attempt.** That is nine call sites, all in tests but one, and it is the point: there is no way to build a grant that does not know which attempt it belongs to, so a third capability cannot reintroduce the gap by minting its own.

**The tie is asserted from outside the process.** `crates/fiddle-acceptance/tests/binary_repair.rs::the_published_evidence_reference_names_the_attempt_the_bundle_is_filed_under` drives the compiled binary through a real repair and reads *both* halves out of the published bundle on disk — the `attempt_id` field and the evidence reference — then checks that the path the run reported contains the same id. `fiddle-runtime`'s `repair_protocol` suite already asserted `repair:1:<ATTEMPT>` against a fixed constant; those assertions were vacuous before, because the constant was fed to the capability directly, and they now mean what they read as.

**Nothing observable changed for the deterministic capability.** `StubMark`'s evidence carries no attempt id, so M0's bundles and its acceptance lane are byte-identical. The ephemeral worktree is now named after the bundle's attempt id rather than a second one, which is invisible to any test — it is removed however the attempt ends — and useful to an operator watching one run.

**This does not touch what an attempt *is*.** One invocation is still one attempt with one id, one journal record and one bundle. ADR 013's point 2 — that an outer retry loop would have to decide whether N tries share an id or publish N bundles — is unaffected and still open; this decision makes the single-id case honest rather than deciding the multiple-id one.

This supersedes no earlier ADR. It closes `docs/BACKLOG.md`'s 2026-08-09 entry "A capability's attempt id is not the bundle's attempt id".
