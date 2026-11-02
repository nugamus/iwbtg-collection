//! WP5b.6 (#264): the family B profiles of kaenbyou and eat-the-lemon in `physics.toml` form
//! (docs/format/physics.md, kid/B.md).

use iwbtg_sim::def::KidFamily;
use iwbtg_sim::def::physics::PhysicsProfileDraft;
use iwbtg_sim::kid::presets;

fn draft(text: &str) -> Result<PhysicsProfileDraft, toml::de::Error> {
    toml::from_str(text)
}

const KAENBYOU: &str = include_str!("../../../docs/format/examples/kaenbyou-physics.toml");
const LEMON: &str = include_str!("../../../docs/format/examples/eat-the-lemon-physics.toml");

#[test]
fn kaenbyou_written_out_in_full_equals_the_b_preset() {
    let mut d = draft(KAENBYOU).expect("kaenbyou's physics.toml parses");
    assert_eq!(presets::apply(&mut d), presets::defaults(KidFamily::B));
}

#[test]
fn eat_the_lemon_is_the_b_preset_plus_its_two_differences() {
    let mut d = draft(LEMON).expect("eat-the-lemon's physics.toml parses");
    let p = presets::apply(&mut d);
    let mut expected = presets::defaults(KidFamily::B);
    expected.jump.blocked_by_pause_press = false;
    expected.events.alarms.clear();
    assert_eq!(p, expected);
}
