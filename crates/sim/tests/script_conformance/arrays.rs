//! Stored arrays in scripts (#997): an array on `this` and on a global built in `create()` is read
//! in a later `step()`, with the GM index rules, the copy rule and the budget charge. Part of the
//! conformance module, so it runs on every CI target (native and wasm32).

use iwbtg_sim::def::script::ScriptRules;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, GameDef, KindDef, KindId, RoomDef, RoomId, ScriptDef, ScriptId, VarId};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::validate::validate;
use iwbtg_sim::script::{ScriptHost, ScriptRuntime};
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, SpawnInit, Value, World};

const SCRIPT: &str = r#"
fn create() {
    this.pattern = [10.0, 11.0, 12.0, 13.0, 14.0];
    global.flags = [0.0, 0.0];
    global.flags[1] = 7.0;
}
fn step() {
    this.res = this.pattern[3];
    global.res = global.flags[1] + to_float(this.pattern.len());
}
fn user(n) {
    let n = to_int(n);
    if n == 0 { this.res = this.pattern[-1]; }
    else if n == 1 { this.res = this.pattern[1.5]; }
    else if n == 2 { this.pattern[10000] = 1.0; }
    else if n == 3 { let a = this.pattern; a[0] = 99.0; this.res = this.pattern[0] + a[0]; }
    else if n == 4 { this.pattern[7] = 1.0; this.res = to_float(len(this.pattern)); }
    else if n == 5 { this.res = this.pattern[6]; }
    else if n == 6 { let global = 5.0; this.res = global; }
    else if n == 7 { this.pattern[2.0] = [1.0, [2.0, 3.0]]; this.pattern[2][1][0] = 4.0; this.res = this.pattern[2][1][0]; }
    else if n == 8 { this.res = global.never_set; }
    else if n == 9 { this.pattern = 42.0; this.res = this.pattern[0] + this.pattern[1]; }
}
"#;

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

    /// Spawns the holder; its `create()` runs inside the spawn.
    fn holder(&mut self) -> InstId {
        let kind = self.kind;
        self.ctx()
            .spawn(kind, Real::ZERO, Real::ZERO, SpawnInit::default())
            .expect("spawn runs create()")
    }

    fn run(&mut self, me: InstId, ev: &EventKind) -> Result<(), String> {
        let mut ctx = self.ctx();
        let scripts = ctx.scripts;
        scripts
            .on_event(&mut ctx, me, ev)
            .map(|_| ())
            .map_err(|e| e.message)
    }

    fn var(&self, me: InstId, id: VarId) -> Option<&Value> {
        self.w.get(me).expect("holder").vars.get(&id)
    }

    fn global(&self, name: &str) -> Option<&Value> {
        // A package name keeps its load-time id; a new one gets a run-time id.
        let id = self.def.var_names.get(name).or_else(|| self.w.var_names.get(name));
        let id = id.expect("a global the script made");
        self.w.globals.get(id)
    }
}

const PATTERN: VarId = VarId(0);
const RES: VarId = VarId(1);

fn game(script_rules: Option<ScriptRules>) -> Game {
    let mut b = GameDefBuilder::new("arrays997");
    let room = b.room(RoomDef::new("room", 320, 240));
    let mut k = KindDef::new("holder", Builtin::Block);
    k.script = Some(ScriptId(0));
    let kind = b.kind(k);
    b.start_room(room);
    let mut def = b.build().expect("def");
    def.scripts.push(ScriptDef {
        name: "scripts/holder.rhai".into(),
        source: SCRIPT.into(),
    });
    def.var_names.insert("pattern".into(), PATTERN);
    def.var_names.insert("res".into(), RES);
    let issues = validate(&def);
    assert!(issues.is_empty(), "the validator accepts arrays and `global`: {issues:?}");
    let rules = RunnerRules::default();
    let scripts = match script_rules {
        None => ScriptRuntime::new(&def, &rules),
        Some(r) => ScriptRuntime::new_with_script_rules(&def, &rules, r),
    }
    .expect("scripts compile");
    Game {
        def,
        rules,
        scripts,
        w: World::new(RoomId(0), 7),
        kind,
    }
}

