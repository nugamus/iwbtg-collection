//! WP1: world API (spawn, destroy, change_kind, first_of, kid, count_of), the collision phase
//! and the state hash (docs/plans/sim-core.md 1, 2.3, 10.1, 12).

use crate::wp1_support as support;

use iwbtg_sim::def::{Builtin, ClassMask, EngineKind, KindDef, KindId, ParamValue, Params, RoomId, VarId};
use iwbtg_sim::events::{ProbeAct, ProbeLog, ProbeOn, ProbeStep, probe_event_set};
use iwbtg_sim::hash::{self, CHECKPOINT_INTERVAL};
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::num::Real;
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::room::RoomOutput;
use iwbtg_sim::rules::collide::PairOrder;
use iwbtg_sim::rules::tick::{ChangeKindProps, FirstRule};
use iwbtg_sim::world::{Flow, InstId, SpawnInit};
use support::*;

fn step(on: ProbeOn, act: ProbeAct) -> ProbeStep {
    ProbeStep::on(on, act)
}

fn classed(
    b: &mut iwbtg_sim::def::testing::GameDefBuilder,
    name: &str,
    classes: ClassMask,
    handlers: &[ClassMask],
) -> KindId {
    let mut k = KindDef::new(name, Builtin::Probe);
    k.events = probe_event_set();
    k.classes = classes;
    k.events.collisions.extend(handlers.iter().copied());
    b.kind(k)
}

#[test]
fn wp1_spawn_runs_create_inside_the_call_and_applies_the_kind() {
    let mut b = builder();
    let sprite = b.sprite_rect((4, 4), (0, 0));
    let creator = probe_kind(&mut b, "creator");
    let mut k = KindDef::new("thing", Builtin::Probe);
    k.events = probe_event_set();
    k.sprite = Some(sprite);
    k.solid = true;
    k.visible = false;
    k.depth = Real::new(-5.0);
    k.classes = ClassMask::KILLER;
    k.params = Params([("speed".to_owned(), ParamValue::Real(Real::new(3.0)))].into());
    let thing = b.kind(k);
    b.def_mut().var_names.insert("speed".into(), VarId(7));
    b.engine_kind(EngineKind::Blood, thing);
    let mut fx = Fx::new(b.build().unwrap());
    let a = fx.probe(
        creator,
        0.0,
        0.0,
        p("A")
            .with(step(
                ProbeOn::Step,
                ProbeAct::Spawn {
                    kind: thing,
                    x: Real::new(10.0),
                    y: Real::new(20.0),
                    child: Some(Box::new(p("T"))),
                },
            ))
            .with(step(ProbeOn::Step, ProbeAct::Log("after ".into()))),
    );
    fx.tick(InputFrame::EMPTY);
    assert!(fx.log().contains("sA CT after "), "{}", fx.log());
    let t = fx.w.instances().last().unwrap();
    assert!(t.id > a);
    assert_eq!(
        (t.rank, t.solid, t.visible, t.depth, t.classes),
        (1, true, false, Real::new(-5.0), ClassMask::KILLER)
    );
    assert_eq!(t.img.sprite(), Some(sprite));
    assert_eq!((t.m.x(), t.m.y()), (Real::new(10.0), Real::new(20.0)));
    assert_eq!(t.vars.get(&VarId(7)), Some(&vr(3.0)), "kind param as a variable");

    let mut c = fx.ctx();
    let init = SpawnInit {
        params: Params([("speed".to_owned(), ParamValue::Real(Real::new(4.0)))].into()),
        rank: Some(9),
        ..SpawnInit::default()
    };
    let id = c.spawn(thing, Real::ZERO, Real::ZERO, init).unwrap();
    let inst = fx.w.get(id).unwrap();
    assert_eq!(
        inst.vars.get(&VarId(7)),
        Some(&vr(4.0)),
        "instance params override"
    );
    assert_eq!(inst.rank, 9, "a room-file rank wins");
    assert_eq!(
        (inst.visible, inst.depth),
        (false, Real::new(-5.0)),
        "without room-file keys the kind's visible and depth stay"
    );

    let mut c = fx.ctx();
    let init = SpawnInit {
        visible: Some(true),
        depth: Some(Real::new(7.0)),
        ..SpawnInit::default()
    };
    let id = c.spawn(thing, Real::ZERO, Real::ZERO, init).unwrap();
    let inst = fx.w.get(id).unwrap();
    assert_eq!(
        (inst.visible, inst.depth),
        (true, Real::new(7.0)),
        "room-file visible and depth override the kind's (#632)"
    );
    let iwbtg_sim::world::KindState::Probe(state) = &inst.state else {
        panic!("probe state")
    };
    assert_eq!(state.events, 1, "Create ran inside spawn");

    let mut c = fx.ctx();
    assert!(
        c.spawn(KindId(99), Real::ZERO, Real::ZERO, SpawnInit::default())
            .is_err()
    );
    assert!(
        c.spawn(thing, Real::new(f64::NAN), Real::ZERO, SpawnInit::default())
            .is_err()
    );
    assert!(
        c.spawn_engine(EngineKind::Bow, Real::ZERO, Real::ZERO, SpawnInit::default())
            .unwrap()
            .is_none()
    );
    assert!(
        c.spawn_engine(EngineKind::Blood, Real::ZERO, Real::ZERO, SpawnInit::default())
            .unwrap()
            .is_some()
    );
}

