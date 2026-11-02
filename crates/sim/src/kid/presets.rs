//! Family presets: fill a draft profile's defaults from its `preset` (docs/plans/sim-core.md
//! 5.1). WP5a owns it (WP5b and WP5c add their family's defaults here by asking WP5a).
//!
//! The A1 preset is the reference game, rely-the-needle-4-2 (kid/A1.md): rely's numbers, event
//! lists and classes. Games of the same family override what differs (little-trap has no
//! `platform_gravity_zero`, and so on). The `ladder` action that starts rely's Step list in plan
//! 5.2 is WP5c; until it lands the Step list starts with `gravity` instead (#232). `ad_nudge`
//! and `flip_swap`, dormant in rely, are left out for the same reason. The other families
//! return the A1 profile with their `preset` recorded: WP5b and WP5c replace those arms.

use crate::def::physics::{
    AirJump, DeathStep, FallCapRule, GravityReapply, GroundAlso, KeyOn, KidAlarm, KidCollision, KidKey,
    PlatformRule, PlatformTake, SavePosition, VineKeys, VineModel, WalkRead, WaterAir,
};
use crate::def::{ClassMask, KidFamily, PhysicsProfile, PhysicsProfileDraft};
use crate::input::Action;
use crate::kid::actions::a1::A1Action;
use crate::kid::actions::common::CommonAction as C;
use crate::kid::collisions::block::BlockRun;
use crate::kid::collisions::platform::PlatformRun;
use crate::kid::death::DeathRun;
use crate::kid::{KidAction, KidCollisionRun};
use crate::num::Real;

fn r(v: f64) -> Real {
    Real::new(v)
}

fn common(a: C) -> KidAction {
    KidAction::Common(a)
}

/// The default profile of a family, before a draft's overrides.
pub fn defaults(family: KidFamily) -> PhysicsProfile {
    let mut p = a1();
    p.preset = family;
    p
}

/// Family A1, reference game rely-the-needle-4-2 (kid/A1.md; plan 5.2 example).
fn a1() -> PhysicsProfile {
    let mut p = PhysicsProfile {
        preset: KidFamily::A1,
        ..PhysicsProfile::default()
    };
    p.numbers.jumps = vec![r(8.5), r(7.0)];
    p.numbers.gravity = r(0.4);
    p.numbers.run_speed = r(3.0);
    p.numbers.max_vspeed = r(9.0);
    p.numbers.max_fall_speed = r(8.0);
    p.numbers.release_mult = r(0.45);
    p.numbers.sprite_threshold = r(0.05);
    p.numbers.image_speed_idle = r(0.2);
    p.numbers.image_speed_run = r(0.5);

    p.input.walk = WalkRead::DirectLatch;
    p.input.vine_keys = VineKeys::Edges;

    p.events.create = vec![
        common(C::CreateDefaults),
        common(C::EnsureWorld),
        common(C::FirstPlaySave),
        common(C::ArmPlayTimer),
        common(C::CreateBow),
    ];
    p.events.step = vec![
        common(C::Gravity),
        common(C::Walk),
        common(C::AirSprite),
        common(C::FallCap),
        common(C::Shoot),
        common(C::Jump),
        common(C::Release),
        common(C::Vines),
        KidAction::A1(A1Action::PlatformGravityZero),
        KidAction::A1(A1Action::FadeIn),
    ];
    p.events.alarms = vec![KidAlarm {
        n: 0,
        run: vec![common(C::PlayTimer)],
    }];
    p.events.keys = vec![KidKey {
        action: Action::Suicide,
        on: KeyOn::Press,
        run: vec![common(C::Kill)],
    }];
    p.events.collisions = vec![
        KidCollision {
            with: ClassMask::BLOCK,
            run: KidCollisionRun::Block(BlockRun::BlockStandard),
        },
        KidCollision {
            with: ClassMask::SOLIDBLOCK,
            run: KidCollisionRun::Block(BlockRun::BlockStandard),
        },
        KidCollision {
            with: ClassMask::PLATFORM,
            run: KidCollisionRun::Platform(PlatformRun::PlatformStandard),
        },
        KidCollision {
            with: ClassMask::KILLER,
            run: KidCollisionRun::Death(DeathRun::Kill),
        },
    ];

    p.jump.ground_classes = ClassMask::BLOCK | ClassMask::SOLIDBLOCK | ClassMask::WATER1;
    p.jump.ground_also = vec![GroundAlso::PlatformFlag, GroundAlso::LadderJump];
    p.jump.ground_sets_djump = true;
    p.jump.air = AirJump::Djump;
    p.jump.water_air = WaterAir::Water2WithoutFlag;
    p.fall_cap.rule = FallCapRule::ClampAbs;
    p.gravity.reapply = GravityReapply::Step;
    p.gravity.block_landing_zeroes = true;
    p.platform.rule = PlatformRule::HalfVspeed;
    p.platform.snap = r(9.0);
    p.platform.take = PlatformTake::Vspeed;
    p.platform.hold_probe = r(4.0);
    p.vine.model = VineModel::DistanceAsym;
    p.vine.left_below = r(2.0);
    p.vine.right_equal = r(1.0);
    p.vine.slide = r(2.0);
    p.vine.kick = [r(15.0), r(-9.0)];
    p.vine.drop = r(3.0);
    p.death.order = vec![
        DeathStep::Sound,
        DeathStep::BloodEmitter,
        DeathStep::DestroyKid,
        DeathStep::GameOver,
        DeathStep::DeathsPlusOne,
        DeathStep::SaveCounters,
    ];
    p.death.select_room_reset = true;
    p.bullet.max = 4;
    p.bullet.speed = r(16.0);
    p.save.position = SavePosition::XyBase100Bytes;
    p.misc.fade_in = true;
    p.misc.fade_step = r(0.1);
    p
}

/// The full profile: the draft's values over its family's defaults. A draft with no `preset`
/// starts from A1.
pub fn apply(p: &mut PhysicsProfileDraft) -> PhysicsProfile {
    let d = std::mem::take(p);
    let mut out = defaults(d.preset.unwrap_or_default());
    if let Some(s) = d.numbers {
        out.numbers = s.over(out.numbers);
    }
    if let Some(s) = d.input {
        out.input = s.over(out.input);
    }
    if let Some(s) = d.events {
        out.events = s.over(out.events);
    }
    if let Some(s) = d.jump {
        out.jump = s.over(out.jump);
    }
    if let Some(s) = d.fall_cap {
        out.fall_cap = s.over(out.fall_cap);
    }
    if let Some(s) = d.gravity {
        out.gravity = s.over(out.gravity);
    }
    if let Some(s) = d.platform {
        out.platform = s.over(out.platform);
    }
    if let Some(s) = d.vine {
        out.vine = s.over(out.vine);
    }
    if let Some(s) = d.water {
        out.water = s;
    }
    if let Some(s) = d.ice {
        out.ice = s;
    }
    if let Some(s) = d.slope {
        out.slope = s;
    }
    if let Some(s) = d.ladder {
        out.ladder = s;
    }
    if let Some(s) = d.flip {
        out.flip = s;
    }
    if let Some(s) = d.death {
        out.death = s.over(out.death);
    }
    if let Some(s) = d.bullet {
        out.bullet = s.over(out.bullet);
    }
    if let Some(s) = d.save {
        out.save = s.over(out.save);
    }
    if let Some(s) = d.misc {
        out.misc = s.over(out.misc);
    }
    // `d.sprites` holds names; the loader resolves them to ids.
    out
}
