//! #240: the A1 kid at the room edge (`events.boundary = "intersect_origin"`, kid/A1.md
//! "Intersect Boundary").

use iwbtg_sim::def::physics::{BoundaryModel, DeathStep};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, EngineKind, GameDef, KindDef, KindId, ParamValue, PhysicsProfile, RoomDef, RoomId,
};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::kid;
use iwbtg_sim::num::Real;
use iwbtg_sim::room::RoomRequest;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, Flow, InstId, SpawnInit, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

struct Kinds {
    kid: KindId,
    edge: KindId,
    table: KindId,
    bow: KindId,
}

fn game(tweak: impl FnOnce(&mut PhysicsProfile)) -> (GameDef, Kinds) {
    let mut b = GameDefBuilder::new("wp5a-boundary");
    let kid_sprite = b.sprite_rect((11, 21), (5, 12));
    let tile = b.sprite_rect((32, 32), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(kid_sprite);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let mut e = KindDef::new("edge", Builtin::EdgeWarp);
    e.sprite = Some(tile);
    e.classes = ClassMask::EDGE_WARP;
    let edge = b.kind(e);
    let mut t = KindDef::new("table", Builtin::EdgeWarp);
    t.classes = ClassMask::EDGE_WARP;
    let table = b.kind(t);
    let bow = b.kind(KindDef::new("bow", Builtin::Inert));
    b.engine_kind(EngineKind::Kid, kid);
    b.engine_kind(EngineKind::Bow, bow);
    let room = b.room(RoomDef::new("a", 800, 608));
    b.room(RoomDef::new("b", 800, 608));
    b.start_room(room);
    let mut p = PhysicsProfile::default();
    p.events.boundary = BoundaryModel::IntersectOrigin;
    p.death.order = vec![DeathStep::DestroyKid, DeathStep::DeathsPlusOne];
    tweak(&mut p);
    b.physics(p);
    (
        b.build().expect("game builds"),
        Kinds {
            kid,
            edge,
            bow,
            table,
        },
    )
}

fn with_kid<T>(
    tweak: impl FnOnce(&mut PhysicsProfile),
    pos: (f64, f64),
    f: impl FnOnce(&mut Ctx, InstId, &Kinds) -> T,
) -> T {
    let (def, k) = game(tweak);
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
    let kid = ctx
        .spawn(k.kid, r(pos.0), r(pos.1), SpawnInit::default())
        .expect("kid");
    f(&mut ctx, kid, &k)
}

/// An edge warp to room 1 that covers the kid's position.
fn edge_warp(ctx: &mut Ctx, k: &Kinds, near: InstId, to: (f64, f64)) -> InstId {
    let (x, y) = {
        let m = &ctx.w.get(near).unwrap().m;
        (m.x(), m.y())
    };
    let mut init = SpawnInit::default();
    init.params
        .0
        .insert("room_to".into(), ParamValue::Room(RoomId(1)));
    init.params.0.insert("warp_x".into(), ParamValue::Real(r(to.0)));
    init.params.0.insert("warp_y".into(), ParamValue::Real(r(to.1)));
    ctx.spawn(k.edge, x - r(16.0), y - r(16.0), init)
        .expect("edge warp")
}

fn boundary(ctx: &mut Ctx, kid: InstId) -> Flow {
    kid::on_event(ctx, kid, &EventKind::IntersectBoundary).expect("event")
}

fn pos(ctx: &Ctx, id: InstId) -> (f64, f64) {
    let m = &ctx.w.get(id).unwrap().m;
    (m.x().get(), m.y().get())
}

#[test]
fn inside_the_room_nothing_happens_and_the_edge_itself_is_inside() {
    for p in [(100.0, 100.0), (800.0, 300.0), (0.0, 0.0), (300.0, 608.0)] {
        with_kid(
            |_| {},
            p,
            |ctx, kid, _| {
                assert_eq!(boundary(ctx, kid), Flow::Continue);
                assert!(ctx.w.get(kid).unwrap().alive, "{p:?} is inside");
                assert_eq!(pos(ctx, kid), p);
            },
        );
    }
}

#[test]
fn outside_with_no_edge_warp_kills_the_kid() {
    for p in [(-1.0, 100.0), (801.0, 100.0), (100.0, -1.0), (100.0, 609.0)] {
        with_kid(
            |_| {},
            p,
            |ctx, kid, _| {
                boundary(ctx, kid);
                assert!(!ctx.w.get(kid).unwrap().alive, "{p:?} is outside");
                assert_eq!(ctx.w.engine.run.deaths, 1);
            },
        );
    }
}

#[test]
fn an_edge_warp_moves_the_kid_and_requests_its_room() {
    with_kid(
        |_| {},
        (-3.0, 100.0),
        |ctx, kid, k| {
            edge_warp(ctx, k, kid, (50.0, 60.0));
            assert_eq!(boundary(ctx, kid), Flow::AbortTick);
            assert!(ctx.w.get(kid).unwrap().alive);
            assert_eq!(pos(ctx, kid), (50.0, 60.0));
            assert_eq!(ctx.w.pending, Some(RoomRequest::Goto(RoomId(1))));
            assert_eq!(ctx.w.engine.run.deaths, 0);
        },
    );
}

#[test]
fn a_warp_to_zero_zero_destroys_the_kid_and_the_bow() {
    with_kid(
        |_| {},
        (-3.0, 100.0),
        |ctx, kid, k| {
            let bow = ctx.spawn(k.bow, r(0.0), r(0.0), SpawnInit::default()).unwrap();
            edge_warp(ctx, k, kid, (0.0, 0.0));
            boundary(ctx, kid);
            assert!(!ctx.w.get(kid).unwrap().alive && !ctx.w.get(bow).unwrap().alive);
            assert_eq!(ctx.w.pending, Some(RoomRequest::Goto(RoomId(1))));
            assert_eq!(ctx.w.engine.run.deaths, 0, "a warp is not a death");
        },
    );
}

fn wrapped(from: (f64, f64), vspeed: f64, xprevious: f64) -> (f64, f64) {
    with_kid(
        |_| {},
        from,
        |ctx, kid, k| {
            {
                let m = &mut ctx.w.get_mut(kid).unwrap().m;
                m.set_vspeed(r(vspeed)).unwrap();
                m.set_xprevious(r(xprevious)).unwrap();
            }
            // The warp target itself lies outside the room, so the wrap runs on it.
            let target = from;
            edge_warp(ctx, k, kid, target);
            ctx.w.get_mut(kid).unwrap().m.set_x(r(from.0)).unwrap();
            boundary(ctx, kid);
            pos(ctx, kid)
        },
    )
}

#[test]
fn the_wrap_rules() {
    // x < 0: x = room_width - 16, y undoes this step's vspeed.
    assert_eq!(wrapped((-5.0, 100.0), 2.0, 7.0), (784.0, 98.0));
    // x > room_width: x = 16, same.
    assert_eq!(wrapped((805.0, 100.0), -3.0, 7.0), (16.0, 103.0));
    // y < 0: y = room_height - 16, x = xprevious.
    assert_eq!(wrapped((100.0, -5.0), 2.0, 70.0), (70.0, 592.0));
    // y > room_height: y = 16, x = xprevious.
    assert_eq!(wrapped((100.0, 610.0), 2.0, 70.0), (70.0, 16.0));
}

#[test]
fn the_x_wrap_wins_when_both_are_outside() {
    assert_eq!(wrapped((-5.0, -5.0), 1.0, 70.0), (784.0, -6.0));
}

#[test]
fn the_event_set_follows_the_model() {
    let mut p = PhysicsProfile::default();
    let e = kid::event_set(&p);
    assert!(!e.intersect_boundary && !e.outside_room);
    p.events.boundary = BoundaryModel::IntersectOrigin;
    assert!(kid::event_set(&p).intersect_boundary);
    p.events.boundary = BoundaryModel::OutsideRoomKill;
    let e = kid::event_set(&p);
    assert!(e.outside_room && !e.intersect_boundary);
    p.events.boundary = BoundaryModel::EndStepOrigin;
    assert!(kid::event_set(&p).end_step);
}

#[test]
fn none_ignores_the_events() {
    with_kid(
        |p| p.events.boundary = BoundaryModel::None,
        (-9.0, 0.0),
        |ctx, kid, _| {
            assert_eq!(boundary(ctx, kid), Flow::Continue);
            assert!(ctx.w.get(kid).unwrap().alive);
        },
    );
}

/// A room-wide per-edge table (NY roomout) far from the kid, with the left edge set to room 1.
fn table(ctx: &mut Ctx, k: &Kinds, left: bool, right: bool) {
    let mut init = SpawnInit::default();
    if left {
        init.params
            .0
            .insert("left_room".into(), ParamValue::Room(RoomId(1)));
    }
    if right {
        init.params
            .0
            .insert("right_room".into(), ParamValue::Room(RoomId(1)));
    }
    init.params
        .0
        .insert("down_room".into(), ParamValue::Room(RoomId(1)));
    ctx.spawn(k.table, r(700.0), r(500.0), init).expect("edge table");
}

#[test]
fn an_edge_table_names_the_room_of_the_crossed_edge() {
    with_kid(
        |_| {},
        (-3.0, 100.0),
        |ctx, kid, k| {
            table(ctx, k, true, false);
            assert_eq!(boundary(ctx, kid), Flow::AbortTick);
            assert!(ctx.w.get(kid).unwrap().alive, "the kid persists");
            assert_eq!(ctx.w.pending, Some(RoomRequest::Goto(RoomId(1))));
            assert_eq!(pos(ctx, kid).0, 784.0, "and wraps like any boundary crossing");
        },
    );
}

#[test]
fn an_edge_table_kills_on_an_unset_edge_but_not_at_the_top() {
    with_kid(
        |_| {},
        (803.0, 100.0),
        |ctx, kid, k| {
            table(ctx, k, true, false);
            boundary(ctx, kid);
            assert!(!ctx.w.get(kid).unwrap().alive, "right is unset");
        },
    );
    with_kid(
        |_| {},
        (100.0, -3.0),
        |ctx, kid, k| {
            table(ctx, k, true, true);
            boundary(ctx, kid);
            assert!(ctx.w.get(kid).unwrap().alive, "nothing at the top");
            assert_eq!(ctx.w.pending, None);
            assert_eq!(pos(ctx, kid).1, 592.0);
        },
    );
    with_kid(
        |_| {},
        (100.0, 611.0),
        |ctx, kid, k| {
            table(ctx, k, false, false);
            assert_eq!(boundary(ctx, kid), Flow::AbortTick);
            assert_eq!(ctx.w.pending, Some(RoomRequest::Goto(RoomId(1))));
        },
    );
}
