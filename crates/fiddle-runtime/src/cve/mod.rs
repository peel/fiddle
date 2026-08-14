//! The CVE mitigation capability.
//!
//! One directory, so that "what does fiddle do with a scanner's findings?" is
//! answered by reading one module list. [`version`] is the comparison every
//! other part of it asks whether a finding is already fixed.

pub mod version;
