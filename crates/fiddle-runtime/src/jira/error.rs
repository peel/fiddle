#[derive(Debug, thiserror::Error)]
pub enum JiraError {
    #[error("the site refused the credential with {status}")]
    Unauthorized { status: u16 },

    #[error("the credential may not read this issue: {status}")]
    Forbidden { status: u16 },

    #[error("the site holds no issue `{key}`")]
    Absent { key: String },

    #[error("the site answered with something that is not an issue: {0}")]
    Malformed(String),

    #[error("the site could not be reached: {0}")]
    Unreachable(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variant(error: &JiraError) -> &'static str {
        match error {
            JiraError::Unauthorized { .. } => "unauthorized",
            JiraError::Forbidden { .. } => "forbidden",
            JiraError::Absent { .. } => "absent",
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
            JiraError::Malformed("the body is not an issue".into()),
            JiraError::Unreachable("connection refused".into()),
        ]
    }

    #[test]
    fn every_read_failure_explains_itself_and_names_no_credential() {
        let planted = "Basic aWRlbnQ6dG9rZW4=";
        assert!(
            format!("{}", JiraError::Unreachable(planted.into())).contains(planted),
            "a credential in the text is detectable, so the check below is not vacuous"
        );
        for case in cases() {
            let said = format!("{case}");
            let named = variant(&case);
            assert!(!said.trim().is_empty(), "{named} explains itself");
            assert!(
                !said.contains("Basic "),
                "{named} quotes an authorization header: {said}"
            );
        }
    }

    #[test]
    fn the_five_read_failures_read_as_five_failures() {
        let cases = cases();
        let mut named: Vec<&str> = cases.iter().map(variant).collect();
        named.sort_unstable();
        assert_eq!(
            named,
            [
                "absent",
                "forbidden",
                "malformed",
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
}
