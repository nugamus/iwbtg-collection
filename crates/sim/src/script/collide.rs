//! Collision queries for scripts (#777): `place_meeting`, `place_free`, `instance_place`,
//! `collision_point`, `collision_rect` and `collision_line`.
//!
//! Each is a read-only call into `collide::query`, the module the built-ins use, so a script and
//! a built-in see the same world: the same masks and 1 px rules, the first instance in R21
//! order, destroyed-this-step instances invisible (R22) and empty masks as the queries treat
//! them (#926). Nothing here changes the world.
//!
//! A target is a string or an instance handle. A string is a role (a named collision class,
//! `"block"`, `"killer"`, the GM parent objects' stand-in), `"all"`, or a kind, which covers the
//! kinds the loader derived from it as `instances(kind)` does. A role wins over a kind of the
//! same name. [`resolves`] is the load-time check of a literal (`validate::NAME_ARGS`).

use std::rc::Rc;

use rhai::{Dynamic, Engine, ImmutableString};

use super::api::runtime_error;
use super::budget::cost;
use super::docs::Docs;
use super::host::{Host, Shared};
use super::inst::Inst;
use crate::collide::place_origin;
use crate::collide::query::{self, Target};
use crate::def::{ClassMask, GameDef};
use crate::error::SimError;
use crate::num::Real;
use crate::world::{Ctx, InstId};

type Res<T> = Result<T, Box<rhai::EvalAltResult>>;

/// The target string meaning every instance (GML `all`).
const ALL: &str = "all";

/// Whether a target string names something: a role, `"all"` or a kind of `def`.
#[must_use]
pub fn resolves(def: &GameDef, name: &str) -> bool {
    name == ALL || ClassMask::from_name(name).is_some() || def.kinds.iter().any(|k| k.name == name)
}

/// The query target a script passes: a role, `"all"`, a kind (with its derived kinds) or an
/// instance handle.
fn target(host: &Host, t: &Dynamic, f: &str) -> Res<Target> {
    if let Some(i) = t.clone().try_cast::<Inst>() {
        return Ok(Target::Inst(i.0));
    }
    let Some(name) = t.clone().try_cast::<ImmutableString>() else {
        return Err(runtime_error(format!(
            "{f}: the target must be a role, a kind name or an instance, got {}",
            t.type_name()
        )));
    };
    if name == ALL {
        return Ok(Target::Any);
    }
    if let Some(c) = ClassMask::from_name(&name) {
        return Ok(Target::Class(c));
    }
    host.kinds
        .get(name.as_str())
        .map(|k| Target::KindTree(*k))
        .ok_or_else(|| runtime_error(format!("{f}: `{name}` is neither a role nor a kind")))
}

/// A number argument as a `Real`.
#[allow(clippy::cast_precision_loss, reason = "script integers stay far below 2^53")]
fn real(d: &Dynamic, f: &str) -> Res<Real> {
    if let Ok(v) = d.as_float() {
        Ok(Real::new(v))
    } else if let Ok(i) = d.as_int() {
        Ok(Real::new(i as f64))
    } else {
        Err(runtime_error(format!(
            "{f}: expected a number, got {}",
            d.type_name()
        )))
    }
}

/// The instance a `notme` argument names: an instance handle, or `()` for none.
fn notme(d: &Dynamic, f: &str) -> Res<Option<InstId>> {
    if d.is_unit() {
        return Ok(None);
    }
    d.clone().try_cast::<Inst>().map(|i| Some(i.0)).ok_or_else(|| {
        runtime_error(format!(
            "{f}: `notme` must be an instance or (), got {}",
            d.type_name()
        ))
    })
}

fn sim_err(e: SimError) -> Box<rhai::EvalAltResult> {
    runtime_error(e.to_string())
}

/// `Some(id)` as a handle, `None` as `()`.
fn handle(id: Option<InstId>) -> Dynamic {
    id.map_or(Dynamic::UNIT, |id| Dynamic::from(Inst(id)))
}

/// Runs a query on the lent world after checking that `i` (the instance placed, if any) is
/// alive: a destroyed instance's handle is a script error, as for its properties.
fn query<R>(s: &Shared, me: Option<Inst>, f: impl FnOnce(&Ctx) -> Result<R, SimError>) -> Res<R> {
    s.with_ctx(|ctx| {
        if let Some(i) = me
            && !ctx.w.exists(i.0)
        {
            return Err(runtime_error(format!("instance {} no longer exists", i.0.0)));
        }
        f(ctx).map_err(sim_err)
    })?
}

