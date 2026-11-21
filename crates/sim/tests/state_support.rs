//! Shared fixture for the restart, save and view tests: a boot room, a play room and a
//! menu room, with inert kinds standing in for the kid, the bow and a block.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, RoomDef, RoomId, RoomRole};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::save::SaveSlot;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, SpawnInit, World};

pub(crate) fn r(v: f64) -> Real {
    Real::new(v)
}

pub(crate) struct G {
    pub(crate) def: GameDef,
    pub(crate) boot: RoomId,
    pub(crate) play: RoomId,
    pub(crate) menu: RoomId,
    pub(crate) kid: KindId,
    pub(crate) bow: KindId,
    pub(crate) block: KindId,
}

pub(crate) fn game(edit: impl FnOnce(&mut GameDef)) -> G {
    let mut b = GameDefBuilder::new("wp4b");
    let spr = b.sprite_rect((4, 4), (0, 0));
    let big = b.sprite_rect((10, 10), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Inert);
    k.persistent = true;
    k.classes = ClassMask::KID;
    k.sprite = Some(spr);
    let kid = b.kind(k);
    let mut kf = KindDef::new("kid2", Builtin::Inert);
    kf.persistent = true;
    kf.classes = ClassMask::KID_FLIPPED;
    kf.sprite = Some(spr);
    let kid2 = b.kind(kf);
    let bow = b.kind(KindDef::new("bow", Builtin::Inert));
    let mut bl = KindDef::new("block", Builtin::Inert);
    bl.solid = true;
    bl.classes = ClassMask::BLOCK;
    bl.sprite = Some(big);
    let block = b.kind(bl);
    let mut w = KindDef::new("world", Builtin::Inert);
    w.persistent = true;
    let world = b.kind(w);
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::KidFlipped, kid2);
    b.engine_kind(EngineKind::Bow, bow);
    b.boot(world);
    let mut rb = RoomDef::new("boot", 800, 608);
    rb.role = RoomRole::Boot;
    let boot = b.room(rb);
    let play = b.room(RoomDef::new("play", 3200, 608));
    let mut rm = RoomDef::new("menu", 800, 608);
    rm.role = RoomRole::Menu;
    let menu = b.room(rm);
    let mut def = b.build().unwrap();
    edit(&mut def);
    G {
        def,
        boot,
        play,
        menu,
        kid,
        bow,
        block,
    }
}

pub(crate) fn with<T>(g: &G, w: &mut World, f: impl FnOnce(&mut Ctx) -> T) -> T {
    let rules = RunnerRules::default();
    let scripts = ScriptRuntime::default();
    let mut ctx = Ctx {
        def: &g.def,
        rules: &rules,
        w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    f(&mut ctx)
}

pub(crate) fn spawn(g: &G, w: &mut World, kind: KindId, x: f64, y: f64) -> InstId {
    with(g, w, |c| c.spawn(kind, r(x), r(y), SpawnInit::default()).unwrap())
}

pub(crate) fn slot(room: RoomId) -> SaveSlot {
    SaveSlot {
        room,
        x: r(49.0),
        y: r(119.0),
        precision: Default::default(),
        point: None,
        flipped: false,
        facing: -1,
        difficulty: 2,
        deaths: 5,
        time: 9,
        bosses: 3,
        items: 4,
        clear: true,
    }
}

pub(crate) fn alive(w: &World, kind: KindId) -> Vec<InstId> {
    w.iter_alive().filter(|i| i.kind == kind).map(|i| i.id).collect()
}
