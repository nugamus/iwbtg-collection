//! GML string functions (#1532): 1-based indices and the GM8 `string()` formatting
//! (NUM-053/NUM-054). Part of the conformance module, so it runs on every CI target.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, GameDef, KindDef, KindId, RoomDef, RoomId, ScriptDef, ScriptId, VarId};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::{ScriptHost, ScriptRuntime};
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, SpawnInit, Value, World};

const SCRIPT: &str = r#"
fn user(n) {
    let n = to_int(n);
    if n < 10 {
        if n == 0 { this.name = string(12.0); }
        else if n == 1 { this.name = string(1.5); }
        else if n == 2 { this.name = string(0.125); }
        else if n == 3 { this.name = string(0.375); }
        else if n == 4 { this.name = string(183.125); }
        else if n == 5 { this.name = string(0.015); }
        else if n == 6 { this.name = string(183.005); }
        else if n == 7 { this.name = string(1.115); }
        else if n == 8 { this.name = string(99.995); }
        else { this.name = string(-0.001); }
    } else if n < 20 {
        if n == 10 { this.name = string(551.6999999999999); }
        else if n == 11 { this.name = string("x"); }
        else if n == 12 { this.name = string_format(1.2345, 8, 2); }
        else if n == 13 { this.name = string_format(3.0, 3, 0); }
        else if n == 14 { this.res = string_length("abc"); }
        else if n == 15 { this.name = string_char_at("abc", 2); }
        else if n == 16 { this.name = string_char_at("abc", 0); }
        else if n == 17 { this.name = string_char_at("abc", 4); }
        else if n == 18 { this.res = string_pos("bc", "abc"); }
        else { this.res = string_pos("z", "abc"); }
    } else if n < 30 {
        if n == 20 { this.name = string_copy("abcdef", 2, 3); }
        else if n == 21 { this.name = string_copy("abcdef", 5, 10); }
        else if n == 22 { this.name = string_delete("abcdef", 2, 3); }
        else if n == 23 { this.name = string_insert("XY", "ab", 1); }
        else if n == 24 { this.name = string_insert("XY", "ab", 2); }
        else if n == 25 { this.name = string_insert("XY", "ab", 9); }
        else if n == 26 { this.name = string_replace("aXa", "a", "b"); }
        else if n == 27 { this.name = string_replace_all("aXa", "a", "b"); }
        else if n == 28 { this.res = string_count("aa", "aaaa"); }
        else { this.name = string_upper("aBc"); }
    } else {
        if n == 30 { this.name = string_lower("aBc"); }
        else if n == 31 { this.name = string_digits("a1b2"); }
        else if n == 32 { this.name = string_letters("a1b2"); }
        else if n == 33 { this.name = string_lettersdigits("a1-"); }
        else if n == 34 { this.name = chr(65); }
        else if n == 35 { this.res = ord("A"); }
        else if n == 36 { this.res = ord(""); }
        else { this.name = chr(256); }
    }
}
"#;

const RES: VarId = VarId(0);
const NAME: VarId = VarId(1);

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
            .spawn(kind, Real::ZERO, Real::ZERO, SpawnInit::default())
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

    fn var(&self, me: InstId, id: VarId) -> Option<&Value> {
        self.w.get(me).expect("holder").vars.get(&id)
    }

    fn name(&self, me: InstId) -> &str {
        match self.var(me, NAME) {
            Some(Value::Str(s)) => s,
            other => panic!("no name: {other:?}"),
        }
    }

    fn res(&self, me: InstId) -> f64 {
        match self.var(me, RES) {
            Some(Value::Real(r)) => r.get(),
            other => panic!("no res: {other:?}"),
        }
    }

    /// Runs `n` and returns the stored name.
    fn run_name(&mut self, me: InstId, n: u8) -> String {
        self.user(me, n).expect("call");
        self.name(me).to_owned()
    }

    /// Runs `n` and returns `res`.
    fn run_res(&mut self, me: InstId, n: u8) -> f64 {
        self.user(me, n).expect("call");
        self.res(me)
    }
}

