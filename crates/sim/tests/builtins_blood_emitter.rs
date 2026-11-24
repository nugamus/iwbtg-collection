//! blood_emitter: the draw order, the step count, the angle sweep, the client particles,
//! the restart key and the alarm.

/// The engine hooks without the sound model (WP10) and timing triggers (WP8b), which are
/// not implemented yet.
use crate::builtins_rely_c_support::*;
use iwbtg_sim::builtins::{self, BuiltinOutput};
use iwbtg_sim::def::{Builtin, KindDef, Params};
use iwbtg_sim::events::{EventKind, ProbeAct, ProbeOn, ProbeState, ProbeStep, probe_event_set};
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::rng::Rng;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::tick;
use iwbtg_sim::world::{Ctx, KindState, World};

#[test]
fn emitter_draw_order_is_frame_gravity_direction_speed_per_drop() {
    let g = game(params(&[("per_step", real(3.0))]), Vec::new);
    let mut w = World::new(g.room, 99);
    let em = spawn(&g, &mut w, g.emitter, 350.0, 66.4);
    assert_eq!(w.get(em).unwrap().alarms[0], r(20.0));
    let mut mirror = Rng::new(99);
    let mut expected = Vec::new();
    for _ in 0..3 {
        let frame = mirror.choose(3).unwrap();
        let gravity = r(0.1) + mirror.random(r(0.2));
        let dir = mirror.irandom(r(35.0)).unwrap() * r(10.0);
        let speed = mirror.random(r(6.0));
        expected.push((frame, gravity, dir, speed));
    }
    fire(&g, &mut w, em, &EventKind::Step);
    let drops = alive(&w, g.blood);
    assert_eq!(drops.len(), 3);
    for (id, (frame, gravity, dir, speed)) in drops.into_iter().zip(expected) {
        let i = w.get(id).unwrap();
        assert_eq!((i.m.x(), i.m.y()), (r(350.0), r(66.4)));
        assert_eq!(i.img.index(), r(f64::from(frame)));
        assert_eq!(i.m.gravity(), gravity);
        assert_eq!((i.m.direction(), i.m.speed()), (dir, speed));
    }
    assert_eq!(w.rng, mirror, "3 drops use 4 draws each");
    // rely draws 40 * 4 = 160 here (and 2 more from the runner's own particles, not modeled).
    let g = game(Params::default(), Vec::new);
    let mut w = World::new(g.room, 99);
    let em = spawn(&g, &mut w, g.emitter, 0.0, 0.0);
    fire(&g, &mut w, em, &EventKind::Step);
    assert_eq!(alive(&w, g.blood).len(), 40);
    let mut m = Rng::new(99);
    for _ in 0..160 {
        m.next_unit();
    }
    assert_eq!(w.rng, m);
}

#[test]
fn emitter_runs_19_steps_from_a_collision_phase_and_20_from_a_key_press() {
    let em_params = params(&[("per_step", real(1.0))]);
    // Made between ticks (as in a collision event of the previous tick): its Step starts
    // next tick, after the alarm, so it gets 19 Steps.
    let g = game(em_params.clone(), Vec::new);
    let mut w = World::new(g.room, 1);
    spawn(&g, &mut w, g.emitter, 400.0, 300.0);
    assert_eq!(count_after_ticks(&g, &mut w, 30), 19);
    assert!(alive(&w, g.emitter).is_empty());
    // Made in a Key press event: it runs the creation tick's Step too: 20.
    let g = game(em_params, || {
        let mut k = KindDef::new("killer", Builtin::Probe);
        k.events = probe_event_set();
        vec![k]
    });
    let mut w = World::new(g.room, 1);
    let killer = spawn(&g, &mut w, kind_id(&g, "killer"), 0.0, 0.0);
    w.get_mut(killer).unwrap().state = KindState::Probe(ProbeState::new("K", None).with(ProbeStep::on(
        ProbeOn::KeyPress,
        ProbeAct::Spawn {
            kind: g.emitter,
            x: r(400.0),
            y: r(300.0),
            child: None,
        },
    )));
    let rules = RunnerRules::default();
    let scripts = ScriptRuntime::default();
    {
        let mut ctx = Ctx {
            def: &g.def,
            rules: &rules,
            w: &mut w,
            scripts: &scripts,
            dll_mode: DllMode::Stub,
        };
        let bit = Action::Suicide.bit();
        tick::run_tick_with(
            &mut ctx,
            InputFrame {
                held: bit,
                pressed: bit,
                released: bit,
            },
            &HOOKS,
        )
        .unwrap();
    }
    assert_eq!(
        alive(&w, g.blood).len(),
        1,
        "the creation tick already ran a Step"
    );
    assert_eq!(count_after_ticks(&g, &mut w, 30), 20);
}

