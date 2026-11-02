//! WP1: the tick pipeline, phase order, dispatch order, R8 and R9, halting, destroy, draw,
//! alarms, keys, the direct latch, boundary events, animation slots and timelines
//! (docs/plans/sim-core.md 2.1, 2.2, 12).

#[path = "wp1_support.rs"]
mod support;

use iwbtg_sim::def::{Builtin, ClassMask, KindDef, RoomId, TimelineDef, TimelineId};
use iwbtg_sim::events::{EventSet, ProbeAct, ProbeOn, ProbeState, ProbeStep, dispatch_order};
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::num::Real;
use iwbtg_sim::room::RoomRequest;
use iwbtg_sim::rules::tick::{
    AnimSlot, BoundarySlot, DirectLatch, DispatchOrder, JoinPhase, RoomRequestRule,
};
use iwbtg_sim::timeline;
use iwbtg_sim::world::{SimStatus, Value};
use iwbtg_sim::{SimError, tick};
use support::*;

fn step(on: ProbeOn, act: ProbeAct) -> ProbeStep {
    ProbeStep::on(on, act)
}

#[test]
fn wp1_one_tick_runs_every_phase_in_the_recorded_order() {
    let mut b = builder();
    let sprite = b.sprite_rect((16, 16), (8, 8));
    let mut ka = KindDef::new("a", Builtin::Probe);
    ka.events = iwbtg_sim::events::probe_event_set();
    ka.events.collisions.push(ClassMask::BLOCK);
    ka.sprite = Some(sprite);
    let ka = b.kind(ka);
    let mut kb = KindDef::new("b", Builtin::Probe);
    kb.events = iwbtg_sim::events::probe_event_set();
    kb.classes = ClassMask::BLOCK;
    let kb = b.kind(kb);
    b.def_mut().timelines.push(TimelineDef {
        name: "tl".into(),
        moments: vec![0],
        ..TimelineDef::default()
    });
    let mut fx = Fx::new(b.build().unwrap());
    // A crosses the right edge (box 787..803 in an 800 px room), has alarm 0 at 1, a timeline
    // at 0 and a path that ends this tick; B is a touching non-solid block.
    let a = fx.probe(ka, 795.0, 100.0, p("A"));
    let bb = fx.probe(kb, 100.0, 100.0, p("B"));
    {
        let inst = fx.w.get_mut(a).unwrap();
        inst.alarms[0] = Real::ONE;
        timeline::start(&mut inst.timeline, TimelineId(0), Real::ZERO).unwrap();
    }
    fx.touch(bb, true);
    fx.set(END_PATH, Value::Inst(a));
    let r = Action::Restart.bit();
    fx.tick(InputFrame {
        held: r,
        pressed: r,
        released: 0,
    });
    let expected = format!(
        "bA bB t0A a0A khrestartA kprestartA khrestartB kprestartB sA sB pA iA k{}A eA eB dA dB nA ",
        bb.0
    );
    assert_eq!(fx.log(), expected);
    assert_eq!(fx.w.tick, 1);
    assert_eq!(fx.w.room_tick, 1);
}

#[test]
fn wp1_dispatch_orders_follow_the_rule() {
    // t-ord-1 shape: world (rank 4) created first, player (rank 0), two blocks (rank 1), the
    // second with the lower depth.
    let mut b = builder();
    let kp = probe_kind(&mut b, "player");
    let kblock = probe_kind(&mut b, "block");
    let kw = probe_kind(&mut b, "world");
    b.order(&[kp, kblock, kw]);
    let def = b.build().unwrap();
    for (mode, want) in [
        (DispatchOrder::RankThenCreation, "sP sB1 sB2 sW "),
        (DispatchOrder::Creation, "sW sP sB1 sB2 "),
        (DispatchOrder::DepthThenRankThenCreation, "sB1 sB2 sP sW "),
    ] {
        let mut fx = Fx::new(def.clone());
        fx.rules.tick.dispatch = mode;
        let only_step = |tag: &str| {
            p(tag)
                .style(iwbtg_sim::events::ProbeLog::Silent)
                .with(step(ProbeOn::Step, ProbeAct::Log(format!("s{tag} "))))
        };
        fx.probe(kw, 0.0, 0.0, only_step("W"));
        fx.probe(kp, 0.0, 0.0, only_step("P"));
        let b1 = fx.probe(kblock, 0.0, 0.0, only_step("B1"));
        let b2 = fx.probe(kblock, 0.0, 0.0, only_step("B2"));
        fx.w.get_mut(b1).unwrap().depth = Real::new(100.0);
        fx.w.get_mut(b2).unwrap().depth = Real::new(50.0);
        fx.tick(InputFrame::EMPTY);
        assert_eq!(fx.log(), want, "{mode:?}");
        let order = dispatch_order(&fx.w, &fx.rules.tick, |_| true);
        assert_eq!(order.len(), 4);
    }
}

