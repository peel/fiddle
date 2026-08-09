//! What became of an attempt to change something outside this process.
//!
//! The whole of M2 rests on one distinction, and it is the only thing this
//! module carries today: **a lost answer is not a failed write.** A request
//! whose response never arrived may have landed, and the only honest thing to
//! say about it is that nobody knows. Collapsing that third value into either
//! of the other two produces a duplicate external effect — report a landed
//! write as failed and the retry performs it twice; report a refused one as
//! committed and the world never gets the change at all.
//!
//! The executor that walks validate → identity → postcondition → policy →
//! authorize → delegate → observe → receipt arrives in the next task of this
//! milestone, and lands here beside [`EffectOutcome`]. What is here now is the
//! vocabulary the `gh` adapter classifies into, because the adapter is what
//! first has to make the judgment.

/// The three-valued result an ambiguous write forces.
///
/// Serialized in `snake_case` because it reaches a published bundle, where the
/// consumer matching on it is not this crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectOutcome {
    /// The change is known to have landed, because it was read back.
    Committed,
    /// The change is known not to have landed, because something refused it in
    /// terms that leave no room for it having happened anyway.
    NotCommitted,
    /// Nobody knows. Resolved by reading the world, never by retrying the
    /// mutation.
    Unknown,
}
