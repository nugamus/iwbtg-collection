//! Collision query conformance (#777): every script collision query against the built-in query
//! of `collide::query` on the same world, over a sweep of positions and targets.
//!
//! The world has rectangle and precise masks, an empty mask, a kind and a kind derived from it,
//! an instance destroyed this step (R22, invisible) and ranks that differ from creation order
//! (R21, which one is first). Each sample asserts the script and the built-in agree; the
//! results are also hashed per query and target into `collision.txt`, so a change in what the
//! queries return shows in review. `UPDATE_CONFORMANCE=1 cargo test -p iwbtg-sim --test it
//! script_conformance::` rewrites it.

use std::fmt::Write as _;

use iwbtg_sim::collide::{
    Target, collision_line, collision_point, collision_rectangle, instance_place, place_free, place_meeting,
};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, GameDef, KindDef, KindId, RoomDef, RoomId, ScriptDef, ScriptId, VarId,
};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptHost;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, SpawnInit, Value, World};

const EXPECTED: &str = include_str!("collision.txt");

/// User event `n` runs query `n` with the arguments in the probe's variables and writes the
/// result to `res`: a bool as 1 or 0, an instance as its id, `()` as -1.
const PROBE: &str = r#"
fn out(h) { if h == () { -1.0 } else { h.id } }
fn b(v) { if v { 1.0 } else { 0.0 } }
fn user(n) {
    let t = this.tgt;
    let ax = this.ax;
    let ay = this.ay;
    let bx = this.bx;
    let by = this.by;
    let n = to_int(n);
    this.res = if n == 0 { b(place_meeting(this, ax, ay, t)) }
        else if n == 1 { b(place_free(this, ax, ay)) }
        else if n == 2 { out(instance_place(this, ax, ay, t)) }
        else if n == 3 { out(collision_point(ax, ay, t)) }
        else if n == 4 { out(collision_point(ax, ay, t, false, this)) }
        else if n == 5 { out(collision_rect(ax, ay, bx, by, t)) }
        else if n == 6 { out(collision_rect(ax, ay, bx, by, t, false, this)) }
        else if n == 7 { out(collision_line(ax, ay, bx, by, t)) }
        else if n == 8 { out(collision_line(ax, ay, bx, by, t, false, this)) }
        else if n == 9 { b(this.place_meeting(ax, ay, t)) }
        else { -2.0 };
}
"#;

const QUERIES: [&str; 10] = [
    "place_meeting",
    "place_free",
    "instance_place",
    "collision_point",
    "collision_point prec=false notme",
    "collision_rect",
    "collision_rect prec=false notme",
    "collision_line",
    "collision_line prec=false notme",
    "place_meeting method",
];

struct World2 {
    def: GameDef,
    rules: RunnerRules,
    scripts: ScriptRuntime,
    w: World,
    probe: InstId,
    /// Targets by name, as a script names them, with the built-in target.
    targets: Vec<(String, Value, Target)>,
}

impl World2 {
    fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            def: &self.def,
            rules: &self.rules,
            w: &mut self.w,
            scripts: &self.scripts,
            dll_mode: DllMode::Stub,
        }
    }

    fn spawn(&mut self, kind: KindId, x: f64, y: f64) -> InstId {
        self.ctx()
            .spawn(kind, Real::new(x), Real::new(y), SpawnInit::default())
            .expect("spawn")
    }
}

const VARS: [&str; 6] = ["ax", "ay", "bx", "by", "tgt", "res"];

