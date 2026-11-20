//! WP5c.4c (#300): slope runs and actions on hand-built worlds with rectangular slopes (the logic
//! reads only overlaps, so a rectangle stands in for the precise mask). The kid's box is x-5..x+5,
//! y-12..y+8 (origin (5, 12), 11 x 21).

use iwbtg_sim::def::physics::{BlockSlopeStep, Downhill, MirrorContact};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, GameDef, KidFamily, KindDef, KindId, PhysicsProfile, RoomDef};
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::actions::slope::SlopeAction;
use iwbtg_sim::kid::collisions::slope::SlopeRun;
use iwbtg_sim::kid::{self, KidAction, KidCollisionRun, KidState, presets};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, KindState, SpawnInit, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

struct Kinds {
    kid: KindId,
    slope: KindId,
    mirror: KindId,
    floor: KindId,
}

fn game(tweak: impl FnOnce(&mut PhysicsProfile)) -> (GameDef, Kinds) {
    let mut b = GameDefBuilder::new("wp5c-slope");
    let mask = b.sprite_rect((11, 21), (5, 12));
    let tile = b.sprite_rect((32, 32), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(mask);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let solid = |name: &str, classes: ClassMask, b: &mut GameDefBuilder| {
        let mut k = KindDef::new(name, Builtin::Inert);
        k.sprite = Some(tile);
        k.classes = classes;
        k.solid = true;
        b.kind(k)
    };
    let slope = solid("slope", ClassMask::SLOPE, &mut b);
    let mirror = solid("mirror", ClassMask::SLOPE_MIRROR, &mut b);
    let floor = solid("floor", ClassMask::BLOCK, &mut b);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut p = presets::defaults(KidFamily::A1);
    tweak(&mut p);
    b.physics(p);
    (
        b.build().expect("game builds"),
        Kinds {
            kid,
            slope,
            mirror,
            floor,
        },
    )
}

fn with_kid<T>(tweak: impl FnOnce(&mut PhysicsProfile), f: impl FnOnce(&mut Ctx, InstId, &Kinds) -> T) -> T {
    let (def, k) = game(tweak);
    let rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("rules");
    let mut w = World::new(def.start_room, 1);
    let scripts = ScriptRuntime::default();
    let mut ctx = Ctx {
        def: &def,
        rules: &rules,
        w: &mut w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    let kid = ctx
        .spawn(k.kid, r(100.0), r(100.0), SpawnInit::default())
        .expect("kid");
    kid::run_action(&mut ctx, kid, &KidAction::Common(CommonAction::CreateDefaults)).expect("create");
    f(&mut ctx, kid, &k)
}

fn put(ctx: &mut Ctx, kind: KindId, x: f64, y: f64) -> InstId {
    ctx.spawn(kind, r(x), r(y), SpawnInit::default()).expect("spawn")
}

fn speeds(ctx: &mut Ctx, id: InstId, hs: f64, vs: f64) {
    let m = &mut ctx.w.get_mut(id).unwrap().m;
    m.set_hspeed(r(hs)).unwrap();
    m.set_vspeed(r(vs)).unwrap();
}

fn at(ctx: &Ctx, id: InstId) -> (f64, f64, f64) {
    let m = &ctx.w.get(id).unwrap().m;
    (m.x().get(), m.y().get(), m.hspeed().get())
}

fn kstate<'a>(ctx: &'a mut Ctx, id: InstId) -> &'a mut KidState {
    match &mut ctx.w.get_mut(id).unwrap().state {
        KindState::Kid(s) => s,
        _ => unreachable!(),
    }
}

fn input(ctx: &mut Ctx, held: &[Action], pressed: &[Action]) {
    let bits = |l: &[Action]| l.iter().fold(0u32, |a, x| a | x.bit());
    ctx.w.input.latch(InputFrame {
        held: bits(held),
        pressed: bits(pressed),
        released: 0,
    });
}

fn collide(ctx: &mut Ctx, kid: InstId, other: InstId, run: SlopeRun) {
    kid::run_collision(ctx, kid, other, &KidCollisionRun::Slope(run)).expect("run");
}

fn rainbow(p: &mut PhysicsProfile) {
    p.slope.block_step = BlockSlopeStep::FreeSpot;
}

