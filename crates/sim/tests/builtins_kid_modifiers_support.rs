//! Shared fixture for the jump_refresher, spring, conveyor and kid_param_zone tests: a small game
//! with a real kid.

use iwbtg_sim::builtins;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, GameDef, KindDef, KindId, ParamValue, Params, RoomDef};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::kid::api::{self, KidField};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, SpawnInit, Value, World};

pub(crate) fn r(v: f64) -> Real {
    Real::new(v)
}

pub(crate) fn real(v: f64) -> ParamValue {
    ParamValue::Real(r(v))
}

pub(crate) fn word(s: &str) -> ParamValue {
    ParamValue::Str(s.to_owned())
}

pub(crate) fn params(items: &[(&str, ParamValue)]) -> Params {
    let mut p = Params::default();
    for (k, v) in items {
        p.0.insert((*k).to_owned(), v.clone());
    }
    p
}

pub(crate) struct G {
    pub(crate) def: GameDef,
    pub(crate) w: World,
}

/// A game with a kid (class KID, 8 x 8) and one `thing` of `base` with `p` (32 x 32).
pub(crate) fn game(base: Builtin, p: Params) -> G {
    let mut b = GameDefBuilder::new("wp6c-1");
    let spr = b.sprite_rect((32, 32), (0, 0));
    let small = b.sprite_rect((8, 8), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.classes = ClassMask::KID;
    k.sprite = Some(small);
    let kid = b.kind(k);
    let mut thing = KindDef::new("thing", base);
    thing.params = p;
    thing.sprite = Some(spr);
    b.kind(thing);
    b.engine_kind(iwbtg_sim::def::EngineKind::Kid, kid);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut def = b.build().unwrap();
    for k in &mut def.kinds {
        k.events = builtins::event_set(k.base, &k.clone());
    }
    let w = World::new(room, 1);
    G { def, w }
}

pub(crate) fn kind(g: &G, name: &str) -> KindId {
    KindId(u32::try_from(g.def.kinds.iter().position(|k| k.name == name).unwrap()).unwrap())
}

pub(crate) fn with<T>(g: &mut G, f: impl FnOnce(&mut Ctx) -> T) -> T {
    let rules = RunnerRules::default();
    let scripts = ScriptRuntime::default();
    let mut ctx = Ctx {
        def: &g.def,
        rules: &rules,
        w: &mut g.w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    f(&mut ctx)
}

pub(crate) fn spawn(g: &mut G, name: &str, x: f64, y: f64) -> InstId {
    let k = kind(g, name);
    with(g, |c| c.spawn(k, r(x), r(y), SpawnInit::default()).unwrap())
}

pub(crate) fn fire(g: &mut G, id: InstId, ev: &EventKind) {
    with(g, |c| builtins::on_event(c, id, ev).unwrap());
}

pub(crate) fn touch(g: &mut G, id: InstId, kid: InstId) {
    fire(
        g,
        id,
        &EventKind::Collision {
            with: ClassMask::KID,
            other: kid,
        },
    );
}

pub(crate) fn read(g: &G, kid: InstId, name: &str) -> f64 {
    let f = KidField {
        name: name.into(),
        index: None,
    };
    match api::read_field(&g.w, kid, &f) {
        Some(Value::Real(v)) => v.get(),
        other => panic!("{name}: {other:?}"),
    }
}

pub(crate) fn djump(g: &G, kid: InstId) -> bool {
    read(g, kid, "djump") > 0.0
}

pub(crate) fn pos(g: &G, id: InstId) -> (f64, f64) {
    let m = &g.w.get(id).unwrap().m;
    (m.x().get(), m.y().get())
}
