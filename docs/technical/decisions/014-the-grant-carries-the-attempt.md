# 014 — A capability learns its attempt id from its grant

Date: 2026-08-09
Status: accepted
Cites: ExecutionGrant::authorise, ExecutionGrant::attempt_id, RunContext, fiddle_runtime::attempt, mint_attempt_id, crates/fiddle-acceptance/tests/binary_repair.rs::the_published_evidence_reference_names_the_attempt_the_bundle_is_filed_under

## Context

`FixtureRepair` publishes `repair:<changed>:<attempt>`, and the last field promises a tie to the record of the same attempt. Two attempt ids existed per run, both real and both unique, and they did not name each other. So the published reference named an attempt that appeared in no bundle and on no disk.

## Decision

Make the `ExecutionGrant` carry the attempt id, and delete `RepairConfig.attempt`. Give `RunContext` an `attempt` field that `fiddle_runtime::attempt` fills from the id it already minted. Stop re-exporting `mint_attempt_id`, so the front door offers no way to mint one at the edge.

## Consequences

- `ExecutionGrant` is no longer `Copy`, because `AttemptId` owns a `String`. The grant passes by value into `execute` once per execution, so the clone is per-attempt rather than per-call.
- Every construction site of a grant or a `RunContext` names an attempt. That is nine sites, all in tests but one. It is the point: a third capability cannot reintroduce the gap.
- The tie is asserted from outside the process. `the_published_evidence_reference_names_the_attempt_the_bundle_is_filed_under` drives the compiled binary through a real repair and reads both halves from the published bundle.
- Nothing observable changed for the deterministic capability. `StubMark`'s evidence carries no attempt id, so M0's bundles and its acceptance lane are byte-identical.
- The project gave up the cheaper closure. Publishing `repair:<changed>` with no identifier was one line against this decision's eight files.

`fiddle_runtime::attempt` minted the id the journal and the bundle are filed under. `main.rs` minted a second into `RepairConfig.attempt`, because it had to build the capability first.

Nothing was malformed before: the worktree was uniquely named and the evidence was well-formed. What was wrong is worse than a missing field, because the format implied a tie that did not hold. A reader who followed it correctly would find nothing, and could not tell that from a bundle somebody had deleted. No test asserted the two agreed, which is why it survived.

`docs/BACKLOG.md`'s 2026-08-09 entry named two ways out. One passes an id into `AttemptContext` and gives up the minted-once property. The other hands the id to the capability at execute time. This is the second, taken through the grant, so `Capability::execute`'s signature is unchanged and only one argument's type is wider.

**Why the grant is the right home.** A grant is not "you may do this". It is "this attempt authorises you to do this". It already carries the capability id for that reason, because an implementation must refuse a grant naming a different capability. The attempt is the other half of the same sentence. Putting the id anywhere else leaves a capability holding two claims about which run it is part of. That restates the defect rather than removing it.

**Where an attempt id is minted did not move.** It is still minted once, in `attempt`, before anything is recorded. So the collision property survives. What changed is that the id now travels to the capability. It travels along the one channel that already means "you are authorised as part of this run".

**Why the cheaper closure was rejected.** The tie is worth having, and three places promised it already. Those were the module's documentation, the field's, and the reference's shape. This decision pays once, and the alternative pays a small tax forever. An operator holding a repair's evidence and a directory of bundles could not pair them. That is the question a receipt exists to answer.

The ephemeral worktree now carries the bundle's attempt id rather than a second one. No test can see that, and an operator watching one run can. `fiddle-runtime`'s `repair_protocol` suite already asserted `repair:1:<ATTEMPT>` against a fixed constant. Those assertions were vacuous, because the constant was fed to the capability directly. They now mean what they read as.

This does not touch what an attempt is. One invocation is still one attempt, one journal record and one bundle. ADR 013's second point stays open, and this decision makes the single-id case honest rather than deciding the multiple-id one. It closes `docs/BACKLOG.md`'s 2026-08-09 entry "A capability's attempt id is not the bundle's attempt id".