/// Charges `collision_line` one unit per pixel it samples, before it samples them.
fn charge_line(s: &Shared, x1: Real, y1: Real, x2: Real, y2: Real) -> Res<()> {
    let n = s.with_ctx(|ctx| -> Result<u64, SimError> {
        let r = &ctx.rules.collide;
        let dx = place_origin(x2, r)?.abs_diff(place_origin(x1, r)?);
        let dy = place_origin(y2, r)?.abs_diff(place_origin(y1, r)?);
        Ok(dx.max(dy).saturating_add(1))
    })?;
    let (base, per) = cost("collision_line");
    s.charge(base.saturating_add(per.saturating_mul(n.map_err(sim_err)?)))
}

/// Registers the collision queries on `engine`.
#[allow(clippy::too_many_lines, reason = "one registration per script-visible name")]
pub(crate) fn register_collision_api(engine: &mut Engine, shared: &Rc<Shared>, docs: &mut Docs) {
    // ---- Place tests: the instance's collision mask placed at (x, y) ----
    let s = Rc::clone(shared);
    docs.add(
        "place_meeting",
        "`place_meeting(i, x, y, target)`: GML `place_meeting`. Whether `i`'s collision mask placed at (`x`, `y`) overlaps an instance of `target` other than `i`. The target is a role (`\"block\"`, `\"killer\"`), `\"all\"`, a kind (with the kinds derived from it) or an instance; a role wins over a kind of the same name. Destroyed and deactivated instances and empty masks are never met. Changes nothing.",
    );
    engine.register_fn(
        "place_meeting",
        move |i: Inst, x: Dynamic, y: Dynamic, t: Dynamic| -> Res<bool> {
            let (x, y) = (real(&x, "place_meeting")?, real(&y, "place_meeting")?);
            let t = s.with_world(|_, h| target(h, &t, "place_meeting"))??;
            query(&s, Some(i), |ctx| query::place_meeting(ctx, i.0, x, y, t))
        },
    );
    let s = Rc::clone(shared);
    docs.add(
        "place_free",
        "`place_free(i, x, y)`: GML `place_free`. Whether `i`'s collision mask placed at (`x`, `y`) meets no solid instance. Changes nothing.",
    );
    engine.register_fn(
        "place_free",
        move |i: Inst, x: Dynamic, y: Dynamic| -> Res<bool> {
            let (x, y) = (real(&x, "place_free")?, real(&y, "place_free")?);
            query(&s, Some(i), |ctx| query::place_free(ctx, i.0, x, y))
        },
    );
    let s = Rc::clone(shared);
    docs.add(
        "instance_place",
        "`instance_place(i, x, y, target)`: GML `instance_place`. The instance `place_meeting` would meet, the first in event order (rank, then id) when several do, or `()`. Changes nothing.",
    );
    engine.register_fn(
        "instance_place",
        move |i: Inst, x: Dynamic, y: Dynamic, t: Dynamic| -> Res<Dynamic> {
            let (x, y) = (real(&x, "instance_place")?, real(&y, "instance_place")?);
            let t = s.with_world(|_, h| target(h, &t, "instance_place"))??;
            query(&s, Some(i), |ctx| query::instance_place(ctx, i.0, x, y, t)).map(handle)
        },
    );

    // ---- Shape tests: a point, a rectangle or a line in the room ----
    let s = Rc::clone(shared);
    docs.add(
        "collision_point",
        "`collision_point(x, y, target)` or `collision_point(x, y, target, prec, notme)`: GML `collision_point`. The first instance of `target` (event order) at the room pixel (`x`, `y`) rounds to, or `()`. `prec` (default true) tests precise masks pixel by pixel, else the bounding box; `notme` is an instance to skip, or `()`. Changes nothing.",
    );
    {
        let s2 = Rc::clone(&s);
        engine.register_fn(
            "collision_point",
            move |x: Dynamic, y: Dynamic, t: Dynamic| -> Res<Dynamic> {
                point(&s2, &x, &y, &t, true, &Dynamic::UNIT)
            },
        );
        engine.register_fn(
            "collision_point",
            move |x: Dynamic, y: Dynamic, t: Dynamic, prec: bool, nm: Dynamic| -> Res<Dynamic> {
                point(&s, &x, &y, &t, prec, &nm)
            },
        );
    }
    let s = Rc::clone(shared);
    docs.add(
        "collision_rect",
        "`collision_rect(x1, y1, x2, y2, target)` or `collision_rect(x1, y1, x2, y2, target, prec, notme)`: GML `collision_rectangle`. The first instance of `target` (event order) touching the rectangle between the two corners (both included, in any order), or `()`. `prec` and `notme` as for `collision_point`. Changes nothing.",
    );
    {
        let s2 = Rc::clone(&s);
        engine.register_fn(
            "collision_rect",
            move |x1: Dynamic, y1: Dynamic, x2: Dynamic, y2: Dynamic, t: Dynamic| -> Res<Dynamic> {
                shape(&s2, Shape::Rect, [&x1, &y1, &x2, &y2], &t, true, &Dynamic::UNIT)
            },
        );
        engine.register_fn(
            "collision_rect",
            move |x1: Dynamic,
                  y1: Dynamic,
                  x2: Dynamic,
                  y2: Dynamic,
                  t: Dynamic,
                  prec: bool,
                  nm: Dynamic|
                  -> Res<Dynamic> { shape(&s, Shape::Rect, [&x1, &y1, &x2, &y2], &t, prec, &nm) },
        );
    }
    let s = Rc::clone(shared);
    docs.add(
        "collision_line",
        "`collision_line(x1, y1, x2, y2, target)` or `collision_line(x1, y1, x2, y2, target, prec, notme)`: GML `collision_line`. The first instance of `target` (event order) on the 8-connected pixel line between the two rounded ends, both included, or `()`. `prec` and `notme` as for `collision_point`. Costs one budget unit per pixel sampled. Changes nothing.",
    );
    {
        let s2 = Rc::clone(&s);
        engine.register_fn(
            "collision_line",
            move |x1: Dynamic, y1: Dynamic, x2: Dynamic, y2: Dynamic, t: Dynamic| -> Res<Dynamic> {
                shape(&s2, Shape::Line, [&x1, &y1, &x2, &y2], &t, true, &Dynamic::UNIT)
            },
        );
        engine.register_fn(
            "collision_line",
            move |x1: Dynamic,
                  y1: Dynamic,
                  x2: Dynamic,
                  y2: Dynamic,
                  t: Dynamic,
                  prec: bool,
                  nm: Dynamic|
                  -> Res<Dynamic> { shape(&s, Shape::Line, [&x1, &y1, &x2, &y2], &t, prec, &nm) },
        );
    }
}

