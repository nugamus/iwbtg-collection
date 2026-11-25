//! Paths in the script API (#1530): a script starts a path, automatic motion advances it, and
//! the instance's `path_*` properties read and write the run. Part of the conformance module, so
//! it runs on every CI target.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, GameDef, KindDef, KindId, PathDef, RoomDef, RoomId, ScriptDef, ScriptId, VarId,
};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::validate::validate;
use iwbtg_sim::script::{ScriptHost, ScriptRuntime};
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::tick;
use iwbtg_sim::world::{Ctx, InstId, SpawnInit, Value, World};

const SCRIPT: &str = r#"
fn user(n) {
    let n = to_int(n);
    if n == 0 { this.path_start("line", 4.0, "stop", false); }
    else if n == 1 { this.path_start("line", 4.0, "restart", true); }
    else if n == 2 { this.path_end(); }
    else if n == 3 { this.path_speed = 8.0; }
    else if n == 4 { this.path_endaction = 3.0; }
    else if n == 5 { this.path_scale = 2.0; }
    else if n == 6 { this.res = this.path_index; }
    else if n == 7 { this.res = this.path_position; }
    else if n == 8 { this.res = this.path_positionprevious; }
    else if n == 9 { this.res = this.path_x; }
    else if n == 10 { this.res = this.path_y; }
    else if n == 11 { this.res = this.path_speed; }
    else if n == 12 { this.res = this.path_endaction; }
    else if n == 13 { this.res = this.path_orientation; }
    else if n == 14 { this.res = this.path_scale; }
    else if n == 15 { this.path_position = 0.5; }
    else if n == 16 { this.path_orientation = 90.0; }
    else if n == 17 { let p = "no" + "pe"; this.path_start(p, 4.0, "stop", false); }
    else if n == 18 { this.path_scale = 1.0; }
}
"#;

const RES: VarId = VarId(0);

struct Game {
    def: GameDef,
    rules: RunnerRules,
    scripts: ScriptRuntime,
    w: World,
    kind: KindId,
}

impl Game {
    fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            def: &self.def,
            rules: &self.rules,
            w: &mut self.w,
            scripts: &self.scripts,
            dll_mode: DllMode::Stub,
        }
    }

    /// Spawns the holder at (100, 100).
    fn holder(&mut self) -> InstId {
        let kind = self.kind;
        self.ctx()
            .spawn(kind, Real::new(100.0), Real::new(100.0), SpawnInit::default())
            .expect("spawn")
    }

    fn user(&mut self, me: InstId, n: u8) -> Result<(), String> {
        let mut ctx = self.ctx();
        let scripts = ctx.scripts;
        scripts
            .on_event(&mut ctx, me, &EventKind::User(n))
            .map(|_| ())
            .map_err(|e| e.message)
    }

    /// One full tick: automatic motion advances a running path, then the events run.
    fn tick(&mut self) {
        let mut c = self.ctx();
        tick::run_tick(&mut c, InputFrame::default()).expect("tick");
    }

    fn res(&self, me: InstId) -> f64 {
        match self.w.get(me).expect("holder").vars.get(&RES) {
            Some(Value::Real(r)) => r.get(),
            other => panic!("no res: {other:?}"),
        }
    }

    fn pos(&self, me: InstId) -> (f64, f64) {
        let inst = self.w.get(me).expect("holder");
        (inst.m.x().get(), inst.m.y().get())
    }
}

fn game() -> Game {
    let mut b = GameDefBuilder::new("paths1530");
    let room = b.room(RoomDef::new("room", 320, 240));
    let kind = b.kind(KindDef::new("holder", Builtin::Block));
    b.start_room(room);
    let mut def = b.build().expect("def");
    def.paths.push(PathDef {
        name: "line".into(),
        // 40 px along +x: speed 4 advances path_position by 0.1 a tick, speed 8 by 0.2.
        points: vec![
            (Real::ZERO, Real::ZERO, Real::ONE),
            (Real::new(40.0), Real::ZERO, Real::ONE),
        ],
        closed: false,
        smooth: false,
        precision: 4,
    });
    def.scripts.push(ScriptDef {
        name: "scripts/paths.rhai".into(),
        source: SCRIPT.into(),
    });
    def.kinds[kind.0 as usize].script = Some(ScriptId(0));
    def.var_names.insert("res".into(), RES);
    let issues = validate(&def);
    assert!(issues.is_empty(), "the validator accepts paths: {issues:?}");
    let rules = RunnerRules::default();
    let scripts = ScriptRuntime::new(&def, &rules).expect("scripts compile");
    Game {
        def,
        rules,
        scripts,
        w: World::new(RoomId(0), 7),
        kind,
    }
}

