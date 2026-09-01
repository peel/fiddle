#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Scope {
    pub max_files_changed: usize,
    pub max_diff_lines: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Change {
    pub files_changed: usize,
    pub diff_lines: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum OutOfScope {
    #[error(
        "the change exceeds max_files_changed: {changed} files changed, and the bound is {bound}"
    )]
    Files { changed: usize, bound: usize },

    #[error(
        "the change exceeds max_diff_lines: {changed} lines changed, and the bound is {bound}"
    )]
    Lines { changed: usize, bound: usize },
}

impl Scope {
    pub fn admits(&self, change: &Change) -> Result<(), OutOfScope> {
        if change.files_changed > self.max_files_changed {
            return Err(OutOfScope::Files {
                changed: change.files_changed,
                bound: self.max_files_changed,
            });
        }
        if change.diff_lines > self.max_diff_lines {
            return Err(OutOfScope::Lines {
                changed: change.diff_lines,
                bound: self.max_diff_lines,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOUNDED: Scope = Scope {
        max_files_changed: 10,
        max_diff_lines: 500,
    };

    fn change(files_changed: usize, diff_lines: usize) -> Change {
        Change {
            files_changed,
            diff_lines,
        }
    }

    #[test]
    fn a_change_that_reaches_a_bound_is_admitted_and_one_past_it_is_not() {
        assert_eq!(BOUNDED.admits(&change(10, 500)), Ok(()));
        assert_eq!(
            BOUNDED.admits(&change(11, 500)),
            Err(OutOfScope::Files {
                changed: 11,
                bound: 10
            }),
            "the bound is the largest change this scope admits, and one file more is refused"
        );
        assert_eq!(
            BOUNDED.admits(&change(10, 501)),
            Err(OutOfScope::Lines {
                changed: 501,
                bound: 500
            }),
            "and one line more is refused on the other bound, so neither row above \
             is passing because this scope refuses nothing"
        );
    }

    #[test]
    fn each_bound_refuses_the_same_change_on_its_own() {
        let measured = change(3, 30);
        assert_eq!(
            Scope {
                max_files_changed: 2,
                max_diff_lines: 500
            }
            .admits(&measured),
            Err(OutOfScope::Files {
                changed: 3,
                bound: 2
            }),
            "one change, and the file bound alone refuses it"
        );
        assert_eq!(
            Scope {
                max_files_changed: 10,
                max_diff_lines: 20
            }
            .admits(&measured),
            Err(OutOfScope::Lines {
                changed: 30,
                bound: 20
            }),
            "the same change, and the line bound alone refuses it"
        );
        assert_eq!(
            BOUNDED.admits(&measured),
            Ok(()),
            "and the same change inside both bounds is admitted, so the two rows \
             above are the bounds biting and not this change being refused by every \
             scope"
        );
    }

    #[test]
    fn a_refusal_names_the_bound_it_broke_and_not_the_other() {
        for (refusal, named, unnamed) in [
            (
                OutOfScope::Files {
                    changed: 3,
                    bound: 2,
                },
                "max_files_changed",
                "max_diff_lines",
            ),
            (
                OutOfScope::Lines {
                    changed: 30,
                    bound: 20,
                },
                "max_diff_lines",
                "max_files_changed",
            ),
        ] {
            let said = refusal.to_string();
            assert!(
                said.contains(named) && !said.contains(unnamed),
                "a refusal an operator reads must name `{named}` and not `{unnamed}`: {said}"
            );
            assert!(
                said.contains("and the bound is"),
                "and it must print the bound beside the measurement, or the number \
                 it refused is unreadable: {said}"
            );
        }
    }
}