#[test]
fn wp1_ids_are_monotonic_and_never_reused() {
    let mut b = builder();
    let k = probe_kind(&mut b, "p");
    let mut fx = Fx::new(b.build().unwrap());
    let a = fx.probe(
        k,
        0.0,
        0.0,
        p("A").with(step(ProbeOn::Step, ProbeAct::DestroySelf)),
    );
    let bb = fx.probe(k, 0.0, 0.0, p("B"));
    assert_eq!((a, bb), (InstId::FIRST, InstId(100_002)));
    fx.tick(InputFrame::EMPTY);
    let c = fx.probe(k, 0.0, 0.0, p("C"));
    assert_eq!(c, InstId(100_003));
}

#[test]
fn wp1_first_of_kid_and_count_of() {
    let mut b = builder();
    let kid = classed(&mut b, "kid", ClassMask::KID, &[]);
    let flipped = classed(&mut b, "kid2", ClassMask::KID_FLIPPED, &[]);
    b.order(&[flipped, kid]);
    let mut fx = Fx::new(b.build().unwrap());
    let k1 = fx.probe(kid, 0.0, 0.0, p("K1"));
    let k2 = fx.probe(flipped, 0.0, 0.0, p("K2"));
    let rules = fx.rules.tick.clone();
    // Dispatch order: the flipped kind has the lower rank.
    assert_eq!(fx.w.kid(&rules), Some(k2));
    let mut lowest = rules.clone();
    lowest.first_rule = FirstRule::LowestId;
    assert_eq!(fx.w.kid(&lowest), Some(k1));
    assert_eq!(fx.w.count_of(ClassMask::KID | ClassMask::KID_FLIPPED), 2);
    assert_eq!(fx.w.first_of(ClassMask::BLOCK, &rules), None);
    // A destroyed kid leaves queries at once (R22 default), unless dead_in_queries.
    assert_eq!(fx.ctx().destroy(k2), Flow::Continue);
    assert!(!fx.w.exists(k2) && fx.w.get(k2).is_some());
    assert_eq!(fx.w.kid(&rules), Some(k1));
    let mut dead = rules.clone();
    dead.dead_in_queries = true;
    assert_eq!(fx.w.kid(&dead), Some(k2));
    assert_eq!(fx.w.count_of(ClassMask::KID | ClassMask::KID_FLIPPED), 1);
    // Deactivated instances are ignored.
    fx.w.get_mut(k1).unwrap().active = false;
    assert_eq!(fx.w.kid(&rules), None);
}

#[test]
fn wp1_change_kind_runs_destroy_and_create_and_takes_the_kind() {
    let mut b = builder();
    let sprite = b.sprite_rect((4, 4), (0, 0));
    let from = classed(&mut b, "from", ClassMask::BLOCK, &[]);
    let mut to = KindDef::new("to", Builtin::Probe);
    to.events = probe_event_set();
    to.classes = ClassMask::KILLER;
    to.sprite = Some(sprite);
    to.solid = true;
    let to = b.kind(to);
    let def = b.build().unwrap();
    for props in [ChangeKindProps::KindDefaults, ChangeKindProps::KeepInstance] {
        let mut fx = Fx::new(def.clone());
        fx.rules.tick.change_kind = props;
        let a = fx.probe(from, 0.0, 0.0, p("A"));
        fx.w.get_mut(a).unwrap().alarms[3] = Real::new(9.0);
        assert_eq!(fx.ctx().change_kind(a, to, true), Flow::Continue);
        // The new state is a fresh probe state with no log, so only Destroy is logged.
        assert_eq!(fx.log(), "DA ");
        let i = fx.w.get(a).unwrap();
        assert_eq!((i.kind, i.rank, i.classes), (to, 1, ClassMask::KILLER));
        assert_eq!(i.alarms[3], Real::new(9.0), "instance variables stay");
        let defaults = props == ChangeKindProps::KindDefaults;
        assert_eq!(i.solid, defaults);
        assert_eq!(i.img.sprite().is_some(), defaults);
    }
}

