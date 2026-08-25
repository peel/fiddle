use fiddle_runtime::effect::Effect;

#[derive(Effect)]
#[effect(
    name = "effect_marks_no_payload_field",
    minimum = "automatic",
    target = "{repo}",
    state = (),
    error = ()
)]
struct EffectMarksNoPayloadField {
    repo: String,
}

fn main() {}
