//! WP5a.1: kid state, globals and `kid::api` on hand-built worlds (docs/plans/sim-core.md 1.6).

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, GameDef, KindDef, KindId, ParamValue, Params, RoomDef};
use iwbtg_sim::error::SimError;
use iwbtg_sim::kid::api::{self, KidField, KillCause, ParamZone};
use iwbtg_sim::kid::{KidGlobals, KidState};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{InstId, KindState, SpawnInit, Value};
use iwbtg_sim::{Ctx, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

fn game() -> (GameDef, KindId, KindId) {
    let mut b = GameDefBuilder::new("wp5a-api");
    let kid = b.kind(KindDef::new("kid", Builtin::Kid));
    let block = b.kind(KindDef::new("block", Builtin::Inert));
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    let mut p = iwbtg_sim::def::PhysicsProfile::default();
    p.death.order = vec![iwbtg_sim::def::physics::DeathStep::DestroyKid];
    b.physics(p);
    (b.build().expect("game builds"), kid, block)
}

fn with_world<T>(f: impl FnOnce(&mut Ctx, InstId, InstId) -> T) -> T {
    let (def, kid, block) = game();
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
    let k = ctx
        .spawn(kid, r(100.0), r(200.0), SpawnInit::default())
        .expect("kid");
    let b = ctx
        .spawn(block, r(0.0), r(0.0), SpawnInit::default())
        .expect("block");
    f(&mut ctx, k, b)
}

fn kid_state(ctx: &Ctx, id: InstId) -> KidState {
    match &ctx.w.get(id).expect("kid").state {
        KindState::Kid(s) => (**s).clone(),
        other => panic!("not a kid: {other:?}"),
    }
}

fn field(name: &str, index: Option<u32>) -> KidField {
    KidField {
        name: name.into(),
        index,
    }
}

fn zone(items: &[(&str, ParamValue)]) -> ParamZone {
    ParamZone {
        params: Params(items.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()),
    }
}

#[test]
fn defaults() {
    let s = KidState::default();
    assert_eq!(s.facing, 1);
    assert_eq!(s.yflag, 1);
    assert!(!s.djump && !s.on_platform && !s.frozen && !s.invincible);
    let g = KidGlobals::default();
    assert!(!g.grav && !g.frozen && !g.frozen2);
    assert_eq!(g.image_xscale, 0);
}

#[test]
fn facing_and_non_kid() {
    with_world(|ctx, k, b| {
        assert_eq!(api::facing(ctx.w, k), 1);
        match &mut ctx.w.get_mut(k).unwrap().state {
            KindState::Kid(s) => s.facing = -1,
            _ => unreachable!(),
        }
        assert_eq!(api::facing(ctx.w, k), -1);
        assert_eq!(api::facing(ctx.w, b), 1);
    });
}

#[test]
fn refresh_and_platform_flag() {
    with_world(|ctx, k, b| {
        assert!(!kid_state(ctx, k).djump);
        api::refresh_air_jump(ctx, k);
        let s = kid_state(ctx, k);
        assert!(s.djump);
        assert_eq!(s.cur_jumps, 1);
        api::set_on_platform(ctx, k, true);
        assert!(kid_state(ctx, k).on_platform);
        api::set_on_platform(ctx, k, false);
        assert!(!kid_state(ctx, k).on_platform);
        // Not a kid: nothing happens, nothing panics.
        api::refresh_air_jump(ctx, b);
        api::set_on_platform(ctx, b, true);
    });
}

#[test]
fn param_zone_writes_numbers() {
    with_world(|ctx, k, _| {
        let z = zone(&[
            ("run_speed", ParamValue::Real(r(5.0))),
            ("maxVspeed", ParamValue::Real(r(30.0))),
            ("max_fall_speed", ParamValue::Real(r(1.0))),
            ("jump", ParamValue::Real(r(16.0))),
            ("jump2", ParamValue::Real(r(13.0))),
            ("jump3", ParamValue::Real(r(6.0))),
        ]);
        api::apply_param_zone(ctx, k, &z).expect("applies");
        let s = kid_state(ctx, k);
        assert_eq!(s.run_speed, r(5.0));
        assert_eq!(s.max_vspeed, r(30.0));
        assert_eq!(s.max_fall_speed, r(1.0));
        assert_eq!(s.jump_speeds, vec![r(16.0), r(13.0), r(6.0)]);
        assert_eq!(
            api::read_field(ctx.w, k, &field("jump", Some(2))),
            Some(Value::Real(r(13.0)))
        );
        assert_eq!(
            api::read_field(ctx.w, k, &field("maxSpeed", None)),
            Some(Value::Real(r(5.0)))
        );
    });
}

#[test]
fn param_zone_errors_apply_nothing() {
    with_world(|ctx, k, b| {
        let bad_name = zone(&[
            ("run_speed", ParamValue::Real(r(5.0))),
            ("gravity_x", ParamValue::Real(r(1.0))),
        ]);
        assert!(matches!(
            api::apply_param_zone(ctx, k, &bad_name),
            Err(SimError::Unsupported { .. })
        ));
        let nan = zone(&[("run_speed", ParamValue::Real(r(f64::NAN)))]);
        assert!(matches!(
            api::apply_param_zone(ctx, k, &nan),
            Err(SimError::NonFinite { .. })
        ));
        let text = zone(&[("run_speed", ParamValue::Str("x".into()))]);
        assert!(matches!(
            api::apply_param_zone(ctx, k, &text),
            Err(SimError::Unsupported { .. })
        ));
        assert_eq!(kid_state(ctx, k).run_speed, Real::ZERO, "nothing applied");
        let ok = zone(&[("run_speed", ParamValue::Real(r(5.0)))]);
        assert!(matches!(
            api::apply_param_zone(ctx, b, &ok),
            Err(SimError::Invariant { .. })
        ));
    });
}

#[test]
fn bow_snapshot_copies_position_and_image() {
    with_world(|ctx, k, _| {
        let i = ctx.w.get_mut(k).unwrap();
        i.m.set_x(r(12.5)).unwrap();
        i.m.set_y(r(-3.0)).unwrap();
        i.img.set_xscale(r(-1.0)).unwrap();
        i.img.set_alpha(r(0.3)).unwrap();
        let s = api::bow_snapshot(ctx.w, k);
        assert_eq!(
            (s.x, s.y, s.xscale, s.yscale, s.alpha),
            (r(12.5), r(-3.0), r(-1.0), r(1.0), r(0.3))
        );
    });
}

#[test]
fn read_field_names() {
    with_world(|ctx, k, b| {
        match &mut ctx.w.get_mut(k).unwrap().state {
            KindState::Kid(s) => {
                s.djump = true;
                s.cur_jumps = 2;
                s.yflag = -1;
                s.jump_speeds = vec![r(8.5), r(7.0)];
            }
            _ => unreachable!(),
        }
        let get = |n, i| api::read_field(ctx.w, k, &field(n, i));
        assert_eq!(get("djump", None), Some(Value::Real(Real::ONE)));
        assert_eq!(get("onPlatform", None), Some(Value::Real(Real::ZERO)));
        assert_eq!(get("curJumps", None), Some(Value::Real(r(2.0))));
        assert_eq!(get("yflag", None), Some(Value::Real(r(-1.0))));
        assert_eq!(get("jump", None), Some(Value::Real(r(8.5))));
        assert_eq!(
            get("jump", Some(1)),
            Some(Value::Real(r(8.5))),
            "1-based like B's array"
        );
        assert_eq!(get("jump", Some(2)), Some(Value::Real(r(7.0))));
        assert_eq!(get("jump2", None), Some(Value::Real(r(7.0))));
        assert_eq!(get("jump", Some(0)), None);
        assert_eq!(get("jump", Some(3)), None);
        assert_eq!(get("djump", Some(0)), None);
        assert_eq!(get("nosuch", None), None);
        assert_eq!(api::read_field(ctx.w, b, &field("djump", None)), None);
    });
}

#[test]
fn kill_destroys_the_kid_unless_invincible() {
    with_world(|ctx, k, b| {
        match &mut ctx.w.get_mut(k).unwrap().state {
            KindState::Kid(s) => s.invincible = true,
            _ => unreachable!(),
        }
        api::kill(ctx, k, KillCause::Script).expect("kill");
        assert!(ctx.w.get(k).unwrap().alive, "muteki ignores the kill");
        match &mut ctx.w.get_mut(k).unwrap().state {
            KindState::Kid(s) => s.invincible = false,
            _ => unreachable!(),
        }
        api::kill(ctx, k, KillCause::Suicide).expect("kill");
        assert!(!ctx.w.get(k).unwrap().alive);
        assert!(
            api::kill(ctx, b, KillCause::Script).is_err(),
            "a block is not a kid"
        );
    });
}

#[test]
fn djump_and_cur_jumps_setters() {
    with_world(|ctx, k, b| {
        api::refresh_air_jump(ctx, k);
        api::set_air_jump(ctx, k, false);
        let s = kid_state(ctx, k);
        assert!(!s.djump);
        assert_eq!(s.cur_jumps, 1, "the counter is not touched");
        api::set_cur_jumps(ctx, k, 2);
        assert_eq!(kid_state(ctx, k).cur_jumps, 2);
        // Not a kid: nothing happens.
        api::set_air_jump(ctx, b, true);
        api::set_cur_jumps(ctx, b, 2);
        api::set_triple_jump(ctx, b, true);
    });
}

#[test]
fn triple_jump_setter() {
    with_world(|ctx, k, _| {
        api::set_triple_jump(ctx, k, true);
        assert!(kid_state(ctx, k).tjump);
        api::set_triple_jump(ctx, k, false);
        assert!(!kid_state(ctx, k).tjump);
    });
}

#[test]
fn run_jump_release_cuts_an_upward_speed_and_ignores_a_fall() {
    with_world(|ctx, k, b| {
        let mult = ctx.def.physics.numbers.release_mult;
        ctx.w.get_mut(k).unwrap().m.set_vspeed(r(-8.0)).unwrap();
        api::run_jump_release(ctx, k).expect("cuts");
        assert_eq!(ctx.w.get(k).unwrap().m.vspeed(), r(-8.0) * mult);
        ctx.w.get_mut(k).unwrap().m.set_vspeed(r(3.0)).unwrap();
        api::run_jump_release(ctx, k).expect("no-op");
        assert_eq!(ctx.w.get(k).unwrap().m.vspeed(), r(3.0));
        assert!(api::run_jump_release(ctx, b).is_err(), "not a kid");
    });
}
