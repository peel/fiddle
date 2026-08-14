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
//! that is where the child which does it is spawned.

pub mod attribute;
pub mod go;
pub mod project;
pub mod version;
