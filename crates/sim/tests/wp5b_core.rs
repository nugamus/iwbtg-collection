//! WP5b.1 (#259): family B's core numbers and jump model on hand-built worlds (kid/B.md Create,
//! playerMove step 8, playerJump, Alarm 0, Step).
//!
//! The profile is built by hand; kaenbyou's and eat-the-lemon's profiles are WP5b.6 (#264).

use iwbtg_sim::def::physics::{
    AirJump, FallCapRule, FlipKind, GravityReapply, GroundAlso, JumpCancel, WaterAir,
};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, GameDef, KindDef, KindId, PhysicsProfile, RestartKind, RetryKidSteps, RoomDef, RoomId,
};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::actions::b::BAction;
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::api::{self, KidField};
use iwbtg_sim::kid::{self, KidAction, KidState};
use iwbtg_sim::num::Real;
use iwbtg_sim::restart::run_restart;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::save::SaveSlot;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, KindState, SpawnInit, Value, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

struct Kinds {
    kid: KindId,
    block: KindId,
    water: KindId,
}

fn b_profile() -> PhysicsProfile {
    let mut p = PhysicsProfile::default();
    p.numbers.jumps = vec![r(8.5), r(7.0)];
    p.numbers.gravity = r(0.4);
    p.numbers.run_speed = r(3.0);
    p.numbers.max_vspeed = r(9.0);
    p.numbers.release_mult = r(0.45);
    p.numbers.sprite_threshold = r(0.05);
    p.jump.ground_classes = ClassMask::BLOCK | ClassMask::SLOPE_WALL;
    p.jump.ground_also = vec![GroundAlso::PlatformFlag];
    p.jump.air = AirJump::CurJumps;
    p.jump.water_air = WaterAir::WaterUnlimited;
    p.jump.max_jumps = 2;
    p.jump.inf_jump = false;
    p.jump.blocked_by_pause_press = true;
    p.jump.cancel = JumpCancel::Edge;
    p.fall_cap.rule = FallCapRule::ClampAgainstGravity;
    p.gravity.reapply = GravityReapply::EndStepSlope;
    p.flip.model = FlipKind::Yflag;
    p
}

fn game(tweak: impl FnOnce(&mut PhysicsProfile)) -> (GameDef, Kinds) {
    let mut b = GameDefBuilder::new("wp5b-core");
    let kid_sprite = b.sprite_rect((11, 21), (5, 12));
    let tile = b.sprite_rect((32, 32), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(kid_sprite);
    k.classes = ClassMask::KID;
    k.persistent = true;
    let kid = b.kind(k);
    b.engine_kind(iwbtg_sim::def::EngineKind::Kid, kid);
    let mut blk = KindDef::new("block", Builtin::Inert);
    blk.sprite = Some(tile);
    blk.classes = ClassMask::BLOCK;
    blk.solid = true;
    let block = b.kind(blk);
    let mut w = KindDef::new("water", Builtin::Inert);
    w.sprite = Some(tile);
    w.classes = ClassMask::WATER1;
    let water = b.kind(w);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut p = b_profile();
    tweak(&mut p);
    b.physics(p);
    (b.build().expect("game builds"), Kinds { kid, block, water })
}

fn with_kid<T>(tweak: impl FnOnce(&mut PhysicsProfile), f: impl FnOnce(&mut Ctx, InstId, &Kinds) -> T) -> T {
    let (def, k) = game(tweak);
    let rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("rules");
    let mut w = World::new(def.start_room, 1);
    w.room_speed = r(50.0);
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
    f(&mut ctx, kid, &k)
}

/// A kid after `create_defaults`.
fn with_created_kid<T>(
    tweak: impl FnOnce(&mut PhysicsProfile),
    f: impl FnOnce(&mut Ctx, InstId, &Kinds) -> T,
) -> T {
    with_kid(tweak, |ctx, kid, k| {
        run(ctx, kid, CommonAction::CreateDefaults);
        f(ctx, kid, k)
    })
}

fn run(ctx: &mut Ctx, kid: InstId, a: CommonAction) {
    kid::run_action(ctx, kid, &KidAction::Common(a)).expect("action");
}

fn input(ctx: &mut Ctx, held: &[Action], pressed: &[Action], released: &[Action]) {
    let bits = |l: &[Action]| l.iter().fold(0u32, |a, x| a | x.bit());
    ctx.w.input.latch(InputFrame {
        held: bits(held),
        pressed: bits(pressed),
        released: bits(released),
    });
}

fn spawn(ctx: &mut Ctx, kind: KindId, x: f64, y: f64) -> InstId {
    ctx.spawn(kind, r(x), r(y), SpawnInit::default()).expect("spawn")
}

fn set_v(ctx: &mut Ctx, id: InstId, vs: f64) {
    ctx.w.get_mut(id).unwrap().m.set_vspeed(r(vs)).unwrap();
}

fn vs(ctx: &Ctx, id: InstId) -> f64 {
    ctx.w.get(id).unwrap().m.vspeed().get()
}

fn kstate<'a>(ctx: &'a mut Ctx, id: InstId) -> &'a mut KidState {
    match &mut ctx.w.get_mut(id).unwrap().state {
        KindState::Kid(s) => s,
        _ => unreachable!(),
    }
}