#[test]
fn wp1_emit_queues_output() {
    let mut fx = Fx::new(builder().build().unwrap());
    fx.ctx().emit(OutputEvent::Room(RoomOutput::GameRestarted));
    assert_eq!(fx.w.output.len(), 1);
}

/// The kid probe K with handlers `handlers`, and others as given (kind, solid, touching).
fn collision_fixture(
    handlers: &[ClassMask],
    pair_order: PairOrder,
    recheck_nonsolid: bool,
) -> (Fx, InstId, InstId, InstId) {
    let mut b = builder();
    let kid = classed(&mut b, "kid", ClassMask::KID, handlers);
    let block = classed(&mut b, "block", ClassMask::BLOCK, &[]);
    let killer = classed(&mut b, "killer", ClassMask::KILLER, &[]);
    let mut fx = Fx::new(b.build().unwrap());
    fx.rules.collide.pair_order = pair_order;
    fx.rules.collide.recheck_nonsolid = recheck_nonsolid;
    let k = fx.probe(kid, 0.0, 0.0, p("K"));
    // The killer is created first, so id order alone would put it first.
    let x = fx.probe(killer, 0.0, 0.0, p("X").style(ProbeLog::Silent));
    let bl = fx.probe(block, 0.0, 0.0, p("B").style(ProbeLog::Silent));
    fx.w.get_mut(bl).unwrap().solid = true;
    fx.touch(x, true);
    fx.touch(bl, true);
    (fx, k, bl, x)
}

fn kid_collisions(log: &str) -> Vec<String> {
    log.split(' ')
        .filter(|t| t.starts_with('k') && t.ends_with('K') || *t == "put")
        .map(str::to_owned)
        .collect()
}

#[test]
fn wp1_collision_pairs_follow_pair_order() {
    let handlers = [ClassMask::KILLER, ClassMask::BLOCK];
    let (mut fx, _, bl, x) = collision_fixture(&handlers, PairOrder::OtherRankThenId, true);
    fx.tick(InputFrame::EMPTY);
    // By the other's rank: block (rank 1) before killer (rank 2); the solid pair goes through
    // the put-back.
    assert_eq!(
        kid_collisions(&fx.log()),
        vec!["put".to_owned(), format!("k{}K", bl.0), format!("k{}K", x.0)]
    );
    let (mut fx, _, bl, x) = collision_fixture(&handlers, PairOrder::HandlerListOrder, true);
    fx.tick(InputFrame::EMPTY);
    assert_eq!(
        kid_collisions(&fx.log()),
        vec![format!("k{}K", x.0), "put".to_owned(), format!("k{}K", bl.0)]
    );
}

/// The kid probe with `handlers` touching one platform (`PLATFORM | BLOCK`): the kid's
/// collision log for one collision phase.
fn platform_hits(handlers: &[ClassMask]) -> (Vec<String>, InstId) {
    let mut b = builder();
    let kid = classed(&mut b, "kid", ClassMask::KID, handlers);
    let platform = classed(&mut b, "platform", ClassMask::PLATFORM | ClassMask::BLOCK, &[]);
    let mut fx = Fx::new(b.build().unwrap());
    fx.probe(kid, 0.0, 0.0, p("K"));
    let pl = fx.probe(platform, 0.0, 0.0, p("P").style(ProbeLog::Silent));
    fx.touch(pl, true);
    let mut c = fx.ctx();
    let flow = iwbtg_sim::events::collision_phase_with(&mut c, &HOOKS);
    assert_eq!(flow, Flow::Continue);
    (kid_collisions(&fx.log()), pl)
}

