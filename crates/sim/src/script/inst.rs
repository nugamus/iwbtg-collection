//! The instance API of the script engine (architecture "Script API"; WP9.2).
//!
//! A script sees an instance as an [`Inst`] handle. Instance ids are never reused, so a handle
//! to a destroyed instance stays safe: reads and writes through it are script errors, and
//! `exists(handle)` says whether it is still alive. Built-in fields are properties (`this.x`,
//! `this.hspeed`, ...); the package's custom variables (`GameDef::var_names`) are reached
//! through the same syntax (`this.hp`), by the string indexer.
//!
//! Every number a script reads is a FLOAT. Setters and arguments accept an integer literal too
//! and convert it, so `this.x = 5` works.

use std::rc::Rc;

use rhai::{Array, Dynamic, Engine, ImmutableString, Map};

use super::api::runtime_error;
use super::host::Shared;
use crate::alarm;
use crate::def::{ClassMask, KindId, ParamValue, Params};
use crate::error::SimError;
use crate::kid::{self, api::KillCause};
use crate::num::Real;
use crate::output::OutputEvent;
use crate::script::ScriptOutput;
use crate::world::{InstId, Instance, SpawnInit, Value, World};

type Res<T> = Result<T, Box<rhai::EvalAltResult>>;

/// A script's handle to an instance.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Inst(pub InstId);

/// A number argument: an integer or a float.
#[allow(clippy::cast_precision_loss, reason = "script integers are small")]
fn num(d: &Dynamic, what: &str) -> Res<f64> {
    if let Ok(f) = d.as_float() {
        Ok(f)
    } else if let Ok(i) = d.as_int() {
        Ok(i as f64)
    } else {
        Err(runtime_error(format!(
            "{what}: expected a number, got {}",
            d.type_name()
        )))
    }
}

fn sim_err(e: SimError) -> Box<rhai::EvalAltResult> {
    runtime_error(e.to_string())
}

fn gone(i: Inst) -> Box<rhai::EvalAltResult> {
    runtime_error(format!("instance {} no longer exists", i.0.0))
}

fn live(w: &World, i: Inst) -> Res<&Instance> {
    w.get(i.0).filter(|x| x.alive).ok_or_else(|| gone(i))
}

fn live_mut(w: &mut World, i: Inst) -> Res<&mut Instance> {
    w.get_mut(i.0).filter(|x| x.alive).ok_or_else(|| gone(i))
}

type Get = fn(&Instance) -> Real;
type Set = fn(&mut Instance, Real) -> Result<(), SimError>;

fn real_prop(engine: &mut Engine, s: &Rc<Shared>, name: &'static str, get: Get, set: Option<Set>) {
    {
        let s = Rc::clone(s);
        engine.register_get(name, move |i: &mut Inst| -> Res<f64> {
            let i = *i;
            s.with_world(|w, _| live(w, i).map(|x| get(x).get()))?
        });
    }
    if let Some(set) = set {
        let s = Rc::clone(s);
        engine.register_set(name, move |i: &mut Inst, v: Dynamic| -> Res<()> {
            let v = Real::new(num(&v, name)?);
            let i = *i;
            s.with_world(|w, _| live_mut(w, i).and_then(|x| set(x, v).map_err(sim_err)))?
        });
    }
}

fn value_to_dynamic(v: Option<&Value>, zero: bool, name: &str) -> Res<Dynamic> {
    match v {
        Some(Value::Real(r)) => Ok(Dynamic::from_float(r.get())),
        Some(Value::Str(s)) => Ok(Dynamic::from(ImmutableString::from(s.as_str()))),
        Some(Value::Inst(i)) => Ok(Dynamic::from(Inst(*i))),
        Some(Value::None) | None if zero => Ok(Dynamic::from_float(0.0)),
        Some(Value::None) | None => Err(runtime_error(format!("variable `{name}` is not set"))),
    }
}

