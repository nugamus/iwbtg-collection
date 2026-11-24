//! #309: the water built-in's `djump_ignoring_flag`, `cur_jumps` and `release_same_tick` params,
//! on a real kid.

use iwbtg_sim::builtins;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, GameDef, KindDef, KindId, ParamValue, Params, RoomDef};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::KidState;
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, KindState, SpawnInit, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

fn game() -> (GameDef, KindId, KindId) {
    let mut b = GameDefBuilder::new("wp5c-water");
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let water = b.kind(KindDef::new("water", Builtin::Water));
    let room = b.room(RoomDef::new("r", 800, 608));
    b.start_room(room);
    b.physics(iwbtg_sim::kid::presets::defaults(iwbtg_sim::def::KidFamily::A1));
    (b.build().expect("game builds"), kid, water)
}

fn run(params: &[(&str, ParamValue)], jump: (bool, bool), f: impl FnOnce(&mut Ctx, InstId)) {
    let (def, kid, water) = game();
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
    let mut p = Params::default();
    for (k, v) in params {
        p.0.insert((*k).to_owned(), v.clone());
    }
    let wi = ctx
        .spawn(
            water,
            r(0.0),
            r(0.0),
            SpawnInit {
                params: p,
                ..SpawnInit::default()
            },
        )
        .unwrap();
    let k = ctx.spawn(kid, r(0.0), r(0.0), SpawnInit::default()).unwrap();
    let bit = Action::Jump.bit();
    ctx.w.input.latch(InputFrame {
        held: 0,
        pressed: if jump.0 { bit } else { 0 },
        released: if jump.1 { bit } else { 0 },
    });
    builtins::on_event(
        &mut ctx,
        wi,
        &EventKind::Collision {
            with: ClassMask::KID,
            other: k,
        },
    )
    .unwrap();
    f(&mut ctx, k);
}

fn state(ctx: &Ctx, k: InstId) -> KidState {
    match &ctx.w.get(k).unwrap().state {
        KindState::Kid(s) => (**s).clone(),
        _ => unreachable!(),
    }
}

fn word(s: &str) -> ParamValue {
    ParamValue::Str(s.to_owned())
}

#[test]
fn djump_ignoring_flag_jumps_with_the_flag_set_and_clears_it() {
    let p = [
        ("air_jump", word("djump_ignoring_flag")),
        ("refresh", word("on_touch")),
    ];
    run(&p, (true, false), |ctx, k| {
        assert_eq!(ctx.w.get(k).unwrap().m.vspeed(), r(-7.0));
        assert!(!state(ctx, k).djump);
    });
    run(&p, (false, false), |ctx, k| {
        assert_eq!(ctx.w.get(k).unwrap().m.vspeed(), r(0.0), "no press, no jump");
        assert!(state(ctx, k).djump, "the refresh stays");
    });
}

#[test]
fn cur_jumps_is_set_on_touch() {
    run(
        &[("cur_jumps", ParamValue::Real(r(2.0)))],
        (false, false),
        |ctx, k| {
            assert_eq!(state(ctx, k).cur_jumps, 2);
        },
    );
}

#[test]
fn release_same_tick_cuts_the_jump_only_on_a_release() {
    let p = [
        ("release_same_tick", ParamValue::Bool(true)),
        ("fall_cap", ParamValue::Real(r(-1.0))),
    ];
    for (released, cut) in [(true, true), (false, false)] {
        let (def, kid, water) = game();
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
        let mut params = Params::default();
        for (k, v) in &p {
            params.0.insert((*k).to_owned(), v.clone());
        }
        let wi = ctx
            .spawn(
                water,
                r(0.0),
                r(0.0),
                SpawnInit {
                    params,
                    ..SpawnInit::default()
                },
            )
            .unwrap();
        let k = ctx.spawn(kid, r(0.0), r(0.0), SpawnInit::default()).unwrap();
        ctx.w.get_mut(k).unwrap().m.set_vspeed(r(-8.0)).unwrap();
        ctx.w.input.latch(InputFrame {
            held: 0,
            pressed: 0,
            released: if released { Action::Jump.bit() } else { 0 },
        });
        builtins::on_event(
            &mut ctx,
            wi,
            &EventKind::Collision {
                with: ClassMask::KID,
                other: k,
            },
        )
        .unwrap();
        let mult = ctx.def.physics.numbers.release_mult;
        let want = if cut { r(-8.0) * mult } else { r(-8.0) };
        assert_eq!(ctx.w.get(k).unwrap().m.vspeed(), want, "released {released}");
    }
}
