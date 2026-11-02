//! The script API registered on the Rhai engine (architecture "Script API"; docs/plans/sim-core.md
//! 11). WP9.1 registers the numeric and random part; WP9.2 adds the instance API.
//!
//! Every number a script sees is a FLOAT (decision 0001, rule 5): the functions take and return
//! `f64`, and call `num` with the run's [`NumRules`](crate::rules::num::NumRules). Nothing here
//! calls platform math: sin, cos, pow and the rest are the `libm` software versions in `num`.
//! Rhai's own float functions are never registered (no `BasicMathPackage`).

use std::rc::Rc;

use rhai::{Array, Dynamic, Engine, EvalAltResult, Position};

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

/// Raw comparisons stay exact IEEE (decision 0001, rule 4). Rhai's built-in float operators
/// compare with a relative epsilon, which is neither IEEE nor GM8's rule. These overloads win
/// over the built-ins because fast operators are off.
#[allow(
    clippy::float_cmp,
    clippy::cast_precision_loss,
    reason = "exact IEEE comparison is the point; integers in scripts stay far below 2^53"
)]
fn register_raw_comparisons(engine: &mut Engine) {
    engine.register_fn("==", |a: f64, b: f64| -> bool { a == b });
    engine.register_fn("!=", |a: f64, b: f64| -> bool { a != b });
    engine.register_fn("<", |a: f64, b: f64| -> bool { a < b });
    engine.register_fn("<=", |a: f64, b: f64| -> bool { a <= b });
    engine.register_fn(">", |a: f64, b: f64| -> bool { a > b });
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
pub(crate) fn register_numeric(engine: &mut Engine, shared: &Rc<Shared>) {
    // ---- GML comparisons and truth (NumRules) ----
    macro_rules! cmp {
        ($name:literal, $f:path) => {{
            let s = Rc::clone(shared);
            engine.register_fn($name, move |a: f64, b: f64| -> bool { $f(r(a), r(b), &s.num) });
        }};
    }
    cmp!("gm_eq", num::gm_eq);
    cmp!("gm_ne", num::gm_ne);
    cmp!("gm_lt", num::gm_lt);
    cmp!("gm_le", num::gm_le);
    cmp!("gm_gt", num::gm_gt);
    cmp!("gm_ge", num::gm_ge);
    {
        let s = Rc::clone(shared);
        engine.register_fn("gm_bool", move |x: f64| -> bool { num::gm_bool(r(x), &s.num) });
    }
    {
        let s = Rc::clone(shared);
        engine.register_fn("gm_repeat", move |x: f64| -> Res<i64> {
            let n = num::gm_repeat_count(r(x), &s.num).map_err(|e| num_err("gm_repeat", e))?;
            i64::try_from(n).map_err(|_| runtime_error("gm_repeat: count out of range"))
        });
    }

    register_raw_comparisons(engine);

    // ---- Conversions and division ----
    engine.register_fn("to_float", |x: i64| -> f64 { x as f64 });
    engine.register_fn("to_int", |x: f64| -> Res<i64> {
        num::to_int_trunc(r(x)).map_err(|e| num_err("to_int", e))
    });
    // `/` on two integers gives a FLOAT, as in GML (`1 / 2 == 0.5`); a zero divisor is an error.
    engine.register_fn("/", |a: i64, b: i64| -> Res<f64> {
        if b == 0 {
            return Err(runtime_error("division by zero"));
        }
        Ok(a as f64 / b as f64)
    });
    engine.register_fn("div", |a: f64, b: f64| -> Res<f64> {
        num::gm_div(r(a), r(b))
            .map(Real::get)
            .map_err(|e| num_err("div", e))
    });
    engine.register_fn("mod", |a: f64, b: f64| -> Res<f64> {
        num::gm_mod(r(a), r(b))
            .map(Real::get)
            .map_err(|e| num_err("mod", e))
    });

    // ---- Rounding, sign, min and max (NumRules) ----
    macro_rules! unary_rules {
        ($name:literal, $f:path) => {{
            let s = Rc::clone(shared);
            engine.register_fn($name, move |x: f64| -> f64 { $f(r(x), &s.num).get() });
        }};
    }
    unary_rules!("round", num::round);
    unary_rules!("floor", num::floor);
    unary_rules!("ceil", num::ceil);
    unary_rules!("sign", num::sign);
    {
        let s = Rc::clone(shared);
        engine.register_fn("min", move |a: f64, b: f64| -> f64 {
            num::gm_min(r(a), r(b), &s.num).get()
        });
        let s = Rc::clone(shared);
        engine.register_fn("max", move |a: f64, b: f64| -> f64 {
            num::gm_max(r(a), r(b), &s.num).get()
        });
    }
    engine.register_fn("abs", |x: f64| -> f64 { num::abs(r(x)).get() });
    engine.register_fn("frac", |x: f64| -> f64 { num::frac(r(x)).get() });
    engine.register_fn("clamp", |x: f64, lo: f64, hi: f64| -> f64 {
        num::clamp(r(x), r(lo), r(hi)).get()
    });
    engine.register_fn("lerp", |a: f64, b: f64, t: f64| -> f64 {
        num::lerp(r(a), r(b), r(t)).get()
    });

    // ---- Transcendentals: libm in software, never std ----
    engine.register_fn("sqrt", |x: f64| -> f64 { num::sqrt(r(x)).get() });
    engine.register_fn("sqr", |x: f64| -> f64 { x * x });
    engine.register_fn("pow", |x: f64, y: f64| -> f64 { num::pow(r(x), r(y)).get() });
    engine.register_fn("exp", |x: f64| -> f64 { num::exp(r(x)).get() });
    engine.register_fn("ln", |x: f64| -> f64 { num::ln(r(x)).get() });
    engine.register_fn("sin", |x: f64| -> f64 { num::sin(r(x)).get() });
    engine.register_fn("cos", |x: f64| -> f64 { num::cos(r(x)).get() });
    engine.register_fn("tan", |x: f64| -> f64 { num::tan(r(x)).get() });
    engine.register_fn("arctan2", |y: f64, x: f64| -> f64 {
        num::atan2(r(y), r(x)).get()
    });
    engine.register_fn("degtorad", |x: f64| -> f64 { num::degtorad(r(x)).get() });
    engine.register_fn("radtodeg", |x: f64| -> f64 { num::radtodeg(r(x)).get() });
    engine.register_fn("dsin", |x: f64| -> f64 { num::dsin(r(x)).get() });
    engine.register_fn("dcos", |x: f64| -> f64 { num::dcos(r(x)).get() });
    engine.register_fn("darctan2", |y: f64, x: f64| -> f64 {
        num::darctan2(r(y), r(x)).get()
    });
    engine.register_fn("point_direction", |x1: f64, y1: f64, x2: f64, y2: f64| -> f64 {
        num::point_direction(r(x1), r(y1), r(x2), r(y2)).get()
    });
    engine.register_fn("point_distance", |x1: f64, y1: f64, x2: f64, y2: f64| -> f64 {
        num::point_distance(r(x1), r(y1), r(x2), r(y2)).get()
    });
    engine.register_fn("lengthdir_x", |len: f64, dir: f64| -> f64 {
        num::lengthdir_x(r(len), r(dir)).get()
    });
    engine.register_fn("lengthdir_y", |len: f64, dir: f64| -> f64 {
        num::lengthdir_y(r(len), r(dir)).get()
    });

    // ---- The one RNG (GM8-compatible, seeded from the replay header) ----
    {
        let s = Rc::clone(shared);
        engine.register_fn("random", move |x: f64| -> f64 {
            s.rng.borrow_mut().random(r(x)).get()
        });
        let s = Rc::clone(shared);
        engine.register_fn("random_range", move |a: f64, b: f64| -> f64 {
            s.rng.borrow_mut().random_range(r(a), r(b)).get()
        });
        let s = Rc::clone(shared);
        engine.register_fn("irandom", move |x: f64| -> Res<f64> {
            s.rng
                .borrow_mut()
                .irandom(r(x))
                .map(Real::get)
                .map_err(|e| runtime_error(e.to_string()))
        });
        let s = Rc::clone(shared);
        engine.register_fn("irandom_range", move |a: f64, b: f64| -> Res<f64> {
            s.rng
                .borrow_mut()
                .irandom_range(r(a), r(b))
                .map(Real::get)
                .map_err(|e| runtime_error(e.to_string()))
        });
        let s = Rc::clone(shared);
        engine.register_fn("choose", move |items: Array| -> Res<Dynamic> {
            let n = u32::try_from(items.len()).map_err(|_| runtime_error("choose: too many items"))?;
            let i = s
                .rng
                .borrow_mut()
                .choose(n)
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
        engine.register_fn("tick", move || -> f64 { s.tick.get() as f64 });
        let s = Rc::clone(shared);
        engine.register_fn("room_tick", move || -> f64 { s.room_tick.get() as f64 });
    }
}