#[test]
fn wp1_one_other_matching_two_handlers_runs_only_the_most_specific() {
    // C8, R12b (Confirmed (recorder), showdown t-mot-08-a): a platform runs the kid's platform
    // handler only, whatever the handler order.
    let both = [ClassMask::BLOCK, ClassMask::PLATFORM];
    let (hits, pl) = platform_hits(&both);
    assert_eq!(hits, vec![format!("k{}K", pl.0)]);
    let (hits, _) = platform_hits(&[ClassMask::PLATFORM, ClassMask::BLOCK]);
    assert_eq!(hits.len(), 1);
    let platform = ClassMask::PLATFORM | ClassMask::BLOCK;
    assert_eq!(
        iwbtg_sim::events::pair_handler(&both, platform),
        Some((1, ClassMask::PLATFORM))
    );
    // help-the-fairy-tale's platform also counts as solidblock: still platform, then solidblock.
    let three = [ClassMask::BLOCK, ClassMask::SOLIDBLOCK, ClassMask::PLATFORM];
    let htft = platform | ClassMask::SOLIDBLOCK;
    assert_eq!(
        iwbtg_sim::events::pair_handler(&three, htft),
        Some((2, ClassMask::PLATFORM))
    );
    assert_eq!(
        iwbtg_sim::events::pair_handler(&three[..2], htft),
        Some((1, ClassMask::SOLIDBLOCK))
    );
}

#[test]
fn wp1_with_only_a_block_handler_block_runs_for_a_platform() {
    // t-mot-08-b: with the platform event cleared, the block event runs for the platform.
    let (hits, pl) = platform_hits(&[ClassMask::BLOCK]);
    assert_eq!(hits, vec![format!("k{}K", pl.0)]);
    assert_eq!(
        iwbtg_sim::events::pair_handler(&[ClassMask::BLOCK], ClassMask::PLATFORM | ClassMask::BLOCK),
        Some((0, ClassMask::BLOCK))
    );
    assert_eq!(
        iwbtg_sim::events::pair_handler(&[ClassMask::KILLER], ClassMask::BLOCK),
        None
    );
}

#[test]
fn wp1_pairs_are_rechecked_before_their_event() {
    // The kid's block event moves him off the killer (clears the killer's touch flag).
    let run = |recheck: bool| {
        let (mut fx, k, bl, x) = collision_fixture(
            &[ClassMask::BLOCK, ClassMask::KILLER],
            PairOrder::OtherRankThenId,
            recheck,
        );
        let state = p("K").with(step(ProbeOn::Collision, ProbeAct::SetGlobal(touch_var(x), vr(0.0))).once());
        fx.w.get_mut(k).unwrap().state = iwbtg_sim::world::KindState::Probe(state);
        fx.tick(InputFrame::EMPTY);
        (kid_collisions(&fx.log()), bl, x)
    };
    let (hits, bl, _) = run(true);
    assert_eq!(
        hits,
        vec!["put".to_owned(), format!("k{}K", bl.0)],
        "C7b on: the killer pair is skipped"
    );
    let (hits, bl, x) = run(false);
    assert_eq!(
        hits,
        vec!["put".to_owned(), format!("k{}K", bl.0), format!("k{}K", x.0)],
        "C7b off"
    );
    // A solid pair is always re-checked (C7): two blocks, the first event clears the second.
    let mut b = builder();
    let kid = classed(&mut b, "kid", ClassMask::KID, &[ClassMask::BLOCK]);
    let block = classed(&mut b, "block", ClassMask::BLOCK, &[]);
    let mut fx = Fx::new(b.build().unwrap());
    fx.rules.collide.recheck_nonsolid = false;
    let k = fx.probe(kid, 0.0, 0.0, p("K"));
    let b1 = fx.probe(block, 0.0, 0.0, p("B1").style(ProbeLog::Silent));
    let b2 = fx.probe(block, 0.0, 0.0, p("B2").style(ProbeLog::Silent));
    for id in [b1, b2] {
        fx.w.get_mut(id).unwrap().solid = true;
        fx.touch(id, true);
    }
    let state = p("K").with(step(ProbeOn::Collision, ProbeAct::SetGlobal(touch_var(b2), vr(0.0))).once());
    fx.w.get_mut(k).unwrap().state = iwbtg_sim::world::KindState::Probe(state);
    fx.tick(InputFrame::EMPTY);
    assert_eq!(
        kid_collisions(&fx.log()),
        vec!["put".to_owned(), format!("k{}K", b1.0)]
    );
}

