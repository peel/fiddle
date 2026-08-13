//! Free text a run promotes into what it publishes.
//!
//! Every other field of a [`ReportBundle`](crate::ReportBundle) is a value from
//! a closed set — an id, a status, a schema name, a hash. The three reasons on
//! [`RunOutcome`](crate::RunOutcome) and the summary on
//! [`ProgressEntry`](crate::ProgressEntry) are the exceptions: they are prose,
//! and prose is where whatever the run happened to be holding ends up. A gateway
//! response body, a check runner's stderr, an `io::Error` — each of them has
//! reached one of those four fields by way of `error.to_string()`.
//!
//! # Why a type rather than a call
//!
//! Because the alternative had already failed twice. The rule "sanitise a string
//! before publishing it" is a rule someone has to remember at every site that
//! builds one, and the sites are spread across two crates and grow with every
//! new failure mode. A `String` field accepts anything; a [`Published`] field
//! accepts only what came out of [`Published::of`]. So a variant added later
//! cannot bypass the policy by being written correctly-looking — there is no
//! other way to fill it in.
//!
//! # What the policy is, and what it deliberately is not
//!
//! It is a **bound**: no published string exceeds [`PUBLISHED_TEXT_LIMIT`]
//! characters, and one that would have is cut with a marker saying how long it
//! really was. That is the whole of it, and the narrowness is the point — a
//! bound is a property of the text alone, so it holds against every input
//! including one chosen by somebody hostile.
//!
//! It is **not** a redactor. Nothing here inspects the text for secrets, because
//! a denylist over content an adversary picks is not a guarantee: a gateway that
//! echoed a credential in a shape no pattern anticipated would pass any such
//! filter. The two things that could carry a secret into a published field are
//! handled where they enter instead, which is the only place the question has an
//! answer:
//!
//! - a **provider response body** is never quoted at all — see
//!   `fiddle_runtime::agent::classify`;
//! - a **workspace command's output** is relativised against its own root at the
//!   moment the result is built — see `fiddle_runtime::workspace::command`.
//!
//! This module is what keeps the *size* of the published document a property of
//! fiddle rather than of whatever printed the most.

/// The ceiling, in characters, on any single string a run publishes.
///
/// Characters and not bytes, because the cut has to land on a boundary and a
/// byte bound would have to round down to one anyway; and because "how much
/// text" is what a reader is bounded by.
///
/// 2048 is chosen against the two things that fill these fields. A run's own
/// diagnostics — a journal path, a bound that was exhausted, a change set that
/// could not be written — are a line or two, so the bound never touches them.
/// A check runner's output is unbounded and its most useful part is at the top:
/// the first compiler error, the first failing assertion. 2048 characters is
/// roughly twenty-five lines, which carries that and stops well short of
/// letting one failing `cargo test` decide how large a published bundle is.
///
/// The full output is not lost by being cut here. It reached the model through
/// `run_check`, and it is the check's own to reproduce; what a bundle carries is
/// the head of it and an honest statement that there was more.
pub const PUBLISHED_TEXT_LIMIT: usize = 2048;

/// A string that has been through the publication policy.
///
/// Serialized transparently, so a field of this type is indistinguishable from
/// the `String` it replaced in the `--json` payload and in `report.json`. The
/// wire contract did not change; what changed is that nothing can fill the field
/// without going through [`Published::of`].
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct Published(String);

impl Published {
    /// `text`, bounded to [`PUBLISHED_TEXT_LIMIT`] characters.
    ///
    /// The **only** constructor, which is the whole design. A string that is
    /// already short enough is carried through unchanged, so the overwhelming
    /// majority of bundles — every one M0 publishes — are byte-identical to
    /// what they were before this type existed.
    ///
    /// What is cut is the tail, not the head: a diagnostic says what happened
    /// first and elaborates afterwards, so the first characters are the ones
    /// worth keeping. The marker names the *original* length rather than how
    /// many characters were dropped, because that is the number a reader needs
    /// to decide whether to go and look at the source.
    pub fn of(text: impl AsRef<str>) -> Self {
        let text = text.as_ref();
        let length = text.chars().count();
        if length <= PUBLISHED_TEXT_LIMIT {
            return Published(text.to_string());
        }

        let marker = format!(" […truncated; {length} characters in full]");
        // The marker is part of the bound, not an exemption from it: what is
        // published is at most PUBLISHED_TEXT_LIMIT characters including it.
        // `saturating_sub` covers the degenerate case of a limit smaller than
        // the marker, where the honest answer is to publish the marker alone.
        let kept = PUBLISHED_TEXT_LIMIT.saturating_sub(marker.chars().count());
        let head: String = text.chars().take(kept).collect();
        Published(format!("{head}{marker}"))
    }

