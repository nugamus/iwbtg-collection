//! Paths in the script API (#1530; docs/architecture/sim/scripts.md, census group "paths").
//!
//! `i.path_start(...)` and `i.path_end()` start and stop a [`PathRun`](crate::path::PathRun);
//! the instance's `path_*` properties read and write it. The run advances in automatic motion
//! (`motion::apply_auto_motion`), the same one the `path_platform` built-in uses, so a path a
//! script starts is hashed, replays and ends like a built-in's.

use std::rc::Rc;

use rhai::{Dynamic, Engine, ImmutableString};

use super::api::runtime_error;
use super::docs::Docs;
use super::host::Shared;
use super::inst::{Inst, live, live_mut, num};
use crate::def::{GameDef, PathId};
use crate::error::SimError;
use crate::num::{self, Real};
use crate::path::{self, PathEnd};
use crate::world::Instance;

type Res<T> = Result<T, Box<rhai::EvalAltResult>>;

fn sim_err(e: SimError) -> Box<rhai::EvalAltResult> {
    runtime_error(e.to_string())
}

/// The path with this package name. A literal is checked at load (`validate::Names::Path`).
fn path_id(def: &GameDef, name: &str) -> Res<PathId> {
    def.paths
        .iter()
        .position(|p| p.name == name)
        .and_then(|i| u32::try_from(i).ok())
        .map(PathId)
        .ok_or_else(|| runtime_error(format!("unknown path `{name}`")))
}

/// GM `path_endaction` codes: 0 stop, 1 restart, 2 continue, 3 reverse.
fn end_action(name: &str) -> Res<PathEnd> {
    match name.to_ascii_lowercase().as_str() {
        "stop" => Ok(PathEnd::Stop),
        "restart" => Ok(PathEnd::Restart),
        "continue" => Ok(PathEnd::Continue),
        "reverse" => Ok(PathEnd::Reverse),
        other => Err(runtime_error(format!(
            "unknown path end action `{other}` (stop, restart, continue, reverse)"
        ))),
    }
}

fn end_code(action: PathEnd) -> f64 {
    match action {
        PathEnd::Stop => 0.0,
        PathEnd::Restart => 1.0,
        PathEnd::Continue => 2.0,
        PathEnd::Reverse => 3.0,
    }
}

fn end_from_code(code: f64) -> Res<PathEnd> {
    match code {
        0.0 => Ok(PathEnd::Stop),
        1.0 => Ok(PathEnd::Restart),
        2.0 => Ok(PathEnd::Continue),
        3.0 => Ok(PathEnd::Reverse),
        other => Err(runtime_error(format!(
            "path_endaction {other} is not a path end action (0 stop, 1 restart, 2 continue, 3 reverse)"
        ))),
    }
}

/// `absolute`: a bool, or the GM number 0/1.
#[allow(clippy::float_cmp, reason = "a GM boolean is exactly 0 or 1")]
fn bool_arg(d: &Dynamic, what: &str) -> Res<bool> {
    if let Ok(b) = d.as_bool() {
        return Ok(b);
    }
    let f = num(d, what)?;
    // gm-compare: ok (GM booleans are exactly 0 or 1)
    if f == 0.0 {
        Ok(false)
    // gm-compare: ok
    } else if f == 1.0 {
        Ok(true)
    } else {
        Err(runtime_error(format!("`{what}` must be a bool or 0/1, not {f}")))
    }
}

fn finite(d: &Dynamic, what: &str) -> Res<Real> {
    let r = Real::new(num(d, what)?);
    if !r.is_finite() {
        return Err(runtime_error(format!("`{what}` is not a finite number")));
    }
    Ok(r)
}

fn no_path(name: &str) -> Box<rhai::EvalAltResult> {
    runtime_error(format!(
        "`{name}`: the instance follows no path (call path_start first)"
    ))
}