#[test]
fn wp1_new_instances_join_the_phase_per_r9() {
    let mut b = builder();
    let k0 = probe_kind(&mut b, "early");
    let k1 = probe_kind(&mut b, "creator");
    let k2 = probe_kind(&mut b, "late");
    let def = b.build().unwrap();
    let child = |tag: &str| Some(Box::new(p(tag)));
    for (join, steps_seen) in [
        (JoinPhase::Never, vec!["sX"]),
        (JoinPhase::LaterInOrder, vec!["sX", "sL"]),
        (JoinPhase::Always, vec!["sX", "sE", "sS", "sL"]),
    ] {
        let mut fx = Fx::new(def.clone());
        fx.rules.tick.new_instances_join_phase = join;
        let spawn = |kind, tag: &str| ProbeAct::Spawn {
            kind,
            x: Real::ZERO,
            y: Real::ZERO,
            child: child(tag),
        };
        fx.probe(
            k1,
            0.0,
            0.0,
            p("X")
                .with(step(ProbeOn::Step, spawn(k0, "E")).once())
                .with(step(ProbeOn::Step, spawn(k1, "S")).once())
                .with(step(ProbeOn::Step, spawn(k2, "L")).once()),
        );
        fx.tick(InputFrame::EMPTY);
        let log = fx.log();
        let steps: Vec<&str> = log.split(' ').filter(|t| t.starts_with('s')).collect();
        assert_eq!(steps, steps_seen, "{join:?}: {log}");
        // Every new instance takes part in the later phases of the tick (t-evo-1 row 3).
        for tag in ["E", "S", "L"] {
            assert!(log.contains(&format!("C{tag} ")), "{join:?}: Create of {tag}");
            assert!(log.contains(&format!("e{tag} ")), "{join:?}: End Step of {tag}");
            assert!(log.contains(&format!("d{tag} ")), "{join:?}: Draw of {tag}");
        }
        // Create ran inside the spawning Step, before the next instance's turn.
        assert!(log.find("CE ").unwrap() < log.find("CL ").unwrap());
    }
}

/// Two probes A (rank 0) and B (rank 1); A requests a room in Step and logs after it.
fn room_request_fixture(rule: RoomRequestRule, nested: bool) -> Fx {
    let mut b = builder();
    let ka = probe_kind(&mut b, "a");
    let kb = probe_kind(&mut b, "b");
    let kc = probe_kind(&mut b, "c");
    let mut fx = Fx::new(b.build().unwrap());
    fx.rules.tick.room_request = rule;
    let request = ProbeAct::RequestRoom(RoomRequest::Goto(RoomId(1)));
    let a = if nested {
        let child = p("C").with(step(ProbeOn::Create, request));
        p("A").with(step(
            ProbeOn::Step,
            ProbeAct::Spawn {
                kind: kc,
                x: Real::ZERO,
                y: Real::ZERO,
                child: Some(Box::new(child)),
            },
        ))
    } else {
        p("A").with(step(ProbeOn::Step, request))
    };
    fx.probe(
        ka,
        0.0,
        0.0,
        a.with(step(ProbeOn::Step, ProbeAct::Log("after ".into()))),
    );
    fx.probe(kb, 0.0, 0.0, p("B"));
    fx
}