fn dynamic_to_value(d: &Dynamic, name: &str) -> Res<Value> {
    if d.is_unit() {
        return Ok(Value::None);
    }
    if let Ok(b) = d.as_bool() {
        return Ok(Value::Real(if b { Real::ONE } else { Real::ZERO }));
    }
    if let Ok(f) = num(d, name) {
        let r = Real::new(f);
        if !r.is_finite() {
            return Err(sim_err(SimError::non_finite(name.to_owned())));
        }
        return Ok(Value::Real(r));
    }
    if let Some(s) = d.clone().try_cast::<ImmutableString>() {
        return Ok(Value::Str(s.to_string()));
    }
    if let Some(i) = d.clone().try_cast::<Inst>() {
        return Ok(Value::Inst(i.0));
    }
    Err(runtime_error(format!(
        "variable `{name}` cannot hold a {}",
        d.type_name()
    )))
}

fn param_from(name: &str, d: &Dynamic) -> Res<ParamValue> {
    if let Ok(b) = d.as_bool() {
        return Ok(ParamValue::Bool(b));
    }
    if let Ok(f) = num(d, name) {
        return Ok(ParamValue::Real(Real::new(f)));
    }
    if let Some(s) = d.clone().try_cast::<ImmutableString>() {
        return Ok(ParamValue::Str(s.to_string()));
    }
    Err(runtime_error(format!(
        "spawn: property `{name}` must be a number, bool or string"
    )))
}

fn alarm_index(n: &Dynamic) -> Res<u8> {
    let f = num(n, "alarm")?;
    let i = Real::new(f);
    if !(0.0..12.0).contains(&f) || !i.is_finite() {
        return Err(runtime_error(format!("alarm[{f}] does not exist (0-11)")));
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "checked above"
    )]
    Ok(f as u8)
}

fn kind_id(host: &super::host::Host, name: &str) -> Res<KindId> {
    host.kinds
        .get(name)
        .copied()
        .ok_or_else(|| runtime_error(format!("unknown kind `{name}`")))
}

