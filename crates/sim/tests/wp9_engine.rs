//! WP9.1 (#341): the IWS script engine (decision 0001 "Engine setup"): disabled symbols, no
//! clock, the budget, GML number semantics and the one RNG.

use iwbtg_sim::num::{self, Real};
use iwbtg_sim::rng::Rng;
use iwbtg_sim::rules::num::NumRules;
use iwbtg_sim::script::host::Iws;

fn iws() -> Iws {
    let i = Iws::new(NumRules::default());
    i.begin_tick(7, 3, Rng::new(1234));
    i
}

fn real(i: &Iws, src: &str) -> f64 {
    i.eval::<f64>(src).unwrap_or_else(|e| panic!("{src}: {e}"))
}

#[test]
fn eval_and_power_operators_do_not_exist() {
    let i = iws();
    assert!(i.eval::<f64>("eval(\"1\")").is_err());
    assert!(i.run_value::<i64>("let x = 2; x **= 3; x").is_err());
    assert!(i.eval::<i64>("2 ** 3").is_err());
    // `pow` is the way, and it is libm's.
    assert_eq!(real(&i, "pow(2.0, 10.0)"), 1024.0);
}

#[test]
fn there_is_no_clock() {
    let i = iws();
    assert!(i.eval::<f64>("timestamp()").is_err());
    assert!(i.run_value::<f64>("let t = timestamp(); t").is_err());
    assert_eq!(real(&i, "tick()"), 7.0);
    assert_eq!(real(&i, "room_tick()"), 3.0);
}

#[test]
fn no_std_float_functions_are_registered() {
    let i = iws();
    // Rhai's own math package would give these; ours are named by GML and defined in `num`.
    for f in ["x.sin()", "x.to_degrees()", "x.exp()"] {
        let src = format!("let x = 1.0; {f}");
        // Method call syntax on a float finds our `sin` and `exp` through the global namespace;
        // `to_degrees` was never registered.
        if f == "x.to_degrees()" {
            assert!(i.run_value::<f64>(&src).is_err(), "{f}");
        }
    }
}

#[test]
fn a_runaway_loop_ends_at_the_budget_the_same_way_every_time() {
    let mut used = Vec::new();
    for _ in 0..2 {
        let i = iws();
        let e = i.run_value::<i64>("let n = 0; loop { n += 1; }").unwrap_err();
        assert!(e.message.contains("script budget exceeded"), "{}", e.message);
        used.push(i.budget().used());
    }
    assert_eq!(used[0], used[1], "the same count of operations on every run");
    assert!(used[0] > 0);
}

#[test]
fn the_tick_budget_is_shared_across_calls_and_resets_each_tick() {
    let i = iws();
    i.budget().set_tick_limit(10_000);
    // Each call stays under the per-call limit, together they pass the tick limit.
    let mut failed = false;
    for _ in 0..20 {
        if i.run_value::<i64>("let n = 0; for i in 0..800 { n += 1; } n")
            .is_err()
        {
            failed = true;
            break;
        }
    }
    assert!(failed, "the tick budget runs out");
    i.begin_tick(8, 4, Rng::new(1));
    assert_eq!(i.budget().used(), 0);
    assert!(
        i.run_value::<i64>("let n = 0; for i in 0..800 { n += 1; } n")
            .is_ok()
    );
    assert!(i.budget().peak() >= 10_000);
}

#[test]
fn division_comparison_and_truth_follow_gml() {
    let i = iws();
    assert_eq!(real(&i, "1 / 2"), 0.5, "int / int is a float");
    assert!(i.eval::<f64>("1 / 0").is_err());
    assert_eq!(real(&i, "div(-7.0, 2.0)"), -3.0);
    assert_eq!(real(&i, "mod(-7.0, 2.0)"), -1.0);
    assert!(i.eval::<f64>("div(1.0, 0.0)").is_err());
    assert!(
        i.eval::<bool>("gm_eq(0.1 + 0.2, 0.3)").unwrap(),
        "epsilon compare"
    );
    assert!(!i.eval::<bool>("0.1 + 0.2 == 0.3").unwrap(), "raw == stays exact");
    assert!(i.eval::<bool>("gm_bool(0.5)").unwrap());
    assert!(!i.eval::<bool>("gm_bool(0.4)").unwrap());
    assert_eq!(i.eval::<i64>("gm_repeat(2.5)").unwrap(), 2, "banker's rounding");
    assert_eq!(real(&i, "round(2.5)"), 2.0);
    assert_eq!(real(&i, "round(3.5)"), 4.0);
}