#[test]
fn emitter_angle_sweep_uses_the_chance_per_angle() {
    let sweep = |chance: f64| {
        params(&[
            ("mode", word("angle_sweep")),
            ("chance", real(chance)),
            ("per_angle", real(2.0)),
            ("speed_max", real(15.0)),
        ])
    };
    let g = game(sweep(1.0), Vec::new);
    let mut w = World::new(g.room, 3);
    let em = spawn(&g, &mut w, g.emitter, 0.0, 0.0);
    fire(&g, &mut w, em, &EventKind::Step);
    assert_eq!(alive(&w, g.blood).len(), 37 * 2, "angles 0, 10, ..., 360");
    let dirs: Vec<f64> = alive(&w, g.blood)
        .iter()
        .map(|id| w.get(*id).unwrap().m.direction().get())
        .collect();
    assert!(dirs.contains(&0.0) && dirs.iter().all(|d| *d < 360.0 + 1e-9));
    let g = game(sweep(0.0), Vec::new);
    let mut w = World::new(g.room, 3);
    let em = spawn(&g, &mut w, g.emitter, 0.0, 0.0);
    fire(&g, &mut w, em, &EventKind::Step);
    assert!(alive(&w, g.blood).is_empty());
}

#[test]
fn emitter_particles_go_to_the_client_and_draw_nothing() {
    let g = game(
        params(&[("mode", word("particles")), ("flipped", yes())]),
        Vec::new,
    );
    let mut w = World::new(g.room, 5);
    let em = spawn(&g, &mut w, g.emitter, 49.0, 519.1);
    fire(&g, &mut w, em, &EventKind::Step);
    assert!(alive(&w, g.blood).is_empty());
    assert_eq!(w.rng, Rng::new(5), "the Step draws nothing");
    assert!(
        w.output
            .contains(&OutputEvent::Builtin(BuiltinOutput::DeathParticles {
                x: r(49.0),
                y: r(519.1),
                count: 15,
                flipped: true
            }))
    );
    // The stream draws in the tick's particle update (#766, tests
    // `particles_rng::blood_emitter::*`), with the switch on (the default) or off; the Step
    // never draws.
    let mut rules = RunnerRules::default();
    rules.num.particles_draw_rng = false;
    let scripts = ScriptRuntime::default();
    let mut ctx = Ctx {
        def: &g.def,
        rules: &rules,
        w: &mut w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    builtins::on_event(&mut ctx, em, &EventKind::Step).unwrap();
    assert_eq!(ctx.w.rng, Rng::new(5));
}

#[test]
fn emitter_restart_key_destroys_it_only_when_asked() {
    let g = game(params(&[("destroyed_by_restart", yes())]), Vec::new);
    let set = builtins::event_set(
        Builtin::BloodEmitter,
        &g.def.kinds[usize::try_from(g.emitter.0).unwrap()],
    );
    assert_eq!(set.key_press, Action::Restart.bit());
    let mut w = World::new(g.room, 1);
    let em = spawn(&g, &mut w, g.emitter, 0.0, 0.0);
    fire(&g, &mut w, em, &EventKind::KeyPress(Action::Restart));
    assert!(!w.get(em).unwrap().alive);
    let g = game(Params::default(), Vec::new);
    let set = builtins::event_set(
        Builtin::BloodEmitter,
        &g.def.kinds[usize::try_from(g.emitter.0).unwrap()],
    );
    assert_eq!(set.key_press, 0);
}

#[test]
fn emitter_alarm_destroys_it() {
    let g = game(Params::default(), Vec::new);
    let mut w = World::new(g.room, 1);
    let em = spawn(&g, &mut w, g.emitter, 0.0, 0.0);
    fire(&g, &mut w, em, &EventKind::Alarm(0));
    assert!(!w.get(em).unwrap().alive);
}
