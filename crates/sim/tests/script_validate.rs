//! WP9.3 (#343): the script validator. Each check has a script it accepts and one it rejects.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, KindDef, RoomDef, ScriptDef};
use iwbtg_sim::script::validate::{Issue, Severity, validate};

fn run(source: &str) -> Vec<Issue> {
    let mut b = GameDefBuilder::new("wp9v");
    let room = b.room(RoomDef::new("room", 320, 240));
    b.kind(KindDef::new("orb", Builtin::Block));
    b.start_room(room);
    let mut def = b.build().expect("def");
    def.scripts.push(ScriptDef {
        name: "scripts/t.rhai".into(),
        source: source.into(),
    });
    validate(&def)
}

fn has(issues: &[Issue], sev: Severity, needle: &str) -> bool {
    issues
        .iter()
        .any(|i| i.severity == sev && i.message.contains(needle))
}

fn clean(source: &str) {
    let issues = run(source);
    assert!(issues.is_empty(), "expected no issues, got {issues:#?}");
}

#[test]
fn a_good_script_is_clean() {
    clean(
        "const SPEED = 4.0;\n\
         fn create() { this.hp = 20.0; this.set_alarm(0, 50); }\n\
         fn step() { if gm_le(this.hp, 0.0) { destroy(this); } }\n\
         fn alarm(n) { if n == 0 { instance_create(\"orb\", this.x, this.y); } }\n\
         fn collide(other) { if other.kind == \"orb\" { destroy(other); } }\n",
    );
}

#[test]
fn rng_in_a_right_operand_is_rejected() {
    let i = run("fn step() { if this.x > 1.0 && random(1.0) < 0.5 { destroy(this); } }");
    assert!(has(&i, Severity::Error, "`random` draws from the RNG"), "{i:#?}");
    assert_eq!((i[0].line, i[0].column > 1), (1, true));
}

#[test]
fn non_short_circuit_operators_work_on_bools() {
    use iwbtg_sim::rules::RunnerRules;
    use iwbtg_sim::script::host::Iws;
    let iws = Iws::new(RunnerRules::default().num);
    assert!(!iws.eval::<bool>("true & false").expect("&"));
    assert!(iws.eval::<bool>("false | true").expect("|"));
}

#[test]
fn rng_lint_accepts_the_non_short_circuit_form() {
    clean("fn step() { let r = random(1.0); if this.x > 1.0 & (r < 0.5) { destroy(this); } }");
    // A left operand always runs in both languages.
    clean("fn step() { if random(1.0) < 0.5 && this.x > 1.0 { destroy(this); } }");
}

#[test]
fn state_change_and_script_fns_in_an_operand_are_rejected() {
    let i = run(
        "fn step() { if this.x > 1.0 || exists(kid()) && destroy_it() { } }\nfn destroy_it() { destroy(this); true }",
    );
    assert!(
        has(&i, Severity::Error, "`destroy_it` is a script function"),
        "{i:#?}"
    );
    let i = run("fn step() { if this.x > 1.0 || { this.y = 3.0; true } { } }");
    assert!(has(&i, Severity::Error, "an assignment to a property"), "{i:#?}");
    clean("fn step() { if this.x > 1.0 || pure() { } }\nfn pure() { sin(1.0) > 0.5 }");
}

#[test]
fn float_switch_is_rejected() {
    let i = run("fn alarm(n) { switch this.x { 1.5 => destroy(this), _ => {} } }");
    assert!(has(&i, Severity::Error, "`switch` on a float case"), "{i:#?}");
    clean(
        "fn alarm(n) { switch to_int(n) { 1 => destroy(this), 2 | 3 => { this.set_alarm(0, 1.5); }, _ => {} } }",
    );
}

#[test]
fn bare_number_condition_is_rejected() {
    let i = run("fn step() { if this.hspeed { destroy(this); } }");
    assert!(
        has(&i, Severity::Error, "`hspeed` is a number used as a condition"),
        "{i:#?}"
    );
    // A literal is already a compile error in Rhai.
    let i = run("fn step() { if 1 { destroy(this); } }");
    assert!(
        has(&i, Severity::Error, "Expecting a boolean expression"),
        "{i:#?}"
    );
    clean("fn step() { if gm_bool(this.hspeed) { destroy(this); } }");
}

#[test]
fn raw_float_equality_warns() {
    let i = run("fn step() { if this.x == 1.5 { destroy(this); } }");
    assert!(
        has(&i, Severity::Warning, "use `gm_eq(a, b)`") || has(&i, Severity::Warning, "Use `gm_eq(a, b)`"),
        "{i:#?}"
    );
    assert!(i.iter().all(|x| x.severity == Severity::Warning));
    clean("fn step() { if gm_eq(this.x, 1.5) { destroy(this); } }");
    clean("fn alarm(n) { if n == 3 { destroy(this); } }");
}

#[test]
fn unknown_and_misused_calls_are_rejected() {
    let i = run("fn step() { frobnicate(1.0); }");
    assert!(
        has(&i, Severity::Error, "unknown function `frobnicate`"),
        "{i:#?}"
    );
    let i = run("fn step() { this.set_alarm(0); }");
    assert!(
        has(&i, Severity::Error, "`set_alarm` is called with 2 arguments"),
        "{i:#?}"
    );
    clean("fn step() { helper(2.0); }\nfn helper(a) { this.x = a; }");
}

#[test]
fn event_names_and_arities() {
    let i = run("fn step(n) { }");
    assert!(
        has(&i, Severity::Error, "`step` takes 0 arguments, not 1"),
        "{i:#?}"
    );
    let i = run("fn alarm() { }");
    assert!(
        has(&i, Severity::Error, "`alarm` takes 1 argument, not 0"),
        "{i:#?}"
    );
    clean("fn moment_wave1() { } fn draw(d) { }");
}

#[test]
fn top_level_items_are_only_fn_and_const() {
    let i = run("let x = 1;\nfn step() { }");
    assert!(has(&i, Severity::Error, "only `fn` and `const`"), "{i:#?}");
    assert_eq!(i[0].line, 1);
    let i = run("destroy(kid());");
    assert!(has(&i, Severity::Error, "only `fn` and `const`"), "{i:#?}");
}

#[test]
fn string_literals_must_resolve() {
    let i = run("fn step() { instance_create(\"orbb\", 1, 2); }");
    assert!(has(&i, Severity::Error, "unknown kind \"orbb\""), "{i:#?}");
    let i = run("fn collide(other) { if other.kind == \"nope\" { destroy(other); } }");
    assert!(has(&i, Severity::Error, "unknown kind \"nope\""), "{i:#?}");
    let i = run("fn step() { for k in instances(\"ghost\") { destroy(k); } }");
    assert!(has(&i, Severity::Error, "unknown kind \"ghost\""), "{i:#?}");
}

#[test]
fn disabled_symbols_and_syntax_errors_are_rejected() {
    assert!(has(
        &run("fn step() { let a = 2 ** 3; }"),
        Severity::Error,
        "compile error"
    ));
    assert!(has(&run("fn step() { eval(\"1\"); }"), Severity::Error, ""));
    assert!(has(
        &run("fn step() { let a = ; }"),
        Severity::Error,
        "compile error"
    ));
    assert!(has(
        &run("fn step() { nothere = 1; }"),
        Severity::Error,
        "compile error"
    ));
}
