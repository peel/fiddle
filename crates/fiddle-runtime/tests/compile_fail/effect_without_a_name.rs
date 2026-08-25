use fiddle_runtime::effect::Effect;

#[derive(Effect)]
#[effect(minimum = "automatic", target = "{repo}", state = (), error = ())]
struct EffectWithoutAName {
    #[payload]
    repo: String,
}

fn main() {}
