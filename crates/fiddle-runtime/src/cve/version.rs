//! Is the version a tree ships already at least the version a finding names as
//! fixed?
//!
//! A dozen lines of comparison, in a module of their own, because this is the
//! one answer in the capability that is **wrong silently**. Every other mistake
//! here surfaces as a refusal, a failed command or an empty report; this one
//! surfaces as a CVE reported fixed that is not. Nothing downstream can tell
//! that answer from a true one, and the finding leaves the report.
//!
//! # The pair it exists for
//!
//! The two producers spell a version differently and neither is going to stop.
//! `go list -m` prints `v2.52.9`; the scanner prints `2.52.14`. So the leading
//! `v` has to come off **both** operands before anything is compared, which is
//! why [`at_least`] takes both through [`components`] rather than stripping at
//! its call sites — two call sites would be free to strip one operand and not
//! the other. That is not a hypothetical: it is the pair measured in the
//! pipeline this milestone replaces, and it orders the wrong way round in the
//! direction that hides the finding.
//!
//! # Numeric, not lexical
//!
//! `2.52.9` against `2.52.14` is also the pair that catches a string
//! comparison, and catches it in the dangerous direction: `'9' > '1'` as text,
//! so a lexical `>=` calls the older tree the newer one. Comparison is therefore
//! per component and numeric, and `1.10.0` is above `1.9.0`.
//!
//! # A version this cannot read is not fixed
//!
//! A component that is not a number — a Go pseudo-version's timestamp tail, a
//! release candidate suffix, a distribution's own epoch punctuation — makes
//! [`at_least`] answer `false`, whichever operand it arrived in. That is the
//! fail-closed direction on purpose: a version nobody here can read leaves the
//! finding open, where a person sees it, rather than closing it on a comparison
//! that did not happen.

/// Does `shipped` satisfy a finding whose fix lands in `fixed`?
///
/// Both operands lose a leading `v` and are compared component-wise as numbers.
/// Equal versions satisfy it — a finding names the *first* version carrying the
/// fix, so a tree pinned exactly there is fixed.
///
/// # Where the zero-padding is, and why it is not in [`components`]
///
/// A version with fewer components is the same version with zeros after it:
/// `v3.0.0` and `3.0` are one release, and the two producers do not agree on how
/// many components to print any more than they agree about the `v`. Padding
/// therefore has to happen **here**, because it is a fact about a *pair* —
/// [`components`] sees one operand and cannot know how wide the other is.
///
/// Once both are the same width, `Vec<u64>`'s own ordering *is* component-wise
/// numeric ordering and nothing further is needed. Unpadded it is not: `Vec`
/// compares lexicographically, so `[3, 0]` sorts below `[3, 0, 0]` and a
/// genuinely fixed tree would be reported unfixed.
///
/// A version with a component this cannot read answers `false`, in either
/// position; see the module documentation for why the unreadable case is the
/// open one and not the closed one.
pub fn at_least(shipped: &str, fixed: &str) -> bool {
    // Both operands are destructured in one pattern, so there is a single
    // fail-closed exit rather than one per operand. An unreadable `shipped` and
    // an unreadable `fixed` are different inputs and the same answer, and this
    // is the shape that cannot grow a branch where only one of them is checked.
    let (Some(mut shipped), Some(mut fixed)) = (components(shipped), components(fixed)) else {
        return false;
    };

    let width = shipped.len().max(fixed.len());
    shipped.resize(width, 0);
    fixed.resize(width, 0);

    shipped >= fixed
}

