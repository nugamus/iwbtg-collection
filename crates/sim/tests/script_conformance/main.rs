//! Script conformance suite (decision 0001 "Tests"; WP9.4, #344).
//!
//! Every IWS math function and every operator overload runs over 10,000 fixed inputs, and the
//! results are hashed per function. The hash list and the operation-count fixtures live in
//! `expected.txt` and are compiled into the test, so every CI target (native Linux and Windows,
//! wasm32-wasip1) checks the same bytes: a function that gives another bit on one target fails
//! there. `UPDATE_CONFORMANCE=1 cargo test -p iwbtg-sim --test script_conformance` rewrites the
//! file; the diff is the review.

use std::fmt::Write as _;

use iwbtg_sim::rng::Rng;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::host::Iws;

const N: usize = 10_000;
const EXPECTED: &str = include_str!("expected.txt");

/// Fixed inputs: special values first, then a mix of ranges from a xorshift stream.
fn floats() -> Vec<f64> {
    let mut v = vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        -0.5,
        1e-13,
        -1e-13,
        2e-13,
        0.1,
        0.2,
        0.3,
        90.0,
        180.0,
        270.0,
        360.0,
        -360.0,
        1e6,
        -1e6,
        1e15,
        1e300,
        -1e300,
        5e-324,
        2.5,
        3.5,
        -2.5,
        -3.5,
        0.499_999_999_999_999_94,
        1.5,
        255.0,
    ];
    let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
    while v.len() < N {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        #[allow(clippy::cast_precision_loss, reason = "a fixed test stream")]
        let unit = (s >> 11) as f64 / (1u64 << 53) as f64;
        let x = match v.len() % 4 {
            0 => unit * 2.0 - 1.0,
            1 => (unit * 2.0 - 1.0) * 1000.0,
            2 => (unit * 2.0 - 1.0) * 720.0,
            _ => {
                let f = f64::from_bits(s);
                if f.is_finite() { f } else { 0.0 }
            }
        };
        v.push(x);
    }
    v
}

/// Fixed integers, with the extremes that overflow.
fn ints() -> Vec<i64> {
    let mut v = vec![
        0,
        1,
        -1,
        2,
        -2,
        7,
        -7,
        63,
        64,
        65,
        1000,
        i64::MAX,
        i64::MIN,
        i64::MAX - 1,
        i64::MIN + 1,
    ];
    let mut s: u64 = 0xD1B5_4A32_D192_ED03;
    while v.len() < N {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        #[allow(clippy::cast_possible_wrap, reason = "a fixed test stream")]
        let x = match v.len() % 3 {
            0 => (s % 2001) as i64 - 1000,
            1 => (s % 2_000_001) as i64 - 1_000_000,
            _ => s as i64,
        };
        v.push(x);
    }
    v
}

/// How a result becomes a float for hashing.
#[derive(Clone, Copy)]
enum Out {
    Float,
    Bool,
    Int,
}

use Out::{Bool, Float, Int};

/// One row: a name, one letter per parameter (`f` float, `i` int; parameters are `a`..`d`), the
/// script expression and what it returns.
struct Row {
    name: String,
    args: &'static str,
    expr: String,
    out: Out,
}

fn row(name: &str, args: &'static str, expr: &str, out: Out) -> Row {
    Row {
        name: name.to_owned(),
        args,
        expr: expr.to_owned(),
        out,
    }
}