fn world() -> World2 {
    let mut b = GameDefBuilder::new("conf777");
    let room = b.room(RoomDef::new("room", 320, 240));
    let square = b.sprite_rect((16, 16), (0, 0));
    let small = b.sprite_rect((8, 8), (4, 4));
    let diamond = b.sprite_bits(
        &[
            "...##...", "..####..", ".######.", "########", "########", ".######.", "..####..", "...##...",
        ],
        (0, 0),
    );
    // An empty mask (#926): place tests and precise queries never see it.
    let empty = b.sprite_bits(&["....", "...."], (0, 0));
    let mut probe = KindDef::new("probe", Builtin::Block);
    probe.script = Some(ScriptId(0));
    probe.sprite = Some(small);
    let probe = b.kind(probe);
    let mut block = KindDef::new("wall", Builtin::Block);
    block.sprite = Some(square);
    block.solid = true;
    block.classes = ClassMask::BLOCK;
    let block = b.kind(block);
    let mut gem = KindDef::new("gem", Builtin::Block);
    gem.sprite = Some(diamond);
    let gem = b.kind(gem);
    let mut gem2 = KindDef::new("gem[v=1]", Builtin::Block);
    gem2.sprite = Some(diamond);
    let gem2 = b.kind(gem2);
    let mut ghost = KindDef::new("ghost", Builtin::Block);
    ghost.sprite = Some(empty);
    ghost.classes = ClassMask::BLOCK;
    let ghost = b.kind(ghost);
    // Ranks differ from kind (and creation) order: R21 picks by rank, then id.
    b.order(&[gem2, probe, ghost, block, gem]);
    b.start_room(room);
    let mut def = b.build().expect("def");
    def.scripts.push(ScriptDef {
        name: "scripts/probe.rhai".into(),
        source: PROBE.into(),
    });
    for (n, v) in VARS.iter().zip(0u32..) {
        def.var_names.insert((*n).into(), VarId(v));
    }
    let rules = RunnerRules::default();
    let scripts = ScriptRuntime::new(&def, &rules).expect("scripts compile");
    let mut g = World2 {
        def,
        rules,
        scripts,
        w: World::new(RoomId(0), 7),
        probe: InstId(0),
        targets: Vec::new(),
    };
    g.probe = g.spawn(probe, 40.0, 40.0);
    let w1 = g.spawn(block, 0.0, 0.0);
    g.spawn(block, 16.0, 0.0);
    g.spawn(block, 48.5, 30.0);
    let doomed = g.spawn(block, 70.0, 70.0);
    g.spawn(gem, 30.0, 20.0);
    g.spawn(gem2, 34.0, 22.0); // overlaps the gem; ranks first
    g.spawn(gem, 80.0, 10.0);
    g.spawn(ghost, 60.0, 50.0);
    g.spawn(block, 90.5, 88.0);
    let flow = g.ctx().destroy(doomed);
    assert!(!flow.is_halt());
    let gem_tree = Target::KindTree(gem);
    g.targets = vec![
        ("wall".into(), str_val("wall"), Target::KindTree(block)),
        ("block".into(), str_val("block"), Target::Class(ClassMask::BLOCK)),
        ("gem".into(), str_val("gem"), gem_tree),
        ("gem[v=1]".into(), str_val("gem[v=1]"), Target::KindTree(gem2)),
        ("all".into(), str_val("all"), Target::Any),
        ("instance".into(), Value::Inst(w1), Target::Inst(w1)),
    ];
    g
}

fn str_val(s: &str) -> Value {
    Value::Str(s.to_owned())
}

fn r(v: f64) -> Real {
    Real::new(v)
}

fn of(id: Option<InstId>) -> f64 {
    #[allow(clippy::cast_precision_loss, reason = "test ids are small")]
    id.map_or(-1.0, |i| i.0 as f64)
}

