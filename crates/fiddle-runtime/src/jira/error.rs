use crate::effect::{AdapterError, EffectOutcome, EffectPhase};

#[derive(Debug, thiserror::Error)]
pub enum JiraError {
    #[error("the site refused the credential with {status}")]
    Unauthorized { status: u16 },

    #[error("the credential may not read this issue: {status}")]
    Forbidden { status: u16 },

    #[error("the site holds no issue `{key}`")]
    Absent { key: String },

    #[error(
        "the site holds no issue `{key}`, or it refused the credential, and \
         `/rest/api/3/myself` could not say which: {why}"
    )]
    AbsentOrRefused { key: String, why: String },

    #[error("the site limited this request and it can be sent again: {0}")]
    RateLimited(String),

    #[error("the site answered with something that is not an issue: {0}")]
    Malformed(String),

    #[error("the site could not be reached: {0}")]
    Unreachable(String),
}

impl AdapterError for JiraError {
    fn outcome(&self, phase: EffectPhase) -> EffectOutcome {
        match phase {
            EffectPhase::Inspect => EffectOutcome::NotCommitted,
            EffectPhase::Apply => match self {
                JiraError::Unauthorized { .. }
                | JiraError::Forbidden { .. }
                | JiraError::Absent { .. }
                | JiraError::AbsentOrRefused { .. }
                | JiraError::RateLimited(_) => EffectOutcome::NotCommitted,
                JiraError::Malformed(_) | JiraError::Unreachable(_) => EffectOutcome::Unknown,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLANTED_HEADER: &str = "Basic aWRlbnQ6dG9rZW4=";
    const PLANTED_TOKEN: &str = "ident@example.com:ATATT-jira-api-token-sentinel";

    fn variant(error: &JiraError) -> &'static str {
        match error {
            JiraError::Unauthorized { .. } => "unauthorized",
            JiraError::Forbidden { .. } => "forbidden",
            JiraError::Absent { .. } => "absent",
            JiraError::AbsentOrRefused { .. } => "absent or refused",
            JiraError::RateLimited(_) => "rate limited",
            JiraError::Malformed(_) => "malformed",
            JiraError::Unreachable(_) => "unreachable",
        }
    }

    fn cases() -> Vec<JiraError> {
        vec![
            JiraError::Unauthorized { status: 401 },
            JiraError::Forbidden { status: 403 },
            JiraError::Absent {
                key: "IDENT-1".into(),
            },
            JiraError::AbsentOrRefused {
                key: "IDENT-1".into(),
                why: "HTTP 503".into(),
            },
            JiraError::RateLimited("HTTP 429".into()),
            JiraError::Malformed("the body is not an issue".into()),
            JiraError::Unreachable("connection refused".into()),
        ]
    }

    fn carrying(planted: &str) -> Vec<JiraError> {
        vec![
            JiraError::Absent {
                key: planted.into(),
            },
            JiraError::AbsentOrRefused {
                key: planted.into(),
                why: planted.into(),
            },
            JiraError::RateLimited(planted.into()),
            JiraError::Malformed(planted.into()),
            JiraError::Unreachable(planted.into()),
        ]
    }

    fn carrying_no_text() -> Vec<JiraError> {
        vec![
            JiraError::Unauthorized { status: 401 },
            JiraError::Forbidden { status: 403 },
        ]
    }

    #[test]
    fn every_read_failure_explains_itself_in_exactly_these_words() {
        let pinned = [
            (
                JiraError::Unauthorized { status: 401 },
                "the site refused the credential with 401",
            ),
            (
                JiraError::Forbidden { status: 403 },
                "the credential may not read this issue: 403",
            ),
            (
                JiraError::Absent {
                    key: "IDENT-1".into(),
                },
                "the site holds no issue `IDENT-1`",
            ),
            (
                JiraError::AbsentOrRefused {
                    key: "IDENT-1".into(),
                    why: "HTTP 503".into(),
                },
                "the site holds no issue `IDENT-1`, or it refused the credential, and \
                 `/rest/api/3/myself` could not say which: HTTP 503",
            ),
            (
                JiraError::RateLimited("HTTP 429".into()),
                "the site limited this request and it can be sent again: HTTP 429",
            ),
            (
                JiraError::Malformed("the body is not an issue".into()),
                "the site answered with something that is not an issue: the body is not an issue",
            ),
            (
                JiraError::Unreachable("connection refused".into()),
                "the site could not be reached: connection refused",
            ),
        ];
        let named: Vec<&str> = pinned.iter().map(|(case, _)| variant(case)).collect();
        let all: Vec<&str> = cases().iter().map(variant).collect();
        assert_eq!(named, all, "every variant of JiraError is pinned here");
        for (case, expected) in pinned {
            let said = format!("{case}");
            let named = variant(&case);
            assert!(!said.trim().is_empty(), "{named} explains itself");
            assert_eq!(
                said, expected,
                "{named} says exactly this, so no word the type itself holds can become a credential"
            );
        }
    }

    #[test]
    fn a_payload_reaches_the_text_verbatim_so_its_construction_site_redacts() {
        for planted in [PLANTED_HEADER, PLANTED_TOKEN] {
            for case in carrying(planted) {
                let said = format!("{case}");
                assert!(
                    said.contains(planted),
                    "{} prints a caller's payload verbatim, so JiraHttp redacts before it builds one: {said}",
                    variant(&case)
                );
            }
            for case in carrying_no_text() {
                let said = format!("{case}");
                assert!(
                    !said.contains(planted),
                    "{} carries a status and no caller text, so no payload reaches its words: {said}",
                    variant(&case)
                );
            }
        }
    }

    #[test]
    fn the_seven_read_failures_read_as_seven_failures() {
        let cases = cases();
        let mut named: Vec<&str> = cases.iter().map(variant).collect();
        named.sort_unstable();
        assert_eq!(
            named,
            [
                "absent",
                "absent or refused",
                "forbidden",
                "malformed",
                "rate limited",
                "unauthorized",
                "unreachable"
            ],
            "every variant of JiraError has a case here"
        );
        let mut said: Vec<String> = cases.iter().map(|case| format!("{case}")).collect();
        said.sort();
        let spoken = said.len();
        said.dedup();
        assert_eq!(said.len(), spoken, "two failures read the same: {said:?}");
    }

    #[test]
    fn every_failure_names_what_a_write_that_ended_this_way_did() {
        let pinned = [
            (
                JiraError::Unauthorized { status: 401 },
                EffectOutcome::NotCommitted,
            ),
            (
                JiraError::Forbidden { status: 403 },
                EffectOutcome::NotCommitted,
            ),
            (
                JiraError::Absent {
                    key: "IDENT-1".into(),
                },
                EffectOutcome::NotCommitted,
            ),
            (
                JiraError::AbsentOrRefused {
                    key: "IDENT-1".into(),
                    why: "HTTP 503".into(),
                },
                EffectOutcome::NotCommitted,
            ),
            (
                JiraError::RateLimited("HTTP 429".into()),
                EffectOutcome::NotCommitted,
            ),
            (
                JiraError::Malformed("the body is not an issue".into()),
                EffectOutcome::Unknown,
            ),
            (
                JiraError::Unreachable("connection refused".into()),
                EffectOutcome::Unknown,
            ),
        ];
        let named: Vec<&str> = pinned.iter().map(|(case, _)| variant(case)).collect();
        let all: Vec<&str> = cases().iter().map(variant).collect();
        assert_eq!(named, all, "every variant of JiraError is pinned here");
        for (case, expected) in pinned {
            let named = variant(&case);
            assert_eq!(
                case.outcome(EffectPhase::Apply),
                expected,
                "{named} during a write means this and nothing weaker"
            );
        }
    }

    #[test]
    fn a_lost_answer_during_apply_is_unknown_and_never_not_committed() {
        let lost: Vec<&str> = cases()
            .iter()
            .filter(|case| case.outcome(EffectPhase::Apply) == EffectOutcome::Unknown)
            .map(variant)
            .collect();
        assert_eq!(
            lost,
            ["malformed", "unreachable"],
            "the site may have written and could not say so in exactly these failures"
        );
    }

    #[test]
    fn the_phase_changes_the_answer_so_a_read_reports_no_write() {
        for case in cases() {
            assert_eq!(
                case.outcome(EffectPhase::Inspect),
                EffectOutcome::NotCommitted,
                "{} during a read changed nothing",
                variant(&case)
            );
        }
        let lost = JiraError::Unreachable("timed out".into());
        assert_ne!(
            lost.outcome(EffectPhase::Inspect),
            lost.outcome(EffectPhase::Apply),
            "an implementation that ignores the phase fails here"
        );
    }

    #[test]
    fn a_jira_failure_stands_where_the_executor_holds_an_adapter_error() {
        let held: Box<dyn AdapterError> = Box::new(JiraError::Unreachable("timed out".into()));
        assert_eq!(
            held.outcome(EffectPhase::Apply),
            EffectOutcome::Unknown,
            "the executor boxes an adapter failure and asks it what a write did"
        );
    }
}