fn rows() -> Vec<Row> {
    let mut r = vec![
        // Comparisons and truth.
        row("gm_eq", "ff", "gm_eq(a, b)", Bool),
        row("gm_ne", "ff", "gm_ne(a, b)", Bool),
        row("gm_lt", "ff", "gm_lt(a, b)", Bool),
        row("gm_le", "ff", "gm_le(a, b)", Bool),
        row("gm_gt", "ff", "gm_gt(a, b)", Bool),
        row("gm_ge", "ff", "gm_ge(a, b)", Bool),
        row("gm_bool", "f", "gm_bool(a)", Bool),
        row("gm_repeat", "f", "gm_repeat(a)", Int),
        // Conversions, division, rounding.
        row("to_float", "i", "to_float(a)", Float),
        row("to_int", "f", "to_int(a)", Int),
        row("div", "ff", "div(a, b)", Float),
        row("mod", "ff", "mod(a, b)", Float),
        row("round", "f", "round(a)", Float),
        row("floor", "f", "floor(a)", Float),
        row("ceil", "f", "ceil(a)", Float),
        row("sign", "f", "sign(a)", Float),
        row("min", "ff", "min(a, b)", Float),
        row("max", "ff", "max(a, b)", Float),
        row("abs", "f", "abs(a)", Float),
        row("frac", "f", "frac(a)", Float),
        row("clamp", "fff", "clamp(a, b, c)", Float),
        row("lerp", "fff", "lerp(a, b, c)", Float),
        // Transcendentals (libm in software).
        row("sqrt", "f", "sqrt(a)", Float),
        row("sqr", "f", "sqr(a)", Float),
        row("pow", "ff", "pow(a, b)", Float),
        row("exp", "f", "exp(a)", Float),
        row("ln", "f", "ln(a)", Float),
        row("sin", "f", "sin(a)", Float),
        row("cos", "f", "cos(a)", Float),
        row("tan", "f", "tan(a)", Float),
        row("arctan2", "ff", "arctan2(a, b)", Float),
        row("degtorad", "f", "degtorad(a)", Float),
        row("radtodeg", "f", "radtodeg(a)", Float),
        row("dsin", "f", "dsin(a)", Float),
        row("dcos", "f", "dcos(a)", Float),
        row("darctan2", "ff", "darctan2(a, b)", Float),
        row("point_direction", "ffff", "point_direction(a, b, c, d)", Float),
        row("point_distance", "ffff", "point_distance(a, b, c, d)", Float),
        row("lengthdir_x", "ff", "lengthdir_x(a, b)", Float),
        row("lengthdir_y", "ff", "lengthdir_y(a, b)", Float),
        // The one RNG: the draw sequence is part of the hash.
        row("random", "f", "random(a)", Float),
        row("random_range", "ff", "random_range(a, b)", Float),
        row("irandom", "f", "irandom(a)", Float),
        row("irandom_range", "ff", "irandom_range(a, b)", Float),
        row("choose", "fff", "choose([a, b, c])", Float),
    ];
    // Operators and their overloads: float and float, and the mixed forms.
    for args in ["ff", "fi", "if"] {
        for (op, out) in [
            ("+", Float),
            ("-", Float),
            ("*", Float),
            ("/", Float),
            ("%", Float),
            ("<", Bool),
            ("<=", Bool),
            (">", Bool),
            (">=", Bool),
            ("==", Bool),
            ("!=", Bool),
        ] {
            r.push(row(&format!("op {op} {args}"), args, &format!("a {op} b"), out));
        }
    }
    for (op, out) in [
        ("+", Int),
        ("-", Int),
        ("*", Int),
        ("/", Float),
        ("%", Int),
        ("&", Int),
        ("|", Int),
        ("^", Int),
        ("<<", Int),
        (">>", Int),
        ("<", Bool),
        ("<=", Bool),
        (">", Bool),
        (">=", Bool),
        ("==", Bool),
        ("!=", Bool),
    ] {
        r.push(row(&format!("op {op} ii"), "ii", &format!("a {op} b"), out));
    }
    r.push(row("op neg f", "f", "-a", Float));
    r.push(row("op neg i", "i", "-a", Int));
    r
}

fn script(r: &Row) -> String {
    let params: Vec<&str> = ["a", "b", "c", "d"].into_iter().take(r.args.len()).collect();
    let body = match r.out {
        Float => r.expr.clone(),
        Bool => format!("if {} {{ 1.0 }} else {{ 0.0 }}", r.expr),
        Int => format!("to_float({})", r.expr),
    };
    format!("fn t({}) {{ {body} }}", params.join(", "))
}

/// One bit pattern per result; every NaN is the same pattern, an error is its own marker.
fn bits(v: Result<f64, ()>) -> u64 {
    match v {
        Ok(x) if x.is_nan() => 0x7FF8_0000_0000_0000,
        Ok(x) => x.to_bits(),
        Err(()) => 0xDEAD_BEEF_DEAD_BEEF,
    }
}

#[derive(Clone, Copy)]
enum Arg {
    F(f64),
    I(i64),
}

/// Calls `t` with up to four arguments.
fn call(iws: &Iws, ast: &rhai::AST, args: &[Arg]) -> Result<f64, ()> {
    let r = match args {
        [Arg::F(a)] => iws.call_fn::<f64>(ast, "t", (*a,)),
        [Arg::I(a)] => iws.call_fn::<f64>(ast, "t", (*a,)),
        [Arg::F(a), Arg::F(b)] => iws.call_fn::<f64>(ast, "t", (*a, *b)),
        [Arg::F(a), Arg::I(b)] => iws.call_fn::<f64>(ast, "t", (*a, *b)),
        [Arg::I(a), Arg::F(b)] => iws.call_fn::<f64>(ast, "t", (*a, *b)),
        [Arg::I(a), Arg::I(b)] => iws.call_fn::<f64>(ast, "t", (*a, *b)),
        [Arg::F(a), Arg::F(b), Arg::F(c)] => iws.call_fn::<f64>(ast, "t", (*a, *b, *c)),
        [Arg::F(a), Arg::F(b), Arg::F(c), Arg::F(d)] => iws.call_fn::<f64>(ast, "t", (*a, *b, *c, *d)),
        _ => panic!("unsupported argument shape"),
    };
    r.map_err(|_| ())
}

