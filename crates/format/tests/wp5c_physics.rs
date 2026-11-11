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

const MACHINARY: &str = include_str!("../../../docs/format/examples/machinary-physics.toml");
const NEW_YEAR: &str = include_str!("../../../docs/format/examples/eat-the-new-year-physics.toml");
const LITTLE_RUNNER: &str = include_str!("../../../docs/format/examples/little-runner-physics.toml");
const SPOOK: &str = include_str!("../../../docs/format/examples/spook-house-physics.toml");

#[test]
fn machinary_and_little_runner_are_their_presets() {
    assert_eq!(
        presets::apply(&mut draft(MACHINARY)),
        presets::defaults(KidFamily::A4)
    );
    assert_eq!(
        presets::apply(&mut draft(LITTLE_RUNNER)),
        presets::defaults(KidFamily::A5)
    );
}

#[test]
fn eat_the_new_year_adds_a_bow_a_kid_timer_and_its_edges() {
    let p = presets::apply(&mut draft(NEW_YEAR));
    let a4 = presets::defaults(KidFamily::A4);
    assert_eq!(p.events.create.len(), a4.events.create.len() + 2);
    assert_eq!(p.events.alarms.len(), 1);
    assert_eq!(
        p.events.boundary,
        iwbtg_sim::def::physics::BoundaryModel::IntersectOrigin
    );
    assert_eq!(p.numbers.release_mult, a4.numbers.release_mult, "still 0.5");
}

#[test]
fn spook_house_is_a5_with_its_own_step_cap_and_killer() {
    use iwbtg_sim::def::physics::{BoundaryModel, FallCapRule};
    let p = presets::apply(&mut draft(SPOOK));
    assert_eq!(p.fall_cap.rule, FallCapRule::ClampDownOnly);
    assert_eq!(p.events.boundary, BoundaryModel::None);
    assert!(p.misc.idle_needs_block);
    assert_eq!(p.events.end_step.len(), 1);
    assert_eq!(p.events.step.len(), 8);
}

const XIA: &str = include_str!("../../../docs/format/examples/xia-physics.toml");

#[test]
fn xia_is_the_c_preset() {
    use iwbtg_sim::def::physics::{BoundaryModel, FlipKind, VineModel};
    let p = presets::apply(&mut draft(XIA));
    assert_eq!(p, presets::defaults(KidFamily::C));
    assert_eq!(p.flip.model, FlipKind::GravSign);
    assert_eq!(p.vine.model, VineModel::DistanceSymEarly);
    assert_eq!(p.events.boundary, BoundaryModel::EndStepOrigin);
}

const RAINBOW: &str = include_str!("../../../docs/format/examples/rainbow-physics.toml");
const PORONKUSEMA: &str = include_str!("../../../docs/format/examples/poronkusema-physics.toml");
const RORI: &str = include_str!("../../../docs/format/examples/rori-physics.toml");
const NEW_YEAR_SLOPES: &str = include_str!("../../../docs/format/examples/eat-the-new-year-physics.toml");

#[test]
fn the_slope_examples_load_with_their_models() {
    use iwbtg_sim::def::ClassMask;
    use iwbtg_sim::def::physics::{BlockSlopeStep, Downhill};
    use iwbtg_sim::kid::actions::slope::SlopeAction;
    use iwbtg_sim::kid::collisions::slope::SlopeRun;
    use iwbtg_sim::kid::{KidAction, KidCollisionRun};
    let r = presets::apply(&mut draft(RAINBOW));
    assert_eq!(r.slope.block_step, BlockSlopeStep::FreeSpot);
    assert_eq!(r.slope.downhill, Downhill::Rainbow);
    assert!(r.events.step.contains(&KidAction::Slope(SlopeAction::SlopeJump)));
    assert_eq!(
        r.events.end_step,
        vec![KidAction::Slope(SlopeAction::DownhillSnap)]
    );
    assert!(
        r.events
            .collisions
            .iter()
            .any(|c| c.with == ClassMask::SLOPE_MIRROR
                && c.run == KidCollisionRun::Slope(SlopeRun::BlockSlopeStep))
    );
    let p = presets::apply(&mut draft(PORONKUSEMA));
    assert_eq!(p.slope.downhill, Downhill::Por);
    // playerJump: the single jump also counts a nanameblock 1 px below (kid/A1.md poronkusema).
    assert_eq!(
        p.jump.ground_classes,
        ClassMask(ClassMask::BLOCK.0 | ClassMask::SLOPE.0)
    );
    assert!(
        p.events
            .collisions
            .iter()
            .any(|c| c.run == KidCollisionRun::Slope(SlopeRun::SlopePorRise))
    );
    let o = presets::apply(&mut draft(RORI));
    assert_eq!(o.slope.block_step, BlockSlopeStep::Replace);
    let n = presets::apply(&mut draft(NEW_YEAR_SLOPES));
    assert!(n.slope.keep_hspeed);
    assert!(
        n.events
            .collisions
            .iter()
            .any(|c| c.run == KidCollisionRun::Slope(SlopeRun::SlopeYane))
    );
}
