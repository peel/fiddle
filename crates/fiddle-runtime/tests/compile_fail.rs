#[test]
fn an_external_crate_cannot_construct_an_authorization() {
    trybuild::TestCases::new()
        .compile_fail("tests/compile_fail/authorized_effect_is_unforgeable.rs");
}

#[test]
fn a_malformed_effect_attribute_does_not_compile() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/compile_fail/effect_without_a_name.rs");
    cases.compile_fail("tests/compile_fail/effect_with_an_unknown_key.rs");
    cases.compile_fail("tests/compile_fail/effect_target_names_no_field.rs");
    cases.compile_fail("tests/compile_fail/effect_marks_no_payload_field.rs");
}

#[test]
fn a_target_that_names_an_issue_without_a_revision_does_not_compile() {
    trybuild::TestCases::new().compile_fail("tests/compile_fail/jira_target_without_revision.rs");
}

#[test]
fn the_jira_client_can_be_neither_printed_nor_serialized() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/compile_fail/jira_http_is_not_printable.rs");
    cases.compile_fail("tests/compile_fail/jira_http_is_not_serializable.rs");
}
