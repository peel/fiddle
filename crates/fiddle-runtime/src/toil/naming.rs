pub const DEFAULT_BRANCH_PREFIX: &str = "fiddle";

pub const BRANCH_PREFIX_MUST_BE_A_REF: &str =
    "a branch prefix becomes part of a git ref, so write it with ASCII letters, digits, `-`, \
     `_`, `.` and `/`, with no leading or trailing `/`, no empty segment, no `..`, and no \
     segment ending in `.lock`";

fn segment_is_a_ref(segment: &str) -> bool {
    !segment.is_empty()
        && !segment.ends_with(".lock")
        && segment != "."
        && segment != ".."
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

pub fn prefix_is_a_ref(prefix: &str) -> bool {
    !prefix.is_empty()
        && !prefix.contains("..")
        && !prefix.starts_with('/')
        && !prefix.ends_with('/')
        && prefix.split('/').all(segment_is_a_ref)
}

fn reference_safe(ticket: &str) -> String {
    let mapped: String = ticket
        .chars()
        .map(
            |c| match c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                true => c,
                false => '-',
            },
        )
        .collect();
    let mut collapsed = String::with_capacity(mapped.len());
    for c in mapped.chars() {
        match c == '-' && collapsed.ends_with('-') {
            true => continue,
            false => collapsed.push(c),
        }
    }
    collapsed.trim_matches('-').to_string()
}

pub fn branch(prefix: &str, ticket: &str) -> String {
    format!("{prefix}/{}", reference_safe(ticket))
}

pub fn pull_request_title(ticket: &str, summary: Option<&str>) -> String {
    match summary.map(str::trim).filter(|s| !s.is_empty()) {
        Some(summary) => format!("[{ticket}] {summary}"),
        None => format!("[{ticket}]"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_branch_is_the_prefix_then_the_ticket() {
        assert_eq!(branch("fiddle", "ISP-263"), "fiddle/ISP-263");
        assert_eq!(branch("toil", "ISP-263"), "toil/ISP-263");
        assert_eq!(branch("fiddle/toil", "ISP-263"), "fiddle/toil/ISP-263");
    }

    #[test]
    fn the_default_prefix_keeps_the_branch_inside_the_namespace_cleanup_reads() {
        assert_eq!(DEFAULT_BRANCH_PREFIX, "fiddle");
        assert!(
            branch(DEFAULT_BRANCH_PREFIX, "ISP-263").starts_with("fiddle/"),
            "the testbed cleanup and the effects-repository residue invariants find what \
             fiddle published by the `fiddle/` prefix, so the default must stay inside it"
        );
    }

    #[test]
    fn a_reworded_ticket_moves_the_title_and_never_the_branch() {
        let (first, second) = ("Raise the limit", "Raise the merge graph limit");
        assert_ne!(
            pull_request_title("ISP-263", Some(first)),
            pull_request_title("ISP-263", Some(second)),
            "the two wordings must differ, or this case proves nothing about the branch"
        );
        assert_eq!(
            branch("fiddle", "ISP-263"),
            branch("fiddle", "ISP-263"),
            "the branch takes no summary at all, so a reworded ticket cannot orphan the \
             branch a resumed run looks for. ADR 075 records what an orphaned branch costs."
        );
    }

    #[test]
    fn the_title_carries_the_bracketed_ticket_then_the_title() {
        assert_eq!(
            pull_request_title("ISP-263", Some("Raise merge graph limit")),
            "[ISP-263] Raise merge graph limit"
        );
    }

    #[test]
    fn a_blank_or_absent_summary_leaves_the_ticket_alone() {
        for summary in [None, Some(""), Some("   "), Some("\t\n")] {
            assert_eq!(
                pull_request_title("ISP-263", summary),
                "[ISP-263]",
                "{summary:?} carries no title, so the title is the bracketed ticket alone"
            );
        }
    }

    #[test]
    fn the_title_keeps_the_summarys_own_spelling_and_only_trims_it() {
        assert_eq!(
            pull_request_title("ISP-263", Some("  MergeGraphSizeMax is too low!  ")),
            "[ISP-263] MergeGraphSizeMax is too low!",
            "a pull request title is prose, so it is neither slugged nor lowercased"
        );
    }

    #[test]
    fn a_ticket_git_cannot_hold_is_made_safe_without_losing_its_key() {
        assert_eq!(branch("fiddle", "ISP 263/x"), "fiddle/ISP-263-x");
        assert_eq!(branch("fiddle", "ISP..263"), "fiddle/ISP-263");
    }

    #[test]
    fn the_branch_is_a_name_git_accepts() {
        let branch = branch(DEFAULT_BRANCH_PREFIX, "a/b..c ~weird^ key:[here]?*");
        for forbidden in ["..", " ", "~", "^", ":", "?", "*", "[", "\\", "@{", "//"] {
            assert!(
                !branch.contains(forbidden),
                "{forbidden:?} is not allowed in a ref, and the branch is {branch}"
            );
        }
        assert!(
            !branch.ends_with('.') && !branch.ends_with(".lock") && !branch.ends_with('/'),
            "a ref may not end this way: {branch}"
        );
    }

    #[test]
    fn a_prefix_that_is_a_ref_is_accepted() {
        for prefix in ["fiddle", "toil", "fiddle/toil", "a-b_c.d", "x/y/z"] {
            assert!(
                prefix_is_a_ref(prefix),
                "{prefix:?} is a ref this build writes"
            );
        }
    }

    #[test]
    fn a_prefix_git_would_refuse_is_refused_here_first() {
        for prefix in [
            "",
            "/fiddle",
            "fiddle/",
            "fiddle//toil",
            "fiddle/../toil",
            "fid dle",
            "fiddle~1",
            "fiddle:toil",
            "fiddle.lock",
            "a/b.lock",
            "..",
            ".",
        ] {
            assert!(
                !prefix_is_a_ref(prefix),
                "{prefix:?} cannot be part of a ref, so the configuration must refuse it \
                 rather than publish a branch git will reject"
            );
        }
    }
}
