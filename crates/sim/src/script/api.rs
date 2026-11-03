//! The script API registered on the Rhai engine (architecture "Script API"; docs/plans/sim-core.md
//! 11). WP9.1 registers the numeric and random part; WP9.2 adds the instance API.
//!
//! Every number a script sees is a FLOAT (decision 0001, rule 5): the functions take and return
//! `f64`, and call `num` with the run's [`NumRules`](crate::rules::num::NumRules). Nothing here
//! calls platform math: sin, cos, pow and the rest are the `libm` software versions in `num`.
//! Rhai's own float functions are never registered (no `BasicMathPackage`).

use std::rc::Rc;

use rhai::{Array, Dynamic, Engine, EvalAltResult, Position};

use super::docs::Docs;
use super::host::Shared;
use crate::num::{self, NumError, Real};

type Res<T> = Result<T, Box<EvalAltResult>>;

/// A runtime error with a message, at the call's position.
pub(crate) fn runtime_error(message: impl Into<String>) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        Dynamic::from(message.into()),
        Position::NONE,
    ))
}

fn num_err(op: &str, e: NumError) -> Box<EvalAltResult> {
    runtime_error(format!("{op}: {e}"))
}

fn r(v: f64) -> Real {
    Real::new(v)
}

/// A number argument: scripts may pass an integer or a float for any numeric parameter (the
/// engine has both, `irandom(10)` and `irandom(10.0)` are the same call).
#[allow(clippy::cast_precision_loss, reason = "script integers stay far below 2^53")]
fn arg(d: &Dynamic) -> Res<f64> {
    if let Ok(f) = d.as_float() {
        Ok(f)
    } else if let Ok(i) = d.as_int() {
        Ok(i as f64)
    } else {
        Err(runtime_error(format!("expected a number, got {}", d.type_name())))
    }
}

/// A native result as a script result: plain values are `Ok`, fallible ones pass through.
trait IntoRes<T> {
    fn into_res(self) -> Res<T>;
}
macro_rules! plain_into_res {
    ($($t:ty),*) => {$(
        impl IntoRes<$t> for $t {
            fn into_res(self) -> Res<$t> { Ok(self) }
        }
        impl IntoRes<$t> for Res<$t> {
            fn into_res(self) -> Res<$t> { self }
        }
    )*};
}
plain_into_res!(f64, bool, i64);

/// Wrappers that take integer or float arguments: `wrapN` turns a closure over `N` floats into
/// one over `N` script numbers.
macro_rules! wrappers {
    ($($name:ident($($a:ident),+);)*) => {$(
        fn $name<R, T, F>(f: F) -> impl Fn($(wrappers!(@d $a)),+) -> Res<T> + 'static
        where
            F: Fn($(wrappers!(@f $a)),+) -> R + 'static,
            R: IntoRes<T>,
        {
            move |$($a),+| f($(arg(&$a)?),+).into_res()
        }
    )*};
    (@d $a:ident) => { Dynamic };
    (@f $a:ident) => { f64 };
}
wrappers! {
    wrap1(a);
    wrap2(a, b);
    wrap3(a, b, c);
    wrap4(a, b, c, d);
}