fn press_jump(ctx: &mut Ctx) {
    input(ctx, &[Action::Jump], &[Action::Jump], &[]);
}

// ---- Create ----

#[test]
fn create_leaves_gravity_at_zero_and_sets_the_b_numbers() {
    with_created_kid(
        |_| {},
        |ctx, kid, _| {
            let i = ctx.w.get(kid).unwrap();
            assert_eq!(i.m.gravity(), Real::ZERO, "gravity stays 0 until the End Step");
            assert_eq!(i.img.yscale(), Real::ONE);
            let s = kstate(ctx, kid).clone();
            assert_eq!((s.cur_jumps, s.b.max_jumps, s.b.inf_jump), (1, 2, false));
            assert_eq!((s.yflag, s.flipped), (1, false));
            assert_eq!(s.jump_speeds, vec![r(8.5), r(7.0)]);
        },
    );
}

#[test]
fn create_while_reversed_flips_the_kid_and_shifts_it_four_up() {
    with_kid(
        |_| {},
        |ctx, kid, _| {
            ctx.w.engine.kid.b.reverse = true;
            run(ctx, kid, CommonAction::CreateDefaults);
            let s = kstate(ctx, kid).clone();
            assert_eq!((s.yflag, s.cur_jumps), (-1, 1));
            let i = ctx.w.get(kid).unwrap();
            assert_eq!(i.m.y(), r(96.0));
            assert_eq!(i.img.yscale(), r(-1.0));
        },
    );
}

#[test]
fn create_with_gravity_reapplied_each_step_sets_gravity() {
    with_created_kid(
        |p| p.gravity.reapply = GravityReapply::Step,
        |ctx, kid, _| {
            assert_eq!(ctx.w.get(kid).unwrap().m.gravity(), r(0.4));
        },
    );
}

// ---- The cur_jumps jump ----

#[test]
fn the_first_jump_from_a_block_sets_the_counter_to_two() {
    with_created_kid(
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.block, 80.0, 109.0);
            press_jump(ctx);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), -8.5);
            assert_eq!(kstate(ctx, kid).cur_jumps, 2);
        },
    );
}

#[test]
fn walking_off_a_ledge_spends_the_first_jump() {
    with_created_kid(
        |_| {},
        |ctx, kid, _| {
            // In the air with curJumps 1 (never jumped): the press is the air jump, jump[2].
            press_jump(ctx);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), -7.0);
            assert_eq!(kstate(ctx, kid).cur_jumps, 3);
            // The next air press does nothing: curJumps 3 > maxJumps and no water.
            set_v(ctx, kid, 2.0);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), 2.0);
            assert_eq!(kstate(ctx, kid).cur_jumps, 3);
        },
    );
}

