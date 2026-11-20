//! The world built-in's trigger clearing: Room Start clears the trigger ids 0..n and Room End
//! clears the switches only when the params ask.

use iwbtg_sim::builtins;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, KindDef, ParamValue, Params, RoomDef};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, SpawnInit, World};

fn run(p: &[(&str, ParamValue)], ev: &EventKind, setup: impl FnOnce(&mut World)) -> World {
    let mut params = Params::default();
    for (k, v) in p {
        params.0.insert((*k).to_owned(), v.clone());
    }
    let mut b = GameDefBuilder::new("wp6b-clear");
    let mut k = KindDef::new("world", Builtin::World);
    k.params = params;
    let kind = b.kind(k);
    let room = b.room(RoomDef::new("r", 800, 608));
    let mut def = b.build().unwrap();
    for k in &mut def.kinds {
        k.events = builtins::event_set(k.base, &k.clone());
    }
    let mut w = World::new(room, 1);
    let rules = RunnerRules::default();
    let scripts = ScriptRuntime::default();
    let mut ctx = Ctx {
        def: &def,
        rules: &rules,
        w: &mut w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    let me = ctx
        .spawn(kind, Real::ZERO, Real::ZERO, SpawnInit::default())
        .unwrap();
    setup(ctx.w);
    builtins::on_event(&mut ctx, me, ev).unwrap();
    w
}

fn seed(w: &mut World) {
    for i in [0, 5, 10, 11, 12] {
        w.engine.mech.trigger_flags.insert(i);
        w.engine.mech.stop_flags.insert(i);
    }
    w.engine.mech.switches.insert(3);
}

#[test]
fn room_start_clears_trigger_ids_zero_to_n_only() {
    let w = run(
        &[("clear_triggers_to", ParamValue::Real(Real::new(10.0)))],
        &EventKind::RoomStart,
        seed,
    );
    let m = &w.engine.mech;
    assert_eq!(m.trigger_flags.iter().copied().collect::<Vec<_>>(), vec![11, 12]);
    assert_eq!(m.stop_flags.len(), 5, "the stop array is not touched");
    assert!(m.switches.contains(&3));
}

#[test]
fn room_start_without_the_param_clears_nothing() {
    let w = run(&[], &EventKind::RoomStart, seed);
    assert_eq!(w.engine.mech.trigger_flags.len(), 5);
}

#[test]
fn room_end_clears_the_switches_when_asked() {
    let w = run(
        &[("clear_switches_on_room_end", ParamValue::Bool(true))],
        &EventKind::RoomEnd,
        seed,
    );
    assert!(w.engine.mech.switches.is_empty());
    assert_eq!(w.engine.mech.trigger_flags.len(), 5);
    let w = run(&[], &EventKind::RoomEnd, seed);
    assert!(!w.engine.mech.switches.is_empty());
}

#[test]
fn the_event_set_handles_room_end_only_with_the_param() {
    let mut k = KindDef::new("w", Builtin::World);
    assert!(!builtins::event_set(Builtin::World, &k).room_end);
    k.params
        .0
        .insert("clear_switches_on_room_end".into(), ParamValue::Bool(true));
    assert!(builtins::event_set(Builtin::World, &k).room_end);
}
