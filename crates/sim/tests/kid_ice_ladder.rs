//! WP5c.4a (#297): slip blocks, showdown's temp hspeed and ladders on hand-built worlds (kid/A1.md
//! Step actions 2 and 3, End Step; kid/A3.md showdown Step action 3).

use iwbtg_sim::def::ParamValue;
use iwbtg_sim::def::physics::{IceModel, IceRules, LadderEntry};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, GameDef, KindDef, KindId, PhysicsProfile, RoomDef, SpriteId};
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::actions::ice::IceAction;
use iwbtg_sim::kid::actions::ladder::LadderAction;
use iwbtg_sim::kid::{self, KidAction, KidState};
use iwbtg_sim::num::{self, Real};
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, KindState, SpawnInit, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

struct Kinds {
    kid: KindId,
    slip: KindId,
    ladder: KindId,
}

fn game(tweak: impl FnOnce(&mut PhysicsProfile)) -> (GameDef, Kinds, SpriteId, SpriteId) {
    let mut b = GameDefBuilder::new("wp5c-ice");
    let kid_sprite = b.sprite_rect((11, 21), (5, 12));
    let tile = b.sprite_rect((32, 32), (0, 0));
    let back = b.sprite_rect((11, 21), (5, 12));
    let updown = b.sprite_rect((11, 21), (5, 12));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(kid_sprite);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let mut sl = KindDef::new("slipblock", Builtin::Block);
    sl.sprite = Some(tile);
    sl.classes = ClassMask::BLOCK | ClassMask::SLIP;
    sl.solid = true;
    let slip = b.kind(sl);
    let mut ld = KindDef::new("ladder", Builtin::Inert);
    ld.sprite = Some(tile);
    ld.classes = ClassMask::LADDER;
    let ladder = b.kind(ld);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut p = iwbtg_sim::kid::presets::defaults(iwbtg_sim::def::KidFamily::A1);
    p.sprites.back = Some(back);
    p.sprites.ladder_up_down = Some(updown);
    tweak(&mut p);
    b.physics(p);
    (
        b.build().expect("game builds"),
        Kinds { kid, slip, ladder },
        back,
        updown,
    )
}

fn with_kid<T>(
    tweak: impl FnOnce(&mut PhysicsProfile),
    f: impl FnOnce(&mut Ctx, InstId, &Kinds, SpriteId, SpriteId) -> T,
) -> T {
    let (def, k, back, updown) = game(tweak);
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
    f(&mut ctx, kid, &k, back, updown)
}

fn input(ctx: &mut Ctx, held: &[Action]) {
    let bits = |l: &[Action]| l.iter().fold(0u32, |a, x| a | x.bit());
    ctx.w.input.latch(InputFrame {
        held: bits(held),
        pressed: 0,
        released: 0,
    });
}

fn walk(ctx: &mut Ctx, kid: InstId) {
    kid::run_action(ctx, kid, &KidAction::Common(CommonAction::Walk)).expect("walk");
}

fn hs(ctx: &Ctx, id: InstId) -> f64 {
    ctx.w.get(id).unwrap().m.hspeed().get()
}

fn set_hs(ctx: &mut Ctx, id: InstId, v: f64) {
    ctx.w.get_mut(id).unwrap().m.set_hspeed(r(v)).unwrap();
}

fn kstate<'a>(ctx: &'a mut Ctx, id: InstId) -> &'a mut KidState {
    match &mut ctx.w.get_mut(id).unwrap().state {
        KindState::Kid(s) => s,
        _ => unreachable!(),
    }
}

/// A slip block with `slip` under the kid (its top row at y 109, the kid's feet at 108).
fn slip_block(ctx: &mut Ctx, k: &Kinds, slip: f64) -> InstId {
    let mut init = SpawnInit::default();
    init.params.0.insert("slip".into(), ParamValue::Real(r(slip)));
    ctx.spawn(k.slip, r(80.0), r(109.0), init).expect("slip block")
}

// ---- Slip ----

#[test]
fn off_ice_walking_sets_the_speed_outright() {
    with_kid(
        |_| {},
        |ctx, kid, _, _, _| {
            input(ctx, &[Action::Right]);
            walk(ctx, kid);
            assert_eq!(hs(ctx, kid), 3.0);
        },
    );
}

#[test]
fn on_ice_walking_builds_up_by_the_slip_and_stops_at_the_run_speed() {
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            slip_block(ctx, k, 0.2);
            input(ctx, &[Action::Right]);
            walk(ctx, kid);
            assert_eq!(hs(ctx, kid), 0.2);
            for _ in 0..30 {
                walk(ctx, kid);
            }
            assert!(
                (hs(ctx, kid) - 3.0).abs() < 0.21,
                "builds up to about 3 and no further"
            );
            let top = hs(ctx, kid);
            walk(ctx, kid);
            assert!(hs(ctx, kid) >= top - 1e-9 && hs(ctx, kid) <= top + 0.2 + 1e-9);
        },
    );
}