/// One T-ORD-2 run (research, traces/rely-the-needle-4-2/t-ord-2.md) on probe kinds, all
/// instances stacked and touching: the collision events of one collision phase as
/// `(self tag, other tag)`.
///
/// Kinds in rank order: kid P (`KID`), block L (`BLOCK`), save point S (`SAVE`), probes A
/// (`VINE_L`), B (`VINE_R`), C (`LADDER`) and, for run C, D (`VINE_L`, so it matches A's
/// handlers as an instance of A's child does, with A's handlers, but its own rank after B).
/// `kids` are the probe instances in creation order, by tag; `reverse` registers each kind's
/// handlers in reverse order.
fn t_ord_2(events: &[(&str, &[&str])], kids: &[&str], reverse: bool) -> Vec<(String, String)> {
    let class = |k: &str| match k {
        "P" => ClassMask::KID,
        "L" => ClassMask::BLOCK,
        "S" => ClassMask::SAVE,
        "A" | "D" => ClassMask::VINE_L,
        "B" => ClassMask::VINE_R,
        "C" => ClassMask::LADDER,
        _ => unreachable!("kind {k}"),
    };
    let mut b = builder();
    let mut kinds = Vec::new();
    for k in ["P", "L", "S", "A", "B", "C", "D"] {
        let mut handlers: Vec<ClassMask> = events
            .iter()
            .filter(|(me, _)| *me == k)
            .flat_map(|(_, with)| with.iter().map(|w| class(w)))
            .collect();
        if reverse {
            handlers.reverse();
        }
        kinds.push((k, classed(&mut b, k, class(k), &handlers)));
    }
    let order: Vec<KindId> = kinds.iter().map(|(_, id)| *id).collect();
    b.order(&order);
    let kind = |tag: &str| {
        let k = &tag[..1];
        kinds.iter().find(|(n, _)| *n == k).expect("kind").1
    };
    let mut fx = Fx::new(b.build().unwrap());
    let mut tags = Vec::new();
    for tag in ["P", "L", "S"].iter().chain(kids) {
        let id = fx.probe(kind(tag), 0.0, 0.0, p(tag));
        fx.touch(id, true);
        tags.push((id, *tag));
    }
    let mut c = fx.ctx();
    assert_eq!(
        iwbtg_sim::events::collision_phase_with(&mut c, &HOOKS),
        Flow::Continue
    );
    let name = |id: &str| {
        let id: u64 = id.parse().expect("id");
        tags.iter().find(|(i, _)| i.0 == id).expect("tag").1.to_owned()
    };
    fx.log()
        .split(' ')
        .filter(|t| t.starts_with('k'))
        .map(|t| {
            let digits = t[1..].find(|ch: char| !ch.is_ascii_digit()).expect("tag") + 1;
            (t[digits..].to_owned(), name(&t[1..digits]))
        })
        .collect()
}

fn pairs_of(s: &str) -> Vec<(String, String)> {
    s.split_whitespace()
        .map(|p| {
            let (a, b) = p.split_once('-').expect("pair");
            (a.to_owned(), b.to_owned())
        })
        .collect()
}

#[test]
fn wp1_r12_collision_pairs_run_lower_kind_first_t_ord_2() {
    // Run A: handlers registered scrambled; probes created c1, a1, b1, c2, a2.
    let events: &[(&str, &[&str])] = &[
        ("P", &["L", "B", "C"]),
        ("S", &["P"]),
        ("A", &["A", "P", "B", "C"]),
        ("B", &["A", "C"]),
        ("C", &["A", "P"]),
    ];
    // t-ord-2-a.events.txt frame 4, grouped by the outer instance: the kid's pairs (his event
    // first, then the other's), a1's (a1-a2 and a2-a1 both ways), a2's (meeting a1 again), b1's.
    let frame4 = pairs_of(
        "P-L S-P A1-P A2-P P-B1 P-C1 C1-P P-C2 C2-P \
         A1-A2 A2-A1 A1-B1 B1-A1 A1-C1 C1-A1 A1-C2 C2-A1 \
         A2-A1 A1-A2 A2-B1 B1-A2 A2-C1 C1-A2 A2-C2 C2-A2 \
         B1-C1 B1-C2",
    );
    assert_eq!(frame4.len(), 27, "27 collision events per tick in run A");
    assert_eq!(t_ord_2(events, &["C1", "A1", "B1", "C2", "A2"], false), frame4);
    // Run B: registration reversed, probes created a1, a2, b1, c1, c2: the same sequence.
    assert_eq!(t_ord_2(events, &["A1", "A2", "B1", "C1", "C2"], true), frame4);
    // Run C: D (A's child in the trace) has its own rank after B and comes after b1 although
    // d1 is the oldest probe.
    let events: &[(&str, &[&str])] = &[
        ("P", &["L", "A"]),
        ("S", &["P"]),
        ("A", &["P", "B"]),
        ("D", &["P", "B"]),
        ("B", &["A"]),
    ];
    assert_eq!(
        t_ord_2(events, &["D1", "A1", "B1"], false),
        pairs_of("P-L S-P P-A1 A1-P P-D1 D1-P A1-B1 B1-A1 B1-D1 D1-B1")
    );
}

