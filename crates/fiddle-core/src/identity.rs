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

/// The identity of the work itself, as opposed to the request that reached it.
///
/// Distinct from [`InvocationRef`] because the two diverge as soon as more than
/// one kind of request can address the same work: a scheduled sweep and a
/// webhook are different invocations of the same work item. In M0 they coincide
/// — the beans reference *is* the work identity — and keeping the type separate
/// now is what lets a later milestone tell two attempts on one work item apart
/// from two attempts on two.
///
/// Serialized transparently, so a bundle carries the bare
/// `"beans:fiddle-m0-demo"` string a reader can match on.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct WorkRef(pub String);

impl std::fmt::Display for WorkRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The identity of one attempt at some work.
///
/// Every run mints a new one, which is what makes "the second invocation was a
/// genuine attempt, not a cached result" checkable from outside: two bundles
/// carrying the same [`WorkRef`] and different `AttemptId`s are two attempts.
/// It also names the directory a bundle is published into, so two attempts can
/// never collide on one path.
///
/// A `String` rather than a structured timestamp because the core does not get
/// to read a clock; minting one belongs to the runtime, and what the core needs
/// is only that the value is opaque, orderable as text, and safe in a path.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize)]
#[serde(transparent)]
pub struct AttemptId(pub String);

impl std::fmt::Display for AttemptId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
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

    /// A non-empty value written with a character outside
    /// [`InvocationRef::VALUE_GRAMMAR`].
    ///
    /// The offending character is carried rather than only the value, because
    /// the usual defect is one character in an otherwise plausible identifier
    /// and "which one" is the whole of what the caller needs to know.
    #[error(
        "invocation reference value `{value}` contains `{character}`; \
         a value is written with ASCII letters, digits, `-`, `_` and `:` only"
    )]
    IllegalValueCharacter { value: String, character: char },
}

impl FromStr for InvocationRef {
    type Err = InvocationRefError;

    /// Split on the *first* `:` only, so a value may itself contain separators
    /// (`jira:ICE-1:sub` names the value `ICE-1:sub`).
    ///
    /// The value is checked before the scheme is recognised so that the more
    /// specific defect wins: `beans:` is reported as an empty value, and
    /// `beans:../x` as an illegal character, rather than either being dragged
    /// through scheme lookup.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (scheme, value) = s
            .split_once(':')
            .ok_or_else(|| InvocationRefError::Malformed(s.to_string()))?;
        if value.is_empty() {
            return Err(InvocationRefError::EmptyValue);
        }
        if let Some(character) = value.chars().find(|c| !InvocationRef::admits(*c)) {
            return Err(InvocationRefError::IllegalValueCharacter {
                value: value.to_string(),
                character,
            });
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
    /// The character class an invocation reference *value* is written in, in
    /// the words a diagnostic uses.
    ///
    /// **ASCII letters, digits, `-`, `_` and `:`.** Nothing else — not `.`, not
    /// `/`, not `\`, not a space, not a character outside ASCII.
    ///
    /// # Why this class, and not a wider one
    ///
    /// [`InvocationRef::slug`] interpolates the value into the names of the
    /// artefacts a run publishes, and the value is also what the ports are
    /// asked to read. So the value is not free text that happens to be used in
    /// a name; it *is* a name, and it arrives from outside — from `jira`,
    /// `scheduled` and `scanner` sources fiddle does not control. A value that
    /// can be read as a relative path can name a place outside every root the
    /// configuration declares, which is a write primitive handed to whoever
    /// files a ticket.
    ///
    /// The class is therefore an allow-list, checked once here rather than
    /// sanitised at each of the three places a path is derived from it. A
    /// deny-list of the sequences known to traverse would have to be repeated
    /// and kept complete; an allow-list makes the next derived path safe
    /// without anyone remembering it exists.
    ///
    /// `.` is excluded rather than merely `..`, and that is the deliberate
    /// part. Excluding the character outright means no `.` or `..` component
    /// can be *formed* — including by the prefix arithmetic that made the
    /// original report's `beans:../../pwned` look contained when
    /// `beans:../../../pwned` was not — and it also keeps a value from naming a
    /// dot-file. It costs nothing the sources produce: beans ids
    /// (`fiddle-m0-demo`, `fiddle-1p8q`), Jira keys (`ICE-1`), schedule names
    /// (`nightly`) and scanner ids (`cve-2026-1`) are alphanumerics and
    /// hyphens.
    ///
    /// `:` is *kept*, which is the one concession to compatibility. A value may
    /// contain a separator — `jira:ICE-1:sub` names `ICE-1:sub` — and that is
    /// deliberate existing behaviour rather than an accident of the split, so
    /// removing it would break references that parse today. A colon cannot
    /// traverse: it is an ordinary character in a name on the platforms fiddle
    /// targets, and it separates nothing in a path.
    pub const VALUE_GRAMMAR: &'static str = "ASCII letters, digits, `-`, `_` and `:`";

    /// Whether `c` is admitted by [`InvocationRef::VALUE_GRAMMAR`].
    ///
    /// `is_ascii_alphanumeric` rather than `is_alphanumeric`, so a value can
    /// never carry a character that looks like an ASCII one and is not.
    fn admits(c: char) -> bool {
        c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':')
    }

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
    ///
    /// Safe because of [`InvocationRef::VALUE_GRAMMAR`], not because of
    /// anything done here: the scheme is a closed set and the value was
    /// constrained at the parse boundary, so a slug is always one name with no
    /// separator and no `.` component. That is what a caller deriving a path
    /// from it is entitled to rely on, and it is why deriving a *fourth* path
    /// from a slug needs no new sanitising step.
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
        assert_eq!(
            "beans:../../../pwned".parse::<InvocationRef>(),
            Err(InvocationRefError::IllegalValueCharacter {
                value: "../../../pwned".to_string(),
                character: '.',
            })
        );
    }

    /// **A value can never be read as a relative path.**
    ///
    /// `slug()` is interpolated into the names of the artefacts a run
    /// publishes, so a value carrying a separator or a `.` component escapes
    /// the roots the configuration names. The grammar closes that at the parse
    /// boundary rather than at each use site: every one of these is refused
    /// before an `InvocationRef` exists at all, so no later layer can be the
    /// one that forgot to sanitise.
    #[test]
    fn refuses_a_value_that_could_be_read_as_a_path() {
        for text in [
            "beans:../../../pwned",
            "beans:..",
            "beans:.",
            "beans:a/b",
            "beans:/etc/passwd",
            "beans:a\\b",
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

    /// The other half of the grammar: what it must keep admitting.
    ///
    /// These are the shapes the sources fiddle addresses actually produce. A
    /// character class chosen for safety alone would be free to reject them,
    /// which is why they are pinned here beside the rejections.
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

    /// Every character the grammar admits is one a slug can carry, and the slug
    /// stays a single name rather than a path with parts.
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