#[test]
fn on_ice_without_input_the_speed_decays_by_the_slip_and_may_overshoot_zero() {
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            slip_block(ctx, k, 0.4);
            set_hs(ctx, kid, 1.0);
            input(ctx, &[]);
            walk(ctx, kid);
            assert!((hs(ctx, kid) - 0.6).abs() < 1e-9);
            walk(ctx, kid);
            walk(ctx, kid);
            assert!(
                (hs(ctx, kid) + 0.2).abs() < 1e-9,
                "1.0 - 3 * 0.4: past zero, no clamp"
            );
        },
    );
}

#[test]
fn a_block_a_little_below_is_not_ice() {
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            // The probe is 4 px: a slip block 6 px below does not count.
            let mut init = SpawnInit::default();
            init.params.0.insert("slip".into(), ParamValue::Real(r(0.2)));
            ctx.spawn(k.slip, r(80.0), r(114.0), init).unwrap();
            input(ctx, &[Action::Right]);
            walk(ctx, kid);
            assert_eq!(hs(ctx, kid), 3.0);
        },
    );
}

#[test]
fn model_none_ignores_slip_blocks() {
    with_kid(
        |p| p.ice.model = IceModel::None,
        |ctx, kid, k, _, _| {
            slip_block(ctx, k, 0.2);
            input(ctx, &[Action::Right]);
            walk(ctx, kid);
            assert_eq!(hs(ctx, kid), 3.0);
        },
    );
}

#[test]
fn c_clamps_the_slip_to_the_run_speed() {
    with_kid(
        |p| p.ice.clamp = true,
        |ctx, kid, k, _, _| {
            slip_block(ctx, k, 2.0);
            input(ctx, &[Action::Right]);
            walk(ctx, kid);
            assert_eq!(hs(ctx, kid), 2.0);
            walk(ctx, kid);
            assert_eq!(hs(ctx, kid), 3.0, "2 + 2 clamps to 3");
        },
    );
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            slip_block(ctx, k, 2.0);
            input(ctx, &[Action::Right]);
            walk(ctx, kid);
            walk(ctx, kid);
            assert_eq!(hs(ctx, kid), 4.0, "A1 does not clamp");
        },
    );
}

// ---- Showdown's temp hspeed ----

fn temp(ctx: &mut Ctx, kid: InstId) {
    kid::run_action(ctx, kid, &KidAction::Ice(IceAction::IceTempHspeed)).expect("ice");
}

#[test]
fn temp_hspeed_remembers_the_speed_and_pushes_it_back_when_the_keys_let_go() {
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            slip_block(ctx, k, 0.0);
            set_hs(ctx, kid, 3.0);
            temp(ctx, kid);
            assert_eq!(kstate(ctx, kid).temp_hspeed.get(), 3.0, "stored");
            temp(ctx, kid);
            assert_eq!(
                kstate(ctx, kid).temp_hspeed.get(),
                3.0,
                "same sign: kept up to date"
            );
            // The key is let go: hspeed 0, so it gains tempHspeed, which decays.
            set_hs(ctx, kid, 0.0);
            temp(ctx, kid);
            assert_eq!(hs(ctx, kid), 3.0);
            // Bit for bit: the GML literal 0.95 under the recorder model (N14) gives 2.85; the
            // nearest double to 0.95 would give 2.8499999999999996.
            assert_eq!(kstate(ctx, kid).temp_hspeed.get(), 2.85);
            // Opposite sign: also pushed.
            set_hs(ctx, kid, -3.0);
            temp(ctx, kid);
            assert_eq!(hs(ctx, kid), -0.149_999_999_999_999_9);
        },
    );
}

/// The slip-ice glide of `traces/showdown/a3-t19-a.csv` (frames 32-71, tempHspeed column):
/// tempHspeed 3.0 decays by the GML literal `0.95` each tick. Every step matches the recorder's
/// reading of the literal, 0.9500000000000001 (N14, research#104), bit for bit.
#[test]
fn temp_hspeed_glide_matches_the_showdown_trace() {
    const TRACE: [f64; 8] = [
        2.85,
        2.707_500_000_000_000_5,
        2.572_125_000_000_000_7,
        2.443_518_750_000_001,
        2.321_342_812_500_001,
        2.205_275_671_875_001_3,
        2.095_011_888_281_251,
        1.990_261_293_867_188_9,
    ];
    assert_eq!(
        num::gml_literal("0.95").expect("literal").get(),
        IceRules::default().temp_decay.get(),
        "the profile default is the literal's recorder value"
    );
    assert_ne!(
        3.0 * 0.95,
        2.85,
        "the nearest double to 0.95 would not match the trace"
    );
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            slip_block(ctx, k, 0.0);
            set_hs(ctx, kid, 3.0);
            temp(ctx, kid);
            for want in TRACE {
                set_hs(ctx, kid, 0.0);
                temp(ctx, kid);
                assert_eq!(kstate(ctx, kid).temp_hspeed.get(), want);
            }
        },
    );
}

