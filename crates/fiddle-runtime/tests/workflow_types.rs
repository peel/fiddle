use fiddle_core::EffectName;
use fiddle_runtime::capability::workflow::{Step, Workflow, WorkflowError, WorkflowFile};
use std::path::PathBuf;

fn canonical() -> Workflow {
    Workflow::new(
        "triage".into(),
        "triage".into(),
        vec![
            Step::Agent {
                prompt: PathBuf::from("prompts/triage.md"),
                max_turns: 8,
            },
            Step::Check {
                program: "true".into(),
                args: vec![],
                timeout_secs: 30,
            },
            Step::Effect {
                name: EffectName::parse("ensure_pull_request").unwrap(),
            },
        ],
    )
    .unwrap()
}

#[derive(Debug, Eq, PartialEq)]
enum Refused {
    Reading,
    Version(u32),
    NoSteps,
}

fn read(document: &str) -> Result<Workflow, Refused> {
    let file = toml::from_str::<WorkflowFile>(document).map_err(|_| Refused::Reading)?;
    Workflow::try_from(file).map_err(|error| match error {
        WorkflowError::Version(found) => Refused::Version(found),
        WorkflowError::NoSteps => Refused::NoSteps,
    })
}

#[test]
fn a_rust_workflow_round_trips_through_toml_unchanged() {
    let document = toml::to_string(&canonical().to_file()).unwrap();
    let back: Workflow = toml::from_str::<WorkflowFile>(&document)
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(canonical(), back, "toml was:\n{document}");
}

#[test]
fn every_malformed_document_is_refused_for_the_reason_its_label_claims() {
    let cases = [
        (
            "unknown field",
            "version = 1\nname = \"t\"\nstage = \"t\"\nextra = 1\n\n[[steps]]\nkind = \"effect\"\nname = \"ensure_pull_request\"\n",
            "version = 1\nname = \"t\"\nstage = \"t\"\n\n[[steps]]\nkind = \"effect\"\nname = \"ensure_pull_request\"\n",
            Refused::Reading,
        ),
        (
            "unknown field in a step",
            "version = 1\nname = \"t\"\nstage = \"t\"\n\n[[steps]]\nkind = \"effect\"\nname = \"ensure_pull_request\"\nextra = 1\n",
            "version = 1\nname = \"t\"\nstage = \"t\"\n\n[[steps]]\nkind = \"effect\"\nname = \"ensure_pull_request\"\n",
            Refused::Reading,
        ),
        (
            "unknown step",
            "version = 1\nname = \"t\"\nstage = \"t\"\n\n[[steps]]\nkind = \"ask\"\nname = \"ensure_pull_request\"\n",
            "version = 1\nname = \"t\"\nstage = \"t\"\n\n[[steps]]\nkind = \"effect\"\nname = \"ensure_pull_request\"\n",
            Refused::Reading,
        ),
        (
            "unknown version",
            "version = 9\nname = \"t\"\nstage = \"t\"\n\n[[steps]]\nkind = \"effect\"\nname = \"ensure_pull_request\"\n",
            "version = 1\nname = \"t\"\nstage = \"t\"\n\n[[steps]]\nkind = \"effect\"\nname = \"ensure_pull_request\"\n",
            Refused::Version(9),
        ),
        (
            "no steps",
            "version = 1\nname = \"t\"\nstage = \"t\"\nsteps = []\n",
            "version = 1\nname = \"t\"\nstage = \"t\"\n\n[[steps]]\nkind = \"effect\"\nname = \"ensure_pull_request\"\n",
            Refused::NoSteps,
        ),
        (
            "unspellable name",
            "version = 1\nname = \"t\"\nstage = \"t\"\n\n[[steps]]\nkind = \"effect\"\nname = \"Ensure_PR\"\n",
            "version = 1\nname = \"t\"\nstage = \"t\"\n\n[[steps]]\nkind = \"effect\"\nname = \"ensure_pull_request\"\n",
            Refused::Reading,
        ),
    ];

    for (why, malformed, corrected, expected) in cases {
        assert_eq!(
            read(malformed).unwrap_err(),
            expected,
            "{why}: {malformed:?}"
        );
        assert!(
            read(corrected).is_ok(),
            "{why}: correcting only the named fault left the document refused: {corrected:?}"
        );
    }
}

#[test]
fn the_rust_constructor_refuses_what_the_file_path_refuses() {
    assert!(
        Workflow::new("t".into(), "t".into(), vec![]).is_err(),
        "an empty workflow is not work"
    );
    assert_eq!(
        read("version = 1\nname = \"t\"\nstage = \"t\"\nsteps = []\n").unwrap_err(),
        Refused::NoSteps,
        "the file path must refuse an empty workflow by the same rule"
    );
}

#[test]
fn a_step_is_one_of_exactly_three_kinds() {
    let named = |step: &Step| match step {
        Step::Agent { .. } => "agent",
        Step::Check { .. } => "check",
        Step::Effect { .. } => "effect",
    };
    let kinds: Vec<&str> = canonical().to_file().steps.iter().map(named).collect();
    assert_eq!(kinds, ["agent", "check", "effect"]);
}