#[test]
fn free_spot_steps_up_after_the_contact_when_the_spot_is_free() {
    // A low slope (y 106) 5 px ahead: the contact stops at x 104, then 3 right and 3 up.
    with_kid(rainbow, |ctx, kid, k| {
        let s = put(ctx, k.slope, 110.0, 106.0);
        speeds(ctx, kid, 6.0, 0.0);
        collide(ctx, kid, s, SlopeRun::BlockSlopeStep);
        assert_eq!(at(ctx, kid), (107.0, 97.0, 0.0));
    });
}

#[test]
fn free_spot_does_not_step_into_a_taken_spot() {
    with_kid(rainbow, |ctx, kid, k| {
        let s = put(ctx, k.slope, 110.0, 90.0);
        speeds(ctx, kid, 6.0, 0.0);
        collide(ctx, kid, s, SlopeRun::BlockSlopeStep);
        assert_eq!(at(ctx, kid), (104.0, 100.0, 0.0), "contact only");
    });
}

#[test]
fn free_spot_uses_the_mirrored_slope_on_the_left() {
    with_kid(rainbow, |ctx, kid, k| {
        let s = put(ctx, k.mirror, 58.0, 106.0);
        speeds(ctx, kid, -6.0, 0.0);
        collide(ctx, kid, s, SlopeRun::BlockSlopeStep);
        assert_eq!(
            at(ctx, kid),
            (92.0, 97.0, 0.0),
            "contact at 95 (box left 90), then 3 left and up"
        );
    });
}

#[test]
fn replace_steps_up_instead_of_the_contact() {
    with_kid(
        |p| p.slope.block_step = BlockSlopeStep::Replace,
        |ctx, kid, k| {
            let s = put(ctx, k.slope, 110.0, 90.0);
            speeds(ctx, kid, 6.0, 0.0);
            collide(ctx, kid, s, SlopeRun::BlockSlopeStep);
            assert_eq!(
                at(ctx, kid),
                (103.0, 97.0, 0.0),
                "no free-spot test, no contact move"
            );
        },
    );
}

#[test]
fn the_mirror_row_right_hit_contacts_by_the_games_rule() {
    // A right hit on a `slope_mirror` (no climb: the climb looks for a `slope`): rori contacts
    // toward 180 (a bug the original keeps), fairy-tale toward 0, the others by the block rule.
    for (rule, want_x) in [
        (MirrorContact::Normal, 104.0),
        (MirrorContact::Left, 94.0),
        (MirrorContact::Right, 104.0),
    ] {
        with_kid(
            |p| {
                p.slope.block_step = BlockSlopeStep::Replace;
                p.slope.mirror_contact = rule;
            },
            |ctx, kid, k| {
                let s = put(ctx, k.mirror, 110.0, 90.0);
                speeds(ctx, kid, 6.0, 0.0);
                collide(ctx, kid, s, SlopeRun::BlockSlopeStep);
                assert_eq!(at(ctx, kid).0, want_x, "{rule:?}");
            },
        );
    }
    // Against a `slope` the rule does not apply.
    with_kid(
        |p| {
            p.slope.block_step = BlockSlopeStep::FreeSpot;
            p.slope.mirror_contact = MirrorContact::Left;
        },
        |ctx, kid, k| {
            let s = put(ctx, k.slope, 110.0, 90.0);
            speeds(ctx, kid, 6.0, 0.0);
            collide(ctx, kid, s, SlopeRun::BlockSlopeStep);
            assert_eq!(at(ctx, kid).0, 104.0);
        },
    );
}

#[test]
fn por_rise_climbs_by_the_first_free_offset() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            let s = put(ctx, k.slope, 110.0, 106.0);
            speeds(ctx, kid, 6.0, 0.0);
            collide(ctx, kid, s, SlopeRun::SlopePorRise);
            // Offsets 0..2 still overlap (box bottom 108, 107, 106), 3 is free: rise 3.
            assert_eq!(at(ctx, kid), (103.0, 97.0, 0.0));
        },
    );
}

#[test]
fn por_rise_is_zero_when_a_slope_still_overlaps_nine_up() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            let s = put(ctx, k.slope, 110.0, 80.0);
            speeds(ctx, kid, 6.0, 0.0);
            collide(ctx, kid, s, SlopeRun::SlopePorRise);
            // Rise 0, but x still moves 3 toward hspeed.
            assert_eq!(at(ctx, kid), (103.0, 100.0, 0.0));
        },
    );
}

