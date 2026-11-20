//! Shared fixture for the kid A4 and A5 action tests: a hand-built game per family and a
//! kid run through one action at a time.

use iwbtg_sim::def::physics::DeathStep;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KidFamily, KindDef, KindId, PhysicsProfile, RoomDef,
};
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::{self, KidAction, presets};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, SpawnInit, World};

pub(crate) fn r(v: f64) -> Real {
    Real::new(v)
}

pub(crate) struct Kinds {
    pub(crate) kid: KindId,
    pub(crate) block: KindId,
}

pub(crate) fn game(
    family: KidFamily,
    tweak: impl FnOnce(&mut PhysicsProfile),
) -> (GameDef, Kinds, iwbtg_sim::def::SpriteId) {
    let mut b = GameDefBuilder::new("wp5c-a4a5");
    let kid_sprite = b.sprite_rect((11, 21), (5, 12));
    let idle = b.sprite_rect((11, 21), (5, 12));
    let tile = b.sprite_rect((32, 32), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(kid_sprite);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let mut blk = KindDef::new("block", Builtin::Inert);
    blk.sprite = Some(tile);
    blk.classes = ClassMask::BLOCK;
    blk.solid = true;
    let block = b.kind(blk);
    let mut bow = KindDef::new("bow", Builtin::Inert);
    bow.classes = ClassMask::BOW;
    let bow = b.kind(bow);
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::Bow, bow);
    let room = b.room(RoomDef::new("wide", 7232, 608));
    b.start_room(room);
    let mut p = presets::defaults(family);
    p.death.order = vec![DeathStep::DestroyAllKids];
    p.sprites.idle = Some(idle);
    tweak(&mut p);
    b.physics(p);
    (b.build().expect("game builds"), Kinds { kid, block }, idle)
}

pub(crate) fn with_kid<T>(
    family: KidFamily,
    tweak: impl FnOnce(&mut PhysicsProfile),
    f: impl FnOnce(&mut Ctx, InstId, &Kinds, iwbtg_sim::def::SpriteId) -> T,
) -> T {
    let (def, k, idle) = game(family, tweak);
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
    kid::run_action(&mut ctx, kid, &KidAction::Common(CommonAction::CreateDefaults)).expect("create");
    f(&mut ctx, kid, &k, idle)
}

pub(crate) fn spawn(ctx: &mut Ctx, kind: KindId, x: f64, y: f64) -> InstId {
    ctx.spawn(kind, r(x), r(y), SpawnInit::default()).expect("spawn")
}

pub(crate) fn common(ctx: &mut Ctx, kid: InstId, a: CommonAction) {
    kid::run_action(ctx, kid, &KidAction::Common(a)).expect("action");
}

pub(crate) fn set(ctx: &mut Ctx, id: InstId, x: f64, y: f64, hs: f64, vs: f64) {
    let m = &mut ctx.w.get_mut(id).unwrap().m;
    m.set_x(r(x)).unwrap();
    m.set_y(r(y)).unwrap();
    m.set_hspeed(r(hs)).unwrap();
    m.set_vspeed(r(vs)).unwrap();
}

pub(crate) fn pos(ctx: &Ctx, id: InstId) -> (f64, f64) {
    let m = &ctx.w.get(id).unwrap().m;
    (m.x().get(), m.y().get())
}

pub(crate) fn vs(ctx: &Ctx, id: InstId) -> f64 {
    ctx.w.get(id).unwrap().m.vspeed().get()
}

pub(crate) fn input(ctx: &mut Ctx, held: &[Action], pressed: &[Action], released: &[Action]) {
    let bits = |l: &[Action]| l.iter().fold(0u32, |a, x| a | x.bit());
    ctx.w.input.latch(InputFrame {
        held: bits(held),
        pressed: bits(pressed),
        released: bits(released),
    });
}
