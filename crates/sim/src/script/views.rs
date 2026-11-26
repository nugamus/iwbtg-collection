//! Views in the script API (#1531; docs/architecture/sim/scripts.md, census group "view").
//!
//! The GML view functions read and write `World::views`, the eight hashed [`View`]s. A write
//! changes the state the runner follows and the client draws from the next slot on; the explicit
//! camera follow (`view::follow_update`) runs after the event that wrote them.

use std::rc::Rc;

use rhai::{Dynamic, Engine, ImmutableString};

use super::api::runtime_error;
use super::docs::Docs;
use super::host::Shared;
use super::inst::num;
use crate::collide::Target;
use crate::num::Real;
use crate::view::View;

type Res<T> = Result<T, Box<rhai::EvalAltResult>>;

/// A view index argument: a whole number from 0 to 7 (`World::views`).
fn view_index(d: &Dynamic) -> Res<usize> {
    let f = num(d, "view")?;
    // gm-compare: ok (an index is exactly whole or not)
    let whole = crate::num::round::frac(Real::new(f)).get() == 0.0;
    if !Real::new(f).is_finite() || f < 0.0 || !whole {
        return Err(runtime_error(format!(
            "view index {f} is not a whole number (0 to 7)"
        )));
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "checked to be a whole number from 0"
    )]
    let i = f as usize;
    if i >= 8 {
        return Err(runtime_error(format!("view index {f} is not a view (0 to 7)")));
    }
    Ok(i)
}

fn view_missing(i: usize) -> Box<rhai::EvalAltResult> {
    runtime_error(format!("view {i} does not exist (0 to 7)"))
}

fn finite(d: &Dynamic, what: &str) -> Res<Real> {
    let r = Real::new(num(d, what)?);
    if !r.is_finite() {
        return Err(runtime_error(format!("`{what}` is not a finite number")));
    }
    Ok(r)
}

/// A bool argument: a bool, or the GM number 0/1.
#[allow(clippy::float_cmp, reason = "a GM boolean is exactly 0 or 1")]
fn bool_arg(d: &Dynamic, what: &str) -> Res<bool> {
    if let Ok(b) = d.as_bool() {
        return Ok(b);
    }
    let f = num(d, what)?;
    if f == 0.0 {
        Ok(false)
    } else if f == 1.0 {
        Ok(true)
    } else {
        Err(runtime_error(format!("`{what}` must be a bool or 0/1, not {f}")))
    }
}

/// Registers `f` as a read that takes a view index and returns a real.
fn real_get(engine: &mut Engine, shared: &Rc<Shared>, name: &'static str, f: fn(&View) -> Real) {
    let s = Rc::clone(shared);
    engine.register_fn(name, move |i: Dynamic| -> Res<f64> {
        let i = view_index(&i)?;
        s.with_world(|w, _| w.views.get(i).map(|v| f(v).get()).ok_or_else(|| view_missing(i)))?
    });
}

/// Registers `f` as a read that takes a view index and returns a bool.
fn bool_get(engine: &mut Engine, shared: &Rc<Shared>, name: &'static str, f: fn(&View) -> bool) {
    let s = Rc::clone(shared);
    engine.register_fn(name, move |i: Dynamic| -> Res<bool> {
        let i = view_index(&i)?;
        s.with_world(|w, _| w.views.get(i).map(f).ok_or_else(|| view_missing(i)))?
    });
}

/// Registers `f` as a write `(index, real)`.
fn real_set(engine: &mut Engine, shared: &Rc<Shared>, name: &'static str, f: fn(&mut View) -> &mut Real) {
    let s = Rc::clone(shared);
    engine.register_fn(name, move |i: Dynamic, v: Dynamic| -> Res<()> {
        let i = view_index(&i)?;
        let v = finite(&v, name)?;
        s.with_world(|w, _| {
            let view = w.views.get_mut(i).ok_or_else(|| view_missing(i))?;
            *f(view) = v;
            Ok(())
        })?
    });
}

/// Registers `f` as a write `(index, bool)`.
fn bool_set(engine: &mut Engine, shared: &Rc<Shared>, name: &'static str, f: fn(&mut View) -> &mut bool) {
    let s = Rc::clone(shared);
    engine.register_fn(name, move |i: Dynamic, v: Dynamic| -> Res<()> {
        let i = view_index(&i)?;
        let v = bool_arg(&v, name)?;
        s.with_world(|w, _| {
            let view = w.views.get_mut(i).ok_or_else(|| view_missing(i))?;
            *f(view) = v;
            Ok(())
        })?
    });
}

