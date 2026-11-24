//! Shared fixture for the drop_platform, gravity_flip, bullet_blocker, edge_warp and slope tests:
//! a small game with inert kinds standing in for the kid and the world.

use iwbtg_sim::builtins::{self};
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

pub(crate) fn word(s: &str) -> ParamValue {
    ParamValue::Str(s.to_owned())
}

pub(crate) struct G {
    pub(crate) def: GameDef,
    pub(crate) room: RoomId,
    pub(crate) kid: KindId,
    pub(crate) flipped: KindId,
    pub(crate) bullet: KindId,
}

/// A game with a kid, a flipped kid, a bullet and `extra` kinds (32 x 32 sprite).
pub(crate) fn game(extra: Vec<KindDef>) -> G {
    let mut b = GameDefBuilder::new("wp6b-3");
    let spr32 = b.sprite_rect((32, 32), (0, 0));
    let spr4 = b.sprite_rect((4, 4), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Inert);
    k.classes = ClassMask::KID;
    k.sprite = Some(spr4);
    let kid = b.kind(k);
    let mut f = KindDef::new("flipped", Builtin::Inert);
    f.classes = ClassMask::KID_FLIPPED;
    f.sprite = Some(spr4);
    let flipped = b.kind(f);
    let mut bu = KindDef::new("bullet", Builtin::Inert);
    bu.classes = ClassMask::KID_BULLET;
    bu.sprite = Some(spr4);
    let bullet = b.kind(bu);
    b.engine_kind(EngineKind::Kid, kid);
    for mut kd in extra {
        kd.sprite = Some(spr32);
        b.kind(kd);
    }
    let room = b.room(RoomDef::new("r", 800, 608));
    b.room(RoomDef::new("r2", 800, 608));
    let mut rs = RoomDef::new("sel", 800, 608);
    rs.role = RoomRole::Select;
    b.room(rs);
    let mut def = b.build().unwrap();
    for k in &mut def.kinds {
        k.events = builtins::event_set(k.base, &k.clone());
    }
    G {
        def,
        room,
        kid,
        flipped,
        bullet,
    }
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

pub(crate) fn touch(g: &G, w: &mut World, id: InstId, other: InstId, class: ClassMask) -> Flow {
    fire(g, w, id, &EventKind::Collision { with: class, other })
}

pub(crate) fn alive(w: &World, id: InstId) -> bool {
    w.get(id).is_some_and(|i| i.alive)
}

pub(crate) fn pos(w: &World, id: InstId) -> (f64, f64) {
    let m = &w.get(id).unwrap().m;
    (m.x().get(), m.y().get())
}

pub(crate) fn drop_game(mode: &str) -> G {
    game(vec![kind(
        Builtin::DropPlatform,
        "drop",
        params(&[("mode", word(mode))]),
    )])
}

pub(crate) fn flip_game(dir: &str) -> G {
    game(vec![kind(
        Builtin::GravityFlip,
        "flip",
        params(&[("direction", word(dir))]),
    )])
}

pub(crate) fn room(n: u32) -> ParamValue {
    ParamValue::Room(RoomId(n))
}
