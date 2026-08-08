//! Runtime layer for fiddle.
//!
//! Everything that touches the outside world — ports, stub adapters,
//! capabilities, orchestration, and evidence publication — lives here, so
//! `fiddle-core` can stay pure. Later M0 tasks populate those modules; Task 1
//! only establishes the crate and its dependency on the core.

pub use fiddle_core as core;