#[test]
fn wp1_room_request_rules() {
    for nested in [false, true] {
        let mut fx = room_request_fixture(RoomRequestRule::AbortTick, nested);
        fx.tick(InputFrame::EMPTY);
        let log = fx.log();
        assert!(log.ends_with("after "), "the handler finishes: {log}");
        assert!(
            !log.contains("sB") && !log.contains("eA") && !log.contains("dA"),
            "{log}"
        );
        assert_eq!(fx.w.room, RoomId(1));
        assert_eq!(fx.w.globals.get(&ROOM_CHANGES), Some(&vr(1.0)));
        assert_eq!((fx.w.tick, fx.w.room_tick), (1, 0));
        assert_eq!(fx.w.pending, None);
        assert_eq!(fx.w.room_requests, 0);
    }
    let mut fx = room_request_fixture(RoomRequestRule::FinishPhase, false);
    fx.tick(InputFrame::EMPTY);
    let log = fx.log();
    assert!(log.contains("sB ") && !log.contains("eA "), "{log}");
    assert_eq!(fx.w.room, RoomId(1));
    let mut fx = room_request_fixture(RoomRequestRule::FinishTick, false);
    fx.tick(InputFrame::EMPTY);
    let log = fx.log();
    assert!(log.contains("eB ") && log.contains("dB "), "{log}");
    assert_eq!(fx.w.room, RoomId(1));
}

#[test]
fn wp1_last_room_request_wins() {
    let mut fx = Fx::new(builder().build().unwrap());
    let mut c = fx.ctx();
    c.request_room(RoomRequest::Goto(RoomId(1)));
    c.request_room(RoomRequest::Restart);
    assert_eq!(fx.w.pending, Some(RoomRequest::Restart));
    assert_eq!(fx.w.room_requests, 2);
}

#[test]
fn wp1_halt_freezes_the_tick() {
    let mut b = builder();
    let ka = probe_kind(&mut b, "a");
    let kb = probe_kind(&mut b, "b");
    let kc = probe_kind(&mut b, "c");
    let mut fx = Fx::new(b.build().unwrap());
    let a = fx.probe(
        ka,
        0.0,
        0.0,
        p("A").with(step(ProbeOn::BeginStep, ProbeAct::DestroySelf)),
    );
    fx.probe(
        kb,
        0.0,
        0.0,
        p("B").with(step(ProbeOn::Step, ProbeAct::Fail("boom".into()))),
    );
    fx.probe(kc, 0.0, 0.0, p("C"));
    fx.tick(InputFrame::EMPTY);
    assert_eq!(
        fx.w.status,
        SimStatus::Halted(SimError::Script {
            message: "boom".into()
        })
    );
    let log = fx.log();
    assert!(log.contains("sB ") && !log.contains("sC "), "{log}");
    assert_eq!(fx.w.tick, 0, "no tick += 1");
    assert!(fx.w.get(a).is_some(), "no sweep");
}

#[test]
fn wp1_destroy_runs_destroy_now_and_the_rest_of_the_event() {
    let mut b = builder();
    let ka = probe_kind(&mut b, "a");
    let kb = probe_kind(&mut b, "b");
    let mut fx = Fx::new(b.build().unwrap());
    let a = fx.probe(
        ka,
        0.0,
        0.0,
        p("A")
            .with(step(ProbeOn::Step, ProbeAct::DestroySelf))
            .with(step(ProbeOn::Step, ProbeAct::Log("rest ".into()))),
    );
    let bb = fx.probe(kb, 0.0, 0.0, p("B"));
    fx.tick(InputFrame::EMPTY);
    let log = fx.log();
    assert!(log.contains("sA DA rest sB "), "{log}");
    assert!(!log.contains("eA") && !log.contains("dA"), "{log}");
    assert!(fx.w.get(a).is_none(), "swept at row 14");
    let ids: Vec<_> = fx.w.instances().iter().map(|i| i.id).collect();
    assert_eq!(ids, vec![bb]);
    // Destroying an instance that is already gone is a no-op.
    assert_eq!(fx.ctx().destroy(a), iwbtg_sim::world::Flow::Continue);
}

