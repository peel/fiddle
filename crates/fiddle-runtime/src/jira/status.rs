use fiddle_core::{ProjectedStatus, WorkState};

#[derive(Debug)]
pub struct ConfiguredNames {
    ready: Option<String>,
    in_progress: Option<String>,
    in_review: Option<String>,
    blocked: Option<String>,
    done: Option<String>,
}

const CONFIGURABLE: [WorkState; 5] = [
    WorkState::Ready,
    WorkState::InProgress,
    WorkState::InReview,
    WorkState::Blocked,
    WorkState::Done,
];

impl ConfiguredNames {
    pub fn new(
        ready: Option<String>,
        in_progress: Option<String>,
        in_review: Option<String>,
        blocked: Option<String>,
        done: Option<String>,
    ) -> Self {
        ConfiguredNames {
            ready,
            in_progress,
            in_review,
            blocked,
            done,
        }
    }

    fn name_for(&self, state: &WorkState) -> Option<&str> {
        match state {
            WorkState::Ready => self.ready.as_deref(),
            WorkState::InProgress => self.in_progress.as_deref(),
            WorkState::InReview => self.in_review.as_deref(),
            WorkState::Blocked => self.blocked.as_deref(),
            WorkState::Done => self.done.as_deref(),
            WorkState::Unknown => None,
        }
    }

    pub fn state_for(&self, name: &str) -> Option<WorkState> {
        CONFIGURABLE
            .iter()
            .find(|state| self.name_for(state) == Some(name))
            .cloned()
    }
}

enum JiraCategory {
    ToDo,
    InProgress,
    Done,
}

impl JiraCategory {
    fn named(category: &str) -> Option<Self> {
        match category {
            "To Do" => Some(JiraCategory::ToDo),
            "In Progress" => Some(JiraCategory::InProgress),
            "Done" => Some(JiraCategory::Done),
            _ => None,
        }
    }

    fn state(&self) -> WorkState {
        match self {
            JiraCategory::ToDo => WorkState::Ready,
            JiraCategory::InProgress => WorkState::InProgress,
            JiraCategory::Done => WorkState::Done,
        }
    }
}

