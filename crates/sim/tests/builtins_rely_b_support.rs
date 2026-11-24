//! Shared fixture for the rely slice's warp, difficulty_warp and save tests: a small game with
//! inert kinds standing in for the kid.

use iwbtg_sim::builtins;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, ParamValue, Params, RoomDef, RoomId,
};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::state::KidState;
use iwbtg_sim::num::Real;
use iwbtg_sim::room::RoomRequest;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, Flow, InstId, KindState, SpawnInit, World};

pub(crate) fn r(v: f64) -> Real {
    Real::new(v)
}

pub(crate) fn params(items: &[(&str, ParamValue)]) -> Params {
    let mut p = Params::default();
    for (k, v) in items {
        p.0.insert((*k).to_owned(), v.clone());
    }
    p
}

pub(crate) fn real(v: f64) -> ParamValue {
    ParamValue::Real(r(v))
}

pub(crate) fn word(s: &str) -> ParamValue {
    ParamValue::Str(s.to_owned())
}

pub(crate) fn yes() -> ParamValue {
    ParamValue::Bool(true)
}

pub(crate) struct G {
    pub(crate) def: GameDef,
    pub(crate) room: RoomId,
    pub(crate) room2: RoomId,
    pub(crate) kid: KindId,
    pub(crate) flipped: KindId,
    pub(crate) bow: KindId,
}

/// A game with kid kinds, a bow and the kinds `kinds` builds (it gets `room2` for warps).
pub(crate) fn game(kinds: impl FnOnce(RoomId) -> Vec<KindDef>) -> G {
    let mut b = GameDefBuilder::new("wp6a-b");
    let spr32 = b.sprite_rect((32, 32), (0, 0));
    let spr4 = b.sprite_rect((4, 4), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Inert);
    k.persistent = true;
    k.classes = ClassMask::KID;
    k.sprite = Some(spr4);
    let kid = b.kind(k);
    let mut kf = KindDef::new("kid2", Builtin::Inert);
    kf.persistent = true;
    kf.classes = ClassMask::KID_FLIPPED;
    kf.sprite = Some(spr4);
    let flipped = b.kind(kf);
    let bow = b.kind(KindDef::new("bow", Builtin::Inert));
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::KidFlipped, flipped);
    b.engine_kind(EngineKind::Bow, bow);
    let room = b.room(RoomDef::new("r", 800, 608));
    let room2 = b.room(RoomDef::new("r2", 800, 608));
    for mut kd in kinds(room2) {
        kd.sprite = Some(spr32);
        b.kind(kd);
    }
    let mut def = b.build().unwrap();
    def.new_game_room = room;
    G {
        def,
        room,
        room2,
        kid,
        flipped,
        bow,
    }
}

pub(crate) fn kind_id(g: &G, name: &str) -> KindId {
    KindId(u32::try_from(g.def.kinds.iter().position(|k| k.name == name).unwrap()).unwrap())
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

pub(crate) fn fire(g: &G, w: &mut World, id: InstId, ev: &EventKind) -> Flow {
    with(g, w, |c| builtins::on_event(c, id, ev).unwrap())
}

pub(crate) fn touch(kid: InstId) -> EventKind {
    EventKind::Collision {
        with: ClassMask::KID,
        other: kid,
    }
}

pub(crate) fn alive(w: &World, kind: KindId) -> Vec<InstId> {
    w.iter_alive().filter(|i| i.kind == kind).map(|i| i.id).collect()
}

pub(crate) fn frame(w: &mut World, pressed: &[Action]) {
    let bits = pressed.iter().fold(0u32, |acc, x| acc | x.bit());
    w.input.latch(InputFrame {
        held: bits,
        pressed: bits,
        released: 0,
    });
}

pub(crate) fn warp_game(p: impl FnOnce(RoomId) -> Params) -> G {
    game(|room2| {
        let mut k = KindDef::new("warp", Builtin::Warp);
        let mut p = p(room2);
        p.0.insert("room_to".to_owned(), ParamValue::Room(room2));
        k.params = p;
        vec![k]
    })
}

/// A kid with speed (3, `vspeed`) and djump spent, at (`x`, `y`), touched by the warp.
pub(crate) fn warp_a_moving_kid(g: &G, x: f64, y: f64, vspeed: f64) -> (World, InstId) {
    let mut w = World::new(g.room, 1);
    let warp = spawn(g, &mut w, kind_id(g, "warp"), 0.0, 0.0);
    let kid = spawn(g, &mut w, g.kid, x, y);
    let k = w.get_mut(kid).unwrap();
    k.m.set_hspeed(r(3.0)).unwrap();
    k.m.set_vspeed(r(vspeed)).unwrap();
    k.state = KindState::Kid(Box::new(KidState {
        djump: false,
        ..KidState::default()
    }));
    assert_eq!(fire(g, &mut w, warp, &touch(kid)), Flow::AbortTick);
    assert_eq!(w.pending, Some(RoomRequest::Goto(g.room2)));
    (w, kid)
}

pub(crate) fn pos_speed(w: &World, kid: InstId) -> (Real, Real, Real, Real) {
    let k = w.get(kid).unwrap();
    assert!(k.alive);
    (k.m.x(), k.m.y(), k.m.hspeed(), k.m.vspeed())
}

pub(crate) fn djump(w: &World, kid: InstId) -> bool {
    match &w.get(kid).unwrap().state {
        KindState::Kid(s) => s.djump,
        other => panic!("kid state {other:?}"),
    }
}

pub(crate) fn list(v: &[f64]) -> ParamValue {
    ParamValue::List(v.iter().map(|x| real(*x)).collect())
}

/// A gated warp, created (Create run) in a fresh world, with a kid beside it.
pub(crate) fn gated(p: Vec<(&'static str, ParamValue)>) -> (G, World, InstId, InstId) {
    let g = warp_game(move |_| params(&p));
    let mut w = World::new(g.room, 1);
    let warp = spawn(&g, &mut w, kind_id(&g, "warp"), 0.0, 0.0);
    fire(&g, &mut w, warp, &EventKind::Create);
    let kid = spawn(&g, &mut w, g.kid, 5.0, 5.0);
    (g, w, warp, kid)
}

/// A game with one save point kind `sp` taking `p`.
pub(crate) fn save_game(p: Params) -> G {
    game(|_| {
        let mut k = KindDef::new("sp", Builtin::Save);
        k.params = p;
        vec![k]
    })
}

/// A save point at (0, 0) over a kid at (49.4, 567.4) in room r, and the pair's ids.
pub(crate) fn save_world(g: &G) -> (World, InstId, InstId) {
    let mut w = World::new(g.room, 1);
    let sp = spawn(g, &mut w, kind_id(g, "sp"), 0.0, 0.0);
    let kid = spawn(g, &mut w, g.kid, 10.4, 10.4);
    (w, sp, kid)
}
