//! Integer arguments to the numeric script API (#387). The engine has integers and floats, and
//! a script may write `irandom(10)` as well as `irandom(10.0)`: every numeric function takes
//! either, and gives the same result.

use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::host::Iws;
use rhai::Dynamic;

/// Every numeric function, with float arguments written with `{}` placeholders for the integer
/// or float spelling of 3 and 7.
const CALLS: &[&str] = &[
    "gm_eq({3}, {7})",
    "gm_ne({3}, {7})",
    "gm_lt({3}, {7})",
    "gm_le({3}, {7})",
    "gm_gt({3}, {7})",
    "gm_ge({3}, {7})",
    "gm_bool({3})",
    "gm_repeat({3})",
    "to_int({7})",
    "div({7}, {3})",
    "mod({7}, {3})",
    "round({3})",
    "floor({3})",
    "ceil({3})",
    "sign({3})",
    "min({3}, {7})",
    "max({3}, {7})",
    "abs({3})",
    "frac({3})",
    "clamp({7}, {3}, {7})",
    "lerp({3}, {7}, {3})",
    "sqrt({7})",
    "sqr({3})",
    "pow({3}, {7})",
    "exp({3})",
    "ln({7})",
    "sin({3})",
    "cos({3})",
    "tan({3})",
    "arctan2({3}, {7})",
    "degtorad({7})",
    "radtodeg({3})",
    "dsin({7})",
    "dcos({3})",
    "darctan2({3}, {7})",
    "point_direction({3}, {7}, {7}, {3})",
    "point_distance({3}, {7}, {7}, {3})",
    "lengthdir_x({7}, {3})",
    "lengthdir_y({7}, {3})",
    "random({7})",
    "random_range({3}, {7})",
    "irandom({7})",
    "irandom_range({3}, {7})",
];

fn run(src: &str) -> String {
    let iws = Iws::new(RunnerRules::default().num);
    let v: Dynamic = iws
        .rhai_engine()
        .eval::<Dynamic>(src)
        .unwrap_or_else(|e| panic!("`{src}` failed: {e:?}"));
    // Integer results (`to_int`, `gm_repeat`) print the same as the float spelling of the value.
    v.as_int()
        .map_or_else(|_| v.to_string(), |i| format!("{:?}", i as f64))
}

#[test]
fn every_numeric_function_takes_integers_like_floats() {
    for call in CALLS {
        let ints = call.replace("{3}", "3").replace("{7}", "7");
        let floats = call.replace("{3}", "3.0").replace("{7}", "7.0");
        assert_eq!(run(&ints), run(&floats), "`{ints}` differs from `{floats}`");
    }
}

#[test]
fn a_mixed_call_and_a_loop_counter_work() {
    assert_eq!(run("point_distance(0, 0, 3, 4.0)"), run("5.0"));
    assert_eq!(run("let s = 0.0; for i in 0..4 { s += sqr(i); } s"), run("14.0"));
}

#[test]
fn a_non_number_is_an_error_with_a_message() {
    let iws = Iws::new(RunnerRules::default().num);
    let e = iws.rhai_engine().eval::<Dynamic>("sin(\"x\")").unwrap_err();
    assert!(format!("{e:?}").contains("expected a number"), "{e:?}");
}

/// Rhai's own integer overloads (`abs`, `min`, `max`, `sign`...) must not beat ours: every
/// numeric function gives a float for integer arguments.
#[test]
fn integer_arguments_give_float_results() {
    let mut bad = Vec::new();
    for src in [
        "abs(-3)",
        "min(1, 2)",
        "max(1, 2)",
        "min(1, 2.0)",
        "max(1.0, 2)",
        "abs(-3.0)",
        "sign(-3)",
        "floor(3)",
        "ceil(3)",
        "round(3)",
        "clamp(5, 1, 3)",
        "sqr(3)",
        "lerp(1, 3, 1)",
        "frac(3)",
        "mod(7, 3)",
        "div(7, 3)",
        "random(3)",
        "irandom(3)",
        "sqrt(4)",
    ] {
        let v: Dynamic = Iws::new(RunnerRules::default().num)
            .rhai_engine()
            .eval::<Dynamic>(src)
            .unwrap_or_else(|e| panic!("`{src}` failed: {e:?}"));
        if !v.is_float() {
            bad.push(format!("`{src}` gave {}", v.type_name()));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}
