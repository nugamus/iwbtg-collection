//! `start_timeline` / `stop_timeline` in the script API (#1386): a script starts a package
//! timeline on an instance, and its moments run in the row-3 timeline phase of `timeline.rs`,
//! the same clock a kind's `timeline` key and the trigger action use. Part of the conformance
//! module, so it runs on every CI target.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, GameDef, KindDef, RoomDef, RoomId, ScriptDef, ScriptId, TimelineDef, TimelineId, VarId,
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

const M0_TICK: VarId = VarId(0);
const M2_TICK: VarId = VarId(1);

const SCRIPT: &str = r#"
fn create() {
    this.m0_tick = -1.0;
    this.m2_tick = -1.0;
}
fn user(n) {
    let n = to_int(n);
    if n == 0 { this.start_timeline("tl"); }
    else if n == 1 { stop_timeline(this); }
    else if n == 2 { let name = "tl"; start_timeline(this, name); }
    else if n == 3 { let name = "nope"; start_timeline(this, name); }
}
fn moment_m0() { this.m0_tick = tick(); }
fn moment_m2() { this.m2_tick = tick(); }
"#;

struct Game {
    def: GameDef,
    rules: RunnerRules,
    scripts: ScriptRuntime,
    w: World,
    scripted: iwbtg_sim::def::KindId,
    builtin: iwbtg_sim::def::KindId,
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

    fn spawn(&mut self, kind: iwbtg_sim::def::KindId) -> InstId {
        self.ctx()
            .spawn(kind, Real::new(10.0), Real::new(20.0), SpawnInit::default())
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

    fn tick(&mut self) {
        let mut ctx = self.ctx();
        tick::run_tick(&mut ctx, InputFrame::default()).expect("tick");
    }

    fn var(&self, me: InstId, id: VarId) -> f64 {
        match self.w.get(me).expect("holder").vars.get(&id) {
            Some(Value::Real(r)) => r.get(),
            other => panic!("no var: {other:?}"),
        }
    }
}

fn game() -> Game {
    let mut b = GameDefBuilder::new("timelines1386");
    let room = b.room(RoomDef::new("room", 320, 240));
    // The same script runs on both: one starts the timeline itself, the other gets it from its
    // kind's `timeline` key, which `Ctx::spawn` starts through `timeline::start_with_def`.
    let scripted = b.kind(KindDef::new("scripted", Builtin::Scripted));
    let mut with_timeline = KindDef::new("builtin", Builtin::Scripted);
    with_timeline.timeline = Some(TimelineId(0));
    let builtin = b.kind(with_timeline);
    b.start_room(room);
    let mut def = b.build().expect("def");
    def.timelines.push(TimelineDef {
        name: "tl".into(),
        moments: vec![0, 2],
        calls: vec!["m0".into(), "m2".into()],
        looping: false,
    });
    def.scripts.push(ScriptDef {
        name: "scripts/timelines.rhai".into(),
        source: SCRIPT.into(),
    });
    def.kinds[scripted.0 as usize].script = Some(ScriptId(0));
    def.kinds[builtin.0 as usize].script = Some(ScriptId(0));
    def.var_names.insert("m0_tick".into(), M0_TICK);
    def.var_names.insert("m2_tick".into(), M2_TICK);
    let issues = validate(&def);
    assert!(
        issues.is_empty(),
        "the validator accepts the timeline API: {issues:?}"
    );
    let rules = RunnerRules::default();
    let scripts = ScriptRuntime::new(&def, &rules).expect("scripts compile");
    Game {
        def,
        rules,
        scripts,
        w: World::new(RoomId(0), 11),
        scripted,
        builtin,
    }
}

/// #1386: a timeline a script starts runs its moments on the same ticks as one started by the
/// kind's `timeline` key (both go through `timeline::start_with_def` and `timeline::advance`).
/// Moment 0 lands on the first row-3 advance; moment 2 two ticks later.
#[test]
fn a_script_started_timeline_uses_the_builtin_clock() {
    let mut g = game();
    let builtin = g.spawn(g.builtin);
    let scripted = g.spawn(g.scripted);
    // Start at tick 0, as the kind default did for `builtin` during spawn.
    g.user(scripted, 0).expect("start_timeline");
    for _ in 0..3 {
        g.tick();
    }
    assert_eq!(g.var(builtin, M0_TICK), 0.0);
    assert_eq!(g.var(scripted, M0_TICK), 0.0);
    assert_eq!(g.var(builtin, M2_TICK), 2.0);
    assert_eq!(g.var(scripted, M2_TICK), 2.0);
}

/// #1386: the free form resolves a non-literal name at run time and starts the same run.
#[test]
fn a_non_literal_timeline_name_starts_the_run() {
    let mut g = game();
    let me = g.spawn(g.scripted);
    g.user(me, 2).expect("start_timeline with a variable");
    g.tick();
    assert_eq!(g.var(me, M0_TICK), 0.0);
}

/// #1386: `stop_timeline` stops the advance and keeps the position; the later moment never runs.
#[test]
fn stop_timeline_halts_the_run() {
    let mut g = game();
    let me = g.spawn(g.scripted);
    g.user(me, 0).expect("start_timeline");
    g.tick();
    assert_eq!(g.var(me, M0_TICK), 0.0);
    g.user(me, 1).expect("stop_timeline");
    for _ in 0..3 {
        g.tick();
    }
    assert_eq!(g.var(me, M0_TICK), 0.0, "the run did not restart");
    assert_eq!(g.var(me, M2_TICK), -1.0, "moment 2 never ran");
}

/// #1386: an unknown name that is not a literal is a script error at run time.
#[test]
fn an_unknown_non_literal_name_is_a_runtime_error() {
    let mut g = game();
    let me = g.spawn(g.scripted);
    let err = g.user(me, 3).expect_err("unknown timeline");
    assert!(err.contains("unknown timeline"), "{err}");
}

/// #1386: a literal naming no timeline is a load error.
#[test]
fn the_validator_checks_a_literal_timeline_name() {
    let mut def = game().def;
    def.scripts[0].source = "fn step() { this.start_timeline(\"nope\"); }".into();
    let issues = validate(&def);
    assert!(
        issues
            .iter()
            .any(|i| i.message.contains("unknown timeline \"nope\"")),
        "{issues:?}"
    );
}
