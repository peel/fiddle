//! Identity of the work a fiddle run acts on.
//!
//! An [`InvocationRef`] is the identity every command is addressed by, so it is
//! parsed in exactly one place — here — and every other layer consumes the
//! parsed value. The CLI does not re-implement this grammar; it calls
//! [`str::parse`] and renders the resulting [`InvocationRefError`].
//!
//! Parsing is a pure function of its input: no configuration is consulted and
//! nothing outside the process is touched, which is what lets the grammar live
//! in the pure core rather than beside the command that happens to accept it.

use std::str::FromStr;

/// The kind of source an invocation came from.
///
/// The scheme is a closed set rather than a free string: an unrecognised scheme
/// is a rejected invocation, not a source fiddle will try to guess at. M0
/// implements only [`InvocationScheme::Beans`] end to end; the remaining
/// variants are accepted as identities so later milestones add adapters without
/// changing this grammar.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationScheme {
    Beans,
    Jira,
    Scheduled,
    Scanner,
}

impl InvocationScheme {
    /// The text this scheme is written as in an invocation reference.
    ///
    /// This is the single source of the scheme spelling: parsing matches
    /// against it and rendering formats from it, so the two can never drift.
    pub fn as_str(self) -> &'static str {
        match self {
            InvocationScheme::Beans => "beans",
            InvocationScheme::Jira => "jira",
            InvocationScheme::Scheduled => "scheduled",
            InvocationScheme::Scanner => "scanner",
        }
    }

    /// Every scheme, in the order a diagnostic should list them.
    pub const ALL: [InvocationScheme; 4] = [
        InvocationScheme::Beans,
        InvocationScheme::Jira,
        InvocationScheme::Scheduled,
        InvocationScheme::Scanner,
    ];
}

impl std::fmt::Display for InvocationScheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The identity of a capability fiddle can execute.
///
/// A `&'static str` rather than a `String`: capabilities are compiled into the
/// binary, so a capability id is always a literal this build knows about and
/// never a name assembled at runtime. Serialized transparently, so the id
/// appears on the wire as the bare `"stub_mark"` a caller matches on.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct CapabilityId(pub &'static str);

impl std::fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// A parsed `<scheme>:<value>` invocation reference, such as
/// `beans:fiddle-m0-demo`.
///
/// The fields are private so the only way to obtain one is through
/// [`FromStr`]: a value of this type is proof that the grammar was satisfied,
/// which is why no later layer needs to re-validate it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationRef {
    scheme: InvocationScheme,
    value: String,
}

/// Why an invocation reference was rejected.
///
/// One variant per defect, because a caller who wrote `bogus` needs to be told
/// something different from one who wrote `beans:`. Presentation — diagnostic
/// codes, help text, exit codes — is the CLI's business; this enum only names
/// the defect.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InvocationRefError {
    /// No `:` separator at all, so no scheme could be read.
    #[error("invocation reference must be <scheme>:<value>, got `{0}`")]
    Malformed(String),

    /// A scheme was present but is not one fiddle knows.
    #[error("unknown invocation scheme `{0}`; expected one of beans, jira, scheduled, scanner")]
    UnknownScheme(String),

    /// A known scheme followed by nothing, so the reference names no work.
    #[error("invocation reference value must not be empty")]
    EmptyValue,
}

impl FromStr for InvocationRef {
    type Err = InvocationRefError;

    /// Split on the *first* `:` only, so a value may itself contain separators
    /// (`jira:ICE-1:sub` names the value `ICE-1:sub`).
    ///
    /// The emptiness of the value is checked before the scheme is recognised so
    /// that the more specific defect wins: `beans:` is reported as an empty
    /// value rather than being dragged through scheme lookup.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (scheme, value) = s
            .split_once(':')
            .ok_or_else(|| InvocationRefError::Malformed(s.to_string()))?;
        if value.is_empty() {
            return Err(InvocationRefError::EmptyValue);
        }
        let scheme = InvocationScheme::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == scheme)
            .ok_or_else(|| InvocationRefError::UnknownScheme(scheme.to_string()))?;
        Ok(InvocationRef {
            scheme,
            value: value.to_string(),
        })
    }
}

impl InvocationRef {
    /// The source this invocation came from.
    pub fn scheme(&self) -> InvocationScheme {
        self.scheme
    }

    /// The scheme-specific identifier, verbatim as it was written.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The canonical `<scheme>:<value>` text. Round-trips through [`FromStr`].
    pub fn as_str(&self) -> String {
        format!("{}:{}", self.scheme.as_str(), self.value)
    }

    /// A path- and filename-safe rendering, for naming the artefacts a run
    /// publishes about this invocation.
    pub fn slug(&self) -> String {
        format!("{}-{}", self.scheme.as_str(), self.value)
    }
}

impl std::fmt::Display for InvocationRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.scheme.as_str(), self.value)
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
            Err(InvocationRefError::EmptyValue)
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
