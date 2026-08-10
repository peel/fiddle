//! Black-box acceptance suite for fiddle.
//!
//! There is no production code here. Every test under `tests/` launches the
//! compiled `fiddle` binary as a subprocess, or inspects the workspace's own
//! metadata and sources, so nothing in this crate may be linked into a shipped
//! artifact. The library target exists only to give the package a target.