fn hash_row(iws: &Iws, r: &Row, fl: &[f64], it: &[i64]) -> String {
    let ast = iws
        .compile(&script(r))
        .unwrap_or_else(|e| panic!("{}: {}", r.name, e.message));
    iws.begin_tick(0, 0, Rng::new(12345));
    let mut h = blake3::Hasher::new();
    let mut errors = 0u32;
    for n in 0..N {
        let args: Vec<Arg> = r
            .args
            .chars()
            .enumerate()
            .map(|(j, c)| {
                // Offset each parameter's stream so pairs do not line up with themselves.
                let idx = (n + j * 7919) % N;
                if c == 'f' {
                    Arg::F(fl[idx])
                } else {
                    Arg::I(it[idx])
                }
            })
            .collect();
        let out = call(iws, &ast, &args);
        errors += u32::from(out.is_err());
        h.update(&bits(out).to_le_bytes());
    }
    let hex = h.finalize().to_hex();
    format!("{} errors={errors} {}", &hex.as_str()[..32], r.name)
}

/// Operation-count fixtures: a script (called as `t(2.0)`) and its name.
fn op_fixtures() -> Vec<(&'static str, &'static str)> {
    vec![
        ("empty", "fn t(a) { a }"),
        ("arithmetic", "fn t(a) { a * 2.0 + 1.0 - a / 4.0 }"),
        (
            "loop100",
            "fn t(a) { let s = 0.0; for i in 0..100 { s += a; } s }",
        ),
        (
            "while50",
            "fn t(a) { let i = 0; while i < 50 { i += 1; } to_float(i) }",
        ),
        (
            "call_chain",
            "fn g(x) { sin(x) + cos(x) } fn t(a) { g(a) + g(a * 2.0) }",
        ),
        (
            "branches",
            "fn t(a) { if a > 1.0 { sqrt(a) } else if a > 0.0 { a } else { -a } }",
        ),
        (
            "array",
            "fn t(a) { let v = [a, a, a]; v.push(a); to_float(v.len()) }",
        ),
    ]
}

fn ops_line(iws: &Iws, name: &str, src: &str) -> String {
    let ast = iws
        .compile(src)
        .unwrap_or_else(|e| panic!("{name}: {}", e.message));
    iws.begin_tick(0, 0, Rng::new(1));
    iws.call_fn::<f64>(&ast, "t", (2.0_f64,))
        .unwrap_or_else(|e| panic!("{name}: {}", e.message));
    format!("ops {name} {}", iws.budget().used())
}

fn actual() -> String {
    let iws = Iws::new(RunnerRules::default().num);
    let (fl, it) = (floats(), ints());
    let mut out = String::new();
    for r in rows() {
        let _ = writeln!(out, "{}", hash_row(&iws, &r, &fl, &it));
    }
    for (name, src) in op_fixtures() {
        let _ = writeln!(out, "{}", ops_line(&iws, name, src));
    }
    out
}

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "the test, not the sim, reads UPDATE_CONFORMANCE and rewrites the pinned file"
)]
fn conformance_hashes_and_operation_counts_match_expected() {
    let got = actual();
    #[cfg(not(target_family = "wasm"))]
    if std::env::var_os("UPDATE_CONFORMANCE").is_some() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/script_conformance/expected.txt"
        );
        std::fs::write(path, &got).expect("write expected.txt");
        return;
    }
    let want = EXPECTED.replace("\r\n", "\n");
    if got != want {
        let diff: Vec<String> = got
            .lines()
            .zip(want.lines().chain(std::iter::repeat("<missing>")))
            .filter(|(g, w)| g != w)
            .map(|(g, w)| format!("  got  {g}\n  want {w}"))
            .collect();
        panic!(
            "conformance differs ({} lines; UPDATE_CONFORMANCE=1 rewrites expected.txt):\n{}",
            diff.len(),
            diff.join("\n")
        );
    }
}

#[test]
fn eval_and_time_functions_do_not_exist() {
    let iws = Iws::new(RunnerRules::default().num);
    for src in [
        "fn t() { eval(\"1\") }",
        "fn t() { timestamp() }",
        "fn t() { sin(2.0 ** 2.0) }",
    ] {
        assert!(
            iws.compile(src)
                .ok()
                .and_then(|a| iws.call_fn::<f64>(&a, "t", ()).ok())
                .is_none(),
            "{src} must not run"
        );
    }
}