    /// The text, as published.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Published {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ordinary case, and the one that keeps every existing bundle
    /// unchanged: a short diagnostic is published exactly as it was written.
    #[test]
    fn text_within_the_bound_is_untouched() {
        let reason = "could not record the attempt journal at /r/.attempts: denied";
        assert_eq!(Published::of(reason).as_str(), reason);
        assert_eq!(
            serde_json::to_value(Published::of(reason)).unwrap(),
            serde_json::json!(reason),
            "the field must serialize as the bare string it replaced"
        );
    }

    /// The bound is on what is published, marker included — so a reader can
    /// state one number and be right about the whole field.
    ///
    /// # `says_so` was the half this test did not check
    ///
    /// It asserted the character count, the surviving head, and that the original
    /// length *appears*. A marker of bare `" {length}"` — no word, no unit —
    /// satisfies all three, and the count assertion cannot see it at all, because
    /// `kept` is derived from the marker's own length and so the field is exactly
    /// `PUBLISHED_TEXT_LIMIT` characters under any marker whatever. The field could
    /// therefore have been reduced to `xxx…xxx 8192`, handing a reader a number
    /// with nothing saying what it counted or that anything had been dropped.
    /// Measured rather than reasoned: under that mutation `-p fiddle-core --lib`
    /// stayed **64 passed / 0 failed**.
    ///
    /// What the two assertions at the end pin is the marker's *meaning* — that it
    /// says something was truncated, and in what unit the number is. The `…` glyph
    /// is deliberately **not** pinned. It once mattered by accident: U+2026 is
    /// three bytes inside a bound counted in characters, so it donated the whole
    /// 2-byte margin by which `interpretation::a_redirect_instruction_is_capped`
    /// failed when the redirect byte cap was deleted, and respelling it here turned
    /// that assertion green under the mutation it existed to catch. That row now
    /// uses a multi-byte input and draws no margin from here, which is what makes
    /// the glyph a free cosmetic choice rather than something to pin.
    #[test]
    fn text_past_the_bound_is_cut_to_it_and_says_so() {
        let loud = "x".repeat(PUBLISHED_TEXT_LIMIT * 4);
        let published = Published::of(&loud);

        assert_eq!(
            published.as_str().chars().count(),
            PUBLISHED_TEXT_LIMIT,
            "the marker is inside the bound, not added on top of it"
        );
        assert!(
            published.as_str().starts_with("xxxx"),
            "the head is what is kept: {}",
            published.as_str()
        );
        assert!(
            published
                .as_str()
                .contains(&(PUBLISHED_TEXT_LIMIT * 4).to_string()),
            "a reader must be told how much there really was: {}",
            published.as_str()
        );
        assert!(
            published.as_str().contains("truncated"),
            "a number alone does not say anything was dropped: {}",
            published.as_str()
        );
        assert!(
            published.as_str().contains("characters in full"),
            "nor does it say what the number counts: {}",
            published.as_str()
        );
    }

    /// The cut lands on a character boundary rather than a byte one, so a
    /// diagnostic quoting a source file in any script survives being bounded.
    #[test]
    fn a_multibyte_string_is_cut_on_a_character_boundary() {
        let loud = "日".repeat(PUBLISHED_TEXT_LIMIT * 2);
        let published = Published::of(loud);

        assert_eq!(published.as_str().chars().count(), PUBLISHED_TEXT_LIMIT);
        assert!(published.as_str().starts_with('日'));
    }

    /// Exactly at the bound is within it: the marker only appears when
    /// something was actually dropped.
    #[test]
    fn the_bound_is_inclusive() {
        let exact = "y".repeat(PUBLISHED_TEXT_LIMIT);
        assert_eq!(Published::of(&exact).as_str(), exact);
        assert_eq!(
            Published::of("z".repeat(PUBLISHED_TEXT_LIMIT + 1))
                .as_str()
                .chars()
                .count(),
            PUBLISHED_TEXT_LIMIT
        );
    }
}