#[test]
fn a_jump_then_an_air_jump_then_nothing() {
    with_created_kid(
        |_| {},
        |ctx, kid, k| {
            let b = spawn(ctx, k.block, 80.0, 109.0);
            press_jump(ctx);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!((vs(ctx, kid), kstate(ctx, kid).cur_jumps), (-8.5, 2));
            ctx.w.get_mut(b).unwrap().alive = false; // airborne now
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(
                (vs(ctx, kid), kstate(ctx, kid).cur_jumps),
                (-7.0, 3),
                "jump[curJumps] = jump[2]"
            );
            set_v(ctx, kid, 1.0);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), 1.0);
        },
    );
}

#[test]
fn water_gives_unlimited_jumps_at_the_last_speed() {
    with_created_kid(
        |_| {},
        |ctx, kid, k| {
            // The first press in water spends the first jump and gives jump[2] (kaenbyou famb-water).
            spawn(ctx, k.water, 80.0, 109.0);
            press_jump(ctx);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!((vs(ctx, kid), kstate(ctx, kid).cur_jumps), (-7.0, 3));
            // Later presses with water 1 px below: -7 again, the counter stays 3.
            set_v(ctx, kid, 3.0);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!((vs(ctx, kid), kstate(ctx, kid).cur_jumps), (-7.0, 3));
        },
    );
    // Water is not ground: a kid in water with curJumps 3 and no water below does nothing.
    with_created_kid(
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.water, 80.0, 140.0);
            kstate(ctx, kid).cur_jumps = 3;
            press_jump(ctx);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), 0.0);
        },
    );
}

#[test]
fn inf_jump_always_jumps() {
    with_created_kid(
        |_| {},
        |ctx, kid, _| {
            kstate(ctx, kid).cur_jumps = 3;
            kstate(ctx, kid).b.inf_jump = true;
            press_jump(ctx);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!((vs(ctx, kid), kstate(ctx, kid).cur_jumps), (-7.0, 3));
        },
    );
}

#[test]
fn on_platform_is_ground_for_the_first_jump() {
    with_created_kid(
        |_| {},
        |ctx, kid, _| {
            kstate(ctx, kid).on_platform = true;
            press_jump(ctx);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!((vs(ctx, kid), kstate(ctx, kid).cur_jumps), (-8.5, 2));
        },
    );
}

#[test]
fn reversed_gravity_jumps_the_other_way_off_the_ceiling_side() {
    with_kid(
        |_| {},
        |ctx, kid, k| {
            ctx.w.engine.kid.b.reverse = true;
            run(ctx, kid, CommonAction::CreateDefaults); // y 96, yflag -1
            // "Below" is above: the block's bottom row touches the box's top row minus 1.
            spawn(ctx, k.block, 80.0, 60.0); // rows 60..91; the mirrored box reaches up to y - 8
            press_jump(ctx);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), 8.5, "-yflag * 8.5");
            assert_eq!(kstate(ctx, kid).cur_jumps, 2);
        },
    );
}

#[test]
fn a_pause_press_blocks_the_jump_when_the_game_says_so() {
    with_created_kid(
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.block, 80.0, 109.0);
            input(ctx, &[Action::Jump], &[Action::Jump, Action::Pause], &[]);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), 0.0);
        },
    );
    with_created_kid(
        |p| p.jump.blocked_by_pause_press = false,
        |ctx, kid, k| {
            spawn(ctx, k.block, 80.0, 109.0);
            input(ctx, &[Action::Jump], &[Action::Jump, Action::Pause], &[]);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), -8.5, "eat-the-lemon has no pause block");
        },
    );
}

