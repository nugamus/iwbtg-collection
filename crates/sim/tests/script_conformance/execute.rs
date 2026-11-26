//! `script_execute` across two script files (#1533): the literal target resolves against the
//! whole package, `this` stays the calling instance, and arguments/returns pass through. Part of
//! the conformance module, so it runs on every CI target.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, GameDef, KindDef, KindId, RoomDef, RoomId, ScriptDef, ScriptId, VarId};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::validate::validate;
use iwbtg_sim::script::{ScriptHost, ScriptRuntime};
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, SpawnInit, Value, World};

/// The kind's script: the caller.
const CALLER: &str = r#"
fn user(n) {
    let n = to_int(n);
    if n == 0 { this.res = script_execute("double", 5.0); }
    else if n == 1 { script_execute("set_res", 7.0); }
    else if n == 2 { this.res = script_execute("who"); }
    else if n == 3 { this.res = script_execute("add", 1.0, 2.0, 3.0); }
    else { this.res = script_execute("outer"); }
}
"#;

/// A second file: the callees. `set_res` proves `this` is the caller's instance.
const HELPERS: &str = r#"
fn double(x) { x * 2.0 }
fn set_res(x) { this.res = x; }
fn who() { 42.0 }
fn add(a, b, c) { a + b + c }
fn outer() { script_execute("double", 3.0) }
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

    fn holder(&mut self) -> InstId {
        let kind = self.kind;
        self.ctx()
            .spawn(
                kind,
                iwbtg_sim::num::Real::ZERO,
                iwbtg_sim::num::Real::ZERO,
                SpawnInit::default(),
            )
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

    fn res(&self, me: InstId) -> Option<f64> {
        match self.w.get(me).expect("holder").vars.get(&RES) {
            Some(Value::Real(r)) => Some(r.get()),
            _ => None,
        }
    }
}

fn game() -> Game {
    let mut b = GameDefBuilder::new("execute1533");
    let room = b.room(RoomDef::new("room", 320, 240));
    let kind = b.kind(KindDef::new("holder", Builtin::Block));
    b.start_room(room);
    let mut def = b.build().expect("def");
    def.scripts.push(ScriptDef {
        name: "scripts/caller.rhai".into(),
        source: CALLER.into(),
    });
    def.scripts.push(ScriptDef {
        name: "scripts/helpers.rhai".into(),
        source: HELPERS.into(),
    });
    def.kinds[kind.0 as usize].script = Some(ScriptId(0));
    def.var_names.insert("res".into(), RES);
    let issues = validate(&def);
    assert!(issues.is_empty(), "the validator accepts the pair: {issues:?}");
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

/// #1533: the callee's return value comes back, with `this` bound to the caller.
#[test]
fn a_two_file_call_returns_a_value_and_keeps_this() {
    let mut g = game();
    let me = g.holder();
    g.user(me, 0).expect("double");
    approx(g.res(me).expect("res"), 10.0);
    g.user(me, 1).expect("set_res");
    approx(g.res(me).expect("res"), 7.0);
    g.user(me, 2).expect("who");
    approx(g.res(me).expect("res"), 42.0);
    g.user(me, 3).expect("add");
    approx(g.res(me).expect("res"), 6.0);
}

/// #1533: a callee may itself call `script_execute`; the budget, world and `this` are shared.
#[test]
fn a_callee_can_call_again() {
    let mut g = game();
    let me = g.holder();
    g.user(me, 4).expect("outer");
    approx(g.res(me).expect("res"), 6.0);
}

/// #1533: the validator resolves the literal target and the argument count across the package.
#[test]
fn the_validator_resolves_the_literal_target() {
    let mut g = game();
    for (source, want) in [
        (
            "fn user(n) { script_execute(\"nope\", 1.0); }",
            "no script function `nope` taking 1 argument",
        ),
        (
            "fn user(n) { script_execute(\"double\"); }",
            "no script function `double` taking 0 arguments",
        ),
        (
            "fn user(n) { let f = \"double\"; script_execute(f, 1.0); }",
            "needs a literal script name",
        ),
    ] {
        g.def.scripts[0].source = source.into();
        let issues = validate(&g.def);
        assert!(
            issues.iter().any(|i| i.message.contains(want)),
            "{source}: {issues:?}"
        );
    }
}
