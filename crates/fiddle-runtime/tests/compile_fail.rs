#[test]
fn an_external_crate_cannot_construct_an_authorization() {
    trybuild::TestCases::new().compile_fail("tests/compile_fail/authorized_effect_is_unforgeable.rs");
}
