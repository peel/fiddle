//! How a run ended, and how it was asked to run.
//!
//! Both types are pure descriptions: they name a terminal state and an
//! invocation style, and nothing here consults a process, a file, a clock, or
//! the environment. The *consequence* of an outcome — which exit code the
//! process leaves behind — belongs to the CLI, which owns the exit-code table
//! (design §4.5); the core's job is only to make the terminal state a value
//! rather than a control-flow accident.

use crate::published::Published;

/// The typed result of a run.
///
/// Four variants because they are four different things to do next, not four
/// shades of failure: `Retryable` invites the same invocation again,
/// `Suspended` waits for a decision that has not been made, and `Failed` says
/// the invocation will not succeed by being repeated. A run that returned a
/// bare boolean, or an `Err`, would force the caller to re-invent that
/// distinction from a message.
///
/// Serialized externally tagged with snake_case names, so `Completed` is the
/// bare string `"completed"` and the other three carry their reason under their
/// own key. That spelling is the observable contract of the `--json` payload.
///
/// The three reasons are [`Published`] rather than `String`, and that is a
/// guarantee about the type rather than a habit of its callers: whatever a run
/// was holding when it concluded — a subprocess's output, an `io::Error`, a
/// response somebody else authored — reaches a reader through this enum, and
/// [`Published::of`] is the only way to put it there. A fifth variant added
/// later inherits the bound by being written at all. See [`crate::published`]
/// for what the policy covers and what it deliberately leaves to the places
/// text *enters* the process.
///
/// M0 never produces `Suspended`: it has no human decision point. The variant
/// exists so the exit-code table is complete from the start rather than being
/// widened later, which is exactly the kind of change that lets a code drift.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    /// The run reached the state it was asked to reach.
    Completed,

    /// The run stopped short of a decision it is not entitled to make.
    Suspended { reason: Published },

    /// The run failed at something that may succeed on a later attempt.
    ///
    /// The test is behavioural, not a judgement about severity: *would repeating
    /// this invocation, once someone has fixed what the reason names, succeed?*
    /// An unwritable directory passes that test — a permission is correctable and
    /// the run then completes — so every failure to write durable evidence lands
    /// here rather than in [`RunOutcome::Failed`]. Several distinct causes
    /// therefore share this variant, and the `reason` is what keeps them apart:
    /// it names the change set, the attempt journal, or the report bundle.
    Retryable { reason: Published },

    /// The run will not succeed by being repeated as invoked.
    ///
    /// Reserved for exactly that. A world fiddle could not observe belongs here:
    /// asking again does not make an unreadable source readable, and the run has
    /// concluded something about the world rather than tripped over a correctable
    /// obstacle.
    Failed { error: Published },
}

/// How a run was invoked: with a human available to decide, or without one.
///
/// **Nothing branches on the value, and that is no longer because there is
/// nothing to decide.** This build has a decision point — `propose_change` posts
/// a decision request and suspends — and it is entered whether or not a human was
/// declared to be waiting, because what asks the question is the capability and
/// not the mode. So both modes execute identically. The flag is still part of the
/// surface, and the mode is still recorded in what a run publishes, so the
/// milestone that gives an attended run a transport of its own changes behaviour
/// behind a contract callers already depend on rather than adding one.
///
/// This lives in the core rather than beside the Clap definition because the
/// report bundle records it, and the bundle is a core type — a `Mode` owned by
/// the CLI could not be a field of a `fiddle-core` struct. The CLI supplies the
/// argument parsing; the meaning of the value lives here.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// No human is waiting; the run carries on to a terminal outcome.
    #[default]
    Unattended,

    /// A human is available, so a future milestone may suspend for a decision.
    Attended,
}

impl Mode {
    /// The text this mode is written and serialized as.
    ///
    /// The single source of the spelling: parsing matches against it, `Display`
    /// formats from it, and `serde` renames to the same snake_case, so the flag
    /// value and the payload value can never diverge.
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Attended => "attended",
            Mode::Unattended => "unattended",
        }
    }

    /// Every mode, in the order design §4.5 documents them.
    pub const ALL: [Mode; 2] = [Mode::Attended, Mode::Unattended];

    /// Every mode's spelling, for a parser that wants to offer the choices.
    pub const NAMES: [&'static str; 2] = ["attended", "unattended"];
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A `--mode` value that names no mode fiddle knows.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("unknown mode `{0}`; expected one of attended, unattended")]
pub struct UnknownMode(pub String);

impl std::str::FromStr for Mode {
    type Err = UnknownMode;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Mode::ALL
            .into_iter()
            .find(|mode| mode.as_str() == s)
            .ok_or_else(|| UnknownMode(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire spellings are the CLI's `--json` contract: a consumer matches
    /// on `"completed"` as a bare string and reads a reason out of the others.
    #[test]
    fn outcomes_serialize_under_their_variant_names() {
        assert_eq!(
            serde_json::to_value(RunOutcome::Completed).unwrap(),
            serde_json::json!("completed")
        );
        assert_eq!(
            serde_json::to_value(RunOutcome::Suspended {
                reason: Published::of("awaiting approval")
            })
            .unwrap(),
            serde_json::json!({ "suspended": { "reason": "awaiting approval" } })
        );
        assert_eq!(
            serde_json::to_value(RunOutcome::Retryable {
                reason: Published::of("disk full")
            })
            .unwrap(),
            serde_json::json!({ "retryable": { "reason": "disk full" } })
        );
        assert_eq!(
            serde_json::to_value(RunOutcome::Failed {
                error: Published::of("blocked")
            })
            .unwrap(),
            serde_json::json!({ "failed": { "error": "blocked" } })
        );
    }

    /// The flag value and the payload value are the same word, checked rather
    /// than assumed, because they are produced by two different mechanisms.
    #[test]
    fn a_mode_round_trips_through_its_own_spelling() {
        for mode in Mode::ALL {
            assert_eq!(mode.as_str().parse::<Mode>(), Ok(mode));
            assert_eq!(
                serde_json::to_value(mode).unwrap(),
                serde_json::json!(mode.as_str())
            );
        }
        assert_eq!(Mode::default(), Mode::Unattended);
    }

    #[test]
    fn an_unknown_mode_names_the_value_and_the_alternatives() {
        let rejected = "supervised".parse::<Mode>().unwrap_err();
        let message = rejected.to_string();
        assert!(
            message.contains("supervised")
                && message.contains("attended")
                && message.contains("unattended"),
            "got {message}"
        );
    }
}