/// Registers the instance API on `engine`.
#[allow(clippy::too_many_lines, reason = "one registration per script-visible name")]
pub fn register_instance_api(engine: &mut Engine, shared: &Rc<Shared>) {
    engine.register_type_with_name::<Inst>("Instance");
    engine.register_fn("==", |a: Inst, b: Inst| a == b);
    engine.register_fn("!=", |a: Inst, b: Inst| a != b);

    // ---- Built-in fields ----
    real_prop(engine, shared, "x", |i| i.m.x(), Some(|i, v| i.m.set_x(v)));
    real_prop(engine, shared, "y", |i| i.m.y(), Some(|i, v| i.m.set_y(v)));
    real_prop(
        engine,
        shared,
        "hspeed",
        |i| i.m.hspeed(),
        Some(|i, v| i.m.set_hspeed(v)),
    );
    real_prop(
        engine,
        shared,
        "vspeed",
        |i| i.m.vspeed(),
        Some(|i, v| i.m.set_vspeed(v)),
    );
    real_prop(
        engine,
        shared,
        "speed",
        |i| i.m.speed(),
        Some(|i, v| i.m.set_speed(v)),
    );
    real_prop(
        engine,
        shared,
        "direction",
        |i| i.m.direction(),
        Some(|i, v| i.m.set_direction(v)),
    );
    real_prop(
        engine,
        shared,
        "gravity",
        |i| i.m.gravity(),
        Some(|i, v| i.m.set_gravity(v)),
    );
    real_prop(
        engine,
        shared,
        "gravity_direction",
        |i| i.m.gravity_direction(),
        Some(|i, v| i.m.set_gravity_direction(v)),
    );
    real_prop(
        engine,
        shared,
        "friction",
        |i| i.m.friction(),
        Some(|i, v| i.m.set_friction(v)),
    );
    real_prop(engine, shared, "xprevious", |i| i.m.xprevious(), None);
    real_prop(engine, shared, "yprevious", |i| i.m.yprevious(), None);
    real_prop(engine, shared, "xstart", |i| i.m.xstart(), None);
    real_prop(engine, shared, "ystart", |i| i.m.ystart(), None);
    real_prop(
        engine,
        shared,
        "image_index",
        |i| i.img.index(),
        Some(|i, v| i.img.set_index(v)),
    );
    real_prop(
        engine,
        shared,
        "image_speed",
        |i| i.img.speed(),
        Some(|i, v| i.img.set_speed(v)),
    );
    real_prop(
        engine,
        shared,
        "image_xscale",
        |i| i.img.xscale(),
        Some(|i, v| i.img.set_xscale(v)),
    );
    real_prop(
        engine,
        shared,
        "image_yscale",
        |i| i.img.yscale(),
        Some(|i, v| i.img.set_yscale(v)),
    );
    real_prop(
        engine,
        shared,
        "image_angle",
        |i| i.img.angle(),
        Some(|i, v| i.img.set_angle(v)),
    );
    real_prop(
        engine,
        shared,
        "image_alpha",
        |i| i.img.alpha(),
        Some(|i, v| i.img.set_alpha(v)),
    );
    real_prop(
        engine,
        shared,
        "depth",
        |i| i.depth,
        Some(|i, v| {
            if v.is_finite() {
                i.depth = v;
                Ok(())
            } else {
                Err(SimError::non_finite("depth"))
            }
        }),
    );
    {
        let s = Rc::clone(shared);
        engine.register_get("visible", move |i: &mut Inst| -> Res<bool> {
            let i = *i;
            s.with_world(|w, _| live(w, i).map(|x| x.visible))?
        });
        let s = Rc::clone(shared);
        engine.register_set("visible", move |i: &mut Inst, v: bool| -> Res<()> {
            let i = *i;
            s.with_world(|w, _| live_mut(w, i).map(|x| x.visible = v))?
        });
        let s = Rc::clone(shared);
        engine.register_get("solid", move |i: &mut Inst| -> Res<bool> {
            let i = *i;
            s.with_world(|w, _| live(w, i).map(|x| x.solid))?
        });
        let s = Rc::clone(shared);
        engine.register_set("solid", move |i: &mut Inst, v: bool| -> Res<()> {
            let i = *i;
            s.with_world(|w, _| live_mut(w, i).map(|x| x.solid = v))?
        });
        let s = Rc::clone(shared);
        engine.register_get("id", move |i: &mut Inst| -> Res<f64> {
            let i = *i;
            #[allow(clippy::cast_precision_loss, reason = "ids stay far below 2^53")]
            s.with_world(|w, _| live(w, i).map(|x| x.id.0 as f64))?
        });
        let s = Rc::clone(shared);
        engine.register_get("kind", move |i: &mut Inst| -> Res<ImmutableString> {
            let i = *i;
            s.with_world(|w, host| {
                let x = live(w, i)?;
                let kd = host
                    .def
                    .kind(x.kind)
                    .ok_or_else(|| runtime_error("instance has an unknown kind"))?;
                Ok(ImmutableString::from(kd.name.as_str()))
            })?
        });
    }

    // ---- The package's custom variables: `this.hp` ----
    {
        let s = Rc::clone(shared);
        engine.register_indexer_get(move |i: &mut Inst, name: ImmutableString| -> Res<Dynamic> {
            let i = *i;
            s.with_world(|w, host| {
                let id = host
                    .def
                    .var_names
                    .get(name.as_str())
                    .ok_or_else(|| runtime_error(format!("unknown variable `{name}`")))?;
                let x = live(w, i)?;
                value_to_dynamic(x.vars.get(id), host.def.uninit_reads_zero, &name)
            })?
        });
        let s = Rc::clone(shared);
        engine.register_indexer_set(
            move |i: &mut Inst, name: ImmutableString, v: Dynamic| -> Res<()> {
                let i = *i;
                let value = dynamic_to_value(&v, &name)?;
                s.with_world(|w, host| {
                    let id = *host
                        .def
                        .var_names
                        .get(name.as_str())
                        .ok_or_else(|| runtime_error(format!("unknown variable `{name}`")))?;
                    live_mut(w, i)?.vars.insert(id, value);
                    Ok(())
                })?
            },
        );
    }

    // ---- Alarms ----
    {
        let s = Rc::clone(shared);
        engine.register_fn(
            "set_alarm",
            move |i: &mut Inst, n: Dynamic, steps: Dynamic| -> Res<()> {
                let (i, n, steps) = (*i, alarm_index(&n)?, Real::new(num(&steps, "set_alarm")?));
                s.with_world(|w, _| {
                    let x = live_mut(w, i)?;
                    alarm::set(x, n, steps, &s.num).map_err(sim_err)
                })?
            },
        );
        let s = Rc::clone(shared);
        engine.register_fn("get_alarm", move |i: &mut Inst, n: Dynamic| -> Res<f64> {
            let (i, n) = (*i, alarm_index(&n)?);
            s.with_world(|w, _| {
                let x = live(w, i)?;
                Ok(x.alarms.get(usize::from(n)).map_or(-1.0, |a| a.get()))
            })?
        });
    }

    // ---- Instances ----
    {
        let s = Rc::clone(shared);
        engine.register_fn("exists", move |i: Inst| -> bool {
            s.with_world(|w, _| w.exists(i.0)).unwrap_or(false)
        });
        let s = Rc::clone(shared);
        engine.register_fn("destroy", move |i: Inst| -> Res<()> {
            let flow = s.with_ctx(|ctx| ctx.destroy(i.0))?;
            if flow.is_halt() {
                return Err(runtime_error("the sim halted while destroying an instance"));
            }
            Ok(())
        });
        let s = Rc::clone(shared);
        engine.register_fn(
            "instance_create",
            move |kind: ImmutableString, x: Dynamic, y: Dynamic| -> Res<Inst> {
                let (x, y) = (Real::new(num(&x, "spawn")?), Real::new(num(&y, "spawn")?));
                spawn(&s, &kind, x, y, Params::default())
            },
        );
        let s = Rc::clone(shared);
        engine.register_fn(
            "instance_create",
            move |kind: ImmutableString, x: Dynamic, y: Dynamic, props: Map| -> Res<Inst> {
                let (x, y) = (Real::new(num(&x, "spawn")?), Real::new(num(&y, "spawn")?));
                let mut params = Params::default();
                for (k, v) in &props {
                    params.0.insert(k.to_string(), param_from(k, v)?);
                }
                spawn(&s, &kind, x, y, params)
            },
        );
        let s = Rc::clone(shared);
        engine.register_fn("instances", move |kind: ImmutableString| -> Res<Array> {
            s.with_world(|w, host| {
                let k = kind_id(host, &kind)?;
                let mut found: Vec<(u16, InstId)> = w
                    .iter_alive()
                    .filter(|x| x.active && x.kind == k)
                    .map(|x| (x.rank, x.id))
                    .collect();
                found.sort();
                Ok(found.into_iter().map(|(_, id)| Dynamic::from(Inst(id))).collect())
            })?
        });
        let s = Rc::clone(shared);
        engine.register_fn("kid", move || -> Res<Dynamic> {
            s.with_world(|w, host| {
                Ok(w.first_of(ClassMask::KID, &host_tick(host))
                    .map_or(Dynamic::UNIT, |id| Dynamic::from(Inst(id))))
            })?
        });
    }

    // ---- Effects ----
    {
        let s = Rc::clone(shared);
        engine.register_fn("emit", move |name: ImmutableString| -> Res<()> {
            if name.len() > 64 {
                return Err(runtime_error("emit: names are at most 64 bytes"));
            }
            s.with_world(|w, _| {
                w.output.push(OutputEvent::Script(ScriptOutput::Emit {
                    name: name.to_string(),
                }));
            })
        });
        let s = Rc::clone(shared);
        engine.register_fn("kill_kid", move || -> Res<()> {
            let r = s.with_ctx(|ctx| {
                let Some(k) = ctx.w.first_of(ClassMask::KID, &ctx.rules.tick) else {
                    return Ok(());
                };
                kid::api::kill(ctx, k, KillCause::Script).map(|_| ())
            })?;
            r.map_err(sim_err)
        });
    }
}

fn host_tick(host: &super::host::Host) -> crate::rules::TickRules {
    host.rules.tick.clone()
}

fn spawn(s: &Rc<Shared>, kind: &str, x: Real, y: Real, params: Params) -> Res<Inst> {
    let k = s.with_world(|_, host| kind_id(host, kind))??;
    let init = SpawnInit {
        params,
        ..SpawnInit::default()
    };
    let id = s.with_ctx(|ctx| ctx.spawn(k, x, y, init))?;
    id.map(Inst).map_err(sim_err)
}
