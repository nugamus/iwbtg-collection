//! Family presets: fill a draft profile's defaults from its `preset` (docs/plans/sim-core.md
//! 5.1). WP5a owns it (WP5b and WP5c add their family's defaults here by asking WP5a).
//!
//! The A1 preset is the reference game, rely-the-needle-4-2 (kid/A1.md): rely's numbers, event
//! lists and classes. Games of the same family override what differs (little-trap has no
//! `platform_gravity_zero`, and so on). The `ladder` action that starts rely's Step list in plan
//! 5.2 is WP5c; until it lands the Step list starts with `gravity` instead (#232). `ad_nudge`
//! and `flip_swap`, dormant in rely, are left out for the same reason. The other families
//! return the A1 profile with their `preset` recorded: WP5c replaces those arms. Family B is
//! kaenbyou (kid/B.md).

use crate::def::physics::{
    AirJump, BoundaryModel, DeathStep, FallCapRule, FlipKind, GravityReapply, GroundAlso, IceModel, IceRules,
    JumpCancel, KeyOn, KidAlarm, KidCollision, KidKey, PlatformRule, PlatformTake, SavePosition, VineKeys,
    VineModel, WalkRead, WaterAir,
};
use crate::def::{ClassMask, KidFamily, PhysicsProfile, PhysicsProfileDraft};
use crate::input::Action;
use crate::kid::actions::a1::A1Action;
use crate::kid::actions::a3::A3Action;
use crate::kid::actions::a5::A5Action;
use crate::kid::actions::b::BAction;
use crate::kid::actions::c::CAction;
use crate::kid::actions::common::CommonAction as C;
use crate::kid::actions::flip::FlipAction;
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
    let mut p = match family {
        KidFamily::A2 => a2(),
        KidFamily::A3 => a3(),
        KidFamily::A4 => a4(),
        KidFamily::A5 => a5(),
        KidFamily::C => c(),
        KidFamily::B => b(),
        _ => a1(),
    };
    p.preset = family;
    p
}

/// Family A2, reference game deride-kotaro-from-aslant (kid/A2.md): rely's engine without the
/// fade-in and the platform gravity rule. Animus and easy-miku-2 add `ad_nudge` to the Step list.
fn a2() -> PhysicsProfile {
    let mut p = a1();
    // Kotaro's player2 (kid/A2.md): vine probes look down, solidblock does not restore `djump`.
    p.vine.probe_down_fixed = true;
    p.flip.solidblock_no_refresh = true;
    p.events.step = vec![
        common(C::Gravity),
        common(C::Walk),
        common(C::AirSprite),
        common(C::FallCap),
        common(C::Shoot),
        common(C::Jump),
        common(C::Release),
        common(C::Vines),
        KidAction::Flip(FlipAction::FlipSwap),
    ];
    p.misc.fade_in = false;
    p
}

/// Family A3, reference game wanna (kid/A3.md): the game's own vine-key flags, walking that
/// ignores vines, gravity reapplied after the jump code, the killer test in the End Step and the
/// held Suicide key in the Step. Showdown overrides the jumps and more (docs/format/examples).
fn a3() -> PhysicsProfile {
    let mut p = a1();
    p.ice = IceRules::default(); // no slip blocks in this family's walk
    p.input.vine_keys = VineKeys::LatchedFlags;
    p.vine.walk_ignores_vines = true;
    p.events.create = vec![
        common(C::CreateDefaults),
        common(C::EnsureWorld),
        common(C::CreateBow),
        common(C::FirstPlaySave),
        common(C::ArmPlayTimer),
    ];
    p.events.step = vec![
        common(C::Walk),
        common(C::AirSprite),
        common(C::FallCap),
        common(C::Shoot),
        common(C::Jump),
        common(C::Release),
        common(C::Gravity),
        KidAction::Flip(FlipAction::FlipSwap),
        KidAction::A3(A3Action::KeyFlags),
        common(C::Vines),
        KidAction::A3(A3Action::SuicideCheck),
    ];
    p.events.end_step = vec![KidAction::B(BAction::KillerCheck)];
    p.events.keys = vec![];
    p.events.collisions = vec![
        KidCollision {
            with: ClassMask::BLOCK,
            run: KidCollisionRun::Block(BlockRun::BlockStandard),
        },
        KidCollision {
            with: ClassMask::PLATFORM,
            run: KidCollisionRun::Platform(PlatformRun::PlatformStandard),
        },
    ];
    p.jump.ground_classes = ClassMask::BLOCK;
    p.jump.ground_also = vec![GroundAlso::PlatformFlag];
    p.jump.water_air = WaterAir::None;
    p.gravity.block_landing_zeroes = false;
    p.death.order = vec![
        DeathStep::Sound,
        DeathStep::BloodEmitter,
        DeathStep::DestroyAllKids,
        DeathStep::GameOver,
        DeathStep::DeathsPlusOne,
        DeathStep::SaveCounters,
    ];
    p.death.select_room_reset = false;
    p.death.killer_check_menu_rooms = true;
    p.misc.fade_in = false;
    p
}

