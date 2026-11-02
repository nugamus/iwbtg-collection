//! #232: the A1 preset's Step list gets gravity back after a landing without a ladder action.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, KidFamily, KindDef, RoomDef};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::{self, KidAction, presets};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, KindState, SpawnInit, World};

#[test]
fn the_a1_step_list_has_no_ladder_and_starts_with_gravity() {
    let p = presets::defaults(KidFamily::A1);
    assert_eq!(
        p.events.step.first(),
        Some(&KidAction::Common(CommonAction::Gravity))
    );
    assert_eq!(
        p.events
            .step
            .iter()
            .filter(|a| **a == KidAction::Common(CommonAction::Gravity))
            .count(),
        1
    );
}

#[test]
fn a_landing_zeroes_gravity_and_the_next_step_restores_it() {
    let mut b = GameDefBuilder::new("wp5a-gravity");
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut p = presets::defaults(KidFamily::A1);
    // Only the part of Step this test is about: no input, no collision world.
    p.events.create = vec![KidAction::Common(CommonAction::CreateDefaults)];
    p.events.step = vec![KidAction::Common(CommonAction::Gravity)];
    b.physics(p);
    let def = b.build().expect("game builds");
    let rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("rules");
    let mut w = World::new(def.start_room, 1);
    let scripts = ScriptRuntime::default();
    let mut ctx = Ctx {
        def: &def,
        rules: &rules,
        w: &mut w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    let id = ctx
        .spawn(kid, Real::new(0.0), Real::new(0.0), SpawnInit::default())
        .unwrap();
    assert_eq!(
        ctx.w.get(id).unwrap().m.gravity(),
        Real::new(0.4),
        "Create sets it"
    );
    // block_standard's landing sets gravity to 0.
    ctx.w.get_mut(id).unwrap().m.set_gravity(Real::ZERO).unwrap();
    kid::on_event(&mut ctx, id, &EventKind::Step).unwrap();
    assert_eq!(
        ctx.w.get(id).unwrap().m.gravity(),
        Real::new(0.4),
        "the next Step restores it"
    );
    // On a ladder the reset is skipped (the ladder action owns gravity then).
    if let KindState::Kid(s) = &mut ctx.w.get_mut(id).unwrap().state {
        s.ladder = true;
    }
    ctx.w.get_mut(id).unwrap().m.set_gravity(Real::ZERO).unwrap();
    kid::on_event(&mut ctx, id, &EventKind::Step).unwrap();
    assert_eq!(ctx.w.get(id).unwrap().m.gravity(), Real::ZERO);
}
