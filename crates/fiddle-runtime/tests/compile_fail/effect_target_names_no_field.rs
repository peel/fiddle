use fiddle_runtime::effect::Effect;

#[derive(Effect)]
#[effect(
    name = "effect_target_names_no_field",
    minimum = "automatic",
    target = "{repo}#{branch}",
    state = (),
    error = ()
)]
struct EffectTargetNamesNoField {
    #[payload]
    repo: String,
}

fn main() {}
