//! WP5c.1 (#128): the family A2 and A3 profiles in `physics.toml` form (docs/format/physics.md,
//! kid/A2.md, kid/A3.md).

use iwbtg_sim::def::KidFamily;
use iwbtg_sim::def::physics::{AirJump, PhysicsProfileDraft, VineKeys};
use iwbtg_sim::kid::presets;

const KOTARO: &str = include_str!("../../../docs/format/examples/kotaro-physics.toml");
const WANNA: &str = include_str!("../../../docs/format/examples/wanna-physics.toml");
const SHOWDOWN: &str = include_str!("../../../docs/format/examples/showdown-physics.toml");

fn draft(text: &str) -> PhysicsProfileDraft {
    toml::from_str(text).expect("parses")
}

#[test]
fn kotaro_and_wanna_are_their_presets() {
    assert_eq!(
        presets::apply(&mut draft(KOTARO)),
        presets::defaults(KidFamily::A2)
    );
    assert_eq!(
        presets::apply(&mut draft(WANNA)),
        presets::defaults(KidFamily::A3)
    );
}

#[test]
fn showdown_is_wanna_with_a_third_jump_and_its_own_lists() {
    let p = presets::apply(&mut draft(SHOWDOWN));
    let wanna = presets::defaults(KidFamily::A3);
    assert_eq!(p.jump.air, AirJump::DjumpThenTjump);
    assert!(p.jump.release_twice_while_tjump);
    assert_eq!(p.numbers.jumps.len(), 3);
    assert_eq!(p.input.vine_keys, VineKeys::LatchedFlags, "the A3 flags stay");
    assert!(p.events.end_step.is_empty());
    assert_ne!(p.events.step, wanna.events.step);
}