#[test]
fn temp_hspeed_zeroes_below_the_limit_and_off_ice() {
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            slip_block(ctx, k, 0.0);
            kstate(ctx, kid).temp_hspeed = r(0.2);
            set_hs(ctx, kid, 0.0);
            temp(ctx, kid);
            assert_eq!(kstate(ctx, kid).temp_hspeed.get(), 0.0, "0.2 * 0.95 is below 0.2");
        },
    );
    with_kid(
        |_| {},
        |ctx, kid, _, _, _| {
            kstate(ctx, kid).temp_hspeed = r(2.0);
            temp(ctx, kid);
            assert_eq!(kstate(ctx, kid).temp_hspeed.get(), 0.0, "off ice");
        },
    );
}

// ---- Ladders ----

fn ladder(ctx: &mut Ctx, kid: InstId) {
    kid::run_action(ctx, kid, &KidAction::Ladder(LadderAction::Ladder)).expect("ladder");
}

#[test]
fn up_on_a_ladder_grabs_it_climbs_and_stops_gravity() {
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            ctx.spawn(k.ladder, r(80.0), r(60.0), SpawnInit::default())
                .unwrap();
            ctx.w.get_mut(kid).unwrap().m.set_vspeed(r(5.0)).unwrap();
            input(ctx, &[Action::Up]);
            ladder(ctx, kid);
            assert!(kstate(ctx, kid).ladder && kstate(ctx, kid).ladder_ud);
            let m = &ctx.w.get(kid).unwrap().m;
            assert_eq!(
                (m.y().get(), m.vspeed().get(), m.gravity().get()),
                (97.0, 0.0, 0.0)
            );
            ladder(ctx, kid);
            assert_eq!(ctx.w.get(kid).unwrap().m.y().get(), 94.0);
        },
    );
}

#[test]
fn down_enters_too_unless_the_entry_is_up_only() {
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            ctx.spawn(k.ladder, r(80.0), r(60.0), SpawnInit::default())
                .unwrap();
            input(ctx, &[Action::Down]);
            ladder(ctx, kid);
            assert!(kstate(ctx, kid).ladder);
            assert_eq!(ctx.w.get(kid).unwrap().m.y().get(), 103.0);
        },
    );
    with_kid(
        |p| p.ladder.entry = LadderEntry::UpOnly,
        |ctx, kid, k, _, _| {
            ctx.spawn(k.ladder, r(80.0), r(60.0), SpawnInit::default())
                .unwrap();
            input(ctx, &[Action::Down]);
            ladder(ctx, kid);
            assert!(!kstate(ctx, kid).ladder);
            assert_eq!(
                ctx.w.get(kid).unwrap().m.gravity().get(),
                0.4,
                "gravity reset when off the ladder"
            );
        },
    );
}

#[test]
fn leaving_the_ladder_area_or_jumping_lets_go() {
    with_kid(
        |_| {},
        |ctx, kid, k, _, _| {
            ctx.spawn(k.ladder, r(80.0), r(60.0), SpawnInit::default())
                .unwrap();
            input(ctx, &[Action::Up, Action::Jump]);
            ladder(ctx, kid);
            assert!(!kstate(ctx, kid).ladder, "Jump held leaves");
            assert!(kstate(ctx, kid).ladder_jump);
        },
    );
    with_kid(
        |_| {},
        |ctx, kid, _, _, _| {
            kstate(ctx, kid).ladder = true;
            input(ctx, &[]);
            ladder(ctx, kid);
            assert!(!kstate(ctx, kid).ladder, "no ladder overlaps any more");
        },
    );
}

#[test]
fn ladder_sprites_and_the_jump_reset() {
    with_kid(
        |_| {},
        |ctx, kid, _, back, updown| {
            kstate(ctx, kid).ladder = true;
            kstate(ctx, kid).ladder_ud = true;
            kstate(ctx, kid).ladder_jump = true;
            kid::run_action(ctx, kid, &KidAction::Ladder(LadderAction::LadderSprites)).unwrap();
            let i = ctx.w.get(kid).unwrap();
            assert_eq!(i.img.sprite(), Some(updown), "moving wins over the back sprite");
            assert_eq!(i.img.speed(), r(0.2));
            let _ = back;
            kid::run_action(ctx, kid, &KidAction::Ladder(LadderAction::LadderJumpReset)).unwrap();
            assert!(!kstate(ctx, kid).ladder_jump);
        },
    );
}
