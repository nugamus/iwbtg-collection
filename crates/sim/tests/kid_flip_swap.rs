//! WP5c.4b (#299): group A's flip by object swap (kid/A1.md Step action 5, kid/A2.md player2,
//! kid/A3.md) on hand-built worlds.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KidFamily, KindDef, KindId, PhysicsProfile, RoomDef,
};
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::actions::flip::FlipAction;
use iwbtg_sim::kid::collisions::block::BlockRun;
use iwbtg_sim::kid::collisions::platform::PlatformRun;
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
    flipped: Option<KindId>,
    block: KindId,
    solid: KindId,
    platform: KindId,
    vine_l: KindId,
}

fn game(with_flipped: bool, family: KidFamily, tweak: impl FnOnce(&mut PhysicsProfile)) -> (GameDef, Kinds) {
    let mut b = GameDefBuilder::new("wp5c-flip");
    let mask = b.sprite_rect((11, 21), (5, 12));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(mask);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    b.engine_kind(EngineKind::Kid, kid);
    let flipped = with_flipped.then(|| {
        let mut kf = KindDef::new("kid2", Builtin::Kid);
        kf.sprite = Some(mask);
        kf.classes = ClassMask::KID_FLIPPED;
        let id = b.kind(kf);
        b.engine_kind(EngineKind::KidFlipped, id);
        id
    });
    let tile = b.sprite_rect((32, 32), (0, 0));
    let thing = |name: &str, classes: ClassMask, solid: bool, b: &mut GameDefBuilder| {
        let mut k = KindDef::new(name, Builtin::Inert);
        k.sprite = Some(tile);
        k.classes = classes;
        k.solid = solid;
        b.kind(k)
    };
    let block = thing("block", ClassMask::BLOCK, true, &mut b);
    let solid = thing("solid", ClassMask::SOLIDBLOCK, true, &mut b);
    let platform = thing("platform", ClassMask::PLATFORM, false, &mut b);
    let vine_l = thing("vine_l", ClassMask::VINE_L, false, &mut b);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut p: PhysicsProfile = presets::defaults(family);
    tweak(&mut p);
    b.physics(p);
    (
        b.build().expect("game builds"),
        Kinds {
            kid,
            flipped,
            block,
            solid,
            platform,
            vine_l,
        },
    )
}

fn with_world<T>(with_flipped: bool, f: impl FnOnce(&mut Ctx, &Kinds) -> T) -> T {
    with_family(with_flipped, KidFamily::A1, |_| {}, f)
}

fn with_family<T>(
    with_flipped: bool,
    family: KidFamily,
    tweak: impl FnOnce(&mut PhysicsProfile),
    f: impl FnOnce(&mut Ctx, &Kinds) -> T,
) -> T {
    let (def, k) = game(with_flipped, family, tweak);
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
    f(&mut ctx, &k)
}

fn make(ctx: &mut Ctx, kind: KindId) -> InstId {
    let id = ctx
        .spawn(kind, r(100.0), r(100.0), SpawnInit::default())
        .expect("kid");
    kid::run_action(ctx, id, &KidAction::Common(CommonAction::CreateDefaults)).expect("create");
    id
}

fn kstate<'a>(ctx: &'a mut Ctx, id: InstId) -> &'a mut KidState {
    match &mut ctx.w.get_mut(id).unwrap().state {
        KindState::Kid(s) => s,
        _ => unreachable!(),
    }
}

fn swap(ctx: &mut Ctx, id: InstId) -> Result<(), iwbtg_sim::error::SimError> {
    kid::run_action(ctx, id, &KidAction::Flip(FlipAction::FlipSwap)).map(|_| ())
}

fn of_kind(ctx: &Ctx, kind: KindId) -> Vec<InstId> {
    ctx.w
        .iter_alive()
        .filter(|i| i.kind == kind)
        .map(|i| i.id)
        .collect()
}

#[test]
fn create_of_the_flipped_object_signs_gravity_and_scale() {
    with_world(true, |ctx, k| {
        let n = make(ctx, k.kid);
        assert_eq!(ctx.w.get(n).unwrap().m.gravity(), r(0.4));
        assert!(!kstate(ctx, n).flipped);
        let f = make(ctx, k.flipped.unwrap());
        assert_eq!(ctx.w.get(f).unwrap().m.gravity(), r(-0.4));
        assert_eq!(ctx.w.get(f).unwrap().img.yscale(), r(-1.0));
        assert!(kstate(ctx, f).flipped);
    });
}

