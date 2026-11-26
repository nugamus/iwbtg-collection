//! `param(name)` / `this.param(name)` in the script API (#1389): a script reads an instance's
//! parameters from the object file and the room file, with the built-ins' resolution (a room
//! value overrides the object default). Part of the conformance module, so it runs on every CI
//! target.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, KindDef, ParamValue, RoomDef, RoomId, ScriptDef, ScriptId, VarId};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::num::Real;
use iwbtg_sim::room::{EngineHost, StartPoint, start_game_with};
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::validate::validate;
use iwbtg_sim::script::{ScriptHost, ScriptRuntime};
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, SpawnInit, Value, World};

const RES: VarId = VarId(0);
const MODE_SEEN: VarId = VarId(1);
const SPEED: VarId = VarId(2);
const MODE: VarId = VarId(3);

const SCRIPT: &str = r#"
fn create() {
    // The bare form reads `this`; the method form is the same value.
    this.res = param("speed");
    this.mode_seen = this.param("mode");
}
fn user(n) {
    let n = to_int(n);
    if n == 0 { this.res = this.param("speed"); }
    else if n == 1 { this.res = param("speed"); }
    else if n == 2 { let name = "speed"; this.res = param(name); }
    else if n == 3 { let name = "nope"; this.res = param(name); }
}
"#;

struct Game {
    def: iwbtg_sim::def::GameDef,
    rules: RunnerRules,
    scripts: ScriptRuntime,
    w: World,
    kind: iwbtg_sim::def::KindId,
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

    fn ids(&self) -> Vec<InstId> {
        let kind = self.kind;
        self.w
            .instances()
            .iter()
            .filter(|i| i.kind == kind)
            .map(|i| i.id)
            .collect()
    }

    fn res(&self, id: InstId) -> f64 {
        match self.w.get(id).expect("holder").vars.get(&RES) {
            Some(Value::Real(r)) => r.get(),
            other => panic!("no res: {other:?}"),
        }
    }

    fn mode_seen(&self, id: InstId) -> String {
        match self.w.get(id).expect("holder").vars.get(&MODE_SEEN) {
            Some(Value::Str(s)) => s.clone(),
            other => panic!("no mode_seen: {other:?}"),
        }
    }

    fn user(&mut self, me: InstId, n: u8) -> Result<(), String> {
        let mut ctx = self.ctx();
        let scripts = ctx.scripts;
        scripts
            .on_event(&mut ctx, me, &EventKind::User(n))
            .map(|_| ())
            .map_err(|e| e.message)
    }
}

/// A game whose `holder` object sets the params `speed = 3.0` and `mode = "object"`, with one
/// room placement overriding `speed = 7.0` and a second placement that keeps the default.
fn game() -> Game {
    let mut b = GameDefBuilder::new("param1389");
    let room = b.room(RoomDef::new("room", 320, 240));
    let mut holder = KindDef::new("holder", Builtin::Scripted);
    holder
        .params
        .0
        .insert("speed".into(), ParamValue::Real(Real::new(3.0)));
    holder
        .params
        .0
        .insert("mode".into(), ParamValue::Str("object".into()));
    let kind = b.kind(holder);
    b.start_room(room);
    // File order: the room-file override first, the object default second.
    b.place(room, kind, 10.0, 20.0)
        .expect("place override")
        .params
        .0
        .insert("speed".into(), ParamValue::Real(Real::new(7.0)));
    b.place(room, kind, 30.0, 40.0).expect("place default");
    let mut def = b.build().expect("def");
    def.scripts.push(ScriptDef {
        name: "scripts/param.rhai".into(),
        source: SCRIPT.into(),
    });
    // The loader interns every object and placement param; the script's own variables are
    // declared too (docs/architecture/sim/world.md, "Variable ids").
    for (name, id) in [
        ("speed", SPEED),
        ("mode", MODE),
        ("res", RES),
        ("mode_seen", MODE_SEEN),
    ] {
        def.var_names.insert(name.into(), id);
    }
    def.kinds[kind.0 as usize].script = Some(ScriptId(0));
    let issues = validate(&def);
    assert!(issues.is_empty(), "the validator accepts param: {issues:?}");
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

/// #1389: the room-file value overrides the object-file default for the instance that sets it,
/// and the other instance keeps the object default. `create` reads both with the bare form and
/// the method form.
#[test]
fn a_room_override_wins_over_the_object_default() {
    let mut g = game();
    {
        let mut ctx = g.ctx();
        start_game_with(
            &mut ctx,
            &StartPoint::Room {
                room: RoomId(0),
                difficulty: 0,
            },
            &mut EngineHost,
        )
        .expect("start the room");
    }
    let ids = g.ids();
    assert_eq!(ids.len(), 2, "both placements were created");
    assert_eq!(g.res(ids[0]), 7.0, "the room-file override");
    assert_eq!(g.res(ids[1]), 3.0, "the object-file default");
    assert_eq!(g.mode_seen(ids[0]), "object");
    assert_eq!(g.mode_seen(ids[1]), "object");
}

/// #1389: after create, the method form, the bare form and a non-literal name all read the same
/// effective value.
#[test]
fn every_form_reads_the_effective_param() {
    let mut g = game();
    let kind = g.kind;
    let me = g
        .ctx()
        .spawn(kind, Real::new(10.0), Real::new(20.0), SpawnInit::default())
        .expect("spawn");
    for n in 0..=2 {
        g.user(me, n).expect("read the param");
        assert_eq!(g.res(me), 3.0, "form {n}");
    }
}

/// #1389: a name the instance has no parameter for is a script error at run time (a non-literal
/// name cannot be checked at load).
#[test]
fn a_missing_param_is_a_runtime_error() {
    let mut g = game();
    let kind = g.kind;
    let me = g
        .ctx()
        .spawn(kind, Real::new(10.0), Real::new(20.0), SpawnInit::default())
        .expect("spawn");
    let err = g.user(me, 3).expect_err("a missing param must fail");
    assert!(err.contains("not set"), "{err}");
}

/// #1389: a literal naming no parameter of the object or its room placements is a load error.
#[test]
fn a_missing_param_literal_is_a_load_error() {
    let mut def = game().def;
    def.scripts[0].source = "fn step() { this.res = param(\"nope\"); }".into();
    // `res` is declared, so the only error is the missing param literal.
    def.var_names.insert("res".into(), RES);
    let issues = validate(&def);
    assert!(
        issues
            .iter()
            .any(|i| i.message.contains("`nope` is not a parameter")),
        "{issues:?}"
    );
}
