use crate::effect::{AdapterError, EffectOutcome, EffectPhase, RetryAdvice};

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

    #[error("this deployment holds no `[jira]` configuration, so no request was sent")]
    Unconfigured,

    #[error("{count} issues carry the marker `{marker}`, and this write files one issue or none")]
    Ambiguous { marker: String, count: usize },
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
                | JiraError::RateLimited(_)
                | JiraError::Unconfigured
                | JiraError::Ambiguous { .. } => EffectOutcome::NotCommitted,
                JiraError::Malformed(_) | JiraError::Unreachable(_) => EffectOutcome::Unknown,
            },
        }
    }

    fn advice(&self) -> RetryAdvice {
        match self {
            JiraError::Unauthorized { .. }
            | JiraError::Forbidden { .. }
            | JiraError::Absent { .. }
            | JiraError::AbsentOrRefused { .. }
            | JiraError::RateLimited(_)
            | JiraError::Malformed(_)
            | JiraError::Unreachable(_)
            | JiraError::Unconfigured
            | JiraError::Ambiguous { .. } => RetryAdvice::default(),
        }
    }

    fn is_worth_reading_again(&self) -> bool {
        match self {
            JiraError::AbsentOrRefused { .. }
            | JiraError::RateLimited(_)
            | JiraError::Unreachable(_) => true,
            JiraError::Unauthorized { .. }
            | JiraError::Forbidden { .. }
            | JiraError::Absent { .. }
            | JiraError::Malformed(_)
            | JiraError::Unconfigured
            | JiraError::Ambiguous { .. } => false,
        }
    }

    fn duplicates(&self) -> Option<usize> {
        match self {
            JiraError::Ambiguous { count, .. } => Some(*count),
            JiraError::Unauthorized { .. }
            | JiraError::Forbidden { .. }
            | JiraError::Absent { .. }
            | JiraError::AbsentOrRefused { .. }
            | JiraError::RateLimited(_)
            | JiraError::Malformed(_)
            | JiraError::Unreachable(_)
            | JiraError::Unconfigured => None,
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
            JiraError::Unconfigured => "unconfigured",
            JiraError::Ambiguous { .. } => "ambiguous",
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
            JiraError::Unconfigured,
            JiraError::Ambiguous {
                marker: "fx-abc123".into(),
                count: 2,
            },
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
            JiraError::Ambiguous {
                marker: planted.into(),
                count: 2,
            },
        ]
    }

    fn carrying_no_text() -> Vec<JiraError> {
        vec![
            JiraError::Unauthorized { status: 401 },
            JiraError::Forbidden { status: 403 },
            JiraError::Unconfigured,
        ]
    }

    #[test]
    fn every_jira_failure_explains_itself_in_exactly_these_words() {
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
            (
                JiraError::Unconfigured,
                "this deployment holds no `[jira]` configuration, so no request was sent",
            ),
            (
                JiraError::Ambiguous {
                    marker: "fx-abc123".into(),
                    count: 2,
                },
                "2 issues carry the marker `fx-abc123`, and this write files one issue or none",
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
    fn the_nine_jira_failures_read_as_nine_failures() {
        let cases = cases();
        let mut named: Vec<&str> = cases.iter().map(variant).collect();
        named.sort_unstable();
        assert_eq!(
            named,
            [
                "absent",
                "absent or refused",
                "ambiguous",
                "forbidden",
                "malformed",
                "rate limited",
                "unauthorized",
                "unconfigured",
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
            (JiraError::Unconfigured, EffectOutcome::NotCommitted),
            (
                JiraError::Ambiguous {
                    marker: "fx-abc123".into(),
                    count: 2,
                },
                EffectOutcome::NotCommitted,
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

    #[test]
    fn every_failure_says_whether_a_second_read_can_settle_it() {
        let pinned = [
            (JiraError::Unauthorized { status: 401 }, false),
            (JiraError::Forbidden { status: 403 }, false),
            (
                JiraError::Absent {
                    key: "IDENT-1".into(),
                },
                false,
            ),
            (
                JiraError::AbsentOrRefused {
                    key: "IDENT-1".into(),
                    why: "HTTP 503".into(),
                },
                true,
            ),
            (JiraError::RateLimited("HTTP 429".into()), true),
            (
                JiraError::Malformed("the body is not an issue".into()),
                false,
            ),
            (JiraError::Unreachable("connection refused".into()), true),
            (JiraError::Unconfigured, false),
            (
                JiraError::Ambiguous {
                    marker: "fx-abc123".into(),
                    count: 2,
                },
                false,
            ),
        ];
        let named: Vec<&str> = pinned.iter().map(|(case, _)| variant(case)).collect();
        let all: Vec<&str> = cases().iter().map(variant).collect();
        assert_eq!(named, all, "every variant of JiraError is pinned here");
        for (case, expected) in pinned {
            let named = variant(&case);
            assert_eq!(
                case.is_worth_reading_again(),
                expected,
                "{named} answers whether a later read can settle it, and nothing weaker"
            );
        }
    }

    #[test]
    fn a_later_read_settles_these_failures_and_the_rest_stand_as_they_are() {
        let again: Vec<&str> = cases()
            .iter()
            .filter(|case| case.is_worth_reading_again())
            .map(variant)
            .collect();
        let standing: Vec<&str> = cases()
            .iter()
            .filter(|case| !case.is_worth_reading_again())
            .map(variant)
            .collect();
        assert_eq!(
            again,
            ["absent or refused", "rate limited", "unreachable"],
            "a later read settles exactly these, so an adapter that reads every failure \
             again fails here"
        );
        assert_eq!(
            standing,
            [
                "unauthorized",
                "forbidden",
                "absent",
                "malformed",
                "unconfigured",
                "ambiguous"
            ],
            "these answers do not change by asking twice, so an adapter that reads no \
             failure again fails here"
        );
    }

    #[test]
    fn every_failure_says_how_many_objects_it_read_where_at_most_one_was_expected() {
        let pinned = [
            (JiraError::Unauthorized { status: 401 }, None),
            (JiraError::Forbidden { status: 403 }, None),
            (
                JiraError::Absent {
                    key: "IDENT-1".into(),
                },
                None,
            ),
            (
                JiraError::AbsentOrRefused {
                    key: "IDENT-1".into(),
                    why: "HTTP 503".into(),
                },
                None,
            ),
            (JiraError::RateLimited("HTTP 429".into()), None),
            (
                JiraError::Malformed("the body is not an issue".into()),
                None,
            ),
            (JiraError::Unreachable("connection refused".into()), None),
            (JiraError::Unconfigured, None),
            (
                JiraError::Ambiguous {
                    marker: "fx-abc123".into(),
                    count: 2,
                },
                Some(2),
            ),
        ];
        let named: Vec<&str> = pinned.iter().map(|(case, _)| variant(case)).collect();
        let all: Vec<&str> = cases().iter().map(variant).collect();
        assert_eq!(named, all, "every variant of JiraError is pinned here");
        for (case, expected) in pinned {
            let named = variant(&case);
            assert_eq!(
                case.duplicates(),
                expected,
                "{named} names how many objects it read, and the executor turns exactly the \
                 named ones into DuplicateState"
            );
        }
    }

    #[test]
    fn exactly_one_failure_names_a_count_and_it_is_the_one_whose_words_carry_that_count() {
        let counted: Vec<&str> = cases()
            .iter()
            .filter(|case| case.duplicates().is_some())
            .map(variant)
            .collect();
        assert_eq!(
            counted,
            ["ambiguous"],
            "an adapter that named every failure a duplicate observation, or none, fails here"
        );
        let ambiguous = JiraError::Ambiguous {
            marker: "fx-abc123".into(),
            count: 3,
        };
        assert_eq!(ambiguous.duplicates(), Some(3));
        assert!(
            format!("{ambiguous}").contains('3'),
            "the count the executor reads is the count the reader reads: {ambiguous}"
        );
    }

    #[test]
    fn the_failure_whose_words_promise_another_attempt_is_the_one_read_again() {
        let limited = JiraError::RateLimited("HTTP 429".into());
        let refused = JiraError::Forbidden { status: 403 };
        assert!(
            format!("{limited}").contains("can be sent again"),
            "the rate limited failure promises another attempt in its own words: {limited}"
        );
        assert!(
            limited.is_worth_reading_again(),
            "and the adapter keeps that promise, or the text and the behaviour disagree"
        );
        assert!(
            !format!("{refused}").contains("can be sent again"),
            "a refused credential promises no other attempt: {refused}"
        );
        assert!(
            !refused.is_worth_reading_again(),
            "and the adapter makes none"
        );
    }

    #[test]
    fn no_failure_advises_a_wait_because_no_header_reaches_this_type() {
        for case in cases() {
            assert_eq!(
                case.advice(),
                RetryAdvice::default(),
                "{} holds a status and the site's words and no header, so it advises no \
                 measured wait",
                variant(&case)
            );
            assert!(
                !case.advice().wants_a_wait(),
                "{} must not ask for a wait no response supplied",
                variant(&case)
            );
        }
        let limited = JiraError::RateLimited("HTTP 429".into());
        assert!(
            limited.is_worth_reading_again(),
            "a jira 429 is read again because its status says so"
        );
        assert!(
            !limited.advice().wants_a_wait(),
            "and never because a Retry-After header reached it, because none does"
        );
    }

    #[test]
    fn a_boxed_jira_failure_answers_for_itself_and_not_from_the_trait_default() {
        let limited: Box<dyn AdapterError> = Box::new(JiraError::RateLimited("HTTP 429".into()));
        assert!(
            limited.is_worth_reading_again(),
            "the executor boxes an adapter failure and asks whether to read again, and the \
             trait default answers false"
        );
    }
}
