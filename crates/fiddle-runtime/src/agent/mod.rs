//! The bounded rig: everything a model can see, say, or do.
//!
//! [`tools`] is the whole of that surface. Four tools, each one a typed Rig
//! [`Tool`](rig_agent::tool::Tool), and nothing else reaches the outside world
//! on the model's behalf.
//!
//! The property the module exists to hold is a separation of two channels that
//! look alike from inside a tool and are not alike at all. A tool's *arguments*
//! are authored by the model; a tool's *context* is authored by the host. Which
//! workspace is being repaired, whether the attempt is still live, and which
//! program the check runs are all host facts, so they travel through
//! [`ToolHost`](tools::ToolHost) in Rig's
//! [`ToolContext`](rig_agent::tool::ToolContext) — never as a field of an
//! `Args` struct, and never in an advertised JSON schema. A schema is a menu:
//! anything named on it is something the model may fill in, and a workspace root
//! the model may fill in is not a workspace root at all.
//!
//! [`audit`] is the other half of the same idea, applied to what is written
//! down rather than to what is granted. The tools record themselves; the Rig
//! hook in [`audit`] only watches. Which of the two an operator ends up
//! trusting is decided here, not later.

pub mod audit;
pub mod tools;

pub use audit::AuditHook;
pub use tools::{
    CheckOutcome, ListFiles, NoArgs, ReadFile, ReadFileArgs, RunCheck, ToolError, ToolHost,
    WriteFile, WriteFileArgs, WriteReceipt,
};

/// What the runtime observed for itself over one attempt.
///
/// Recorded by the tools rather than by a Rig hook, so that evidence never
/// depends on hook behaviour: Rig's own documentation calls hooks controls
/// rather than authorization, and a control that stops firing must not be able
/// to silently empty the record of what happened.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct ToolReceipts {
    pub calls: Vec<ToolReceipt>,
}

/// One tool call, as the runtime saw it.
///
/// Three fields, and the shortness is a decision rather than a placeholder.
/// Receipts are published in the evidence bundle, so every field has to be safe
/// to publish without anybody re-reading it first — which rules out the
/// requested path (model-authored, and unbounded), the file contents, and the
/// resolved path (the operator's filesystem layout). What is left answers the
/// questions a bundle is actually asked: which tools ran, how each went, and
/// where the time went.
///
/// `outcome` is one of six classes, and which writer produced it is part of
/// reading it correctly:
///
/// | outcome | written by | means |
/// |---|---|---|
/// | `ok` | the tool body | it did the thing |
/// | `refused` | the tool body | we declined, before the filesystem was touched |
/// | `cancelled` | the tool body | the attempt was stopped from outside |
/// | `failed` | the tool body | we acted and the world did not cooperate |
/// | `malformed` | [`AuditHook`] | the model's arguments did not decode, so no body ran |
/// | `unknown_tool` | [`AuditHook`] | the model named a tool that does not exist |
///
/// The first four are the record proper and do not depend on a hook being
/// installed. The last two describe calls that never reach a tool body at all,
/// which is why nothing but a hook could ever have seen them; see [`audit`] for
/// why that does not make the evidence hook-contingent.
///
/// `duration_ms` is zero for the two hook-written classes, honestly: there was
/// no body to time.
///
/// A `&'static str` rather than an enum because the set is closed at the points
/// that write it and the only consumers are a serializer and a human reading
/// JSON.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ToolReceipt {
    pub tool: String,
    pub outcome: &'static str,
    pub duration_ms: u64,
}
