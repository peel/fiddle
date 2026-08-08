//! Pure domain core for fiddle.
//!
//! This crate carries the shared types and the decision functions that map an
//! observed world onto an assessment. It is deliberately pure: no process,
//! filesystem, network, environment, or clock access, and no async runtime.
//! Later M0 tasks populate `identity`, `observation`, `assessment`, `outcome`,
//! and `report` here; Task 1 only establishes the crate and its boundary.
