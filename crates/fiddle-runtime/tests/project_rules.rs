mod fixture;

use fiddle_core::AttemptId;
use fiddle_runtime::agent::{ProjectRules, RuleFile, Withheld, RULES_CAP_BYTES};
use fiddle_runtime::workspace::Workspace;
use std::path::Path;
use tokio_util::sync::CancellationToken;

const RULES: &str = "Never change an assertion in a test.\n";

fn attempt(n: u8) -> AttemptId {
    AttemptId(format!("01JQZX000000000000000000{n}"))
}

fn committed(repo: &Path, message: &str) -> String {
    fixture::git(repo, &["add", "-A"]);
    fixture::git(
        repo,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            message,
        ],
    );
    fixture::git_says(repo, &["rev-parse", "HEAD"])
}

fn cut(repo: &Path, root: &Path, n: u8) -> Workspace {
    Workspace::create(repo, root, &attempt(n), CancellationToken::new())
        .expect("a workspace cut from the fixture's HEAD")
}

#[test]
fn a_link_to_a_rules_file_is_quoted_as_the_file_it_names() {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = fixture::trivial_repo(dir.path());
    std::fs::write(repo.join("CLAUDE.md"), RULES).unwrap();
    std::os::unix::fs::symlink("CLAUDE.md", repo.join("AGENTS.md")).unwrap();
    let base = committed(&repo, "rules, one linked");
    let workspace = cut(&repo, &dir.path().join("ws"), 1);

    assert_eq!(workspace.cut_from(), base);
    let rules = ProjectRules::at(&workspace, workspace.cut_from()).unwrap();

    assert_eq!(
        rules.files(),
        &[
            RuleFile::Quoted {
                name: "CLAUDE.md",
                through: None,
                text: RULES.to_string(),
            },
            RuleFile::Quoted {
                name: "AGENTS.md",
                through: Some("CLAUDE.md".to_string()),
                text: RULES.to_string(),
            },
        ]
    );
}

#[test]
fn the_rules_are_the_commits_and_not_the_working_trees() {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = fixture::trivial_repo(dir.path());
    std::fs::write(repo.join("CLAUDE.md"), RULES).unwrap();
    committed(&repo, "rules");
    let workspace = cut(&repo, &dir.path().join("ws"), 1);
    std::fs::write(workspace.root().join("CLAUDE.md"), "Anything goes.\n").unwrap();
    std::fs::write(workspace.root().join("AGENTS.md"), "Anything goes.\n").unwrap();

    let rules = ProjectRules::at(&workspace, workspace.cut_from()).unwrap();

    assert_eq!(
        rules.files(),
        &[RuleFile::Quoted {
            name: "CLAUDE.md",
            through: None,
            text: RULES.to_string(),
        }],
        "the working tree rewrote CLAUDE.md and added AGENTS.md, and neither is the commit's"
    );
}

#[test]
fn a_file_at_the_cap_is_quoted_and_one_byte_more_is_withheld() {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = fixture::trivial_repo(dir.path());
    let at_the_cap = "x".repeat(RULES_CAP_BYTES as usize);
    let over_it = "y".repeat(RULES_CAP_BYTES as usize + 1);
    std::fs::write(repo.join("CLAUDE.md"), &at_the_cap).unwrap();
    std::fs::write(repo.join("AGENTS.md"), &over_it).unwrap();
    committed(&repo, "rules at and over the cap");
    let workspace = cut(&repo, &dir.path().join("ws"), 1);

    let rules = ProjectRules::at(&workspace, workspace.cut_from()).unwrap();

    assert_eq!(
        rules.files(),
        &[
            RuleFile::Quoted {
                name: "CLAUDE.md",
                through: None,
                text: at_the_cap,
            },
            RuleFile::Withheld {
                name: "AGENTS.md",
                why: Withheld::OverTheCap {
                    bytes: RULES_CAP_BYTES + 1,
                },
            },
        ]
    );
    let fenced = rules.fenced().unwrap();
    assert!(!fenced.contains(&"y".repeat(64)), "{fenced}");
}

#[test]
fn a_link_out_of_the_project_bytes_that_are_not_text_and_a_directory_quote_nothing() {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = fixture::trivial_repo(dir.path());
    std::fs::write(repo.join("CLAUDE.md"), [0xff, 0xfe, b'r', b'u', b'l', b'e']).unwrap();
    std::os::unix::fs::symlink("../outside.md", repo.join("AGENTS.md")).unwrap();
    committed(&repo, "rules nobody can quote");
    let workspace = cut(&repo, &dir.path().join("ws"), 1);

    let rules = ProjectRules::at(&workspace, workspace.cut_from()).unwrap();

    assert_eq!(
        rules.files(),
        &[
            RuleFile::Withheld {
                name: "CLAUDE.md",
                why: Withheld::NotText { bytes: 6 },
            },
            RuleFile::Withheld {
                name: "AGENTS.md",
                why: Withheld::LinkToNoFile {
                    target: "../outside.md".to_string(),
                },
            },
        ]
    );
    let fenced = rules.fenced().unwrap();
    assert!(
        fenced.contains("`CLAUDE.md` holds 6 bytes") && fenced.contains("`read_file`"),
        "{fenced}"
    );

    std::fs::remove_file(repo.join("CLAUDE.md")).unwrap();
    std::fs::remove_file(repo.join("AGENTS.md")).unwrap();
    std::fs::create_dir_all(repo.join("CLAUDE.md")).unwrap();
    std::fs::write(repo.join("CLAUDE.md/notes.md"), RULES).unwrap();
    committed(&repo, "a directory named like a rules file");
    let workspace = cut(&repo, &dir.path().join("ws2"), 2);

    let rules = ProjectRules::at(&workspace, workspace.cut_from()).unwrap();

    assert_eq!(rules.files(), &[], "a directory is no rules file");
    assert_eq!(rules.fenced(), None);
}

#[test]
fn a_revision_the_workspace_cannot_name_is_an_error_and_not_an_empty_answer() {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = fixture::trivial_repo(dir.path());
    let workspace = cut(&repo, &dir.path().join("ws"), 1);

    assert!(
        ProjectRules::at(&workspace, "origin/no-such-branch").is_err(),
        "a base fiddle could not resolve is not a project without rules"
    );
    assert!(ProjectRules::at(&workspace, workspace.cut_from())
        .unwrap()
        .files()
        .is_empty());
}
