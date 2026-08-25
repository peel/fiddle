use fiddle_runtime::core::{EffectId, PayloadHash};
use fiddle_runtime::effect::AuthorizedEffect;

fn main() {
    let _forged: AuthorizedEffect<()> = AuthorizedEffect {
        effect_id: EffectId("0000000000000000".to_string()),
        payload_hash: PayloadHash("0000000000000000".to_string()),
        operation: (),
    };
}