#[test]
fn frozen_and_missing_presses_do_nothing() {
    with_created_kid(
        |_| {},
        |ctx, kid, k| {
            spawn(ctx, k.block, 80.0, 109.0);
            input(ctx, &[Action::Jump], &[], &[]);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), 0.0);
            kstate(ctx, kid).frozen = true;
            press_jump(ctx);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(vs(ctx, kid), 0.0);
        },
    );
}

// ---- Jump cancel ----

#[test]
fn the_edge_cut_works_against_gravity_both_ways() {
    with_created_kid(
        |_| {},
        |ctx, kid, _| {
            set_v(ctx, kid, -8.0);
            input(ctx, &[], &[], &[Action::Jump]);
            run(ctx, kid, CommonAction::Release);
            assert_eq!(vs(ctx, kid), (r(-8.0) * r(0.45)).get());
            kstate(ctx, kid).yflag = -1;
            set_v(ctx, kid, 8.0);
            run(ctx, kid, CommonAction::Release);
            assert_eq!(
                vs(ctx, kid),
                (r(8.0) * r(0.45)).get(),
                "reversed: against gravity is down"
            );
        },
    );
}

#[test]
fn poll_alarm_arms_on_the_first_jump_and_cuts_when_the_key_is_up() {
    with_created_kid(
        |p| p.jump.cancel = JumpCancel::PollAlarm,
        |ctx, kid, k| {
            spawn(ctx, k.block, 80.0, 109.0);
            press_jump(ctx);
            run(ctx, kid, CommonAction::Jump);
            assert_eq!(ctx.w.get(kid).unwrap().alarms[0], r(1.0), "armed for one tick");
            // The release edge does nothing in this mode.
            input(ctx, &[], &[], &[Action::Jump]);
            run(ctx, kid, CommonAction::Release);
            assert_eq!(vs(ctx, kid), -8.5);
            // Held: re-armed, no cut.
            input(ctx, &[Action::Jump], &[], &[]);
            kid::run_action(ctx, kid, &KidAction::B(BAction::CancelPoll)).unwrap();
            assert_eq!((vs(ctx, kid), ctx.w.get(kid).unwrap().alarms[0]), (-8.5, r(1.0)));
            // Up and rising: cut once and the alarm stops.
            input(ctx, &[], &[], &[]);
            kid::run_action(ctx, kid, &KidAction::B(BAction::CancelPoll)).unwrap();
            assert_eq!(vs(ctx, kid), (r(-8.5) * r(0.45)).get());
            assert_eq!(ctx.w.get(kid).unwrap().alarms[0], Real::ZERO);
        },
    );
}

// ---- Fall cap ----

#[test]
fn the_fall_cap_clamps_only_the_fall() {
    with_created_kid(
        |_| {},
        |ctx, kid, _| {
            for (before, after) in [(12.0, 9.0), (-12.0, -12.0), (8.0, 8.0)] {
                set_v(ctx, kid, before);
                run(ctx, kid, CommonAction::FallCap);
                assert_eq!(vs(ctx, kid), after);
            }
            kstate(ctx, kid).yflag = -1;
            for (before, after) in [(-12.0, -9.0), (12.0, 12.0)] {
                set_v(ctx, kid, before);
                run(ctx, kid, CommonAction::FallCap);
                assert_eq!(vs(ctx, kid), after, "reversed: vspeed * yflag decides");
            }
        },
    );
}

// ---- Frozen2 stops the whole Step ----