/// A version as numbers, or nothing at all.
///
/// The strip lives here rather than at [`at_least`]'s two operands, so an
/// operand that reached the comparison unstripped would have to have skipped
/// this function entirely.
///
/// `Option` over the whole vector rather than per component, because a
/// component silently defaulted to `0` is exactly the reading that would call
/// `1.0.0-rc1` version one and close a finding the release candidate does not
/// fix. `collect` into an `Option` short-circuits on the first unreadable
/// component, so that is what the missing arm means: not "no components", but
/// "a component nobody here can compare".
///
/// # Why the crate can see it
///
/// [`crate::cve::group`] bounds a bump to the major and minor a fix lands in,
/// which means reading two components of a version — the same reading, over the
/// same two producers' spellings. It calls this rather than splitting on `.`
/// again, so there is one answer to *what does this version string mean* and one
/// place where the leading `v` comes off. A second reader beside it would be a
/// second place for the mixed-prefix pair in this module's header to be got
/// wrong, and this time in a comparison of majors, where getting it wrong reads
/// as a spurious refusal or a crossed API break.
pub(crate) fn components(version: &str) -> Option<Vec<u64>> {
    version
        .strip_prefix('v')
        .unwrap_or(version)
        .split('.')
        .map(|component| component.parse::<u64>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mixed_prefix_pair_that_reported_an_unfixed_cve_as_fixed() {
        // `go list -m` prints v2.52.9 and Wiz prints 2.52.14. Comparing them
        // without stripping `v` from BOTH operands orders the pair wrong, and the
        // wrong answer is silent: an unfixed CVE reports as fixed.
        assert!(at_least("v2.52.14", "2.52.9"));
        assert!(!at_least("v2.52.9", "2.52.14"));
        assert!(at_least("2.52.14", "v2.52.9"));
        assert!(
            at_least("v0.54.0", "0.54.0"),
            "equal versions satisfy at_least"
        );
    }

    #[test]
    fn comparison_is_numeric_and_not_lexical() {
        assert!(
            at_least("1.10.0", "1.9.0"),
            "10 > 9 numerically, and '1' < '9' lexically"
        );
    }

    /// A missing component is a zero, in both directions.
    ///
    /// The two producers do not agree on how many components a version has
    /// either: a scanner may name `3.0` as the fix for a tree pinned at
    /// `v3.0.0`. Those are one version, and the pair is here because `Vec`'s own
    /// ordering would call the shorter one smaller — which would leave a
    /// genuinely fixed tree reported as unfixed. The last two rows are what
    /// stops the padding being satisfied by truncating instead: a pair that
    /// differs *before* the missing component still has to order by that
    /// difference.
    #[test]
    fn a_missing_component_is_a_zero() {
        assert!(at_least("v3.0.0", "3.0"), "3.0.0 is 3.0");
        assert!(at_least("3.0", "v3.0.0"), "and so is the reverse");
        assert!(!at_least("2.52", "2.52.9"), "but 2.52 is below 2.52.9");
        assert!(at_least("v2.53", "2.52.9"), "and 2.53 is above it");
    }

    /// A version this cannot read leaves the finding open.
    ///
    /// Asserted in both positions, because the two operands come from different
    /// producers and only one of them is a Go module version — an unreadable
    /// `shipped` and an unreadable `fixed` are separate paths into the same
    /// answer. Without the negative here, a component parsed as `0` would pass
    /// every other test in this module: `1.0.0-rc1` would read as `1.0.0` and
    /// close a finding the release candidate does not fix.
    ///
    /// These rows are all negative, so on their own they cannot tell a correct
    /// fail-closed from an `at_least` that never answers `true` — measured, not
    /// assumed: this test is the one that **passed** against the inert stub this
    /// module was first committed against. What makes it an assertion is the
    /// positive rows in its two neighbours, and it is not load-bearing without
    /// them.
    #[test]
    fn an_unreadable_version_is_not_fixed() {
        assert!(
            !at_least("v1.0.0-rc1", "1.0.0"),
            "a candidate is not the release"
        );
        assert!(
            !at_least("v1.0.0", "1.0.0-rc1"),
            "nor is it a fix a tree can satisfy"
        );
        assert!(
            !at_least("v0.0.0-20230101120000-abcdef123456", "0.31.0"),
            "a pseudo-version carries no comparable release number"
        );
    }
}