#[test]
fn flip_swap_hands_over_to_the_flipped_object_four_up_and_back_four_down() {
    with_world(true, |ctx, k| {
        let n = make(ctx, k.kid);
        kstate(ctx, n).facing = -1;
        swap(ctx, n).expect("no-op with grav 0");
        assert!(of_kind(ctx, k.flipped.unwrap()).is_empty());
        ctx.w.engine.kid.grav = true;
        swap(ctx, n).expect("swap");
        assert!(ctx.w.get(n).is_none_or(|i| !i.alive), "the old instance is gone");
        let f = of_kind(ctx, k.flipped.unwrap());
        assert_eq!(f.len(), 1);
        assert_eq!(ctx.w.get(f[0]).unwrap().m.y().get(), 96.0);
        assert_eq!(ctx.w.engine.kid.image_xscale, -1, "the facing is handed over");
        // A kid that is already flipped matches grav: nothing happens.
        let f0 = make(ctx, k.flipped.unwrap());
        swap(ctx, f0).expect("no-op");
        assert!(ctx.w.get(f0).unwrap().alive);
        // And back.
        ctx.w.engine.kid.grav = false;
        swap(ctx, f0).expect("swap back");
        let back = of_kind(ctx, k.kid);
        assert_eq!(back.len(), 1);
        assert_eq!(ctx.w.get(back[0]).unwrap().m.y().get(), 104.0);
    });
}

#[test]
fn a_game_without_a_flipped_object_cannot_swap() {
    with_world(false, |ctx, k| {
        let n = make(ctx, k.kid);
        ctx.w.engine.kid.grav = true;
        assert!(swap(ctx, n).is_err());
    });
}

#[test]
fn gravity_keeps_its_sign_for_the_flipped_kid() {
    with_world(true, |ctx, k| {
        let f = make(ctx, k.flipped.unwrap());
        ctx.w.get_mut(f).unwrap().m.set_gravity(Real::ZERO).unwrap();
        kid::run_action(ctx, f, &KidAction::Common(CommonAction::Gravity)).unwrap();
        assert_eq!(ctx.w.get(f).unwrap().m.gravity(), r(-0.4));
        let n = make(ctx, k.kid);
        ctx.w.get_mut(n).unwrap().m.set_gravity(Real::ZERO).unwrap();
        kid::run_action(ctx, n, &KidAction::Common(CommonAction::Gravity)).unwrap();
        assert_eq!(ctx.w.get(n).unwrap().m.gravity(), r(0.4));
    });
}

#[test]
fn the_group_a_presets_list_the_swap() {
    for family in [KidFamily::A1, KidFamily::A2, KidFamily::A3] {
        let p = presets::defaults(family);
        assert!(
            p.events.step.contains(&KidAction::Flip(FlipAction::FlipSwap)),
            "{family:?}"
        );
    }
    for family in [KidFamily::A4, KidFamily::A5] {
        let p = presets::defaults(family);
        assert!(
            !p.events.step.contains(&KidAction::Flip(FlipAction::FlipSwap)),
            "{family:?}"
        );
    }
}

fn input(ctx: &mut Ctx, held: &[Action], pressed: &[Action], released: &[Action]) {
    let bits = |l: &[Action]| l.iter().fold(0u32, |a, x| a | x.bit());
    ctx.w.input.latch(InputFrame {
        held: bits(held),
        pressed: bits(pressed),
        released: bits(released),
    });
}

fn put(ctx: &mut Ctx, kind: KindId, x: f64, y: f64) -> InstId {
    ctx.spawn(kind, r(x), r(y), SpawnInit::default()).expect("spawn")
}

fn vs(ctx: &Ctx, id: InstId) -> f64 {
    ctx.w.get(id).unwrap().m.vspeed().get()
}

fn collide(ctx: &mut Ctx, kid: InstId, other: InstId, run: KidCollisionRun) {
    kid::run_collision(ctx, kid, other, &run).expect("collision");
}

const BLOCK_STANDARD: KidCollisionRun = KidCollisionRun::Block(BlockRun::BlockStandard);

#[test]
fn the_flipped_kid_jumps_and_cuts_against_gravity() {
    with_world(true, |ctx, k| {
        let f = make(ctx, k.flipped.unwrap());
        // Its ground is the ceiling; the mirrored box starts at y 92, so a block ending at 91 touches.
        put(ctx, k.block, 90.0, 60.0);
        input(ctx, &[Action::Jump], &[Action::Jump], &[]);
        kid::run_action(ctx, f, &KidAction::Common(CommonAction::Jump)).unwrap();
        assert_eq!(vs(ctx, f), 8.5, "up is down: the jump is positive");
        ctx.w.get_mut(f).unwrap().m.set_vspeed(r(8.0)).unwrap();
        input(ctx, &[], &[], &[Action::Jump]);
        kid::run_action(ctx, f, &KidAction::Common(CommonAction::Release)).unwrap();
        assert!((vs(ctx, f) - 3.6).abs() < 1e-9, "0.45 * 8");
    });
}

