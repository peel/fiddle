//! The CVE mitigation capability.
//!
//! One directory, so that "what does fiddle do with a scanner's findings?" is
//! answered by reading one module list. [`version`] is the comparison every
//! other part of it asks whether a finding is already fixed, [`project`] is
//! where a scanner's document becomes findings at all — the boundary the prose
//! in that document stops at — and [`attribute`] is what turns a finding into
//! the one edit that could fix it.

pub mod attribute;
pub mod project;
pub mod version;