/// Raw comparisons stay exact IEEE (decision 0001, rule 4). Rhai's built-in float operators
/// compare with a relative epsilon, which is neither IEEE nor GM8's rule. These overloads win
/// over the built-ins because fast operators are off.
#[allow(
    clippy::float_cmp,
    clippy::cast_precision_loss,
    reason = "exact IEEE comparison is the point; integers in scripts stay far below 2^53"
)]
fn register_raw_comparisons(engine: &mut Engine, docs: &mut Docs) {
    docs.add("==", "Equality. Numbers compare exactly (IEEE), instances by identity; use `gm_eq` for GML's epsilon comparison.");
    engine.register_fn("==", |a: f64, b: f64| -> bool { a == b });
    docs.add(
        "!=",
        "Inequality, exact like `==`; use `gm_ne` for GML's epsilon comparison.",
    );
    engine.register_fn("!=", |a: f64, b: f64| -> bool { a != b });
    docs.add("<", "Less than, exact IEEE; use `gm_lt` for GML's rule.");
    engine.register_fn("<", |a: f64, b: f64| -> bool { a < b });
    docs.add(
        "<=",
        "Less than or equal, exact IEEE; use `gm_le` for GML's rule.",
    );
    engine.register_fn("<=", |a: f64, b: f64| -> bool { a <= b });
    docs.add(">", "Greater than, exact IEEE; use `gm_gt` for GML's rule.");
    engine.register_fn(">", |a: f64, b: f64| -> bool { a > b });
    docs.add(
        ">=",
        "Greater than or equal, exact IEEE; use `gm_ge` for GML's rule.",
    );
    engine.register_fn(">=", |a: f64, b: f64| -> bool { a >= b });
    engine.register_fn("==", |a: i64, b: f64| -> bool { a as f64 == b });
    engine.register_fn("==", |a: f64, b: i64| -> bool { a == b as f64 });
    engine.register_fn("!=", |a: i64, b: f64| -> bool { a as f64 != b });
    engine.register_fn("!=", |a: f64, b: i64| -> bool { a != b as f64 });
    engine.register_fn("<", |a: i64, b: f64| -> bool { (a as f64) < b });
    engine.register_fn("<", |a: f64, b: i64| -> bool { a < b as f64 });
    engine.register_fn("<=", |a: i64, b: f64| -> bool { a as f64 <= b });
    engine.register_fn("<=", |a: f64, b: i64| -> bool { a <= b as f64 });
    engine.register_fn(">", |a: i64, b: f64| -> bool { a as f64 > b });
    engine.register_fn(">", |a: f64, b: i64| -> bool { a > b as f64 });
    engine.register_fn(">=", |a: i64, b: f64| -> bool { a as f64 >= b });
    engine.register_fn(">=", |a: f64, b: i64| -> bool { a >= b as f64 });
}