#[test]
fn wp1_draw_slot_uses_depth_then_creation_and_skips_invisible() {
    let mut b = builder();
    let k = probe_kind(&mut b, "p");
    let mut fx = Fx::new(b.build().unwrap());
    let draw_only = |tag: &str| {
        p(tag)
            .style(iwbtg_sim::events::ProbeLog::Silent)
            .with(step(ProbeOn::Late, ProbeAct::Log(format!("d{tag} "))))
    };
    for (tag, depth, visible) in [
        ("Z1", 0.0, true),
        ("Deep", 100.0, true),
        ("Mid", 50.0, true),
        ("Hidden", 200.0, false),
        ("Z2", -0.0, true),
    ] {
        let id = fx.probe(k, 0.0, 0.0, draw_only(tag));
        let inst = fx.w.get_mut(id).unwrap();
        inst.depth = Real::new(depth);
        inst.visible = visible;
    }
    fx.tick(InputFrame::EMPTY);
    assert_eq!(fx.log(), "dDeep dMid dZ1 dZ2 ");
}

#[test]
fn wp1_alarms_count_fire_rearm_and_idle() {
    let mut b = builder();
    let k = probe_kind(&mut b, "p");
    let mut only0 = KindDef::new("only0", Builtin::Probe);
    only0.events = EventSet {
        alarms: 1,
        ..EventSet::default()
    };
    let only0 = b.kind(only0);
    let mut fx = Fx::new(b.build().unwrap());
    let a = fx.probe(
        k,
        0.0,
        0.0,
        p("A")
            .with(step(ProbeOn::Alarm(0), ProbeAct::SetAlarm(0, Real::new(3.0))))
            .with(step(ProbeOn::Alarm(0), ProbeAct::SetAlarm(1, Real::new(2.0)))),
    );
    let once = fx.probe(k, 0.0, 0.0, p("O"));
    let partial = fx.probe(only0, 0.0, 0.0, p("P"));
    fx.w.get_mut(a).unwrap().alarms[0] = Real::new(3.0);
    fx.w.get_mut(once).unwrap().alarms[0] = Real::ONE;
    fx.w.get_mut(partial).unwrap().alarms[5] = Real::new(7.0);
    let alarms = |fx: &Fx, id| {
        let i = fx.w.get(id).unwrap();
        (i.alarms[0].get(), i.alarms[1].get())
    };
    fx.tick(InputFrame::EMPTY);
    assert_eq!(alarms(&fx, a), (2.0, -1.0));
    assert_eq!(alarms(&fx, once).0, 0.0, "fired, reads 0");
    assert_eq!(
        tokens_of(&fx.take_log(), "O"),
        vec!["bO", "a0O", "sO", "eO", "dO"]
    );
    fx.tick(InputFrame::EMPTY);
    assert_eq!(alarms(&fx, a), (1.0, -1.0));
    assert_eq!(alarms(&fx, once).0, -1.0, "then -1, no second fire");
    fx.tick(InputFrame::EMPTY);
    // Fired, re-armed in its own event with no extra decrement; alarm 1 set by Alarm 0 counts
    // down in the same tick (num-eps-alpha row 45).
    assert_eq!(alarms(&fx, a), (3.0, 1.0));
    let log = fx.take_log();
    assert!(!log.contains("a0O"), "{log}");
    assert_eq!(
        tokens_of(&log, "A").iter().filter(|t| t.starts_with('a')).count(),
        1
    );
    fx.tick(InputFrame::EMPTY);
    assert_eq!(alarms(&fx, a), (2.0, 0.0));
    assert!(fx.take_log().contains("a1A "));
    assert_eq!(
        fx.w.get(partial).unwrap().alarms[5],
        Real::new(7.0),
        "unhandled alarms never count"
    );
}

#[test]
fn wp1_key_events_held_then_press_then_release_in_bit_order() {
    let mut b = builder();
    let k = probe_kind(&mut b, "p");
    let mut jump_only = KindDef::new("jump_only", Builtin::Probe);
    jump_only.events = EventSet {
        key_press: Action::Jump.bit(),
        ..EventSet::default()
    };
    let jump_only = b.kind(jump_only);
    let mut fx = Fx::new(b.build().unwrap());
    fx.probe(k, 0.0, 0.0, p("A"));
    fx.probe(jump_only, 0.0, 0.0, p("J"));
    let (l, j, s) = (Action::Left.bit(), Action::Jump.bit(), Action::Shoot.bit());
    fx.tick(InputFrame {
        held: l | j,
        pressed: j,
        released: s,
    });
    let log = fx.log();
    assert!(
        log.contains("khleftA khjumpA kpjumpA krshootA kpjumpJ sA "),
        "{log}"
    );
}

