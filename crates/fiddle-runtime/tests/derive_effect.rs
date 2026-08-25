use fiddle_runtime::core::{EffectName, HumanDecisionRequirement, ENSURE_BRANCH_PUBLISHED};
use fiddle_runtime::effect::{describe, EffectDescriptor, IntegrationOperation, BUILT_IN};
use fiddle_runtime::EnsureBranchPublished;

fn operation() -> EnsureBranchPublished {
    EnsureBranchPublished::new("acme/r".into(), "topic".into(), "abc123".into())
}

#[test]
fn the_generated_output_equals_the_hand_written_one() {
    let op = operation();
    assert_eq!(op.target(), "refs/heads/topic");
    assert_eq!(op.minimum(), HumanDecisionRequirement::Automatic);
    assert_eq!(
        op.payload(),
        r#"{"repo":"acme/r","sha":"abc123"}"#,
        "keys are sorted by field name and the value is the pre-conversion payload"
    );
}

#[test]
fn the_payload_names_the_wire_keys_and_not_the_struct_fields() {
    let payload: serde_json::Value = serde_json::from_str(&operation().payload()).unwrap();
    let keys: Vec<&str> = payload
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, vec!["repo", "sha"]);
}

#[test]
fn the_registry_and_the_operation_read_one_attribute() {
    let generated: EffectDescriptor = EnsureBranchPublished::descriptor();
    assert_eq!(generated.name, ENSURE_BRANCH_PUBLISHED);
    assert_eq!(generated.minimum, operation().minimum());

    let held = describe(&EffectName::parse(ENSURE_BRANCH_PUBLISHED).unwrap())
        .expect("a shipped name is registered");
    assert_eq!(
        *held, generated,
        "the registry entry is the generated descriptor, not a second hand-written one"
    );
    assert!(BUILT_IN.contains(&generated));
}