#[test]
fn yane_h_climbs_only_when_ground_is_under_the_kid_too() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            let s = put(ctx, k.slope, 110.0, 80.0);
            put(ctx, k.floor, 90.0, 109.0);
            speeds(ctx, kid, 6.0, 2.0);
            collide(ctx, kid, s, SlopeRun::SlopeYane);
            assert_eq!(at(ctx, kid), (103.0, 97.0, 0.0));
        },
    );
    with_kid(
        |_| {},
        |ctx, kid, k| {
            let s = put(ctx, k.slope, 110.0, 80.0);
            speeds(ctx, kid, 6.0, 2.0);
            collide(ctx, kid, s, SlopeRun::SlopeYane);
            assert_eq!(at(ctx, kid).0, 100.0, "no ground below: no climb");
        },
    );
}

#[test]
fn yane_m_mirrors_the_climb_and_h_has_no_leftward_move() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            let s = put(ctx, k.mirror, 58.0, 80.0);
            put(ctx, k.floor, 90.0, 109.0);
            speeds(ctx, kid, -6.0, 2.0);
            collide(ctx, kid, s, SlopeRun::SlopeYane);
            assert_eq!(at(ctx, kid), (97.0, 97.0, 0.0));
        },
    );
    with_kid(
        |_| {},
        |ctx, kid, k| {
            let s = put(ctx, k.slope, 58.0, 80.0);
            put(ctx, k.floor, 90.0, 109.0);
            speeds(ctx, kid, -6.0, 2.0);
            collide(ctx, kid, s, SlopeRun::SlopeYane);
            assert_eq!(at(ctx, kid).0, 100.0, "H with hspeed < 0: no contact move");
        },
    );
}

#[test]
fn yane_m_downhill_hit_contacts_toward_180() {
    // kid/A4.md: yaneM with hspeed > 0 contacts toward direction 180 (left), although the kid
    // moves right; nothing blocks on the left, so he moves the full abs(hspeed) left.
    with_kid(
        |_| {},
        |ctx, kid, k| {
            let s = put(ctx, k.mirror, 106.0, 80.0);
            speeds(ctx, kid, 6.0, 0.0);
            collide(ctx, kid, s, SlopeRun::SlopeYane);
            assert_eq!(at(ctx, kid), (94.0, 100.0, 0.0));
        },
    );
}

#[test]
fn yane_walks_down_its_own_slope_by_the_steps_h() {
    // kid/A4.md step 4: H with h < 0 and a yaneH 1 px below moves 3 left and 3 down; M is the
    // mirror (h > 0). The kid's bottom row is y 108, so a slope at y 109 is 1 px below.
    let cases = [
        // (mirror slope, h, expected)
        (false, -1, (97.0, 103.0, 0.0)),
        (false, 1, (100.0, 100.0, 0.0)),
        (false, 0, (100.0, 100.0, 0.0)),
        (true, 1, (103.0, 103.0, 0.0)),
        (true, -1, (100.0, 100.0, 0.0)),
    ];
    for (mirror, h, want) in cases {
        with_kid(
            |_| {},
            |ctx, kid, k| {
                let s = put(ctx, if mirror { k.mirror } else { k.slope }, 90.0, 109.0);
                kstate(ctx, kid).ext.walk_h = h;
                collide(ctx, kid, s, SlopeRun::SlopeYane);
                assert_eq!(at(ctx, kid), want, "mirror {mirror}, h {h}");
            },
        );
    }
    // Only a slope of the event's own object counts: the H event with a yaneM below does nothing.
    with_kid(
        |_| {},
        |ctx, kid, k| {
            let s = put(ctx, k.slope, 300.0, 300.0);
            put(ctx, k.mirror, 90.0, 109.0);
            kstate(ctx, kid).ext.walk_h = -1;
            collide(ctx, kid, s, SlopeRun::SlopeYane);
            assert_eq!(at(ctx, kid), (100.0, 100.0, 0.0));
        },
    );
}

#[test]
fn walk_keeps_h_for_the_slope_event() {
    for (held, want) in [(Action::Left, -1), (Action::Right, 1)] {
        with_kid(
            |_| {},
            |ctx, kid, _| {
                input(ctx, &[held], &[]);
                kid::run_action(ctx, kid, &KidAction::Common(CommonAction::Walk)).unwrap();
                assert_eq!(kstate(ctx, kid).ext.walk_h, want);
            },
        );
    }
}