fn of_bool(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

/// The built-in query `n` on the same world.
fn builtin(g: &mut World2, n: u8, a: [f64; 4], t: Target) -> f64 {
    let me = g.probe;
    let ctx = g.ctx();
    let [ax, ay, bx, by] = a.map(r);
    match n {
        0 | 9 => of_bool(place_meeting(&ctx, me, ax, ay, t).expect("place_meeting")),
        1 => of_bool(place_free(&ctx, me, ax, ay).expect("place_free")),
        2 => of(instance_place(&ctx, me, ax, ay, t).expect("instance_place")),
        3 => of(collision_point(&ctx, ax, ay, t, true, None).expect("point")),
        4 => of(collision_point(&ctx, ax, ay, t, false, Some(me)).expect("point")),
        5 => of(collision_rectangle(&ctx, ax, ay, bx, by, t, true, None).expect("rect")),
        6 => of(collision_rectangle(&ctx, ax, ay, bx, by, t, false, Some(me)).expect("rect")),
        7 => of(collision_line(&ctx, ax, ay, bx, by, t, true, None).expect("line")),
        8 => of(collision_line(&ctx, ax, ay, bx, by, t, false, Some(me)).expect("line")),
        _ => unreachable!(),
    }
}

/// The script query `n` on the probe.
fn scripted(g: &mut World2, n: u8, a: [f64; 4], tgt: &Value) -> f64 {
    let me = g.probe;
    let inst = g.w.get_mut(me).expect("probe");
    for (i, v) in a.iter().enumerate() {
        inst.vars
            .insert(VarId(u32::try_from(i).unwrap()), Value::Real(r(*v)));
    }
    inst.vars.insert(VarId(4), tgt.clone());
    g.scripts.begin_tick();
    let scripts = g.scripts.clone();
    let mut ctx = g.ctx();
    scripts
        .on_event(&mut ctx, me, &EventKind::User(n))
        .unwrap_or_else(|e| panic!("{}: {e}", QUERIES[usize::from(n)]));
    match g.w.get(me).expect("probe").vars.get(&VarId(5)) {
        Some(Value::Real(v)) => v.get(),
        other => panic!("res is {other:?}"),
    }
}

/// The sample points: a sweep over the room's corner with half-pixel steps (C12 rounding).
fn samples() -> Vec<[f64; 4]> {
    let mut out = Vec::new();
    let mut y = -4.0;
    while y < 100.0 {
        let mut x = -4.0;
        while x < 104.0 {
            out.push([x, y, x + 13.5, y + 7.0]);
            x += 2.5;
        }
        y += 3.5;
    }
    out
}

fn actual(g: &mut World2) -> String {
    let mut out = String::new();
    let pts = samples();
    for n in 0..10u8 {
        let targets: Vec<(String, Value, Target)> = if n == 1 {
            vec![("solid".into(), Value::None, Target::Any)]
        } else {
            g.targets.clone()
        };
        for (name, tv, t) in &targets {
            let mut h = blake3::Hasher::new();
            let mut hits = 0u32;
            for a in &pts {
                let want = builtin(g, n, *a, *t);
                let got = scripted(g, n, *a, tv);
                assert!(
                    want.to_bits() == got.to_bits(),
                    "{} {name} at {a:?}: script {got}, built-in {want}",
                    QUERIES[usize::from(n)]
                );
                hits += u32::from(got > 0.0);
                h.update(&got.to_le_bytes());
            }
            assert!(
                hits > 0 && (hits as usize) < pts.len(),
                "{} {name}: {hits} of {} samples hit; the sweep tests nothing",
                QUERIES[usize::from(n)],
                pts.len()
            );
            let hex = h.finalize().to_hex();
            let _ = writeln!(
                out,
                "{} hits={hits} {} {name}",
                &hex.as_str()[..32],
                QUERIES[usize::from(n)]
            );
        }
    }
    out
}

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "the test, not the sim, reads UPDATE_CONFORMANCE and rewrites the pinned file"
)]
fn script_collision_queries_match_the_builtin_queries() {
    let mut g = world();
    let got = actual(&mut g);
    #[cfg(not(target_family = "wasm"))]
    if std::env::var_os("UPDATE_CONFORMANCE").is_some() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/script_conformance/collision.txt"
        );
        std::fs::write(path, &got).expect("write collision.txt");
        return;
    }
    assert_eq!(
        got,
        EXPECTED.replace("\r\n", "\n"),
        "UPDATE_CONFORMANCE=1 rewrites collision.txt"
    );
}

/// The destroyed wall (R22) is never found, and the derived gem ranks before the gem it
/// overlaps (R21): checked on the built-in side, which the sweep ties the scripts to.
#[test]
fn the_world_exercises_r21_and_r22() {
    let mut g = world();
    let ctx = g.ctx();
    assert_eq!(
        collision_point(&ctx, r(75.0), r(75.0), Target::Any, false, None).unwrap(),
        None,
        "R22: destroyed this step"
    );
    let first = collision_point(&ctx, r(37.5), r(25.5), Target::Any, true, None)
        .unwrap()
        .expect("gems overlap here");
    assert_eq!(
        g.w.get(first).unwrap().kind,
        KindId(3),
        "R21: gem[v=1] ranks first"
    );
    let ctx = g.ctx();
    assert_eq!(
        collision_point(&ctx, r(60.0), r(50.0), Target::Any, true, None).unwrap(),
        None,
        "#926: a precise query misses the empty mask"
    );
}

#[test]
fn bad_targets_and_dead_handles_are_script_errors() {
    let mut g = world();
    for (src, needle) in [
        (
            "fn user(n) { collision_point(1.0, 1.0, \"nope\"); }",
            "neither a role nor a kind",
        ),
        (
            "fn user(n) { collision_point(1.0, 1.0, 3.0); }",
            "the target must be",
        ),
        (
            "fn user(n) { destroy(this); place_free(this, 0.0, 0.0); }",
            "no longer exists",
        ),
    ] {
        g.def.scripts[0].source = src.into();
        let scripts = ScriptRuntime::new(&g.def, &g.rules).expect("compiles");
        g.scripts = scripts.clone();
        let me = g.probe;
        let mut ctx = g.ctx();
        let err = scripts
            .on_event(&mut ctx, me, &EventKind::User(0))
            .expect_err(src);
        assert!(err.message.contains(needle), "{src}: {}", err.message);
        g = world();
    }
}