pub fn project(names: &ConfiguredNames, id: &str, name: &str, category: &str) -> ProjectedStatus {
    let state = names
        .state_for(name)
        .or_else(|| JiraCategory::named(category).map(|known| known.state()))
        .unwrap_or(WorkState::Unknown);

    ProjectedStatus {
        state,
        jira_status_id: id.to_string(),
        jira_status_name: name.to_string(),
        jira_status_category: category.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::WorkState;

    fn names(configured: &[(&str, &str)]) -> ConfiguredNames {
        let of = |key: &str| {
            configured
                .iter()
                .find(|(configured_key, _)| *configured_key == key)
                .map(|(_, jira_name)| (*jira_name).to_string())
        };
        ConfiguredNames::new(
            of("ready"),
            of("in_progress"),
            of("in_review"),
            of("blocked"),
            of("done"),
        )
    }

    #[test]
    fn a_configured_name_decides_before_a_category_does() {
        let names = names(&[("in_review", "Awaiting Security Review")]);
        let projected = project(&names, "10001", "Awaiting Security Review", "In Progress");
        assert_eq!(
            projected.state,
            WorkState::InReview,
            "the configured name wins over the category"
        );
    }

    #[test]
    fn an_unconfigured_name_falls_back_to_its_category() {
        let projected = project(&names(&[]), "10002", "Awaiting QA", "In Progress");
        assert_eq!(projected.state, WorkState::InProgress);
    }

    #[test]
    fn a_status_that_neither_resolves_is_unknown_and_never_a_guess() {
        let projected = project(&names(&[]), "10003", "Awaiting QA", "Marzipan");
        assert_eq!(projected.state, WorkState::Unknown);
        assert_eq!(
            projected.jira_status_name, "Awaiting QA",
            "the Jira facts survive the Unknown"
        );
        assert_eq!(projected.jira_status_category, "Marzipan");
    }

    #[test]
    fn two_statuses_sharing_one_name_stay_distinguishable() {
        let names = names(&[("done", "Closed")]);
        let first = project(&names, "10010", "Closed", "Done");
        let second = project(&names, "10011", "Closed", "Done");
        assert_eq!(first.state, second.state);
        assert_eq!(
            (first.jira_status_id.as_str(), second.jira_status_id.as_str()),
            ("10010", "10011"),
            "each projection reports the id it was given, so a lookup keyed on name alone loses the difference Jira keeps"
        );
    }

    #[test]
    fn each_configured_key_reaches_the_state_it_names() {
        let cases = [
            ("ready", WorkState::Ready),
            ("in_progress", WorkState::InProgress),
            ("in_review", WorkState::InReview),
            ("blocked", WorkState::Blocked),
            ("done", WorkState::Done),
        ];
        for (key, expected) in cases {
            let projected = project(&names(&[(key, "Bespoke")]), "10100", "Bespoke", "Marzipan");
            assert_eq!(
                projected.state, expected,
                "the name configured under {key} must reach {expected:?}, and the category cannot supply it"
            );
        }
    }

    #[test]
    fn a_name_configured_for_one_state_reaches_no_other() {
        let names = names(&[("blocked", "Waiting on Vendor")]);
        assert_eq!(
            project(&names, "10004", "Waiting on Vendor", "Marzipan").state,
            WorkState::Blocked
        );
        assert_eq!(
            project(&names, "10005", "Waiting on Legal", "Marzipan").state,
            WorkState::Unknown,
            "a name nobody configured resolves through no other key's configuration"
        );
    }

    #[test]
    fn a_configured_name_matches_exactly_and_a_near_miss_is_a_defect_not_a_guess() {
        let names = names(&[("in_review", "Awaiting Security Review")]);
        let over = |name: &str| project(&names, "10001", name, "Marzipan").state;

        assert_eq!(
            over("Awaiting Security Review"),
            WorkState::InReview,
            "the name written exactly as configured resolves"
        );
        for near_miss in [
            "awaiting security review",
            "AWAITING SECURITY REVIEW",
            " Awaiting Security Review",
            "Awaiting Security Review ",
            "Awaiting  Security Review",
        ] {
            assert_eq!(
                over(near_miss),
                WorkState::Unknown,
                "`{near_miss}` differs from the configured name, so the deployment has a configuration defect and must see Unknown"
            );
        }
    }

    #[test]
    fn a_category_matches_exactly_too() {
        let over = |category: &str| project(&names(&[]), "10006", "Awaiting QA", category).state;

        assert_eq!(over("In Progress"), WorkState::InProgress);
        for near_miss in ["in progress", "IN PROGRESS", "In Progress ", "InProgress"] {
            assert_eq!(
                over(near_miss),
                WorkState::Unknown,
                "`{near_miss}` is not a category Jira names, so nothing may be inferred from it"
            );
        }
    }

    #[test]
    fn each_jira_category_reaches_the_coarse_state_it_means() {
        let cases = [
            ("To Do", WorkState::Ready),
            ("In Progress", WorkState::InProgress),
            ("Done", WorkState::Done),
        ];
        for (category, expected) in cases {
            assert_eq!(
                project(&names(&[]), "10007", "Awaiting QA", category).state,
                expected,
                "the {category} category means {expected:?}"
            );
        }
    }

    #[test]
    fn two_done_category_statuses_keep_the_difference_only_their_names_carry() {
        let resolved = project(&names(&[]), "10012", "Resolved", "Done");
        let rejected = project(&names(&[]), "10013", "Rejected", "Done");

        assert_eq!(resolved.state, WorkState::Done);
        assert_eq!(rejected.state, WorkState::Done);
        assert_eq!(
            (
                resolved.jira_status_name.as_str(),
                rejected.jira_status_name.as_str()
            ),
            ("Resolved", "Rejected"),
            "the typed state cannot tell a resolution from a rejection, so each projection must report the name it was given"
        );
    }

    #[test]
    fn a_projection_reports_every_jira_fact_it_was_given() {
        assert_eq!(
            project(&names(&[]), "10008", "Awaiting QA", "Marzipan"),
            ProjectedStatus {
                state: WorkState::Unknown,
                jira_status_id: "10008".to_string(),
                jira_status_name: "Awaiting QA".to_string(),
                jira_status_category: "Marzipan".to_string(),
            }
        );
    }
}