#[test]
fn a_slope_in_ground_classes_gives_the_single_jump() {
    // Poronkusema's playerJump also counts a nanameblock 1 px below (kid/A1.md).
    for (classes, want) in [
        (ClassMask(ClassMask::BLOCK.0 | ClassMask::SLOPE.0), -8.5),
        (ClassMask::BLOCK, 0.0),
    ] {
        with_kid(
            |p| p.jump.ground_classes = classes,
            |ctx, kid, k| {
                put(ctx, k.slope, 90.0, 109.0);
                kstate(ctx, kid).djump = false;
                input(ctx, &[Action::Jump], &[Action::Jump]);
                kid::run_action(ctx, kid, &KidAction::Common(CommonAction::Jump)).unwrap();
                assert_eq!(ctx.w.get(kid).unwrap().m.vspeed().get(), want);
            },
        );
    }
}

#[test]
fn slope_jump_forces_a_single_jump_within_the_probe() {
    with_kid(
        |p| p.slope.jump_probe = r(2.0),
        |ctx, kid, k| {
            put(ctx, k.slope, 90.0, 110.0);
            kstate(ctx, kid).djump = false;
            input(ctx, &[Action::Jump], &[Action::Jump]);
            kid::run_action(ctx, kid, &KidAction::Slope(SlopeAction::SlopeJump)).unwrap();
            assert_eq!(ctx.w.get(kid).unwrap().m.vspeed().get(), -8.5);
            assert!(kstate(ctx, kid).djump);
        },
    );
    with_kid(
        |p| p.slope.jump_probe = r(2.0),
        |ctx, kid, k| {
            put(ctx, k.slope, 90.0, 112.0);
            input(ctx, &[Action::Jump], &[Action::Jump]);
            kid::run_action(ctx, kid, &KidAction::Slope(SlopeAction::SlopeJump)).unwrap();
            assert_eq!(ctx.w.get(kid).unwrap().m.vspeed().get(), 0.0, "3 px away");
        },
    );
}

fn snap(ctx: &mut Ctx, kid: InstId) {
    kid::run_action(ctx, kid, &KidAction::Slope(SlopeAction::DownhillSnap)).unwrap();
}

#[test]
fn rainbow_snap_follows_a_slope_down_to_contact() {
    with_kid(
        |p| p.slope.downhill = Downhill::Rainbow,
        |ctx, kid, k| {
            put(ctx, k.slope, 90.0, 112.0);
            snap(ctx, kid);
            assert_eq!(at(ctx, kid).1, 103.0);
        },
    );
}

#[test]
fn por_snap_needs_the_slope_last_step_and_remembers_it() {
    with_kid(
        |p| p.slope.downhill = Downhill::Por,
        |ctx, kid, k| {
            put(ctx, k.slope, 90.0, 112.0);
            snap(ctx, kid);
            assert_eq!(at(ctx, kid).1, 100.0, "not on a slope last step: no snap");
            assert!(!kstate(ctx, kid).on_slope_last);
            kstate(ctx, kid).on_slope_last = true;
            snap(ctx, kid);
            assert_eq!(at(ctx, kid).1, 103.0);
            assert!(kstate(ctx, kid).on_slope_last, "a slope is 1 px below now");
        },
    );
}

#[test]
fn fairy_tale_snap_needs_no_block_below() {
    with_kid(
        |p| p.slope.downhill = Downhill::FairyTale,
        |ctx, kid, k| {
            put(ctx, k.slope, 90.0, 110.0);
            snap(ctx, kid);
            assert_eq!(at(ctx, kid).1, 101.0);
        },
    );
    with_kid(
        |p| p.slope.downhill = Downhill::FairyTale,
        |ctx, kid, k| {
            put(ctx, k.slope, 90.0, 110.0);
            put(ctx, k.floor, 90.0, 109.0);
            snap(ctx, kid);
            assert_eq!(at(ctx, kid).1, 100.0);
        },
    );
}

#[test]
fn keep_hspeed_leaves_hspeed_on_a_downhill_slope() {
    for (keep, want) in [(true, 0.5), (false, -3.0)] {
        with_kid(
            |p| p.slope.keep_hspeed = keep,
            |ctx, kid, k| {
                put(ctx, k.slope, 90.0, 109.0);
                speeds(ctx, kid, 0.5, 0.0);
                input(ctx, &[Action::Left], &[]);
                kid::run_action(ctx, kid, &KidAction::Common(CommonAction::Walk)).unwrap();
                assert_eq!(at(ctx, kid).2, want, "keep_hspeed {keep}");
            },
        );
    }
}

#[test]
fn the_slope_mirror_class_has_its_name() {
    assert!(
        ClassMask::NAMED
            .iter()
            .any(|(c, n)| *c == ClassMask::SLOPE_MIRROR && *n == "slope_mirror")
    );
}