fn game() -> Game {
    let mut b = GameDefBuilder::new("strings1532");
    let room = b.room(RoomDef::new("room", 320, 240));
    let kind = b.kind(KindDef::new("holder", Builtin::Block));
    b.start_room(room);
    let mut def = b.build().expect("def");
    def.scripts.push(ScriptDef {
        name: "scripts/strings.rhai".into(),
        source: SCRIPT.into(),
    });
    def.kinds[kind.0 as usize].script = Some(ScriptId(0));
    def.var_names.insert("res".into(), RES);
    def.var_names.insert("name".into(), NAME);
    let issues = iwbtg_sim::script::validate::validate(&def);
    assert!(issues.is_empty(), "the validator accepts strings: {issues:?}");
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

/// NUM-053/NUM-054: an integer has no decimals; anything else has exactly two, rounded from the
/// exact binary value with ties to even.
#[test]
fn string_of_a_real_follows_the_gm8_rule() {
    let mut g = game();
    let me = g.holder();
    for (n, want) in [
        (0, "12"),
        (1, "1.50"),
        (2, "0.12"),
        (3, "0.38"),
        (4, "183.12"),
        (5, "0.01"),
        (6, "183.00"),
        (7, "1.11"),
        (8, "100.00"),
        (9, "-0.00"),
        (10, "551.70"),
        (11, "x"),
    ] {
        assert_eq!(g.run_name(me, n), want, "case {n}");
    }
}

/// `string_format(x, total, dec)` pads to `total` with `dec` decimals.
#[test]
fn string_format_pads_and_rounds() {
    let mut g = game();
    let me = g.holder();
    assert_eq!(g.run_name(me, 12), "    1.23");
    assert_eq!(g.run_name(me, 13), "  3");
}

/// The index functions are 1-based, and an out-of-range index gives "" or the unchanged string.
#[test]
fn indices_are_one_based() {
    let mut g = game();
    let me = g.holder();
    assert_eq!(g.run_res(me, 14), 3.0);
    assert_eq!(g.run_name(me, 15), "b");
    assert_eq!(g.run_name(me, 16), "");
    assert_eq!(g.run_name(me, 17), "");
    assert_eq!(g.run_res(me, 18), 2.0, "string_pos(\"bc\", \"abc\")");
    assert_eq!(g.run_res(me, 19), 0.0, "not found");
    assert_eq!(g.run_name(me, 20), "bcd");
    assert_eq!(g.run_name(me, 21), "ef", "the shorter tail");
    assert_eq!(g.run_name(me, 22), "aef");
    assert_eq!(g.run_name(me, 23), "XYab");
    assert_eq!(g.run_name(me, 24), "aXYb");
    assert_eq!(g.run_name(me, 25), "abXY");
}

/// Replace/count, case and the ASCII filters.
#[test]
fn replace_case_and_filters() {
    let mut g = game();
    let me = g.holder();
    assert_eq!(g.run_name(me, 26), "bXa", "one replacement");
    assert_eq!(g.run_name(me, 27), "bXb", "all replacements");
    assert_eq!(g.run_res(me, 28), 2.0, "non-overlapping");
    assert_eq!(g.run_name(me, 29), "ABC");
    assert_eq!(g.run_name(me, 30), "abc");
    assert_eq!(g.run_name(me, 31), "12");
    assert_eq!(g.run_name(me, 32), "ab");
    assert_eq!(g.run_name(me, 33), "a1");
}

/// `chr`/`ord` are the byte codes 0-255.
#[test]
fn chr_and_ord_use_codes() {
    let mut g = game();
    let me = g.holder();
    assert_eq!(g.run_name(me, 34), "A");
    assert_eq!(g.run_res(me, 35), 65.0);
    assert_eq!(g.run_res(me, 36), 0.0, "ord of an empty string");
    let e = g.user(me, 37).expect_err("chr past a byte");
    assert!(e.contains("chr(256) is not a character code"), "{e}");
}
