use std::str::FromStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationScheme {
    Beans,
    Jira,
    Scheduled,
    Scanner,
    Cve,
}

impl InvocationScheme {
    pub fn as_str(self) -> &'static str {
        match self {
            InvocationScheme::Beans => "beans",
            InvocationScheme::Jira => "jira",
            InvocationScheme::Scheduled => "scheduled",
            InvocationScheme::Scanner => "scanner",
            InvocationScheme::Cve => "cve",
        }
    }

    pub const ALL: [InvocationScheme; 5] = [
        InvocationScheme::Beans,
        InvocationScheme::Jira,
        InvocationScheme::Scheduled,
        InvocationScheme::Scanner,
        InvocationScheme::Cve,
    ];

    fn of(text: &str) -> Option<Self> {
        InvocationScheme::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == text)
    }

    pub fn stands_alone(self) -> bool {
        matches!(self, InvocationScheme::Cve)
    }

    pub fn listed() -> String {
        InvocationScheme::listed_where(|_| true)
    }

    pub fn listed_naming_work() -> String {
        InvocationScheme::listed_where(|scheme| !scheme.stands_alone())
    }

    pub fn listed_standing_alone() -> String {
        InvocationScheme::listed_where(InvocationScheme::stands_alone)
    }

    fn listed_where(admits: impl Fn(Self) -> bool) -> String {
        InvocationScheme::ALL
            .into_iter()
            .filter(|scheme| admits(*scheme))
            .map(InvocationScheme::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl std::fmt::Display for InvocationScheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct CapabilityId(pub &'static str);

impl std::fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct WorkRef(pub String);

impl std::fmt::Display for WorkRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize)]
#[serde(transparent)]
pub struct AttemptId(pub String);

impl std::fmt::Display for AttemptId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationRef {
    scheme: InvocationScheme,
    value: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InvocationRefError {
    #[error(
        "`{0}` is not an invocation reference: it is neither a scheme followed by \
         the work it names nor one of the schemes that discover their own work \
         ({alone})",
        alone = InvocationScheme::listed_standing_alone(),
    )]
    Malformed(String),

    #[error(
        "unknown invocation scheme `{scheme}`; expected one of {known}",
        scheme = .0,
        known = InvocationScheme::listed(),
    )]
    UnknownScheme(String),

    #[error("invocation reference value must not be empty")]
    EmptyValue { scheme: Option<InvocationScheme> },

    #[error(
        "invocation reference value `{value}` contains `{character}`; \
         a value is written with ASCII letters, digits, `-`, `_` and `:` only"
    )]
    IllegalValueCharacter { value: String, character: char },
}

impl FromStr for InvocationRef {
    type Err = InvocationRefError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((scheme, value)) = s.split_once(':') else {
            return match InvocationScheme::of(s) {
                Some(scheme) if scheme.stands_alone() => Ok(InvocationRef {
                    scheme,
                    value: String::new(),
                }),
                _ => Err(InvocationRefError::Malformed(s.to_string())),
            };
        };
        if value.is_empty() {
            return Err(InvocationRefError::EmptyValue {
                scheme: InvocationScheme::of(scheme),
            });
        }
        if let Some(character) = value.chars().find(|c| !InvocationRef::admits(*c)) {
            return Err(InvocationRefError::IllegalValueCharacter {
                value: value.to_string(),
                character,
            });
        }
        let scheme = InvocationScheme::of(scheme)
            .ok_or_else(|| InvocationRefError::UnknownScheme(scheme.to_string()))?;
        Ok(InvocationRef {
            scheme,
            value: value.to_string(),
        })
    }
}

impl InvocationRef {
    pub const VALUE_GRAMMAR: &'static str = "ASCII letters, digits, `-`, `_` and `:`";

    fn admits(c: char) -> bool {
        c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':')
    }

    pub fn scheme(&self) -> InvocationScheme {
        self.scheme
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn as_str(&self) -> String {
        if self.value.is_empty() {
            self.scheme.as_str().to_string()
        } else {
            format!("{}:{}", self.scheme.as_str(), self.value)
        }
    }

    pub fn slug(&self) -> String {
        if self.value.is_empty() {
            self.scheme.as_str().to_string()
        } else {
            format!("{}-{}", self.scheme.as_str(), self.value)
        }
    }
}

