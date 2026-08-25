use fiddle_runtime::effect::Effect;

#[derive(Effect)]
#[effect(
    name = "effect_with_an_unknown_key",
    minimum = "automatic",
    target = "{repo}",
    state = (),
    error = (),
    retries = 3
)]
struct EffectWithAnUnknownKey {
    #[payload]
    repo: String,
}

fn main() {}