#[test]
fn wp1_direct_read_latch_follows_the_rule() {
    let mut b = builder();
    let k = probe_kind(&mut b, "p");
    let def = b.build().unwrap();
    let right = Action::Right.bit();
    for (latch, want) in [(DirectLatch::Latch, "D1 D0 "), (DirectLatch::None, "D0 D0 ")] {
        let mut fx = Fx::new(def.clone());
        fx.rules.tick.direct_latch = latch;
        fx.probe(
            k,
            0.0,
            0.0,
            ProbeState::new("", Some(LOG))
                .style(iwbtg_sim::events::ProbeLog::Silent)
                .with(step(ProbeOn::Step, ProbeAct::DirectRead(right)).at(1))
                .with(step(ProbeOn::Step, ProbeAct::DirectRead(right)).at(2)),
        );
        // Tick 0: a tap inside the tick, nobody reads; tick 1 reads it; tick 2 reads nothing.
        fx.tick(InputFrame {
            held: 0,
            pressed: right,
            released: right,
        });
        fx.idle(2);
        assert_eq!(fx.log(), want, "{latch:?}");
    }
}

#[test]
fn wp1_boundary_events_match_the_recorded_edge_rule() {
    let mut b = builder();
    let sprite = b.sprite_rect((16, 16), (8, 8));
    let mut k = KindDef::new("p", Builtin::Probe);
    k.events = iwbtg_sim::events::probe_event_set();
    k.sprite = Some(sprite);
    let k = b.kind(k);
    let def = b.build().unwrap();
    // Box = round(x) +- 8 in an 800 px room (t-evo-2 rows 16-29, t-evo-1 frames 14-35).
    for (x, want) in [
        (400.0, vec![]),
        (8.0, vec![]),             // bbox_left 0
        (792.0, vec![]),           // bbox_right 800: no Intersect Boundary
        (793.0, vec!["iA"]),       // bbox_right 801
        (808.0, vec!["iA"]),       // bbox_left 800: Intersect Boundary only
        (809.0, vec!["oA", "iA"]), // bbox_left 801: both, Outside Room first
        (7.0, vec!["iA"]),         // bbox_left -1
        (-8.0, vec!["iA"]),        // bbox_right 0
        (-9.0, vec!["oA", "iA"]),  // bbox_right -1
    ] {
        let mut fx = Fx::new(def.clone());
        fx.probe(k, x, 300.0, p("A"));
        fx.tick(InputFrame::EMPTY);
        let log = fx.log();
        let got: Vec<&str> = tokens_of(&log, "A")
            .into_iter()
            .filter(|t| *t == "oA" || *t == "iA")
            .collect();
        assert_eq!(got, want, "x = {x}");
    }
}

#[test]
fn wp1_boundary_and_animation_slots_move() {
    let mut b = builder();
    let sprite = b.sprite_rect((16, 16), (8, 8));
    let mut k = KindDef::new("p", Builtin::Probe);
    k.events = iwbtg_sim::events::probe_event_set();
    k.events.collisions.push(ClassMask::BLOCK);
    k.sprite = Some(sprite);
    k.classes = ClassMask::BLOCK;
    let k = b.kind(k);
    let def = b.build().unwrap();
    let order_of = |boundary, anim| {
        let mut fx = Fx::new(def.clone());
        fx.rules.tick.boundary_slot = boundary;
        fx.rules.tick.anim_slot = anim;
        let a = fx.probe(k, 795.0, 300.0, p("A"));
        fx.probe(k, 100.0, 300.0, p("B").style(iwbtg_sim::events::ProbeLog::Silent));
        fx.touch(a, true);
        fx.tick(InputFrame::EMPTY);
        let log = fx.log();
        tokens_of(&log, "A")
            .into_iter()
            .map(|t| t.chars().next().unwrap())
            .filter(|c| "iknde".contains(*c))
            .collect::<String>()
    };
    assert_eq!(order_of(BoundarySlot::AfterMotion, AnimSlot::AfterDraw), "ikedn");
    assert_eq!(
        order_of(BoundarySlot::AfterCollision, AnimSlot::AfterDraw),
        "kiedn"
    );
    assert_eq!(
        order_of(BoundarySlot::AfterMotion, AnimSlot::AfterMotion),
        "niked"
    );
}

