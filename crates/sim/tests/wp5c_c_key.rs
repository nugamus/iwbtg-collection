//! #295: family C's key C (kid/C.md "Key C pressed"): the kid moves to the top-left of the start
//! marker, keeping its speeds and `djump`; the C preset lists it under the `extra1` key.

use iwbtg_sim::def::physics::KeyOn;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, KidFamily, KindDef, RoomDef};
use iwbtg_sim::input::Action;
use iwbtg_sim::kid::actions::c::CAction;
use iwbtg_sim::kid::{self, KidAction, presets};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, SpawnInit, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

#[test]
fn the_c_preset_lists_key_c_as_extra1_press() {
    let p = presets::defaults(KidFamily::C);
    assert!(p.events.keys.iter().any(|k| k.action == Action::Extra1
        && k.on == KeyOn::Press
        && k.run == vec![KidAction::C(CAction::TeleportToStart)]));
}

#[test]
fn key_c_moves_the_kid_to_the_marker_and_keeps_its_speeds() {
    for (with_marker, want) in [(true, (544.0, 544.0)), (false, (100.0, 200.0))] {
        let mut b = GameDefBuilder::new("wp5c-ckey");
        let mut k = KindDef::new("kid", Builtin::Kid);
        k.classes = ClassMask::KID;
        let kid = b.kind(k);
        let start = b.kind(KindDef::new("start", Builtin::KidStart));
        let room = b.room(RoomDef::new("r", 800, 608));
        b.start_room(room);
        b.physics(presets::defaults(KidFamily::C));
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
        if with_marker {
            let mut init = SpawnInit::default();
            init.params.0.insert(
                "shift_at".into(),
                iwbtg_sim::def::ParamValue::Str("room_start".into()),
            );
            ctx.spawn(start, r(544.0), r(544.0), init).unwrap();
        }
        let me = ctx.spawn(kid, r(100.0), r(200.0), SpawnInit::default()).unwrap();
        {
            let m = &mut ctx.w.get_mut(me).unwrap().m;
            m.set_hspeed(r(3.0)).unwrap();
            m.set_vspeed(r(-2.0)).unwrap();
        }
        kid::run_action(&mut ctx, me, &KidAction::C(CAction::TeleportToStart)).unwrap();
        let m = &ctx.w.get(me).unwrap().m;
        assert_eq!((m.x().get(), m.y().get()), want, "marker {with_marker}");
        assert_eq!((m.hspeed().get(), m.vspeed().get()), (3.0, -2.0));
    }
}
