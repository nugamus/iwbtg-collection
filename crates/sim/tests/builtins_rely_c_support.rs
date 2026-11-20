//! Shared fixture for the rely slice's blood, blood_emitter, game_over, bow and block proximity-frame
//! tests: a small game with inert kinds standing in for the kid.

use iwbtg_sim::builtins::{self};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, ParamValue, Params, RoomDef, RoomId, RoomRole,
};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::tick;
use iwbtg_sim::world::{Ctx, Flow, InstId, SpawnInit, World};
/// The engine hooks without the sound model (WP10) and timing triggers (WP8b), which are
/// not implemented yet.
pub(crate) const HOOKS: tick::Hooks = tick::Hooks {
    sound_tick: |_| {},
    timing: |_, _| Ok(Vec::new()),
    ..tick::Hooks::ENGINE
};

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
    pub(crate) select: RoomId,
    pub(crate) kid: KindId,
    pub(crate) blood: KindId,
    pub(crate) emitter: KindId,
    pub(crate) block: KindId,
}

/// A game with a kid, blood, an emitter taking `emitter_params`, a block, and `extra` kinds.
pub(crate) fn game(emitter_params: Params, extra: impl FnOnce() -> Vec<KindDef>) -> G {
    let mut b = GameDefBuilder::new("wp6a-c");
    let spr32 = b.sprite_rect((32, 32), (0, 0));
    let spr4 = b.sprite_rect((4, 4), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Inert);
    k.classes = ClassMask::KID;
    k.sprite = Some(spr4);
    let kid = b.kind(k);
    let mut bl = KindDef::new("blood", Builtin::Blood);
    bl.sprite = Some(spr4);
    let blood = b.kind(bl);
    let mut em = KindDef::new("emitter", Builtin::BloodEmitter);
    em.params = emitter_params;
    let emitter = b.kind(em);
    let mut blk = KindDef::new("block", Builtin::Block);
    blk.solid = true;
    blk.classes = ClassMask::BLOCK;
    blk.sprite = Some(spr32);
    let block = b.kind(blk);
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::Blood, blood);
    b.engine_kind(EngineKind::BloodEmitter, emitter);
    for kd in extra() {
        b.kind(kd);
    }
    let room = b.room(RoomDef::new("r", 800, 608));
    let mut rs = RoomDef::new("sel", 800, 608);
    rs.role = RoomRole::Select;
    let select = b.room(rs);
    let mut def = b.build().unwrap();
    // The loader builds each kind's event set; do the same here.
    for k in &mut def.kinds {
        k.events = builtins::event_set(k.base, &k.clone());
    }
    G {
        def,
        room,
        select,
        kid,
        blood,
        emitter,
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

pub(crate) fn alive(w: &World, kind: KindId) -> Vec<InstId> {
    w.iter_alive().filter(|i| i.kind == kind).map(|i| i.id).collect()
}

pub(crate) fn count_after_ticks(g: &G, w: &mut World, ticks: u32) -> usize {
    for _ in 0..ticks {
        let rules = RunnerRules::default();
        let scripts = ScriptRuntime::default();
        let mut ctx = Ctx {
            def: &g.def,
            rules: &rules,
            w,
            scripts: &scripts,
            dll_mode: DllMode::Stub,
        };
        tick::run_tick_with(&mut ctx, InputFrame::EMPTY, &HOOKS).unwrap();
    }
    alive(w, g.blood).len()
}

pub(crate) fn over_game(p: Params) -> G {
    game(Params::default(), || {
        let mut k = KindDef::new("over", Builtin::GameOver);
        k.params = p;
        vec![k]
    })
}

pub(crate) fn bow_game(p: Params) -> G {
    game(Params::default(), || {
        let mut k = KindDef::new("bow", Builtin::Bow);
        k.persistent = true;
        k.params = p;
        vec![k]
    })
}

pub(crate) fn block02_game() -> G {
    game(Params::default(), || {
        let mut k = KindDef::new("block02", Builtin::Block);
        k.solid = true;
        k.visible = true;
        k.classes = ClassMask::BLOCK;
        k.params = params(&[("proximity_frame", yes())]);
        vec![k]
    })
}

pub(crate) fn frame(w: &World, id: InstId) -> Real {
    w.get(id).unwrap().img.index()
}

/// The block's frame after one Step with the kid's origin at (`kx`, `ky`); the block is at
/// (100, 200).
pub(crate) fn frame_with_kid_at(g: &G, kx: f64, ky: f64) -> Real {
    let mut w = World::new(g.room, 1);
    let blk = spawn(g, &mut w, kind_id(g, "block02"), 100.0, 200.0);
    spawn(g, &mut w, g.kid, kx, ky);
    fire(g, &mut w, blk, &EventKind::Step);
    frame(&w, blk)
}