fn point(s: &Shared, x: &Dynamic, y: &Dynamic, t: &Dynamic, prec: bool, nm: &Dynamic) -> Res<Dynamic> {
    const F: &str = "collision_point";
    let (x, y) = (real(x, F)?, real(y, F)?);
    let t = s.with_world(|_, h| target(h, t, F))??;
    let nm = notme(nm, F)?;
    query(s, None, |ctx| query::collision_point(ctx, x, y, t, prec, nm)).map(handle)
}

#[derive(Clone, Copy)]
enum Shape {
    Rect,
    Line,
}

fn shape(s: &Shared, kind: Shape, c: [&Dynamic; 4], t: &Dynamic, prec: bool, nm: &Dynamic) -> Res<Dynamic> {
    let f = match kind {
        Shape::Rect => "collision_rect",
        Shape::Line => "collision_line",
    };
    let (x1, y1, x2, y2) = (real(c[0], f)?, real(c[1], f)?, real(c[2], f)?, real(c[3], f)?);
    let t = s.with_world(|_, h| target(h, t, f))??;
    let nm = notme(nm, f)?;
    match kind {
        Shape::Rect => query(s, None, |ctx| {
            query::collision_rectangle(ctx, x1, y1, x2, y2, t, prec, nm)
        }),
        Shape::Line => {
            charge_line(s, x1, y1, x2, y2)?;
            query(s, None, |ctx| {
                query::collision_line(ctx, x1, y1, x2, y2, t, prec, nm)
            })
        }
    }
    .map(handle)
}
