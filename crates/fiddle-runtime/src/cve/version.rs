pub fn at_least(shipped: &str, fixed: &str) -> bool {
    let (Some(mut shipped), Some(mut fixed)) = (components(shipped), components(fixed)) else {
        return false;
    };

    let width = shipped.len().max(fixed.len());
    shipped.resize(width, 0);
    fixed.resize(width, 0);

    shipped >= fixed
}

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

    #[test]
    fn a_missing_component_is_a_zero() {
        assert!(at_least("v3.0.0", "3.0"), "3.0.0 is 3.0");
        assert!(at_least("3.0", "v3.0.0"), "and so is the reverse");
        assert!(!at_least("2.52", "2.52.9"), "but 2.52 is below 2.52.9");
        assert!(at_least("v2.53", "2.52.9"), "and 2.53 is above it");
    }

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
