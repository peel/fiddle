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