/// Registers a `path_*` property. A write needs a running path; a read follows GM's defaults
/// (`0`, `-1` for the index, `1` for the scale) when there is none.
fn path_prop(
    engine: &mut Engine,
    shared: &Rc<Shared>,
    name: &'static str,
    get: fn(&Instance) -> Real,
    set: Option<fn(&mut Instance, Real) -> Res<()>>,
) {
    let s = Rc::clone(shared);
    engine.register_get(name, move |i: &mut Inst| -> Res<f64> {
        let i = *i;
        s.with_world(|w, _| live(w, i).map(|x| get(x).get()))?
    });
    if let Some(set) = set {
        let s = Rc::clone(shared);
        engine.register_set(name, move |i: &mut Inst, v: Dynamic| -> Res<()> {
            let v = Real::new(num(&v, name)?);
            let i = *i;
            s.with_world(|w, _| set(live_mut(w, i)?, v))?
        });
    }
}

fn set_position(i: &mut Instance, v: Real) -> Res<()> {
    if !v.is_finite() {
        return Err(runtime_error("`path_position` is not a finite number"));
    }
    let run = i.path.as_mut().ok_or_else(|| no_path("path_position"))?;
    run.position = num::clamp(v, Real::ZERO, Real::ONE);
    Ok(())
}

fn set_speed(i: &mut Instance, v: Real) -> Res<()> {
    if !v.is_finite() {
        return Err(runtime_error("`path_speed` is not a finite number"));
    }
    let run = i.path.as_mut().ok_or_else(|| no_path("path_speed"))?;
    run.speed = v;
    Ok(())
}

fn set_endaction(i: &mut Instance, v: Real) -> Res<()> {
    let action = end_from_code(v.get())?;
    let run = i.path.as_mut().ok_or_else(|| no_path("path_endaction"))?;
    run.end_action = action;
    Ok(())
}

fn set_orientation(i: &mut Instance, v: Real) -> Res<()> {
    if !v.is_finite() {
        return Err(runtime_error("`path_orientation` is not a finite number"));
    }
    let run = i.path.as_mut().ok_or_else(|| no_path("path_orientation"))?;
    run.orientation = v;
    Ok(())
}

fn set_scale(i: &mut Instance, v: Real) -> Res<()> {
    if !v.is_finite() {
        return Err(runtime_error("`path_scale` is not a finite number"));
    }
    let run = i.path.as_mut().ok_or_else(|| no_path("path_scale"))?;
    run.scale = v;
    Ok(())
}

