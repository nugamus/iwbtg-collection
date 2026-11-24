//! Shared fixture for the flag helpers and the boss_gate, boss_item, secret_item, item_viewer, game_clear
//! and music-param tests: a small game with inert kinds standing in for the kid.

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

pub(crate) fn word(s: &str) -> ParamValue {
    ParamValue::Str(s.to_owned())
}

pub(crate) struct G {
    pub(crate) def: GameDef,
    pub(crate) room: RoomId,
    pub(crate) kid: KindId,
}

/// A game with a kid stand-in, a boss stand-in (kind 1) and `extra` kinds (from kind 2).
pub(crate) fn game(extra: Vec<KindDef>) -> G {
    let mut b = GameDefBuilder::new("wp6b-4");
    let spr = b.sprite_rect((32, 32), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Inert);
    k.classes = ClassMask::KID;
    k.sprite = Some(spr);
    let kid = b.kind(k);
    let mut boss = KindDef::new("boss", Builtin::Inert);
    boss.sprite = Some(spr);
    b.kind(boss);
    b.engine_kind(EngineKind::Kid, kid);
    for mut kd in extra {
        kd.sprite = Some(spr);
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

pub(crate) fn touch(g: &G, w: &mut World, id: InstId, other: InstId) {
    fire(
        g,
        w,
        id,
        &EventKind::Collision {
            with: ClassMask::KID,
            other,
        },
    );
}

pub(crate) fn alive(w: &World, id: InstId) -> bool {
    w.get(id).is_some_and(|i| i.alive)
}

pub(crate) fn boss_item_game() -> G {
    game(vec![kind(
        Builtin::BossItem,
        "reward",
        params(&[
            ("boss_index", real(1.0)),
            ("watched_object", ParamValue::Kind(KindId(1))),
        ]),
    )])
}