impl std::fmt::Display for InvocationRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_known_scheme() {
        for (text, expected) in [
            ("beans:fiddle-m0-demo", InvocationScheme::Beans),
            ("jira:ICE-1", InvocationScheme::Jira),
            ("scheduled:nightly", InvocationScheme::Scheduled),
            ("scanner:cve-2026-1", InvocationScheme::Scanner),
        ] {
            let parsed: InvocationRef = text.parse().unwrap();
            assert_eq!(parsed.scheme(), expected);
            assert_eq!(parsed.as_str(), text);
        }
    }

    #[test]
    fn keeps_the_value_verbatim_and_derives_a_slug() {
        let parsed: InvocationRef = "beans:fiddle-m0-demo".parse().unwrap();
        assert_eq!(parsed.value(), "fiddle-m0-demo");
        assert_eq!(parsed.slug(), "beans-fiddle-m0-demo");
    }

    #[test]
    fn a_value_may_itself_contain_a_separator() {
        let parsed: InvocationRef = "jira:ICE-1:sub".parse().unwrap();
        assert_eq!(parsed.value(), "ICE-1:sub");
        assert_eq!(parsed.as_str(), "jira:ICE-1:sub");
    }

    #[test]
    fn rejects_each_malformed_shape_with_its_own_defect() {
        assert_eq!(
            "bogus".parse::<InvocationRef>(),
            Err(InvocationRefError::Malformed("bogus".to_string()))
        );
        assert_eq!(
            "mystery:x".parse::<InvocationRef>(),
            Err(InvocationRefError::UnknownScheme("mystery".to_string()))
        );
        assert_eq!(
            "beans:".parse::<InvocationRef>(),
            Err(InvocationRefError::EmptyValue {
                scheme: Some(InvocationScheme::Beans)
            })
        );
        assert_eq!(
            "mystery:".parse::<InvocationRef>(),
            Err(InvocationRefError::EmptyValue { scheme: None })
        );
        assert_eq!(
            "beans:../../../pwned".parse::<InvocationRef>(),
            Err(InvocationRefError::IllegalValueCharacter {
                value: "../../../pwned".to_string(),
                character: '.',
            })
        );
    }

    #[test]
    fn refuses_a_value_that_could_be_read_as_a_path() {
        for text in [
            "beans:../../../pwned",
            "beans:..",
            "beans:.",
            "beans:a/b",
            "beans:/etc/passwd",
            "beans:a\\b",
            "cve:../../../pwned",
            "cve:a/b",
            "beans:.hidden",
            "beans:work/../../../pwned.json",
            "scanner:%2e%2e",
        ] {
            assert!(
                matches!(
                    text.parse::<InvocationRef>(),
                    Err(InvocationRefError::IllegalValueCharacter { .. })
                ),
                "`{text}` must be refused as a value that is not an identifier, got {:?}",
                text.parse::<InvocationRef>()
            );
        }
    }

    #[test]
    fn still_admits_the_identifiers_real_sources_produce() {
        for text in [
            "beans:fiddle-m0-demo",
            "beans:fiddle-1p8q",
            "jira:ICE-1",
            "jira:ICE-1:sub",
            "scheduled:nightly",
            "scheduled:nightly_sweep",
            "scanner:cve-2026-1",
        ] {
            let parsed: InvocationRef = text
                .parse()
                .unwrap_or_else(|e| panic!("`{text}` must still parse, got {e}"));
            assert_eq!(parsed.as_str(), text);
        }
    }

    #[test]
    fn a_slug_of_an_admitted_value_is_one_name() {
        for text in ["beans:fiddle-m0-demo", "jira:ICE-1:sub", "scanner:a_b-1"] {
            let slug = text.parse::<InvocationRef>().unwrap().slug();
            assert!(
                !slug.contains('/') && !slug.contains('\\') && !slug.contains('.'),
                "a slug names one artefact, got {slug}"
            );
        }
    }

    #[test]
    fn a_self_discovering_scheme_stands_alone_and_round_trips() {
        let parsed: InvocationRef = "cve".parse().expect("a bare `cve` is a complete reference");
        assert_eq!(parsed.scheme(), InvocationScheme::Cve);
        assert_eq!(parsed.value(), "");
        assert_eq!(parsed.as_str(), "cve", "renders bare, never as `cve:`");
        assert_eq!(parsed.to_string(), "cve", "and `Display` agrees with it");
        assert_eq!(parsed.slug(), "cve", "no trailing separator");
        assert_eq!(
            "cve".parse::<InvocationRef>().unwrap(),
            parsed,
            "round trips"
        );
    }

    #[test]
    fn a_colon_with_nothing_after_it_is_still_empty_value() {
        assert_eq!(
            "cve:".parse::<InvocationRef>(),
            Err(InvocationRefError::EmptyValue {
                scheme: Some(InvocationScheme::Cve)
            })
        );
    }

    #[test]
    fn the_bare_form_is_per_scheme_and_not_general() {
        for scheme in InvocationScheme::ALL {
            let bare = scheme.as_str().parse::<InvocationRef>();
            if scheme.stands_alone() {
                assert_eq!(
                    bare.unwrap_or_else(|e| panic!("`{scheme}` must stand alone, got {e}"))
                        .scheme(),
                    scheme
                );
            } else {
                assert_eq!(
                    bare,
                    Err(InvocationRefError::Malformed(scheme.as_str().to_string())),
                    "`{scheme}` names a piece of work and must still be given one"
                );
            }
        }
        assert_eq!(
            InvocationScheme::ALL
                .into_iter()
                .filter(|scheme| scheme.stands_alone())
                .collect::<Vec<_>>(),
            vec![InvocationScheme::Cve],
            "only a self-discovering orchestration may stand alone"
        );
    }

    #[test]
    fn a_valued_cve_reference_still_validates_its_value() {
        let parsed: InvocationRef = "cve:CVE-2026-1234".parse().expect("a finding id parses");
        assert_eq!(parsed.value(), "CVE-2026-1234");
        assert_eq!(parsed.as_str(), "cve:CVE-2026-1234");
        assert_eq!(
            "cve:../../pwned".parse::<InvocationRef>(),
            Err(InvocationRefError::IllegalValueCharacter {
                value: "../../pwned".to_string(),
                character: '.',
            })
        );
    }

    #[test]
    fn a_bare_slug_cannot_collide_with_a_valued_slug() {
        let bare: InvocationRef = "cve".parse().unwrap();
        let valued: InvocationRef = "cve:CVE-2026-1234".parse().unwrap();
        assert_ne!(bare.slug(), valued.slug());
        assert_eq!(bare.slug(), "cve");
        assert_eq!(valued.slug(), "cve-CVE-2026-1234");
    }

    #[test]
    fn the_unknown_scheme_diagnostic_names_every_scheme() {
        let rendered = "mystery:x"
            .parse::<InvocationRef>()
            .expect_err("an unknown scheme is refused")
            .to_string();
        for scheme in InvocationScheme::ALL {
            assert!(
                rendered.contains(scheme.as_str()),
                "`{scheme}` is a scheme a caller may write and must be offered, got {rendered}"
            );
        }
    }

    #[test]
    fn every_scheme_is_advised_by_exactly_one_half_of_the_set() {
        let naming = InvocationScheme::listed_naming_work();
        let alone = InvocationScheme::listed_standing_alone();
        for scheme in InvocationScheme::ALL {
            assert_eq!(
                (
                    naming.contains(scheme.as_str()),
                    alone.contains(scheme.as_str())
                ),
                (!scheme.stands_alone(), scheme.stands_alone()),
                "`{scheme}` belongs to whichever half `stands_alone` puts it in and to \
                 no other, got naming={naming:?} alone={alone:?}"
            );
        }
        assert!(
            !naming.is_empty() && !alone.is_empty(),
            "each half is read as a list in a sentence, so neither may be empty: \
             naming={naming:?} alone={alone:?}"
        );
    }

    #[test]
    fn a_scheme_serializes_as_the_text_it_was_parsed_from() {
        assert_eq!(
            serde_json::to_string(&InvocationScheme::Beans).unwrap(),
            "\"beans\""
        );
        assert_eq!(
            serde_json::to_string(&InvocationScheme::Scheduled).unwrap(),
            "\"scheduled\""
        );
    }
}