/// Registers the view API on `engine` (called from `register_instance_api`; the view functions
/// are free functions over the world, like the `room_*` ones).
#[allow(clippy::too_many_lines, reason = "one registration per script-visible name")]
pub fn register_view_api(engine: &mut Engine, shared: &Rc<Shared>, docs: &mut Docs) {
    for (name, doc, f) in [
        (
            "view_xview",
            "`view_xview(i)`: the view's left edge in the room.",
            view_x as fn(&View) -> Real,
        ),
        (
            "view_yview",
            "`view_yview(i)`: the view's top edge in the room.",
            view_y,
        ),
        ("view_wview", "`view_wview(i)`: the view's width.", view_w),
        ("view_hview", "`view_hview(i)`: the view's height.", view_h),
        (
            "view_angle",
            "`view_angle(i)`: the view's rotation in degrees.",
            view_angle,
        ),
        (
            "view_hborder",
            "`view_hborder(i)`: the follow's horizontal dead-zone half-size.",
            view_hborder,
        ),
        (
            "view_vborder",
            "`view_vborder(i)`: the follow's vertical dead-zone half-size.",
            view_vborder,
        ),
        (
            "view_hspeed",
            "`view_hspeed(i)`: the most the follow moves horizontally a tick (negative snaps).",
            view_hspeed,
        ),
        (
            "view_vspeed",
            "`view_vspeed(i)`: the most the follow moves vertically a tick (negative snaps).",
            view_vspeed,
        ),
    ] {
        docs.add(name, doc);
        real_get(engine, shared, name, f);
    }
    docs.add(
        "view_enabled",
        "`view_enabled(i)`: whether the view is on for the room.",
    );
    bool_get(engine, shared, "view_enabled", |v| v.enabled);
    docs.add("view_visible", "`view_visible(i)`: whether the view is drawn.");
    bool_get(engine, shared, "view_visible", |v| v.visible);
    docs.add(
        "view_object",
        "`view_object(i)`: the kind the view follows, or `()` when it follows none.",
    );
    let s = Rc::clone(shared);
    engine.register_fn("view_object", move |i: Dynamic| -> Res<Dynamic> {
        let i = view_index(&i)?;
        s.with_world(|w, host| {
            let view = w.views.get(i).ok_or_else(|| view_missing(i))?;
            let name = match view.follow {
                Some(Target::Kind(k) | Target::KindTree(k)) => host.def.kind(k).map(|d| d.name.clone()),
                _ => None,
            };
            Ok(name.map_or(Dynamic::UNIT, |n| Dynamic::from(ImmutableString::from(n))))
        })?
    });
    docs.add(
        "view_current",
        "`view_current()`: the view being drawn. There is no draw slot yet, so it is 0.",
    );
    engine.register_fn("view_current", || -> f64 { 0.0 });

    for (name, doc, f) in [
        (
            "set_view_xview",
            "`set_view_xview(i, x)`: moves the view's left edge.",
            set_x as fn(&mut View) -> &mut Real,
        ),
        (
            "set_view_yview",
            "`set_view_yview(i, y)`: moves the view's top edge.",
            set_y,
        ),
        (
            "set_view_wview",
            "`set_view_wview(i, w)`: sets the view's width.",
            set_w,
        ),
        (
            "set_view_hview",
            "`set_view_hview(i, h)`: sets the view's height.",
            set_h,
        ),
        (
            "set_view_angle",
            "`set_view_angle(i, degrees)`: sets the view's rotation.",
            set_angle,
        ),
        (
            "set_view_hborder",
            "`set_view_hborder(i, border)`: sets the follow's horizontal dead-zone half-size.",
            set_hborder,
        ),
        (
            "set_view_vborder",
            "`set_view_vborder(i, border)`: sets the follow's vertical dead-zone half-size.",
            set_vborder,
        ),
        (
            "set_view_hspeed",
            "`set_view_hspeed(i, speed)`: sets the follow's horizontal speed (negative snaps).",
            set_hspeed,
        ),
        (
            "set_view_vspeed",
            "`set_view_vspeed(i, speed)`: sets the follow's vertical speed (negative snaps).",
            set_vspeed,
        ),
    ] {
        docs.add(name, doc);
        real_set(engine, shared, name, f);
    }
    docs.add(
        "set_view_enabled",
        "`set_view_enabled(i, on)`: turns the view on or off for the room.",
    );
    bool_set(engine, shared, "set_view_enabled", |v| &mut v.enabled);
    docs.add(
        "set_view_visible",
        "`set_view_visible(i, on)`: shows or hides the view.",
    );
    bool_set(engine, shared, "set_view_visible", |v| &mut v.visible);
    docs.add(
        "set_view_object",
        "`set_view_object(i, kind)`: makes the view follow a kind (its first matching instance, as a room's `follow` does), or `()` to follow nothing.",
    );
    let s = Rc::clone(shared);
    engine.register_fn("set_view_object", move |i: Dynamic, kind: Dynamic| -> Res<()> {
        let i = view_index(&i)?;
        s.with_world(|w, host| {
            let follow = if kind.is_unit() {
                None
            } else if let Some(name) = kind.clone().try_cast::<ImmutableString>() {
                let id = host
                    .kinds
                    .get(name.as_str())
                    .copied()
                    .ok_or_else(|| runtime_error(format!("unknown kind `{name}`")))?;
                Some(Target::Kind(id))
            } else {
                return Err(runtime_error("set_view_object takes a kind name or `()`"));
            };
            w.views.get_mut(i).ok_or_else(|| view_missing(i))?.follow = follow;
            Ok(())
        })?
    });
}

fn view_x(v: &View) -> Real {
    v.x
}
fn view_y(v: &View) -> Real {
    v.y
}
fn view_w(v: &View) -> Real {
    v.w
}
fn view_h(v: &View) -> Real {
    v.h
}
fn view_angle(v: &View) -> Real {
    v.angle
}
fn view_hborder(v: &View) -> Real {
    v.hborder
}
fn view_vborder(v: &View) -> Real {
    v.vborder
}
fn view_hspeed(v: &View) -> Real {
    v.hspeed
}
fn view_vspeed(v: &View) -> Real {
    v.vspeed
}
fn set_x(v: &mut View) -> &mut Real {
    &mut v.x
}
fn set_y(v: &mut View) -> &mut Real {
    &mut v.y
}
fn set_w(v: &mut View) -> &mut Real {
    &mut v.w
}
fn set_h(v: &mut View) -> &mut Real {
    &mut v.h
}
fn set_angle(v: &mut View) -> &mut Real {
    &mut v.angle
}
fn set_hborder(v: &mut View) -> &mut Real {
    &mut v.hborder
}
fn set_vborder(v: &mut View) -> &mut Real {
    &mut v.vborder
}
fn set_hspeed(v: &mut View) -> &mut Real {
    &mut v.hspeed
}
fn set_vspeed(v: &mut View) -> &mut Real {
    &mut v.vspeed
}