/// Family A4, reference game machinary (kid/A4.md): no vines, release multiplier 0.5, the shifting
/// fall cap, a ground jump that leaves `djump` alone, gravity set at Create only, machinary's fixed
/// room edges. The world counts the play time. eat-the-new-year differs (docs/format/examples).
fn a4() -> PhysicsProfile {
    let mut p = a1();
    p.flip.model = FlipKind::None;
    p.ice = IceRules::default(); // no slip blocks in this family's walk
    p.numbers.release_mult = r(0.5);
    p.vine.model = VineModel::None;
    p.vine.walk_ignores_vines = true;
    p.events.create = vec![
        common(C::CreateDefaults),
        common(C::EnsureWorld),
        common(C::FirstPlaySave),
    ];
    p.events.step = vec![
        common(C::Walk),
        common(C::AirSprite),
        common(C::FallCap),
        common(C::Shoot),
        common(C::Jump),
        common(C::Release),
    ];
    p.events.alarms = vec![];
    p.events.boundary = BoundaryModel::IntersectOriginFixed;
    p.events.collisions = vec![
        KidCollision {
            with: ClassMask::BLOCK,
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
    p.jump.ground_classes = ClassMask::BLOCK;
    p.jump.ground_also = vec![GroundAlso::PlatformFlag];
    p.jump.ground_sets_djump = false;
    p.jump.water_air = WaterAir::None;
    p.fall_cap.rule = FallCapRule::ShiftThenClamp;
    p.gravity.reapply = GravityReapply::CreateOnly;
    p.gravity.block_landing_zeroes = false;
    p.death.order = vec![
        DeathStep::Sound,
        DeathStep::BloodEmitter,
        DeathStep::DestroyAllKids,
        DeathStep::GameOver,
        DeathStep::DeathsPlusOne,
        DeathStep::SaveCounters,
    ];
    p.death.select_room_reset = false;
    p.save.position = SavePosition::XyText2Dec;
    p.misc.fade_in = false;
    p
}

/// Family A5, reference game little-runner (kid/A5.md): rely's engine without vines and the
/// fade-in, gravity and the unstick in the End Step, base-256 saves. go-the-spook-house overrides
/// the Step list, the fall cap and the killer test (docs/format/examples).
fn a5() -> PhysicsProfile {
    let mut p = a1();
    p.flip.model = FlipKind::None;
    p.ice = IceRules::default(); // no slip blocks in this family's walk
    p.vine.model = VineModel::None;
    p.vine.walk_ignores_vines = true;
    p.events.create = vec![
        common(C::CreateDefaults),
        common(C::EnsureWorld),
        common(C::CreateBow),
        common(C::FirstPlaySave),
        common(C::ArmPlayTimer),
    ];
    p.events.step = vec![
        common(C::Walk),
        common(C::AirSprite),
        common(C::FallCap),
        common(C::Shoot),
        common(C::Jump),
        common(C::Release),
    ];
    p.events.end_step = vec![KidAction::A5(A5Action::GravityAndUnstuck)];
    p.events.collisions = vec![
        KidCollision {
            with: ClassMask::BLOCK,
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
    p.jump.ground_classes = ClassMask::BLOCK;
    p.jump.ground_also = vec![GroundAlso::PlatformFlag];
    p.jump.water_air = WaterAir::None;
    p.gravity.reapply = GravityReapply::CreateOnly;
    p.gravity.block_landing_zeroes = false;
    p.death.order = vec![
        DeathStep::Sound,
        DeathStep::BloodEmitter,
        DeathStep::DestroyAllKids,
        DeathStep::GameOver,
        DeathStep::DeathsPlusOne,
        DeathStep::SaveCounters,
    ];
    p.death.select_room_reset = false;
    p.save.position = SavePosition::XyBase256Bytes;
    p.misc.fade_in = false;
    p
}

/// Family C, reference game keep-celebrating-for-xia (kid/C.md): the `grav` sign flip, the
/// vine flags computed before the jump code, a suicide press in the Step, edge and killer tests in
/// the End Step, water 3, the flipped platform rule. Slide blocks, slip and slopes are not in the
/// reference game's rooms (tier 3).
fn c() -> PhysicsProfile {
    let mut p = a1();
    p.ice.clamp = true; // slip is clamped to the run speed
    p.events.create = vec![
        common(C::CreateDefaults),
        common(C::CreateBow),
        common(C::FirstPlaySave),
    ];
    p.events.step = vec![
        KidAction::C(CAction::VineProbe),
        common(C::Walk),
        common(C::AirSprite),
        common(C::FallCap),
        common(C::Shoot),
        common(C::Jump),
        common(C::Release),
        KidAction::C(CAction::SuicidePress),
        common(C::Vines),
    ];
    p.events.end_step = vec![KidAction::B(BAction::KillerCheck)];
    p.events.boundary = BoundaryModel::EndStepOrigin;
    p.events.alarms = vec![];
    // Key C (client action `extra1`): teleport to the start marker (kid/C.md).
    p.events.keys = vec![KidKey {
        action: Action::Extra1,
        on: KeyOn::Press,
        run: vec![KidAction::C(CAction::TeleportToStart)],
    }];
    p.events.collisions = vec![
        KidCollision {
            with: ClassMask::BLOCK,
            run: KidCollisionRun::Block(BlockRun::BlockStandard),
        },
        KidCollision {
            with: ClassMask::PLATFORM,
            run: KidCollisionRun::Platform(PlatformRun::PlatformStandard),
        },
    ];
    p.jump.ground_classes = ClassMask::BLOCK | ClassMask::WATER1;
    p.jump.ground_also = vec![GroundAlso::PlatformFlag];
    p.jump.ground_sets_djump = true;
    p.jump.water_air = WaterAir::Water3Keeps;
    p.fall_cap.rule = FallCapRule::ClampAbs;
    p.gravity.reapply = GravityReapply::CreateOnly;
    p.gravity.block_landing_zeroes = false;
    p.platform.take = PlatformTake::YspeedFlipped;
    p.vine.model = VineModel::DistanceSymEarly;
    p.flip.model = FlipKind::GravSign;
    p.death.order = vec![
        DeathStep::Sound,
        DeathStep::BloodEmitter,
        DeathStep::DestroyKid,
        DeathStep::GameOver,
        DeathStep::DeathsPlusOne,
        DeathStep::SaveCounters,
    ];
    p.death.select_room_reset = false;
    p.save.position = SavePosition::XyBase100Bytes;
    p.misc.fade_in = false;
    p
}

/// Family B, reference game kaenbyou (kid/B.md): the `yflag` flip, `cur_jumps` jumps, the End
/// Step slope code that also turns gravity on, polling killers and Outside Room kills. eat-the-lemon
/// overrides `jump.blocked_by_pause_press` and has no Alarm 0.
fn b() -> PhysicsProfile {
    let mut p = a1();
    p.numbers.max_fall_speed = Real::ZERO;
    p.numbers.air_wall_probe = r(2.0);

    p.events.create = vec![common(C::CreateDefaults), common(C::FirstPlaySave)];
    p.events.room_start = vec![
        KidAction::B(BAction::EnsureBow),
        KidAction::B(BAction::SetViewAngle),
    ];
    p.events.step = vec![
        common(C::StopIfFrozen2),
        KidAction::B(BAction::SyncYflag),
        common(C::Walk),
        common(C::AirSprite),
        common(C::FallCap),
        common(C::Shoot),
        common(C::Jump),
        common(C::Release),
        common(C::Vines),
        KidAction::B(BAction::KillerCheck),
        KidAction::B(BAction::EnsureBow),
    ];
    p.events.end_step = vec![KidAction::B(BAction::SlopeB)];
    p.events.boundary = BoundaryModel::OutsideRoomKill;
    // Alarm 0 never runs with jump cancelling on (kid/B.md Alarm 0): listed for the polling
    // variant, which no shipped game uses.
    p.events.alarms = vec![KidAlarm {
        n: 0,
        run: vec![KidAction::B(BAction::CancelPoll)],
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
    ];

    p.jump.ground_classes = ClassMask::BLOCK | ClassMask::SOLIDBLOCK | ClassMask::SLOPE_WALL;
    p.jump.ground_also = vec![GroundAlso::PlatformFlag];
    p.jump.ground_sets_djump = false;
    p.jump.air = AirJump::CurJumps;
    p.jump.water_air = WaterAir::WaterUnlimited;
    p.jump.max_jumps = 2;
    p.jump.inf_jump = false;
    p.jump.blocked_by_pause_press = true;
    p.jump.cancel = JumpCancel::Edge;
    p.fall_cap.rule = FallCapRule::ClampAgainstGravity;
    p.gravity.reapply = GravityReapply::EndStepSlope;
    p.gravity.block_landing_zeroes = true;
    p.platform.take = PlatformTake::DeltaY;
    p.platform.reversed_above = r(15.0);
    p.platform.reversed_snap = r(24.0);
    p.flip.model = FlipKind::Yflag;
    p.death.order = vec![
        DeathStep::DeathSound,
        DeathStep::BloodEmitter,
        DeathStep::GameOver,
        DeathStep::DestroyAllKids,
        DeathStep::DeathsPlusOne,
        DeathStep::SaveCounters,
    ];
    p.death.first_kid = true;
    p.bullet.offset = [r(2.0), r(2.0)];
    p.save.position = SavePosition::XyPrevious;
    p.save.first_kid = true;
    p.misc.fade_in = false;
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
        KidAction::Flip(FlipAction::FlipSwap),
        KidAction::A1(A1Action::PlatformGravityZero),
        KidAction::A1(A1Action::FadeIn),
    ];
    p.events.boundary = BoundaryModel::IntersectOrigin;
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
    p.ice.model = IceModel::Slip;
    p.flip.model = FlipKind::SwapObject;
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
