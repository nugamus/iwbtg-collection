//! Shared fixture for the playerface, vine, water and moving-platform tests: a small game with
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

pub(crate) fn no() -> ParamValue {
    ParamValue::Bool(false)
}

pub(crate) fn word(s: &str) -> ParamValue {
    ParamValue::Str(s.to_owned())
}

pub(crate) struct G {
    pub(crate) def: GameDef,
    pub(crate) room: RoomId,
    pub(crate) kid: KindId,
    pub(crate) flipped: KindId,
    pub(crate) block: KindId,
}

/// A game with two kid stand-ins, a block and `extra` kinds (each gets a 32 x 32 sprite).
pub(crate) fn game(extra: Vec<KindDef>) -> G {
    let mut b = GameDefBuilder::new("wp6b-1");
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
    let mut blk = KindDef::new("block", Builtin::Block);
    blk.solid = true;
    blk.classes = ClassMask::BLOCK;
    blk.sprite = Some(spr32);
    let block = b.kind(blk);
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
    G {
        def,
        room,
        kid,
        flipped,
        block,
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

pub(crate) fn spawn_at(g: &G, w: &mut World, kind: KindId, x: f64, y: f64, p: Params) -> InstId {
    with(g, w, |c| {
        let init = SpawnInit {
            params: p,
            ..SpawnInit::default()
        };
        c.spawn(kind, r(x), r(y), init).unwrap()
    })
}

pub(crate) fn spawn(g: &G, w: &mut World, kind: KindId, x: f64, y: f64) -> InstId {
    spawn_at(g, w, kind, x, y, Params::default())
}

pub(crate) fn fire(g: &G, w: &mut World, id: InstId, ev: &EventKind) -> Flow {
    with(g, w, |c| builtins::on_event(c, id, ev).unwrap())
}

pub(crate) fn touch(g: &G, w: &mut World, id: InstId, other: InstId, class: ClassMask) {
    fire(g, w, id, &EventKind::Collision { with: class, other });
}

pub(crate) fn set_v(w: &mut World, id: InstId, v: f64) {
    w.get_mut(id).unwrap().m.set_vspeed(r(v)).unwrap();
}

pub(crate) fn vspeed(w: &World, id: InstId) -> Real {
    w.get(id).unwrap().m.vspeed()
}

pub(crate) fn pos(w: &World, id: InstId) -> (f64, f64) {
    let m = &w.get(id).unwrap().m;
    (m.x().get(), m.y().get())
}

pub(crate) fn water_game(p: Params) -> G {
    let mut k = KindDef::new("water", Builtin::Water);
    k.params = p;
    game(vec![k])
}

pub(crate) fn area_water() -> G {
    water_game(params(&[
        ("area_left", real(0.0)),
        ("area_right", real(100.0)),
        ("area_top", real(50.0)),
        ("area_bottom", real(200.0)),
    ]))
}

pub(crate) fn plat_game(p: Params) -> G {
    let mut k = KindDef::new("plat", Builtin::MovingPlatform);
    k.params = p;
    game(vec![k])
}

/// One platform Step, then the runner automatic motion.
pub(crate) fn step(g: &G, w: &mut World, id: InstId) {
    fire(g, w, id, &EventKind::Step);
    let i = w.get_mut(id).unwrap();
    let (h, v) = (i.m.hspeed(), i.m.vspeed());
    let (x, y) = (i.m.x(), i.m.y());
    i.m.set_x(x + h).unwrap();
    i.m.set_y(y + v).unwrap();
}

pub(crate) fn b_game(p: &[(&str, ParamValue)]) -> G {
    let mut items = vec![("carry_rule", word("b_lines"))];
    items.extend_from_slice(p);
    let mut k = KindDef::new("plat", Builtin::MovingPlatform);
    k.params = params(&items);
    let mut tall = KindDef::new("tall", Builtin::Inert);
    tall.classes = ClassMask::KID;
    game(vec![k, tall])
}

/// One End Step, then the runner automatic motion (b_lines runs in End Step, not Step).
pub(crate) fn end_step(g: &G, w: &mut World, id: InstId) {
    fire(g, w, id, &EventKind::EndStep);
    let i = w.get_mut(id).unwrap();
    let (h, v) = (i.m.hspeed(), i.m.vspeed());
    let (x, y) = (i.m.x(), i.m.y());
    i.m.set_x(x + h).unwrap();
    i.m.set_y(y + v).unwrap();
}
