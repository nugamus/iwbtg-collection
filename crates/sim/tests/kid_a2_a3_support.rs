//! Shared fixture for the kid A2 and A3 action tests: a hand-built game per family and a
//! kid run through one action at a time.

use iwbtg_sim::def::physics::DeathStep;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KidFamily, KindDef, KindId, PhysicsProfile, RoomDef,
    room::RoomRole,
};
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::actions::a2::A2Action;
use iwbtg_sim::kid::actions::a3::A3Action;
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::{self, KidAction, KidState, presets};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, KindState, SpawnInit, World};

pub(crate) fn r(v: f64) -> Real {
    Real::new(v)
}

pub(crate) struct Kinds {
    pub(crate) kid: KindId,
    pub(crate) block: KindId,
    pub(crate) vine_l: KindId,
    pub(crate) vine_r: KindId,
    pub(crate) killer: KindId,
}

pub(crate) fn game(family: KidFamily, tweak: impl FnOnce(&mut PhysicsProfile)) -> (GameDef, Kinds) {
    let mut b = GameDefBuilder::new("wp5c-a2a3");
    let kid_sprite = b.sprite_rect((11, 21), (5, 12));
    let tile = b.sprite_rect((32, 32), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(kid_sprite);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let inert = |name: &str, classes: ClassMask, solid: bool, b: &mut GameDefBuilder| {
        let mut k = KindDef::new(name, Builtin::Inert);
        k.sprite = Some(tile);
        k.classes = classes;
        k.solid = solid;
        b.kind(k)
    };
    let block = inert("block", ClassMask::BLOCK, true, &mut b);
    let vine_l = inert("vine_l", ClassMask::VINE_L, false, &mut b);
    let vine_r = inert("vine_r", ClassMask::VINE_R, false, &mut b);
    let killer = inert("killer", ClassMask::KILLER, false, &mut b);
    let mut bow = KindDef::new("bow", Builtin::Inert);
    bow.classes = ClassMask::BOW;
    let bow = b.kind(bow);
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::Bow, bow);
    let play = b.room(RoomDef::new("play", 800, 608));
    let mut menu = RoomDef::new("menu", 800, 608);
    menu.role = RoomRole::Menu;
    b.room(menu);
    let mut select = RoomDef::new("select", 800, 608);
    select.role = RoomRole::Select;
    b.room(select);
    let mut grants = RoomDef::new("room028", 800, 608); // RoomId(3)
    grants.djump_grants_tjump = true;
    b.room(grants);
    b.start_room(play);
    let mut p = presets::defaults(family);
    p.death.order = vec![DeathStep::DestroyAllKids];
    tweak(&mut p);
    b.physics(p);
    (
        b.build().expect("game builds"),
        Kinds {
            kid,
            block,
            vine_l,
            vine_r,
            killer,
        },
    )
}

/// A kid at (100, 100) after `create_defaults`.
pub(crate) fn with_kid<T>(
    family: KidFamily,
    tweak: impl FnOnce(&mut PhysicsProfile),
    f: impl FnOnce(&mut Ctx, InstId, &Kinds) -> T,
) -> T {
    let (def, k) = game(family, tweak);
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
    run(&mut ctx, kid, KidAction::Common(CommonAction::CreateDefaults));
    f(&mut ctx, kid, &k)
}

pub(crate) fn run(ctx: &mut Ctx, kid: InstId, a: KidAction) {
    kid::run_action(ctx, kid, &a).expect("action");
}

pub(crate) fn input(ctx: &mut Ctx, held: &[Action], pressed: &[Action], released: &[Action]) {
    let bits = |l: &[Action]| l.iter().fold(0u32, |a, x| a | x.bit());
    ctx.w.input.latch(InputFrame {
        held: bits(held),
        pressed: bits(pressed),
        released: bits(released),
    });
}

pub(crate) fn spawn(ctx: &mut Ctx, kind: KindId, x: f64, y: f64) -> InstId {
    ctx.spawn(kind, r(x), r(y), SpawnInit::default()).expect("spawn")
}

pub(crate) fn kstate<'a>(ctx: &'a mut Ctx, id: InstId) -> &'a mut KidState {
    match &mut ctx.w.get_mut(id).unwrap().state {
        KindState::Kid(s) => s,
        _ => unreachable!(),
    }
}

pub(crate) fn set_v(ctx: &mut Ctx, id: InstId, hs: f64, vs: f64) {
    let m = &mut ctx.w.get_mut(id).unwrap().m;
    m.set_hspeed(r(hs)).unwrap();
    m.set_vspeed(r(vs)).unwrap();
}

pub(crate) fn hv(ctx: &Ctx, id: InstId) -> (f64, f64) {
    let m = &ctx.w.get(id).unwrap().m;
    (m.hspeed().get(), m.vspeed().get())
}

pub(crate) fn a2(ctx: &mut Ctx, kid: InstId, a: A2Action) {
    run(ctx, kid, KidAction::A2(a));
}

pub(crate) fn a3(ctx: &mut Ctx, kid: InstId, a: A3Action) {
    run(ctx, kid, KidAction::A3(a));
}

pub(crate) fn common(ctx: &mut Ctx, kid: InstId, a: CommonAction) {
    run(ctx, kid, KidAction::Common(a));
}