#[test]
fn frozen2_zeroes_gravity_and_speed_and_skips_the_rest_of_the_step() {
    let steps = vec![
        KidAction::Common(CommonAction::StopIfFrozen2),
        KidAction::Common(CommonAction::Walk),
    ];
    with_created_kid(
        |p| p.events.step = steps.clone(),
        |ctx, kid, _| {
            ctx.w.get_mut(kid).unwrap().m.set_gravity(r(0.4)).unwrap();
            ctx.w.get_mut(kid).unwrap().m.set_hspeed(r(2.0)).unwrap();
            ctx.w.engine.kid.frozen2 = true;
            input(ctx, &[Action::Right], &[Action::Right], &[]);
            kid::on_event(ctx, kid, &EventKind::Step).unwrap();
            let i = ctx.w.get(kid).unwrap();
            assert_eq!(
                (i.m.gravity(), i.m.speed(), i.img.speed()),
                (Real::ZERO, Real::ZERO, Real::ZERO)
            );
            assert_eq!(i.m.hspeed(), Real::ZERO, "speed 0 also clears hspeed");
            // Not frozen: the list goes on to walk.
            ctx.w.engine.kid.frozen2 = false;
            kid::on_event(ctx, kid, &EventKind::Step).unwrap();
            assert_eq!(ctx.w.get(kid).unwrap().m.hspeed(), r(3.0));
        },
    );
}

// ---- Probe fields ----

#[test]
fn the_probe_reads_the_b_fields_and_a_one_based_jump_array() {
    with_created_kid(
        |_| {},
        |ctx, kid, _| {
            let f = |n: &str, i: Option<u32>| {
                api::read_field(
                    ctx.w,
                    kid,
                    &KidField {
                        name: n.into(),
                        index: i,
                    },
                )
            };
            assert_eq!(f("maxJumps", None), Some(Value::Real(r(2.0))));
            assert_eq!(f("infJump", None), Some(Value::Real(Real::ZERO)));
            assert_eq!(f("curJumps", None), Some(Value::Real(Real::ONE)));
            assert_eq!(f("jump", Some(1)), Some(Value::Real(r(8.5))));
            assert_eq!(f("jump", Some(2)), Some(Value::Real(r(7.0))));
        },
    );
}

// ---- #247: the stale global.reverse ----

fn slot(flipped: bool) -> SaveSlot {
    SaveSlot {
        room: RoomId(0),
        x: r(70.0),
        y: r(119.0),
        flipped,
        facing: 1,
        difficulty: 0,
        deaths: 0,
        time: 0,
        bosses: 0,
        items: 0,
        clear: false,
    }
}

#[test]
fn a_retry_creates_the_new_kid_with_the_old_reverse_and_then_loads_the_saved_one() {
    let (mut def, kid) = game(|p| p.events.create = vec![KidAction::Common(CommonAction::CreateDefaults)]);
    def.restart.kind = RestartKind::RoomGotoSaved;
    def.restart.kid_steps = RetryKidSteps {
        clear_frozen: true,
        ..RetryKidSteps::default()
    };
    let rules = RunnerRules::for_runner(def.runner, def.engine_rev).unwrap();
    let mut w = World::new(def.start_room, 1);
    w.room_speed = r(50.0);
    let scripts = ScriptRuntime::default();
    let mut ctx = Ctx {
        def: &def,
        rules: &rules,
        w: &mut w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    ctx.w.save = Some(slot(false)); // the save says: not reversed
    ctx.w.engine.kid.b.reverse = true; // but the player is reversed when R is pressed
    let old = ctx.spawn(kid.kid, r(1.0), r(2.0), SpawnInit::default()).unwrap();
    run_restart(&mut ctx, old).unwrap();
    let new = ctx
        .w
        .iter_alive()
        .find(|i| i.kind == kid.kid)
        .expect("a new kid")
        .id;
    assert_ne!(new, old);
    let s = kstate(&mut ctx, new).clone();
    assert_eq!(
        s.yflag, -1,
        "Create saw the old reverse (stale yflag until the first Step)"
    );
    assert_eq!(ctx.w.get(new).unwrap().img.yscale(), r(-1.0));
    assert!(
        !ctx.w.engine.kid.b.reverse,
        "the load then read reverse = 0 from the save"
    );
    // The loaded y replaces Create's shift.
    assert_eq!(ctx.w.get(new).unwrap().m.y(), r(119.0));
}
