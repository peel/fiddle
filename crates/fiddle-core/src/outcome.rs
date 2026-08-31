use crate::published::Published;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, fiddle_macros::VariantCount)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    Completed,

    Suspended { reason: Published },

    Retryable { reason: Published },

    Rejected { findings: Vec<Published> },

    Failed { error: Published },
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, fiddle_macros::VariantCount,
)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Unattended,

    Attended,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Attended => "attended",
            Mode::Unattended => "unattended",
        }
    }

    pub const ALL: [Mode; Mode::VARIANT_COUNT] = [Mode::Attended, Mode::Unattended];

    pub const NAMES: [&'static str; Mode::VARIANT_COUNT] = ["attended", "unattended"];
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

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
            serde_json::to_value(RunOutcome::Rejected {
                findings: vec![Published::of("a public signature the ticket did not name")]
            })
            .unwrap(),
            serde_json::json!({
                "rejected": { "findings": ["a public signature the ticket did not name"] }
            })
        );
        assert_eq!(
            serde_json::to_value(RunOutcome::Failed {
                error: Published::of("blocked")
            })
            .unwrap(),
            serde_json::json!({ "failed": { "error": "blocked" } })
        );
    }

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