fn real(v: f64) -> Value {
    Value::Real(Real::new(v))
}

fn reals(v: &[f64]) -> Value {
    Value::Array(v.iter().map(|x| real(*x)).collect())
}

#[test]
fn create_builds_five_elements_and_step_reads_element_three() {
    let mut g = game(None);
    let me = g.holder();
    assert_eq!(g.var(me, PATTERN), Some(&reals(&[10.0, 11.0, 12.0, 13.0, 14.0])));
    assert_eq!(g.global("flags"), Some(&reals(&[0.0, 7.0])));
    g.run(me, &EventKind::Step).expect("step");
    assert_eq!(g.var(me, RES), Some(&real(13.0)));
    assert_eq!(g.global("res"), Some(&real(12.0)), "flags[1] + len 5");
}

fn user(g: &mut Game, me: InstId, n: u8) -> Result<(), String> {
    g.run(me, &EventKind::User(n))
}

#[test]
fn negative_fractional_and_over_cap_indices_are_script_errors() {
    let mut g = game(None);
    let me = g.holder();
    for (n, want) in [(0, "negative index"), (1, "fractional index"), (2, "array cap exceeded")] {
        let e = user(&mut g, me, n).expect_err("a bad index halts");
        assert!(e.contains(want), "user {n}: {e}");
    }
    // A whole float index is fine (user 7 writes `[2.0]`).
    user(&mut g, me, 7).expect("whole float index and nested arrays");
    assert_eq!(g.var(me, RES), Some(&real(4.0)));
}

#[test]
fn a_local_copy_never_changes_the_stored_array() {
    let mut g = game(None);
    let me = g.holder();
    user(&mut g, me, 3).expect("copy");
    assert_eq!(g.var(me, RES), Some(&real(10.0 + 99.0)));
    assert_eq!(g.var(me, PATTERN), Some(&reals(&[10.0, 11.0, 12.0, 13.0, 14.0])));
}

#[test]
fn a_write_past_the_end_grows_and_skipped_elements_follow_the_init_policy() {
    // gm8 with uninitialized reads as 0 (the builder's default).
    let mut g = game(None);
    let me = g.holder();
    user(&mut g, me, 4).expect("grow");
    assert_eq!(g.var(me, RES), Some(&real(8.0)));
    user(&mut g, me, 5).expect("an unset element reads 0");
    assert_eq!(g.var(me, RES), Some(&real(0.0)));
    // standard: an unset element or global is a script error.
    let mut g = game(Some(ScriptRules::standard()));
    let me = g.holder();
    user(&mut g, me, 4).expect("grow");
    let e = user(&mut g, me, 5).expect_err("unset element");
    assert!(e.contains("array element 6 is not set"), "{e}");
    let e = user(&mut g, me, 8).expect_err("unset global");
    assert!(e.contains("global `never_set` is not set"), "{e}");
}

#[test]
fn a_bare_write_keeps_the_array_under_gm8_and_replaces_it_under_standard() {
    let mut g = game(None);
    let me = g.holder();
    user(&mut g, me, 9).expect("bare write");
    assert_eq!(g.var(me, RES), Some(&real(42.0 + 11.0)));
    let mut g = game(Some(ScriptRules::standard()));
    let me = g.holder();
    let e = user(&mut g, me, 9).expect_err("a real has no elements under standard");
    assert!(e.contains("Indexer unavailable"), "{e}");
}

#[test]
fn a_local_named_global_shadows_the_handle() {
    let mut g = game(None);
    let me = g.holder();
    user(&mut g, me, 6).expect("shadow");
    assert_eq!(g.var(me, RES), Some(&real(5.0)));
}

#[test]
fn reading_a_stored_array_charges_its_elements() {
    let mut g = game(None);
    let me = g.holder();
    g.w.get_mut(me).expect("holder").vars.insert(PATTERN, reals(&[1.0; 1000]));
    g.scripts.begin_tick();
    g.run(me, &EventKind::Step).expect("step");
    let used = g.scripts.engine().expect("engine").budget().used();
    // `step()` itself is a few dozen operations; reading `pattern` twice copies 2,000 elements,
    // and `len()` writes the copy back (1,000 more).
    assert!(used >= 3000, "used {used}");
}
