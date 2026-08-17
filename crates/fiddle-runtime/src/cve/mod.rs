//! The CVE mitigation capability.
//!
//! One directory, so that "what does fiddle do with a scanner's findings?" is
//! answered by reading one module list. [`version`] is the comparison every
//! other part of it asks whether a finding is already fixed, [`project`] is
//! where a scanner's document becomes findings at all — the boundary the prose
//! in that document stops at — and [`attribute`] is what turns a finding into
//! the one edit that could fix it. [`go`] is the one construction of a running
//! Go toolchain behind that: attribution's rule 2 cannot decide whether a parent
//! carries a fix without changing a tree and asking a module proxy again, and
//! that is where the child which does it is spawned. [`group`] is what follows
//! from attribution's answers: findings share targets, so the unit of work is
//! the edit rather than the advisory, and a target on its own does not say how
//! far it may be moved. [`dedup`] runs before all of that and decides which
//! findings never enter it: a report names findings the tree has already moved
//! past and findings an earlier commit on this branch already fixed, and the
//! first of those is how a run comes to write a *downgrade* under a security
//! fix's commit message — see [`group::GroupError::AlreadyAtTheFix`], the guard
//! this module exists to keep unreachable.
//!
//! [`fold`] is the second of those two questions asked again, at the other end
//! of a run and against different evidence. [`dedup`] runs before any group and
//! reads the branch's history; [`fold`] runs between groups and reads what the
//! previous group's *rescan* observed, because one bump routinely clears
//! advisories filed against a later group and re-attempting them would open a
//! repair against a tree that already carries the fix. Both are refusals to
//! work, and both are dangerous in the same direction — the fold's whole
//! discipline is about the absences it must not read as proof.

pub mod attribute;
pub mod dedup;
pub mod fold;
pub mod go;
pub mod group;
pub mod project;
pub mod version;