#[test]
fn wp1_a_destroyed_other_is_skipped() {
    let (mut fx, k, bl, x) = collision_fixture(
        &[ClassMask::BLOCK, ClassMask::KILLER],
        PairOrder::OtherRankThenId,
        false,
    );
    let state = p("K").with(step(ProbeOn::Collision, ProbeAct::Destroy(x)).once());
    fx.w.get_mut(k).unwrap().state = iwbtg_sim::world::KindState::Probe(state);
    fx.tick(InputFrame::EMPTY);
    assert_eq!(
        kid_collisions(&fx.log()),
        vec!["put".to_owned(), format!("k{}K", bl.0)]
    );
}

/// R22 (Confirmed (recorder), research T-DEAD-1): a kid killed in his block event gets no later
/// Collision event from a save point that overlaps him, because the pair has a dead instance in
/// it. Under the pair order (R12, T-ORD-2) the kid (rank 0) meets the block, dies, and his pair
/// with the save point (rank 2) comes later in his own turn. The second part kills an instance
/// in the kid's turn whose pair belongs to a later outer instance's turn: that is skipped too.
#[test]
fn wp1_r22_a_pair_with_an_instance_killed_earlier_in_the_phase_is_skipped() {
    let mut b = builder();
    let kid = classed(&mut b, "kid", ClassMask::KID, &[ClassMask::BLOCK]);
    let block = classed(&mut b, "block", ClassMask::BLOCK, &[]);
    let save = classed(&mut b, "save", ClassMask::SAVE, &[ClassMask::KID]);
    let mut fx = Fx::new(b.build().unwrap());
    fx.rules.collide.recheck_nonsolid = true;
    let k = fx.probe(
        kid,
        0.0,
        0.0,
        p("K").with(step(ProbeOn::Collision, ProbeAct::DestroySelf).once()),
    );
    let bl = fx.probe(block, 0.0, 0.0, p("B").style(ProbeLog::Silent));
    let sp = fx.probe(save, 0.0, 0.0, p("S"));
    for id in [k, bl, sp] {
        fx.touch(id, true);
    }
    fx.tick(InputFrame::EMPTY);
    let log = fx.log();
    assert!(
        log.contains(&format!("k{}K", bl.0)),
        "the kid's block event ran: {log}"
    );
    assert!(
        tokens_of(&log, "S").iter().all(|t| !t.starts_with('k')),
        "no save point Collision after the kid died: {log}"
    );
    // Control: the same world without the kill runs the save point's event.
    let mut b = builder();
    let kid = classed(&mut b, "kid", ClassMask::KID, &[ClassMask::BLOCK]);
    let block = classed(&mut b, "block", ClassMask::BLOCK, &[]);
    let save = classed(&mut b, "save", ClassMask::SAVE, &[ClassMask::KID]);
    let mut fx = Fx::new(b.build().unwrap());
    fx.rules.collide.recheck_nonsolid = true;
    let k = fx.probe(kid, 0.0, 0.0, p("K"));
    let bl = fx.probe(block, 0.0, 0.0, p("B").style(ProbeLog::Silent));
    let sp = fx.probe(save, 0.0, 0.0, p("S"));
    for id in [k, bl, sp] {
        fx.touch(id, true);
    }
    fx.tick(InputFrame::EMPTY);
    assert!(
        tokens_of(&fx.log(), "S")
            .iter()
            .any(|t| *t == format!("k{}S", k.0)),
        "control: {}",
        fx.log()
    );
    // A later outer instance's pair: the kid's block event destroys the killer X (rank 3); the
    // save point (rank 2), which handles killers, meets X in its own turn, after the kid's.
    let run = |kill: bool| {
        let mut b = builder();
        let kid = classed(&mut b, "kid", ClassMask::KID, &[ClassMask::BLOCK]);
        let block = classed(&mut b, "block", ClassMask::BLOCK, &[]);
        let save = classed(&mut b, "save", ClassMask::SAVE, &[ClassMask::KILLER]);
        let killer = classed(&mut b, "killer", ClassMask::KILLER, &[]);
        b.order(&[kid, block, save, killer]);
        let mut fx = Fx::new(b.build().unwrap());
        let x = fx.probe(killer, 0.0, 0.0, p("X").style(ProbeLog::Silent));
        let sp = fx.probe(save, 0.0, 0.0, p("S"));
        let bl = fx.probe(block, 0.0, 0.0, p("B").style(ProbeLog::Silent));
        let state = if kill {
            p("K").with(step(ProbeOn::Collision, ProbeAct::Destroy(x)).once())
        } else {
            p("K")
        };
        let k = fx.probe(kid, 0.0, 0.0, state);
        for id in [k, bl, sp, x] {
            fx.touch(id, true);
        }
        let mut c = fx.ctx();
        assert_eq!(
            iwbtg_sim::events::collision_phase_with(&mut c, &HOOKS),
            Flow::Continue
        );
        let log = fx.log();
        assert!(
            log.contains(&format!("k{}K", bl.0)),
            "the kid's block event ran: {log}"
        );
        log.contains(&format!("k{}S", x.0))
    };
    assert!(
        !run(true),
        "no save point Collision with the killer destroyed in the kid's turn"
    );
    assert!(run(false), "control: the save point meets the killer");
}