#[test]
fn the_flipped_kid_lands_on_the_ceiling_and_refreshes_there_only() {
    with_world(true, |ctx, k| {
        let f = make(ctx, k.flipped.unwrap());
        let n = make(ctx, k.kid);
        let b = put(ctx, k.block, 90.0, 56.0);
        for (kid, refreshed) in [(f, true), (n, false)] {
            kstate(ctx, kid).djump = false;
            ctx.w.get_mut(kid).unwrap().m.set_vspeed(r(-6.0)).unwrap();
            collide(ctx, kid, b, BLOCK_STANDARD);
            assert_eq!(kstate(ctx, kid).djump, refreshed);
            assert_eq!(vs(ctx, kid), 0.0);
        }
    });
}

#[test]
fn the_flipped_kid_lands_on_a_platform_with_the_reversed_numbers() {
    let tweak = |p: &mut PhysicsProfile| {
        p.platform.reversed_above = r(15.0);
        p.platform.reversed_snap = r(23.0);
    };
    with_family(true, KidFamily::A1, tweak, |ctx, k| {
        let f = make(ctx, k.flipped.unwrap());
        let n = make(ctx, k.kid);
        let p = put(ctx, k.platform, 90.0, 85.0);
        let run = KidCollisionRun::Platform(PlatformRun::PlatformStandard);
        // y - vspeed / 2 = 103 is at least platform.y + 15: the flipped kid lands at y + 23.
        ctx.w.get_mut(f).unwrap().m.set_vspeed(r(-6.0)).unwrap();
        collide(ctx, f, p, run.clone());
        assert_eq!(ctx.w.get(f).unwrap().m.y().get(), 108.0);
        assert!(kstate(ctx, f).on_platform && kstate(ctx, f).djump);
        // The normal kid at the same spot is below the platform top and passes through.
        ctx.w.get_mut(n).unwrap().m.set_vspeed(r(-6.0)).unwrap();
        collide(ctx, n, p, run);
        assert!(!kstate(ctx, n).on_platform);
    });
}

#[test]
fn a_flipped_kid_slides_and_kicks_off_a_vine_with_flipped_signs() {
    with_world(true, |ctx, k| {
        let f = make(ctx, k.flipped.unwrap());
        // A left vine touching the box (x 95): the kid slides, at -2 for the flipped one.
        put(ctx, k.vine_l, 64.0, 96.0);
        let vines = |ctx: &mut Ctx| {
            kid::run_action(ctx, f, &KidAction::Common(CommonAction::Vines)).unwrap();
        };
        vines(ctx);
        assert_eq!(vs(ctx, f), -2.0);
        // No key swap: Right is still the away key; the kick is +15 and +9.
        input(ctx, &[Action::Jump, Action::Right], &[Action::Right], &[]);
        vines(ctx);
        let m = &ctx.w.get(f).unwrap().m;
        assert_eq!((m.hspeed().get(), m.vspeed().get()), (15.0, 9.0));
    });
}

#[test]
fn a2_vine_probes_look_down_even_for_the_flipped_kid() {
    // A block 1 px above the flipped kid (the gravity side): by default the vine does not hold
    // there; with A2 `probe_down_fixed` it still does.
    for (family, holds) in [(KidFamily::A1, false), (KidFamily::A2, true)] {
        with_family(
            true,
            family,
            |_| {},
            |ctx, k| {
                let f = make(ctx, k.flipped.unwrap());
                put(ctx, k.vine_l, 64.0, 96.0);
                put(ctx, k.block, 90.0, 60.0);
                kid::run_action(ctx, f, &KidAction::Common(CommonAction::Vines)).unwrap();
                assert_eq!((vs(ctx, f) + 2.0).abs() < 1e-9, holds, "{family:?}");
            },
        );
    }
}

#[test]
fn a2_solidblock_does_not_refresh_the_flipped_kid_but_block_does() {
    with_family(
        true,
        KidFamily::A2,
        |_| {},
        |ctx, k| {
            let f = make(ctx, k.flipped.unwrap());
            let n = make(ctx, k.kid);
            let solid = put(ctx, k.solid, 90.0, 56.0);
            let block = put(ctx, k.block, 90.0, 56.0);
            for (kid, other, refreshed) in [(f, solid, false), (f, block, true)] {
                kstate(ctx, kid).djump = false;
                ctx.w.get_mut(kid).unwrap().m.set_vspeed(r(-6.0)).unwrap();
                collide(ctx, kid, other, BLOCK_STANDARD);
                assert_eq!(kstate(ctx, kid).djump, refreshed);
            }
            // The normal kid is unaffected: it refreshes on the floor side.
            let floor = put(ctx, k.solid, 90.0, 109.0);
            kstate(ctx, n).djump = false;
            ctx.w.get_mut(n).unwrap().m.set_vspeed(r(6.0)).unwrap();
            collide(ctx, n, floor, BLOCK_STANDARD);
            assert!(kstate(ctx, n).djump);
        },
    );
}
