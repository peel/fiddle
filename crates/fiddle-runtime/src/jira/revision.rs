use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{OffsetDateTime, UtcOffset};

pub fn canonical_revision(updated: &str) -> Option<String> {
    read_instant(updated)?
        .to_offset(UtcOffset::UTC)
        .format(&Rfc3339)
        .ok()
}

fn read_instant(updated: &str) -> Option<OffsetDateTime> {
    let subsecond = format_description!(
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond][offset_hour \
         sign:mandatory][offset_minute]"
    );
    let whole_second = format_description!(
        "[year]-[month]-[day]T[hour]:[minute]:[second][offset_hour sign:mandatory][offset_minute]"
    );
    OffsetDateTime::parse(updated, &Rfc3339)
        .or_else(|_| OffsetDateTime::parse(updated, &subsecond))
        .or_else(|_| OffsetDateTime::parse(updated, &whole_second))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_colonless_offset_jira_cloud_sends_is_read_and_is_never_carried_through_raw() {
        let sent = "2026-08-26T07:00:00.000+0000";
        assert!(
            OffsetDateTime::parse(sent, &Rfc3339).is_err(),
            "`{sent}` is what a measured jira cloud site sends and rfc 3339 cannot spell it, so \
             the two further format descriptions are what make this function answer at all"
        );
        assert_eq!(
            canonical_revision(sent).as_deref(),
            Some("2026-08-26T07:00:00Z"),
            "a raw pass-through reds here, and it would give one state two identities"
        );
    }

    #[test]
    fn one_instant_spelled_four_ways_canonicalises_to_one_revision() {
        let spellings = [
            "2026-08-26T09:15:00.000+0000",
            "2026-08-26T09:15:00+0000",
            "2026-08-26T09:15:00Z",
            "2026-08-26T11:15:00.000+0200",
        ];
        let canonical: Vec<Option<String>> = spellings
            .iter()
            .map(|spelling| canonical_revision(spelling))
            .collect();

        assert_eq!(
            canonical,
            vec![Some("2026-08-26T09:15:00Z".to_string()); spellings.len()],
            "atlassian answers in the reading user's zone, so two spellings of one instant must \
             not become two identities for one state"
        );
    }

    #[test]
    fn two_instants_that_differ_keep_two_revisions() {
        assert_ne!(
            canonical_revision("2026-08-26T09:15:00.000+0000"),
            canonical_revision("2026-08-26T09:15:01.000+0000"),
            "a canonicaliser that collapsed two instants would let a stale approval name a fresh \
             state"
        );
    }

    #[test]
    fn a_time_this_function_cannot_read_answers_nothing_rather_than_its_own_text() {
        for unreadable in [
            "",
            "yesterday",
            "2026-08-26",
            "2026-08-26T09:15:00",
            "2026-13-26T09:15:00.000+0000",
        ] {
            assert_eq!(
                canonical_revision(unreadable),
                None,
                "`{unreadable}` is not a time, and answering with its own text would make an \
                 unreadable field look like a revision"
            );
        }
    }
}
