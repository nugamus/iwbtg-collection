//! Shared fixture for the time_limit, sign, ladder and decor tests: a small game with inert kinds
//! standing in for the kid.

use iwbtg_sim::builtins;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, GameDef, KindDef, KindId, ParamValue, Params, RoomDef};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::num::Real;
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, KindState, SpawnInit, World};

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

/// A game with a kid (class KID, 20 x 32) and `extra` kinds (32 x 32 sprites).
pub(crate) fn game(extra: Vec<KindDef>) -> G {
    let mut b = GameDefBuilder::new("wp6c-3");
    let spr = b.sprite_rect((32, 32), (0, 0));
    let tall = b.sprite_rect((20, 32), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.classes = ClassMask::KID;
    k.sprite = Some(tall);
    let kid = b.kind(k);
    b.engine_kind(iwbtg_sim::def::EngineKind::Kid, kid);
    for mut kd in extra {
        kd.sprite = Some(spr);
        b.kind(kd);
    }
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut def = b.build().unwrap();
    for k in &mut def.kinds {
        k.events = builtins::event_set(k.base, &k.clone());
    }
    let mut w = World::new(room, 1);
    w.room_speed = r(50.0);
    G { def, w }
}

pub(crate) fn kind_of(name: &str, base: Builtin, p: Params) -> KindDef {
    let mut k = KindDef::new(name, base);
    k.params = p;
    k
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

pub(crate) fn touch(g: &mut G, id: InstId, other: InstId, class: ClassMask) {
    fire(g, id, &EventKind::Collision { with: class, other });
}

pub(crate) fn alive(g: &G, id: InstId) -> bool {
    g.w.get(id).is_some_and(|i| i.alive)
}

pub(crate) fn left(g: &G, id: InstId) -> (i64, i64, i64) {
    match &g.w.get(id).unwrap().state {
        KindState::Builtin(builtins::BuiltinState::TimeLimit(t)) => (t.hours, t.minutes, t.seconds),
        _ => panic!("not a time limit"),
    }
}

pub(crate) fn timer_game(p: &[(&str, ParamValue)]) -> G {
    let mut tp = vec![("role", word("timer"))];
    tp.extend_from_slice(p);
    game(vec![
        kind_of("timer", Builtin::TimeLimit, params(&tp)),
        kind_of(
            "zone",
            Builtin::TimeLimit,
            params(&[("role", word("start_zone"))]),
        ),
    ])
}

/// One alarm period: alarm 0 fires, then Step.
pub(crate) fn second(g: &mut G, t: InstId) {
    fire(g, t, &EventKind::Alarm(0));
    if alive(g, t) {
        fire(g, t, &EventKind::Step);
    }
}

pub(crate) fn texts(g: &mut G) -> Vec<String> {
    g.w.output
        .drain(..)
        .filter_map(|e| match e {
            OutputEvent::Builtin(builtins::BuiltinOutput::TextBox { text }) => Some(text),
            _ => None,
        })
        .collect()
}

pub(crate) fn up_pressed(g: &mut G) {
    let bit = Action::Up.bit();
    g.w.input.latch(InputFrame {
        held: bit,
        pressed: bit,
        released: 0,
    });
}
