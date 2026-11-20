//! Shared fixture for the trigger, trap, hidden_block and fake_block tests: a small game with
//! inert kinds standing in for the kid.

use iwbtg_sim::builtins;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, ParamValue, Params, RoomDef, RoomId, RoomRole,
};
use iwbtg_sim::events::EventKind;
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

pub(crate) fn yes() -> ParamValue {
    ParamValue::Bool(true)
}

pub(crate) fn word(s: &str) -> ParamValue {
    ParamValue::Str(s.to_owned())
}

pub(crate) struct G {
    pub(crate) def: GameDef,
    pub(crate) room: RoomId,
    pub(crate) kid: KindId,
}

/// A game with a kid stand-in, a kid bullet stand-in, a trap stopper stand-in and `extra`
/// kinds (each gets a 32 x 32 sprite). Kind ids: kid 0, bullet 1, stopper 2, extras from 3.
pub(crate) fn game(extra: Vec<KindDef>) -> G {
    let mut b = GameDefBuilder::new("wp6b-2");
    let spr32 = b.sprite_rect((32, 32), (0, 0));
    let spr4 = b.sprite_rect((4, 4), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Inert);
    k.classes = ClassMask::KID;
    k.sprite = Some(spr4);
    let kid = b.kind(k);
    let mut bu = KindDef::new("bullet", Builtin::Inert);
    bu.classes = ClassMask::KID_BULLET;
    bu.sprite = Some(spr4);
    b.kind(bu);
    let mut st = KindDef::new("stopper", Builtin::Inert);
    st.classes = ClassMask::TRAP_STOPPER;
    st.sprite = Some(spr32);
    b.kind(st);
    b.engine_kind(EngineKind::Kid, kid);
    for mut kd in extra {
        kd.sprite = Some(spr32);
        b.kind(kd);
    }
    let room = b.room(RoomDef::new("r", 800, 608));
    let mut rs = RoomDef::new("sel", 800, 608);
    rs.role = RoomRole::Select;
    b.room(rs);
    let mut def = b.build().unwrap();
    for k in &mut def.kinds {
        k.events = builtins::event_set(k.base, &k.clone());
    }
    G { def, room, kid }
}

pub(crate) fn kind(base: Builtin, name: &str, p: Params) -> KindDef {
    let mut k = KindDef::new(name, base);
    k.params = p;
    k
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

pub(crate) fn touch(g: &G, w: &mut World, id: InstId, other: InstId, class: ClassMask) {
    fire(g, w, id, &EventKind::Collision { with: class, other });
}

pub(crate) fn alive(w: &World, id: InstId) -> bool {
    w.get(id).is_some_and(|i| i.alive)
}

pub(crate) fn speeds(w: &World, id: InstId) -> (f64, f64) {
    let m = &w.get(id).unwrap().m;
    (m.hspeed().get(), m.vspeed().get())
}

pub(crate) fn trig_game(model: &str, extra: &[(&str, ParamValue)]) -> G {
    let mut items = vec![("model", word(model)), ("trg", real(3.0))];
    items.extend(extra.iter().cloned());
    game(vec![kind(Builtin::Trigger, "trig", params(&items))])
}

pub(crate) fn trap_game(p: &[(&str, ParamValue)]) -> G {
    let mut items = vec![("trigger_model", word("flag_array")), ("trigger_id", real(1.0))];
    items.extend(p.iter().cloned());
    game(vec![kind(Builtin::Trap, "trap", params(&items))])
}

pub(crate) fn step(g: &G, w: &mut World, id: InstId) {
    fire(g, w, id, &EventKind::Step);
    let i = w.get_mut(id).unwrap();
    let (h, v) = (i.m.hspeed(), i.m.vspeed());
    let (x, y) = (i.m.x(), i.m.y());
    i.m.set_x(x + h).unwrap();
    i.m.set_y(y + v).unwrap();
}

pub(crate) fn toward_game(p: &[(&str, ParamValue)]) -> G {
    let mut items = vec![
        ("trigger_model", word("switch_array")),
        ("trigger_id", real(1.0)),
        ("launch", word("step_toward")),
    ];
    items.extend(p.iter().cloned());
    game(vec![kind(Builtin::Trap, "trap", params(&items))])
}

pub(crate) fn pos(w: &World, id: InstId) -> (f64, f64) {
    let i = w.get(id).unwrap();
    (i.m.x().get(), i.m.y().get())
}

pub(crate) fn hidden_game(p: &[(&str, ParamValue)]) -> G {
    game(vec![kind(Builtin::HiddenBlock, "hid", params(p))])
}

pub(crate) fn fake_game(p: &[(&str, ParamValue)]) -> G {
    game(vec![kind(Builtin::FakeBlock, "fake", params(p))])
}