fn approx(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

/// #1530: a relative path starts at the instance, advances one step a tick, and `path_position`,
/// `path_positionprevious`, `path_x` and `path_y` describe the tick.
#[test]
fn a_relative_path_starts_advances_and_ends() {
    let mut g = game();
    let me = g.holder();
    g.user(me, 0).expect("path_start");
    for _ in 0..5 {
        g.tick();
    }
    // 5 ticks at 4 px on a 40 px path: position 0.5, 20 px along.
    approx(g.pos(me).0, 120.0);
    approx(g.pos(me).1, 100.0);
    for (n, want) in [
        (7, 0.5),  // path_position
        (8, 0.4),  // path_positionprevious
        (9, 20.0), // path_x
        (10, 0.0), // path_y
        (11, 4.0), // path_speed
        (12, 0.0), // path_endaction: stop
        (13, 0.0), // path_orientation
        (14, 1.0), // path_scale
        (6, 0.0),  // path_index
    ] {
        g.user(me, n).expect("read");
        approx(g.res(me), want);
    }
}

/// #1530: `path_end` stops the run; the properties fall back to GM's defaults.
#[test]
fn path_end_stops_the_run() {
    let mut g = game();
    let me = g.holder();
    g.user(me, 0).expect("path_start");
    for _ in 0..5 {
        g.tick();
    }
    let (x, _) = g.pos(me);
    g.user(me, 2).expect("path_end");
    for _ in 0..3 {
        g.tick();
    }
    approx(g.pos(me).0, x);
    for (n, want) in [(6, -1.0), (7, 0.0), (11, 0.0), (14, 1.0)] {
        g.user(me, n).expect("read");
        approx(g.res(me), want);
    }
}

/// #1530: the end action runs. `restart` on an absolute path wraps the position and puts the
/// instance back at the path's start.
#[test]
fn an_absolute_restart_path_wraps() {
    let mut g = game();
    let me = g.holder();
    g.user(me, 1).expect("path_start");
    // The running 0.1 sum reaches 0.9999999999999999 after ten ticks (as the trace shows); the
    // eleventh passes 1.0 and restarts, keeping the remainder.
    for _ in 0..11 {
        g.tick();
    }
    g.user(me, 7).expect("path_position");
    approx(g.res(me), 0.1);
    approx(g.pos(me).0, 4.0);
    approx(g.pos(me).1, 0.0);
}

/// #1530: writing `path_speed`, `path_position`, `path_scale` and `path_orientation` changes the
/// advance; `path_endaction` writes read back as GM's code.
#[test]
fn the_writable_properties_change_the_advance() {
    let mut g = game();
    let me = g.holder();
    g.user(me, 0).expect("path_start");
    g.tick();
    approx(g.pos(me).0, 104.0);
    // 8 px a tick from 0.1: 0.3, 12 px along.
    g.user(me, 3).expect("path_speed = 8");
    g.tick();
    approx(g.pos(me).0, 112.0);
    // From 0.5: 0.7, 28 px along.
    g.user(me, 15).expect("path_position = 0.5");
    g.tick();
    approx(g.pos(me).0, 128.0);
    // The same step at scale 2: 56 px along.
    g.user(me, 5).expect("path_scale = 2");
    g.user(me, 15).expect("path_position = 0.5");
    g.tick();
    approx(g.pos(me).0, 156.0);
    // Scale back to 1 and orient the path 90 degrees: the same 28 px are no longer +x but -y.
    g.user(me, 18).expect("path_scale = 1");
    g.user(me, 16).expect("path_orientation = 90");
    g.user(me, 15).expect("path_position = 0.5");
    g.tick();
    approx(g.pos(me).0, 100.0);
    approx(g.pos(me).1, 72.0);
    for (n, want) in [(13, 90.0), (14, 1.0)] {
        g.user(me, n).expect("read");
        approx(g.res(me), want);
    }
    g.user(me, 4).expect("path_endaction = reverse");
    g.user(me, 12).expect("path_endaction");
    approx(g.res(me), 3.0);
}

/// #1530: an unknown path name is a script error at run time (a literal is caught at load).
#[test]
fn an_unknown_path_is_a_script_error() {
    let mut g = game();
    let me = g.holder();
    let e = g.user(me, 17).expect_err("unknown path");
    assert!(e.contains("unknown path `nope`"), "{e}");
}

/// #1530: a literal path name is checked by the validator.
#[test]
fn the_validator_checks_a_literal_path_name() {
    let mut g = game();
    g.def.scripts[0].source = "fn user(n) { this.path_start(\"nope\", 4.0, \"stop\", false); }".into();
    let issues = validate(&g.def);
    assert!(issues.iter().any(|i| i.message.contains("nope")), "{issues:?}");
}