#[test]
fn wp1_timeline_runs_moments_from_the_position_then_advances() {
    let mut b = builder();
    let k = probe_kind(&mut b, "p");
    b.def_mut().timelines.push(TimelineDef {
        name: "tl".into(),
        moments: vec![0, 2, 3],
        ..TimelineDef::default()
    });
    let def = b.build().unwrap();
    let moments_per_tick = |speed: f64, looping: bool, ticks: u32| {
        let mut fx = Fx::new(def.clone());
        let a = fx.probe(k, 0.0, 0.0, p("A"));
        {
            let inst = fx.w.get_mut(a).unwrap();
            timeline::start(&mut inst.timeline, TimelineId(0), Real::ZERO).unwrap();
            let run = inst.timeline.as_mut().unwrap();
            run.speed = Real::new(speed);
            run.looping = looping;
        }
        let mut out = Vec::new();
        for _ in 0..ticks {
            fx.tick(InputFrame::EMPTY);
            let log = fx.take_log();
            out.push(
                tokens_of(&log, "A")
                    .into_iter()
                    .filter(|t| t.starts_with('t'))
                    .map(|t| t.trim_end_matches('A').to_owned())
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        (
            out,
            fx.w.get(a).unwrap().timeline.as_ref().unwrap().position.get(),
        )
    };
    // Moment N on the (N+1)th tick after setting position 0 between ticks; position reads 1
    // after the first tick (audio-rori-sync-b.csv).
    let (out, pos) = moments_per_tick(1.0, false, 5);
    assert_eq!(out, vec!["t0", "", "t2", "t3", ""]);
    assert_eq!(pos, 5.0);
    let (out, _) = moments_per_tick(2.0, false, 3);
    assert_eq!(out, vec!["t0", "t2,t3", ""]);
    let (out, _) = moments_per_tick(1.0, true, 6);
    assert_eq!(out, vec!["t0", "", "t2", "t3", "t0", ""]);
}

#[test]
fn wp1_end_of_path_runs_after_motion() {
    let mut b = builder();
    let k = probe_kind(&mut b, "p");
    let mut fx = Fx::new(b.build().unwrap());
    let a = fx.probe(k, 0.0, 0.0, p("A"));
    fx.set(END_PATH, Value::Inst(a));
    fx.tick(InputFrame::EMPTY);
    assert!(fx.log().contains("sA pA eA "), "{}", fx.log());
}

#[test]
fn wp1_real_time_reads_are_tick_based() {
    let mut fx = Fx::new(builder().build().unwrap());
    assert_eq!(tick::real_time_fps(&fx.def), Real::new(50.0));
    fx.w.tick = 125;
    assert_eq!(tick::real_time_ms(&fx.w, &fx.def).unwrap(), Real::new(2500.0));
    fx.w.tick = u64::MAX;
    assert!(tick::real_time_ms(&fx.w, &fx.def).is_err());
}

#[test]
fn wp1_tick_counter_overflow_halts_through_the_error() {
    let mut fx = Fx::new(builder().build().unwrap());
    fx.w.tick = u64::MAX;
    let mut c = fx.ctx();
    let r = tick::run_tick_with(&mut c, InputFrame::EMPTY, &HOOKS);
    assert!(matches!(r, Err(SimError::Overflow { .. })));
}

/// `timeline::start_with_def` begins a run with `timeline_loop` set when the def says `loop`.
#[test]
fn start_with_def_applies_the_timeline_loop_flag() {
    let mut b = builder();
    b.def_mut().timelines.push(TimelineDef {
        name: "once".into(),
        moments: vec![0],
        ..TimelineDef::default()
    });
    b.def_mut().timelines.push(TimelineDef {
        name: "again".into(),
        moments: vec![0],
        looping: true,
        ..TimelineDef::default()
    });
    let def = b.build().unwrap();
    let mut run = None;
    timeline::start_with_def(&mut run, &def, TimelineId(0), Real::ZERO).unwrap();
    assert!(!run.as_ref().unwrap().looping);
    timeline::start_with_def(&mut run, &def, TimelineId(1), Real::ZERO).unwrap();
    assert!(run.as_ref().unwrap().looping);
}