/// Registers the numeric and random functions on `engine`.
pub(crate) fn register_numeric(engine: &mut Engine, shared: &Rc<Shared>, docs: &mut Docs) {
    // ---- GML comparisons and truth (NumRules) ----
    macro_rules! cmp {
        ($name:literal, $f:path) => {{
            let s = Rc::clone(shared);
            engine.register_fn(
                $name,
                wrap2(move |a: f64, b: f64| -> bool { $f(r(a), r(b), &s.num) }),
            );
        }};
    }
    docs.add("gm_eq", "GML `a == b` on reals: equal within the run's epsilon.");
    cmp!("gm_eq", num::gm_eq);
    docs.add("gm_ne", "GML `a != b` on reals, with the run's epsilon.");
    cmp!("gm_ne", num::gm_ne);
    docs.add("gm_lt", "GML `a < b` on reals, with the run's epsilon.");
    cmp!("gm_lt", num::gm_lt);
    docs.add("gm_le", "GML `a <= b` on reals, with the run's epsilon.");
    cmp!("gm_le", num::gm_le);
    docs.add("gm_gt", "GML `a > b` on reals, with the run's epsilon.");
    cmp!("gm_gt", num::gm_gt);
    docs.add("gm_ge", "GML `a >= b` on reals, with the run's epsilon.");
    cmp!("gm_ge", num::gm_ge);
    {
        let s = Rc::clone(shared);
        docs.add("gm_bool", "GML truthiness of a real (true from 0.5).");
        engine.register_fn(
            "gm_bool",
            wrap1(move |x: f64| -> bool { num::gm_bool(r(x), &s.num) }),
        );
    }
    {
        let s = Rc::clone(shared);
        docs.add(
            "gm_repeat",
            "The iteration count GML's `repeat` gives for a real (rounded); a bad count is an error.",
        );
        engine.register_fn(
            "gm_repeat",
            wrap1(move |x: f64| -> Res<i64> {
                let n = num::gm_repeat_count(r(x), &s.num).map_err(|e| num_err("gm_repeat", e))?;
                i64::try_from(n).map_err(|_| runtime_error("gm_repeat: count out of range"))
            }),
        );
    }

    register_raw_comparisons(engine, docs);

    // ---- Conversions and division ----
    docs.add("to_float", "An integer as a float.");
    engine.register_fn("to_float", |x: i64| -> f64 { x as f64 });
    docs.add(
        "to_int",
        "A float truncated toward zero as an integer; a non-finite or huge value is an error.",
    );
    engine.register_fn(
        "to_int",
        wrap1(|x: f64| -> Res<i64> { num::to_int_trunc(r(x)).map_err(|e| num_err("to_int", e)) }),
    );
    // `/` on two integers gives a FLOAT, as in GML (`1 / 2 == 0.5`); a zero divisor is an error.
    docs.add(
        "/",
        "Division. Two integers give a float (`1 / 2` is 0.5); a zero integer divisor is an error.",
    );
    engine.register_fn("/", |a: i64, b: i64| -> Res<f64> {
        if b == 0 {
            return Err(runtime_error("division by zero"));
        }
        Ok(a as f64 / b as f64)
    });
    docs.add(
        "div",
        "GML `a div b`: integer division truncated toward zero; a zero divisor is an error.",
    );
    engine.register_fn(
        "div",
        wrap2(|a: f64, b: f64| -> Res<f64> {
            num::gm_div(r(a), r(b))
                .map(Real::get)
                .map_err(|e| num_err("div", e))
        }),
    );
    docs.add(
        "mod",
        "GML `a mod b`: `a - (a div b) * b`; a zero divisor is an error.",
    );
    engine.register_fn(
        "mod",
        wrap2(|a: f64, b: f64| -> Res<f64> {
            num::gm_mod(r(a), r(b))
                .map(Real::get)
                .map_err(|e| num_err("mod", e))
        }),
    );

    // ---- Rounding, sign, min and max (NumRules) ----
    macro_rules! unary_rules {
        ($name:literal, $f:path) => {{
            let s = Rc::clone(shared);
            engine.register_fn($name, wrap1(move |x: f64| -> f64 { $f(r(x), &s.num).get() }));
        }};
    }
    docs.add(
        "round",
        "GML `round`: the nearest integer under the run's rounding rule.",
    );
    unary_rules!("round", num::round);
    docs.add("floor", "Largest integer not above `x`.");
    unary_rules!("floor", num::floor);
    docs.add("ceil", "Smallest integer not below `x`.");
    unary_rules!("ceil", num::ceil);
    docs.add("sign", "-1, 0 or 1 by the sign of `x`.");
    unary_rules!("sign", num::sign);
    // Rhai's own `sign` for a float returns an integer and beats the `Dynamic` form above, so
    // the float and integer forms are registered exactly.
    {
        let s = Rc::clone(shared);
        engine.register_fn("sign", move |x: f64| -> f64 { num::sign(r(x), &s.num).get() });
        let s = Rc::clone(shared);
        #[allow(clippy::cast_precision_loss, reason = "script integers stay far below 2^53")]
        engine.register_fn("sign", move |x: i64| -> f64 {
            num::sign(r(x as f64), &s.num).get()
        });
    }
    {
        let s = Rc::clone(shared);
        docs.add("min", "The smaller of two numbers (GML tie rules).");
        engine.register_fn(
            "min",
            wrap2(move |a: f64, b: f64| -> f64 { num::gm_min(r(a), r(b), &s.num).get() }),
        );
        let s = Rc::clone(shared);
        docs.add("max", "The larger of two numbers (GML tie rules).");
        engine.register_fn(
            "max",
            wrap2(move |a: f64, b: f64| -> f64 { num::gm_max(r(a), r(b), &s.num).get() }),
        );
    }
    docs.add("abs", "Absolute value.");
    engine.register_fn("abs", wrap1(|x: f64| -> f64 { num::abs(r(x)).get() }));
    docs.add("frac", "The fractional part of `x`, with the sign of `x`.");
    engine.register_fn("frac", wrap1(|x: f64| -> f64 { num::frac(r(x)).get() }));
    docs.add("clamp", "`x` limited to the range `lo` to `hi`.");
    engine.register_fn(
        "clamp",
        wrap3(|x: f64, lo: f64, hi: f64| -> f64 { num::clamp(r(x), r(lo), r(hi)).get() }),
    );
    docs.add("lerp", "Linear interpolation from `a` to `b` by `t`.");
    engine.register_fn(
        "lerp",
        wrap3(|a: f64, b: f64, t: f64| -> f64 { num::lerp(r(a), r(b), r(t)).get() }),
    );

    // ---- Transcendentals: libm in software, never std ----
    docs.add("sqrt", "Square root.");
    engine.register_fn("sqrt", wrap1(|x: f64| -> f64 { num::sqrt(r(x)).get() }));
    docs.add("sqr", "`x` squared.");
    engine.register_fn("sqr", wrap1(|x: f64| -> f64 { x * x }));
    docs.add("pow", "`x` to the power `y` (software libm). There is no `**`.");
    engine.register_fn(
        "pow",
        wrap2(|x: f64, y: f64| -> f64 { num::pow(r(x), r(y)).get() }),
    );
    docs.add("exp", "e to the power `x` (software libm).");
    engine.register_fn("exp", wrap1(|x: f64| -> f64 { num::exp(r(x)).get() }));
    docs.add("ln", "Natural logarithm (software libm).");
    engine.register_fn("ln", wrap1(|x: f64| -> f64 { num::ln(r(x)).get() }));
    docs.add("sin", "Sine of an angle in radians (software libm).");
    engine.register_fn("sin", wrap1(|x: f64| -> f64 { num::sin(r(x)).get() }));
    docs.add("cos", "Cosine of an angle in radians (software libm).");
    engine.register_fn("cos", wrap1(|x: f64| -> f64 { num::cos(r(x)).get() }));
    docs.add("tan", "Tangent of an angle in radians (software libm).");
    engine.register_fn("tan", wrap1(|x: f64| -> f64 { num::tan(r(x)).get() }));
    docs.add(
        "arctan2",
        "Angle in radians of the point (`x`, `y`) (software libm).",
    );
    engine.register_fn(
        "arctan2",
        wrap2(|y: f64, x: f64| -> f64 { num::atan2(r(y), r(x)).get() }),
    );
    docs.add("degtorad", "Degrees to radians.");
    engine.register_fn("degtorad", wrap1(|x: f64| -> f64 { num::degtorad(r(x)).get() }));
    docs.add("radtodeg", "Radians to degrees.");
    engine.register_fn("radtodeg", wrap1(|x: f64| -> f64 { num::radtodeg(r(x)).get() }));
    docs.add("dsin", "Sine of an angle in degrees.");
    engine.register_fn("dsin", wrap1(|x: f64| -> f64 { num::dsin(r(x)).get() }));
    docs.add("dcos", "Cosine of an angle in degrees.");
    engine.register_fn("dcos", wrap1(|x: f64| -> f64 { num::dcos(r(x)).get() }));
    docs.add("darctan2", "Angle in degrees of the point (`x`, `y`).");
    engine.register_fn(
        "darctan2",
        wrap2(|y: f64, x: f64| -> f64 { num::darctan2(r(y), r(x)).get() }),
    );
    docs.add(
        "point_direction",
        "Direction in degrees from (`x1`, `y1`) to (`x2`, `y2`), as GML.",
    );
    engine.register_fn(
        "point_direction",
        wrap4(|x1: f64, y1: f64, x2: f64, y2: f64| -> f64 {
            num::point_direction(r(x1), r(y1), r(x2), r(y2)).get()
        }),
    );
    docs.add(
        "point_distance",
        "Distance between (`x1`, `y1`) and (`x2`, `y2`).",
    );
    engine.register_fn(
        "point_distance",
        wrap4(|x1: f64, y1: f64, x2: f64, y2: f64| -> f64 {
            num::point_distance(r(x1), r(y1), r(x2), r(y2)).get()
        }),
    );
    docs.add(
        "lengthdir_x",
        "Horizontal part of a vector of length `len` toward `dir` degrees.",
    );
    engine.register_fn(
        "lengthdir_x",
        wrap2(|len: f64, dir: f64| -> f64 { num::lengthdir_x(r(len), r(dir)).get() }),
    );
    docs.add(
        "lengthdir_y",
        "Vertical part of a vector of length `len` toward `dir` degrees (up is negative, as GML).",
    );
    engine.register_fn(
        "lengthdir_y",
        wrap2(|len: f64, dir: f64| -> f64 { num::lengthdir_y(r(len), r(dir)).get() }),
    );

    // ---- The one RNG (GM8-compatible, seeded from the replay header) ----
    {
        let s = Rc::clone(shared);
        docs.add("random", "A float from 0 up to `x`, from the run's one RNG.");
        engine.register_fn(
            "random",
            wrap1(move |x: f64| -> f64 { s.with_rng(|g| g.random(r(x)).get()) }),
        );
        let s = Rc::clone(shared);
        docs.add(
            "random_range",
            "A float between `a` and `b`, from the run's one RNG.",
        );
        engine.register_fn(
            "random_range",
            wrap2(move |a: f64, b: f64| -> f64 { s.with_rng(|g| g.random_range(r(a), r(b)).get()) }),
        );
        let s = Rc::clone(shared);
        docs.add(
            "irandom",
            "An integer from 0 to `x` inclusive, from the run's one RNG.",
        );
        engine.register_fn(
            "irandom",
            wrap1(move |x: f64| -> Res<f64> {
                s.with_rng(|g| g.irandom(r(x)))
                    .map(Real::get)
                    .map_err(|e| runtime_error(e.to_string()))
            }),
        );
        let s = Rc::clone(shared);
        docs.add(
            "irandom_range",
            "An integer between `a` and `b` inclusive, from the run's one RNG.",
        );
        engine.register_fn(
            "irandom_range",
            wrap2(move |a: f64, b: f64| -> Res<f64> {
                s.with_rng(|g| g.irandom_range(r(a), r(b)))
                    .map(Real::get)
                    .map_err(|e| runtime_error(e.to_string()))
            }),
        );
        let s = Rc::clone(shared);
        docs.add(
            "choose",
            "One element of the array, picked by the run's one RNG; an empty array is an error.",
        );
        engine.register_fn("choose", move |items: Array| -> Res<Dynamic> {
            let n = u32::try_from(items.len()).map_err(|_| runtime_error("choose: too many items"))?;
            let i = s
                .with_rng(|g| g.choose(n))
                .map_err(|e| runtime_error(e.to_string()))?;
            items
                .into_iter()
                .nth(i as usize)
                .ok_or_else(|| runtime_error("choose: index out of range"))
        });
    }

    // ---- Time: ticks only, as floats ----
    {
        let s = Rc::clone(shared);
        docs.add("tick", "The run's tick counter.");
        engine.register_fn("tick", move || -> f64 { s.current_tick() as f64 });
        let s = Rc::clone(shared);
        docs.add("room_tick", "Ticks since the current room started.");
        engine.register_fn("room_tick", move || -> f64 { s.current_room_tick() as f64 });
    }
}