/// Registers the path API on `engine`: `path_start`, `path_end` and the instance's `path_*`
/// properties. Called from `register_instance_api` (the paths act on an instance).
pub fn register_path_api(engine: &mut Engine, shared: &Rc<Shared>, docs: &mut Docs) {
    let s = Rc::clone(shared);
    docs.add(
        "path_start",
        "`i.path_start(path, speed, endaction, absolute)`: starts following a package path, replacing any running one. `path` is the path id; `endaction` is `\"stop\"`, `\"restart\"`, `\"continue\"` or `\"reverse\"`; `absolute` places the instance at the path's own points, else the path is relative to the instance's current position. The run advances in automatic motion from the next tick. A non-finite speed is an error.",
    );
    engine.register_fn(
        "path_start",
        move |i: &mut Inst,
              path: ImmutableString,
              speed: Dynamic,
              endaction: ImmutableString,
              absolute: Dynamic|
              -> Res<()> {
            let i = *i;
            let speed = finite(&speed, "path_speed")?;
            let action = end_action(&endaction)?;
            let absolute = bool_arg(&absolute, "absolute")?;
            s.with_world(|w, host| {
                let id = path_id(&host.def, &path)?;
                let inst = live_mut(w, i)?;
                inst.path = Some(path::start(&inst.m, id, speed, action, absolute).map_err(sim_err)?);
                Ok(())
            })?
        },
    );
    let s = Rc::clone(shared);
    docs.add(
        "path_end",
        "`i.path_end()`: stops following the path (the instance keeps its position); `path_index` reads -1 afterwards.",
    );
    engine.register_fn("path_end", move |i: &mut Inst| -> Res<()> {
        let i = *i;
        s.with_world(|w, _| {
            path::end(&mut live_mut(w, i)?.path);
            Ok(())
        })?
    });

    docs.add(
        "path_index",
        "The path the instance follows, or -1 when it follows none.",
    );
    path_prop(engine, shared, "path_index", index, None);
    docs.add(
        "path_position",
        "Position along the path, 0 to 1 (GM `path_position`); setting it moves the point the next automatic-motion tick places.",
    );
    path_prop(engine, shared, "path_position", position, Some(set_position));
    docs.add(
        "path_positionprevious",
        "`path_position` at the start of the last path step.",
    );
    path_prop(engine, shared, "path_positionprevious", position_previous, None);
    docs.add("path_speed", "Path speed in pixels per tick (GM `path_speed`).");
    path_prop(engine, shared, "path_speed", speed, Some(set_speed));
    docs.add(
        "path_endaction",
        "What the path does at its end: 0 stop, 1 restart, 2 continue, 3 reverse (GM `path_endaction`).",
    );
    path_prop(engine, shared, "path_endaction", endaction, Some(set_endaction));
    docs.add(
        "path_orientation",
        "Path orientation in degrees; the advance turns the path about its first point before the relative anchor and scale apply (M8 default).",
    );
    path_prop(
        engine,
        shared,
        "path_orientation",
        orientation,
        Some(set_orientation),
    );
    docs.add(
        "path_scale",
        "Scale of a relative path's points about its first point (GM `path_scale`); 1 is unscaled. Scale on an absolute path is OPEN (M8).",
    );
    path_prop(engine, shared, "path_scale", scale, Some(set_scale));
    docs.add(
        "path_x",
        "The path point's x at the current position, before scale, orientation and the relative anchor; 0 when there is no path.",
    );
    path_point(engine, shared, "path_x", true);
    docs.add(
        "path_y",
        "The path point's y at the current position, before scale, orientation and the relative anchor; 0 when there is no path.",
    );
    path_point(engine, shared, "path_y", false);
}

fn index(i: &Instance) -> Real {
    i.path
        .as_ref()
        .map_or(Real::new(-1.0), |r| Real::new(f64::from(r.path.0)))
}

fn position(i: &Instance) -> Real {
    i.path.as_ref().map_or(Real::ZERO, |r| r.position)
}

fn position_previous(i: &Instance) -> Real {
    i.path.as_ref().map_or(Real::ZERO, |r| r.position_previous)
}

fn speed(i: &Instance) -> Real {
    i.path.as_ref().map_or(Real::ZERO, |r| r.speed)
}

fn endaction(i: &Instance) -> Real {
    i.path
        .as_ref()
        .map_or(Real::ZERO, |r| Real::new(end_code(r.end_action)))
}

fn orientation(i: &Instance) -> Real {
    i.path.as_ref().map_or(Real::ZERO, |r| r.orientation)
}

fn scale(i: &Instance) -> Real {
    i.path.as_ref().map_or(Real::ONE, |r| r.scale)
}

/// `path_x` (`x` true) or `path_y`: the raw path point, which needs the game definition.
fn path_point(engine: &mut Engine, shared: &Rc<Shared>, name: &'static str, x: bool) {
    let s = Rc::clone(shared);
    engine.register_get(name, move |i: &mut Inst| -> Res<f64> {
        let i = *i;
        s.with_world(|w, host| {
            let inst = live(w, i)?;
            match &inst.path {
                Some(run) => path::position_point(run, &host.def)
                    .map(|p| if x { p.0.get() } else { p.1.get() })
                    .map_err(sim_err),
                None => Ok(0.0),
            }
        })?
    });
}
