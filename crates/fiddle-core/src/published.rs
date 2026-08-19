pub const PUBLISHED_TEXT_LIMIT: usize = 2048;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct Published(String);

impl Published {
    pub fn of(text: impl AsRef<str>) -> Self {
        let text = text.as_ref();
        let length = text.chars().count();
        if length <= PUBLISHED_TEXT_LIMIT {
            return Published(text.to_string());
        }

        let marker = format!(" […truncated; {length} characters in full]");
        let kept = PUBLISHED_TEXT_LIMIT.saturating_sub(marker.chars().count());
        let head: String = text.chars().take(kept).collect();
        Published(format!("{head}{marker}"))
    }

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

    #[test]
    fn a_multibyte_string_is_cut_on_a_character_boundary() {
        let loud = "日".repeat(PUBLISHED_TEXT_LIMIT * 2);
        let published = Published::of(loud);

        assert_eq!(published.as_str().chars().count(), PUBLISHED_TEXT_LIMIT);
        assert!(published.as_str().starts_with('日'));
    }

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