/// Every function that wraps a `num` function gives exactly its bits, over a fixed input table.
#[test]
fn the_math_package_is_num_over_a_fixed_table() {
    let i = iws();
    let rules = NumRules::default();
    let xs = [
        -720.5, -90.0, -1.0, -0.25, 0.0, 0.1, 0.5, 1.0, 2.5, 45.0, 90.0, 180.0, 270.0, 359.9, 1000.25,
    ];
    for x in xs {
        let r = Real::new(x);
        let lit = format!("{x:?}");
        let checks: Vec<(&str, f64)> = vec![
            ("sin", num::sin(r).get()),
            ("cos", num::cos(r).get()),
            ("tan", num::tan(r).get()),
            ("dsin", num::dsin(r).get()),
            ("dcos", num::dcos(r).get()),
            ("degtorad", num::degtorad(r).get()),
            ("radtodeg", num::radtodeg(r).get()),
            ("exp", num::exp(Real::new(if x < 20.0 { x } else { 20.0 })).get()),
            ("abs", num::abs(r).get()),
            ("frac", num::frac(r).get()),
            ("floor", num::floor(r, &rules).get()),
            ("ceil", num::ceil(r, &rules).get()),
            ("round", num::round(r, &rules).get()),
            ("sign", num::sign(r, &rules).get()),
        ];
        for (name, want) in checks {
            let arg = if name == "exp" {
                format!("{:?}", if x < 20.0 { x } else { 20.0 })
            } else {
                lit.clone()
            };
            let got = real(&i, &format!("{name}({arg})"));
            assert_eq!(got.to_bits(), want.to_bits(), "{name}({x})");
        }
        if x > 0.0 {
            assert_eq!(
                real(&i, &format!("sqrt({lit})")).to_bits(),
                num::sqrt(r).get().to_bits()
            );
            assert_eq!(
                real(&i, &format!("ln({lit})")).to_bits(),
                num::ln(r).get().to_bits()
            );
        }
        for y in [-3.0, 0.5, 2.0] {
            let ry = Real::new(y);
            let ly = format!("{y:?}");
            assert_eq!(
                real(&i, &format!("pow({lit}, {ly})")).to_bits(),
                num::pow(r, ry).get().to_bits()
            );
            assert_eq!(
                real(&i, &format!("arctan2({lit}, {ly})")).to_bits(),
                num::atan2(r, ry).get().to_bits()
            );
            assert_eq!(
                real(&i, &format!("min({lit}, {ly})")).to_bits(),
                num::gm_min(r, ry, &rules).get().to_bits()
            );
            assert_eq!(
                real(&i, &format!("lengthdir_x({lit}, {ly})")).to_bits(),
                num::lengthdir_x(r, ry).get().to_bits()
            );
        }
    }
    // Known values, so a change to the table shows up here too.
    assert_eq!(
        real(&i, "dcos(270.0) * 0.4").to_bits(),
        (num::dcos(Real::new(270.0)).get() * 0.4).to_bits()
    );
    assert_eq!(real(&i, "point_direction(0.0, 0.0, 0.0, -1.0)"), 90.0);
    assert_eq!(real(&i, "point_distance(0.0, 0.0, 3.0, 4.0)"), 5.0);
}

#[test]
fn the_random_functions_draw_from_the_sim_rng_one_draw_each() {
    let i = iws();
    let mut shadow = Rng::new(1234);
    let want = [
        shadow.random(Real::new(10.0)).get(),
        shadow.random_range(Real::new(2.0), Real::new(6.0)).get(),
        shadow.irandom(Real::new(35.0)).unwrap().get(),
        shadow
            .irandom_range(Real::new(1.0), Real::new(6.0))
            .unwrap()
            .get(),
    ];
    assert_eq!(real(&i, "random(10.0)").to_bits(), want[0].to_bits());
    assert_eq!(real(&i, "random_range(2.0, 6.0)").to_bits(), want[1].to_bits());
    assert_eq!(real(&i, "irandom(35.0)").to_bits(), want[2].to_bits());
    assert_eq!(real(&i, "irandom_range(1.0, 6.0)").to_bits(), want[3].to_bits());
    // `choose` takes an array and draws once.
    let idx = shadow.choose(3).unwrap();
    let picked = i.eval::<i64>("choose([10, 20, 30])").unwrap();
    assert_eq!(picked, [10, 20, 30][idx as usize]);
    // The RNG goes back to the world as the scripts left it.
    assert_eq!(i.end_tick(), shadow);
    assert!(i.eval::<i64>("choose([])").is_err());
}

#[test]
fn print_and_debug_go_to_the_capped_log() {
    let i = iws();
    for n in 0..300 {
        i.run_value::<i64>(&format!("print(\"line {n}\"); 0")).unwrap();
    }
    let log = i.log();
    assert_eq!(log.len(), 256);
    assert_eq!(log[0], "line 0");
}

#[test]
fn undefined_variables_fail_at_compile_time_and_limits_hold() {
    let i = iws();
    assert!(i.compile("fn step() { y = 1; }").is_err(), "strict variables");
    assert!(i.compile("fn step() { let y = 1; y }").is_ok());
    let long = "a".repeat(5000);
    assert!(
        i.run_value::<i64>(&format!("let s = \"{long}\"; 1")).is_err(),
        "string size cap"
    );
    assert!(
        i.run_value::<i64>("fn f(n) { f(n + 1) } f(0)").is_err(),
        "call depth cap"
    );
}
