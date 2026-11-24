//! Shared fixture for the rely slice's block, spike, kid_start, world-event and kid_bullet tests: a
//! small game with inert kinds standing in for the kid and the world.

use iwbtg_sim::builtins;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, ParamValue, Params, RoomDef, RoomId,
};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::{Action, InputFrame};
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
    pub(crate) block: KindId,
}

/// A game with kid kinds, a 32x32 block kind and any extra kinds `extra` adds.
pub(crate) fn game(
    extra: impl FnOnce(&mut GameDefBuilder, KindId) -> Vec<KindDef>,
    edit: impl FnOnce(&mut GameDef),
) -> G {
    let mut b = GameDefBuilder::new("wp6a");
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
    let mut bl = KindDef::new("block", Builtin::Block);
    bl.solid = true;
    bl.classes = ClassMask::BLOCK;
    bl.sprite = Some(spr32);
    let block = b.kind(bl);
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::KidFlipped, flipped);
    for kd in extra(&mut b, kid) {
        b.kind(kd);
    }
    let room = b.room(RoomDef::new("r", 800, 608));
    let mut def = b.build().unwrap();
    edit(&mut def);
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

pub(crate) fn kind_of<'a>(g: &'a G, name: &str) -> &'a KindDef {
    g.def.kinds.iter().find(|k| k.name == name).unwrap()
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

pub(crate) fn alive(w: &World, kind: KindId) -> Vec<InstId> {
    w.iter_alive().filter(|i| i.kind == kind).map(|i| i.id).collect()
}

pub(crate) fn pos(w: &World, id: InstId) -> (f64, f64) {
    let i = w.get(id).unwrap();
    (i.m.x().get(), i.m.y().get())
}

pub(crate) fn spike_game() -> G {
    game(
        |_, _| {
            let mut s = KindDef::new("spike", Builtin::Spike);
            s.classes = ClassMask::KILLER;
            let mut s2 = KindDef::new("bulletspike", Builtin::Spike);
            s2.classes = ClassMask::KILLER;
            s2.params = params(&[("destroys_bullets", ParamValue::Bool(true))]);
            vec![s, s2]
        },
        |_| {},
    )
}

pub(crate) fn start_game_with(params_: Params) -> (G, World, InstId) {
    let g = game(
        |_, _| {
            let mut k = KindDef::new("start", Builtin::KidStart);
            k.params = params_;
            vec![k]
        },
        |_| {},
    );
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, kind_id(&g, "start"), 32.0, 544.0);
    (g, w, id)
}

pub(crate) fn world_game(p: Params, edit: impl FnOnce(&mut GameDef)) -> (G, World, InstId) {
    let g = game(
        |_, _| {
            let mut k = KindDef::new("world", Builtin::World);
            k.persistent = true;
            k.params = p;
            vec![k]
        },
        edit,
    );
    let mut w = World::new(g.room, 1);
    let id = spawn(&g, &mut w, kind_id(&g, "world"), 0.0, 0.0);
    (g, w, id)
}

pub(crate) fn frame(w: &mut World, held: &[Action], pressed: &[Action]) {
    let bits = |a: &[Action]| a.iter().fold(0u32, |acc, x| acc | x.bit());
    w.input.latch(InputFrame {
        held: bits(held),
        pressed: bits(pressed),
        released: 0,
    });
}

pub(crate) fn bullet_game(check: Option<&str>) -> G {
    game(
        |b, _| {
            let spr = b.sprite_rect((4, 4), (0, 0));
            let mut k = KindDef::new("bullet", Builtin::KidBullet);
            k.classes = ClassMask::KID_BULLET;
            k.sprite = Some(spr);
            if let Some(c) = check {
                k.params = params(&[("block_check", word(c))]);
            }
            let spr32 = b.sprite_rect((32, 32), (0, 0));
            let mut plat = KindDef::new("plat", Builtin::Inert);
            plat.solid = false;
            plat.classes = ClassMask::BLOCK | ClassMask::PLATFORM;
            plat.sprite = Some(spr32);
            plat.visible = false;
            vec![k, plat]
        },
        |_| {},
    )
}

/// A bullet at (x, y) moving `h` a tick, as the shooter leaves it after Create.
pub(crate) fn shoot(g: &G, w: &mut World, x: f64, y: f64, h: f64) -> InstId {
    let id = spawn(g, w, kind_id(g, "bullet"), x, y);
    w.get_mut(id).unwrap().m.set_hspeed(r(h)).unwrap();
    id
}