#[test]
fn wp1_state_hash_covers_state_and_skips_caches() {
    let mut b = builder();
    let k = probe_kind(&mut b, "p");
    let mut fx = Fx::new(b.build().unwrap());
    fx.probe(k, 0.0, 0.0, p("A"));
    let h0 = hash::state_hash(&fx.w);
    assert_eq!(h0, hash::state_hash(&fx.w.clone()));
    // Output, the grid and the request counter are not hashed.
    fx.ctx().emit(OutputEvent::Room(RoomOutput::GameRestarted));
    fx.w.grid = Default::default();
    fx.w.room_requests = 3;
    assert_eq!(hash::state_hash(&fx.w), h0);
    // Everything else is.
    let mut w = fx.w.clone();
    w.setup_applied = true;
    assert_ne!(hash::state_hash(&w), h0, "setup_applied is hashed");
    let mut w = fx.w.clone();
    w.globals.insert(VarId(3), vr(1.0));
    assert_ne!(hash::state_hash(&w), h0);
    let mut w = fx.w.clone();
    w.room = RoomId(1);
    assert_ne!(hash::state_hash(&w), h0);
    let mut w = fx.w.clone();
    w.get_mut(InstId::FIRST).unwrap().depth = Real::new(-0.0);
    assert_ne!(hash::state_hash(&w), h0, "reals hash by their bits");
    // Checkpoints every 500 ticks.
    assert_eq!(
        hash::checkpoint(&fx.w).map(|c| c.to_vec()),
        Some(h0[..16].to_vec())
    );
    fx.w.tick = CHECKPOINT_INTERVAL - 1;
    assert!(hash::checkpoint(&fx.w).is_none());
    fx.w.tick = CHECKPOINT_INTERVAL * 3;
    assert!(hash::checkpoint(&fx.w).is_some());
}

#[test]
fn wp1_unknown_kind_in_dispatch_halts() {
    let mut b = builder();
    let k = probe_kind(&mut b, "p");
    let mut fx = Fx::new(b.build().unwrap());
    let a = fx.probe(k, 0.0, 0.0, p("A"));
    fx.w.get_mut(a).unwrap().kind = KindId(42);
    // Phases see an unknown kind as handling nothing; a direct dispatch halts.
    fx.tick(InputFrame::EMPTY);
    assert_eq!(fx.w.status, iwbtg_sim::SimStatus::Running);
    let flow = iwbtg_sim::events::dispatch(&mut fx.ctx(), a, &iwbtg_sim::events::EventKind::User(0));
    assert_eq!(flow, Flow::Halt);
    assert!(matches!(
        fx.w.status,
        iwbtg_sim::SimStatus::Halted(iwbtg_sim::SimError::Invariant { .. })
    ));
}
