//! Shared fixture for the camera, killer and platform tests: a small game with inert kinds standing
//! in for the kid.

use iwbtg_sim::builtins;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, ParamValue, Params, RoomDef, RoomId,
};
use iwbtg_sim::events::{EventKind, EventSet};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, Flow, InstId, SpawnInit, World};

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

pub(crate) struct G {
    pub(crate) def: GameDef,
    pub(crate) room: RoomId,
    pub(crate) kid: KindId,
    pub(crate) flipped: KindId,
    pub(crate) thing: KindId,
}

/// A game with both kid kinds and one kind `thing` of base `base` taking `p`. The room is
/// 3200 x 1216.
pub(crate) fn game(base: Builtin, p: Params) -> G {
    let mut b = GameDefBuilder::new("wp6a-d");
    let mut k = KindDef::new("kid", Builtin::Inert);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let mut kf = KindDef::new("kid2", Builtin::Inert);
    kf.classes = ClassMask::KID_FLIPPED;
    let flipped = b.kind(kf);
    let mut t = KindDef::new("thing", base);
    t.params = p;
    let thing = b.kind(t);
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::KidFlipped, flipped);
    let room = b.room(RoomDef::new("r", 3200, 1216));
    let mut def = b.build().unwrap();
    for k in &mut def.kinds {
        k.events = builtins::event_set(k.base, &k.clone());
    }
    G {
        def,
        room,
        kid,
        flipped,
        thing,
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

pub(crate) fn fire(g: &G, w: &mut World, id: InstId, ev: &EventKind) -> Flow {
    with(g, w, |c| builtins::on_event(c, id, ev).unwrap())
}

pub(crate) fn view(w: &World) -> (f64, f64) {
    (w.views[0].x.get(), w.views[0].y.get())
}

pub(crate) fn kind_events(g: &G) -> EventSet {
    g.def.kinds[usize::try_from(g.thing.0).unwrap()].events.clone()
}
