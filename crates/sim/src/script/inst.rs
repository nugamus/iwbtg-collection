//! The instance API of the script engine (architecture "Script API").
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
use super::array::{dynamic_to_value, value_to_dynamic};
use super::budget::cost;
use super::docs::Docs;
use super::host::Shared;
use crate::alarm;
use crate::collide::bounce::{Against, move_bounce};
use crate::def::class::{kind_classes, kind_params, picks_other_kind};
use crate::def::script::ValueModel;
use crate::def::{ClassMask, KindId, ParamValue, Params, RoomId, TimelineId};
use crate::error::SimError;
use crate::kid::{self, KidState, api::KillCause};
use crate::num::Real;
use crate::output::OutputEvent;
use crate::room::RoomRequest;
use crate::script::ScriptOutput;
use crate::timeline;
use crate::world::{InstId, Instance, KindState, SpawnInit, World};

type Res<T> = Result<T, Box<rhai::EvalAltResult>>;

/// A script's handle to an instance.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Inst(pub InstId);

/// A number argument: an integer or a float.
#[allow(clippy::cast_precision_loss, reason = "script integers are small")]
pub(super) fn num(d: &Dynamic, what: &str) -> Res<f64> {
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

pub(super) fn live(w: &World, i: Inst) -> Res<&Instance> {
    w.get(i.0).filter(|x| x.alive).ok_or_else(|| gone(i))
}

pub(super) fn live_mut(w: &mut World, i: Inst) -> Res<&mut Instance> {
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

/// A parameter as a script value, mapped as `Ctx::spawn` maps it to the custom variable it stores
/// (a flag is 1.0/0.0, a resource id its index, a list its first value), so `param("x")` and
/// `this.x` agree.
fn param_dynamic(v: &ParamValue) -> Dynamic {
    match v {
        ParamValue::Real(r) => Dynamic::from_float(r.get()),
        ParamValue::Bool(b) => Dynamic::from_float(if *b { 1.0 } else { 0.0 }),
        ParamValue::Str(s) => Dynamic::from(ImmutableString::from(s.as_str())),
        ParamValue::Room(id) => Dynamic::from_float(f64::from(id.0)),
        ParamValue::Kind(id) => Dynamic::from_float(f64::from(id.0)),
        ParamValue::Sound(id) => Dynamic::from_float(f64::from(id.0)),
        ParamValue::Particles(id) => Dynamic::from_float(f64::from(id.0)),
        // GML has no list value: a list reads as its first value, as a single value would.
        ParamValue::List(l) => l.first().map_or(Dynamic::from_float(0.0), param_dynamic),
    }
}

/// The effective parameter `name` of instance `i`: the value the instance was created with (a
/// room-file override, creation-code value or spawn map is stored as the custom variable at
/// spawn), else the kind's object-file default. The same resolution the built-ins get: a room
/// value overrides the object default. A name the instance has no parameter for is an error.
fn read_param(s: &Rc<Shared>, i: Inst, name: &str) -> Res<Dynamic> {
    s.with_world(|w, host| {
        let x = live(w, i)?;
        let kd = host
            .def
            .kind(x.kind)
            .ok_or_else(|| runtime_error(format!("instance {} has unknown kind", i.0.0)))?;
        let stored = host.def.var_names.get(name).and_then(|id| x.vars.get(id));
        match (stored, kd.params.0.get(name)) {
            (Some(v), _) => value_to_dynamic(s, Some(v), host.script_rules, &format!("param `{name}`")),
            (None, Some(v)) => Ok(param_dynamic(v)),
            (None, None) => Err(runtime_error(format!(
                "param `{name}` is not set on instance {}",
                i.0.0
            ))),
        }
    })?
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

/// The kind a script spawn of `name` with `params` creates, chosen as the loader chooses a placed
/// instance's. First the `[defaults.<kind>]` variant the params match (`spike` with
/// `dir: "down"` takes `spike[dir=down]` and its sprite and mask, `GameDef::variant_kind`, #701).
/// Then classes and events (#694, #853): they come from the kind, as a GM instance's come from
/// its object, so when the params that pick them (`def::class::kind_params`: water `variant`,
/// vine `side`, killer `destroy_outside_room`, ...) pick other classes or another event set than
/// that kind's, the spawn takes the kind the loader derived from it with those values for a
/// placed instance (#659). Without one the spawn is an error: a kind is never made at run time.
fn spawn_kind(host: &super::host::Host, name: &str, params: &Params) -> Res<KindId> {
    let k = host.def.variant_kind(kind_id(host, name)?, params);
    let Some(kd) = host.def.kind(k) else {
        return Ok(k);
    };
    let picked: Vec<(&str, &ParamValue)> = kind_params(kd.base)
        .into_iter()
        .filter_map(|key| params.0.get(key).map(|v| (key, v)))
        .collect();
    if picked.is_empty() {
        return Ok(k);
    }
    let mut merged = kd.params.clone();
    for (key, v) in &picked {
        merged.0.insert((*key).to_owned(), (*v).clone());
    }
    // The loader's test (`class_kind`): the merged params' classes and event set against the
    // kind's, with the kind's extra (`counts_as`) classes, which are the kind's classes the params
    // do not give.
    let own = kind_classes(kd.base, &kd.params, ClassMask::NONE);
    let extra = ClassMask(kd.classes.0 & !own.0);
    if !picks_other_kind(kd, &merged, extra) {
        return Ok(k);
    }
    let derived = host
        .def
        .kinds
        .iter()
        .zip(0u32..)
        .find(|(d, i)| d.params == merged && host.def.kind_covers(k, KindId(*i)));
    if let Some((_, i)) = derived {
        return Ok(KindId(i));
    }
    let shown = picked
        .iter()
        .map(|(key, v)| format!("{key} = {}", show_param(v)))
        .collect::<Vec<_>>()
        .join(", ");
    Err(runtime_error(format!(
        "instance_create: `{name}` with {shown} picks other classes or events than the kind's, and no kind derived from `{name}` has them (classes and events come from the kind; place such an instance in a room or give the package an object with these params)"
    )))
}

fn show_param(v: &ParamValue) -> String {
    match v {
        ParamValue::Real(r) => r.get().to_string(),
        ParamValue::Bool(b) => b.to_string(),
        ParamValue::Str(s) => format!("\"{s}\""),
        other => format!("{other:?}"),
    }
}

/// The kid fields a script may read but not write (decision 0004). They have a getter and no
/// setter, so a write reaches the custom-variable indexer; this list keeps it a clear error
/// rather than a silently stored variable with a field's name. `yflag` and `flipped` change only
/// through the gravity knobs, so the mask and gravity cannot disagree.
const READ_ONLY_KID_FIELDS: &[&str] = &[
    "ladderud",
    "ladderjump",
    "yflag",
    "flipped",
    "facing",
    "jump",
    "jump2",
    "jump3",
    "on_slope_last",
    "walk_h",
    "onVineL",
    "onVineR",
];

/// Registers the instance API on `engine`.
#[allow(clippy::too_many_lines, reason = "one registration per script-visible name")]
pub fn register_instance_api(engine: &mut Engine, shared: &Rc<Shared>, docs: &mut Docs) {
    engine.register_type_with_name::<Inst>("Instance");
    engine.register_fn("==", |a: Inst, b: Inst| a == b);
    engine.register_fn("!=", |a: Inst, b: Inst| a != b);

    // ---- Built-in fields ----
    docs.add("x", "Horizontal position.");
    real_prop(engine, shared, "x", |i| i.m.x(), Some(|i, v| i.m.set_x(v)));
    docs.add("y", "Vertical position.");
    real_prop(engine, shared, "y", |i| i.m.y(), Some(|i, v| i.m.set_y(v)));
    docs.add(
        "hspeed",
        "Horizontal speed; setting it updates `speed` and `direction`.",
    );
    real_prop(
        engine,
        shared,
        "hspeed",
        |i| i.m.hspeed(),
        Some(|i, v| i.m.set_hspeed(v)),
    );
    docs.add(
        "vspeed",
        "Vertical speed; setting it updates `speed` and `direction`.",
    );
    real_prop(
        engine,
        shared,
        "vspeed",
        |i| i.m.vspeed(),
        Some(|i, v| i.m.set_vspeed(v)),
    );
    docs.add(
        "speed",
        "Speed along `direction`; setting it updates `hspeed` and `vspeed`.",
    );
    real_prop(
        engine,
        shared,
        "speed",
        |i| i.m.speed(),
        Some(|i, v| i.m.set_speed(v)),
    );
    docs.add(
        "direction",
        "Movement direction in degrees; setting it updates `hspeed` and `vspeed`.",
    );
    real_prop(
        engine,
        shared,
        "direction",
        |i| i.m.direction(),
        Some(|i, v| i.m.set_direction(v)),
    );
    docs.add("gravity", "Gravity added each tick.");
    real_prop(
        engine,
        shared,
        "gravity",
        |i| i.m.gravity(),
        Some(|i, v| i.m.set_gravity(v)),
    );
    docs.add("gravity_direction", "Direction of `gravity` in degrees.");
    real_prop(
        engine,
        shared,
        "gravity_direction",
        |i| i.m.gravity_direction(),
        Some(|i, v| i.m.set_gravity_direction(v)),
    );
    docs.add("friction", "Speed removed each tick.");
    real_prop(
        engine,
        shared,
        "friction",
        |i| i.m.friction(),
        Some(|i, v| i.m.set_friction(v)),
    );
    docs.add("xprevious", "`x` at the start of the tick.");
    real_prop(engine, shared, "xprevious", |i| i.m.xprevious(), None);
    docs.add("yprevious", "`y` at the start of the tick.");
    real_prop(engine, shared, "yprevious", |i| i.m.yprevious(), None);
    docs.add("xstart", "`x` where the instance was created.");
    real_prop(engine, shared, "xstart", |i| i.m.xstart(), None);
    docs.add("ystart", "`y` where the instance was created.");
    real_prop(engine, shared, "ystart", |i| i.m.ystart(), None);
    docs.add("image_index", "Current animation frame.");
    real_prop(
        engine,
        shared,
        "image_index",
        |i| i.img.index(),
        Some(|i, v| i.img.set_index(v)),
    );
    docs.add("image_speed", "Animation frames per tick.");
    real_prop(
        engine,
        shared,
        "image_speed",
        |i| i.img.speed(),
        Some(|i, v| i.img.set_speed(v)),
    );
    docs.add(
        "image_xscale",
        "Horizontal scale of the sprite; on the kid it is the facing, -1 or 1.",
    );
    real_prop(
        engine,
        shared,
        "image_xscale",
        |i| i.img.xscale(),
        Some(|i, v| {
            i.img.set_xscale(v)?;
            // Decision 0004: the kid's `facing` is derived from `image_xscale`, which is its one
            // setter. A non-negative scale faces right, as a loaded kid does.
            if let KindState::Kid(s) = &mut i.state {
                s.facing = if v.get() < 0.0 { -1 } else { 1 };
            }
            Ok(())
        }),
    );
    docs.add("image_yscale", "Vertical scale of the sprite.");
    real_prop(
        engine,
        shared,
        "image_yscale",
        |i| i.img.yscale(),
        Some(|i, v| i.img.set_yscale(v)),
    );
    docs.add("image_angle", "Sprite rotation in degrees.");
    real_prop(
        engine,
        shared,
        "image_angle",
        |i| i.img.angle(),
        Some(|i, v| i.img.set_angle(v)),
    );
    docs.add("image_alpha", "Sprite opacity, 0 to 1.");
    real_prop(
        engine,
        shared,
        "image_alpha",
        |i| i.img.alpha(),
        Some(|i, v| i.img.set_alpha(v)),
    );
    docs.add("depth", "Draw depth; a lower value draws in front.");
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
        docs.add("visible", "Whether the instance is drawn.");
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
        docs.add("solid", "Whether the instance blocks movement.");
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
        docs.add("id", "The instance id (never reused).");
        engine.register_get("id", move |i: &mut Inst| -> Res<f64> {
            let i = *i;
            #[allow(clippy::cast_precision_loss, reason = "ids stay far below 2^53")]
            s.with_world(|w, _| live(w, i).map(|x| x.id.0 as f64))?
        });
        let s = Rc::clone(shared);
        docs.add("kind", "The package name of the instance's kind (`@boss`).");
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
        let s = Rc::clone(shared);
        docs.add(
            "name",
            "The instance's room-file name as a string, or `()` when the room file gives none.",
        );
        engine.register_get("name", move |i: &mut Inst| -> Res<Dynamic> {
            let i = *i;
            s.with_world(|w, host| {
                let x = live(w, i)?;
                Ok(x.name
                    .and_then(|n| n.index())
                    .and_then(|n| host.def.names.get(n))
                    .map_or(Dynamic::UNIT, |name| {
                        Dynamic::from(ImmutableString::from(name.as_str()))
                    }))
            })?
        });
        let s = Rc::clone(shared);
        docs.add(
            "group",
            "The instance's room-file group as a string, or `()` when the room file gives none.",
        );
        engine.register_get("group", move |i: &mut Inst| -> Res<Dynamic> {
            let i = *i;
            s.with_world(|w, host| {
                let x = live(w, i)?;
                Ok(x.group
                    .and_then(|g| g.index())
                    .and_then(|g| host.def.groups.get(g))
                    .map_or(Dynamic::UNIT, |group| {
                        Dynamic::from(ImmutableString::from(group.as_str()))
                    }))
            })?
        });
    }

    // ---- The package's custom variables: `this.hp` ----
    {
        let s = Rc::clone(shared);
        docs.add("index$get$", "Reads a package variable of an instance (`i.hp`; an unknown name is an error), a global (`global.difficulty`) or an element of a stored array (`this.pattern[3]`). An unset variable, global or element follows the run's script init policy (0 under `gm8` with uninitialized reads as 0, else a script error). An array index is a whole number from 0 below 10,000; a fractional or negative one is a script error (OPEN in the GM8 spec). Reading a variable that holds an array copies it.");
        engine.register_indexer_get(move |i: &mut Inst, name: ImmutableString| -> Res<Dynamic> {
            let i = *i;
            s.with_world(|w, host| {
                let id = host
                    .def
                    .var_names
                    .get(name.as_str())
                    .ok_or_else(|| runtime_error(format!("unknown variable `{name}`")))?;
                let x = live(w, i)?;
                value_to_dynamic(
                    &s,
                    x.vars.get(id),
                    host.script_rules,
                    &format!("variable `{name}`"),
                )
            })?
        });
        let s = Rc::clone(shared);
        docs.add("index$set$", "Writes a package variable of an instance (`i.hp = 5`; the name must be declared in the package), a global (`global.flags = [0.0, 0.0]`, created on first write) or an element of a stored array (`this.pattern[3] = 9.0`; past the end grows the array, the skipped elements unset; at most 10,000). `gm8` stores a bool as 1.0/0.0 and a bare write to a variable holding an array writes element 0 (contract `script/global-array-bare-write`); `standard` refuses a bool until the typed value model stores one and replaces the value.");
        engine.register_indexer_set(
            move |i: &mut Inst, name: ImmutableString, v: Dynamic| -> Res<()> {
                let i = *i;
                s.with_world(|w, host| {
                    if READ_ONLY_KID_FIELDS.contains(&name.as_str()) {
                        return Err(runtime_error(format!("`{name}` is a read-only kid field")));
                    }
                    let value = dynamic_to_value(&s, &v, host.script_rules, &name)?;
                    let id = *host
                        .def
                        .var_names
                        .get(name.as_str())
                        .ok_or_else(|| runtime_error(format!("unknown variable `{name}`")))?;
                    let vars = &mut live_mut(w, i)?.vars;
                    match host.script_rules.values {
                        // GM8: the bare name is element 0, so an array keeps its other
                        // elements (#1133, contract `script/global-array-bare-write`).
                        ValueModel::Gm8 => crate::world::assign_bare(vars, id, value),
                        // The standard model replaces the value, as assignment does elsewhere.
                        ValueModel::Typed => {
                            vars.insert(id, value);
                        }
                    }
                    Ok(())
                })?
            },
        );
    }

    // ---- Parameters: `param(name)` and `this.param(name)` ----
    {
        docs.add(
            "param",
            "Reads one of the instance's parameters (`param(\"mode\")` or `this.param(\"mode\")`), from the object file or overridden in the room file. The value is the effective one, resolved as the built-ins resolve it: a room-file value overrides the object-file default. A name that is not set on the instance is an error; a string literal is checked at load.",
        );
        let s = Rc::clone(shared);
        engine.register_fn(
            "param",
            move |i: &mut Inst, name: ImmutableString| -> Res<Dynamic> {
                let i = *i;
                read_param(&s, i, &name)
            },
        );
        // The bare form reads the instance of the call running now (`this`), as `script_execute`
        // keeps it (`host::Inner::execute`).
        let s = Rc::clone(shared);
        engine.register_fn("param", move |name: ImmutableString| -> Res<Dynamic> {
            let i = s
                .this_inst
                .get()
                .ok_or_else(|| runtime_error("param is only available inside a script call"))?;
            read_param(&s, Inst(i), &name)
        });
    }

    // ---- Alarms ----
    {
        let s = Rc::clone(shared);
        docs.add("set_alarm", "`i.set_alarm(n, steps)` sets alarm `n` (0 to 11) to `steps` ticks, rounded as GM does; -1 means not running; a non-finite value or another `n` is an error.");
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
        docs.add(
            "get_alarm",
            "`i.get_alarm(n)` is the ticks left on alarm `n`, or -1 when it is not running.",
        );
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
        docs.add(
            "exists",
            "Whether the instance handle still refers to a live instance.",
        );
        engine.register_fn("exists", move |i: Inst| -> bool {
            s.with_world(|w, _| w.exists(i.0)).unwrap_or(false)
        });
        let s = Rc::clone(shared);
        docs.add(
            "destroy",
            "Destroys an instance now; its Destroy event runs first.",
        );
        engine.register_fn("destroy", move |i: Inst| -> Res<()> {
            let flow = s.with_ctx(|ctx| ctx.destroy(i.0))?;
            if flow.is_halt() {
                return Err(runtime_error("the sim halted while destroying an instance"));
            }
            Ok(())
        });
        for (name, against, doc) in [
            (
                "move_bounce_solid",
                Against::Solid,
                "`i.move_bounce_solid(precise)`: GML `move_bounce_solid`, the Bounce action against solid instances. Changes only `hspeed` and `vspeed`, never the position; the tick's motion then moves by the new speed. Not precise negates `hspeed` when (`x + hspeed`, `y`) is blocked, `vspeed` when (`x`, `y + vspeed`) is, both when only the diagonal is; precise picks a new direction (a multiple of 10) from 36 samples at radius `speed`.",
            ),
            (
                "move_bounce_all",
                Against::All,
                "`i.move_bounce_all(precise)`: as `move_bounce_solid`, against every instance (`place_empty`).",
            ),
        ] {
            let s = Rc::clone(shared);
            docs.add(name, doc);
            engine.register_fn(name, move |i: &mut Inst, precise: bool| -> Res<()> {
                let i = *i;
                s.with_ctx(|ctx| move_bounce(ctx, i.0, precise, against))?
                    .map_err(sim_err)
            });
        }
        let s = Rc::clone(shared);
        docs.add("instance_create", "Creates an instance of a kind at (`x`, `y`) and runs its Create event; an optional map sets its parameters. Params that match a `[defaults.<kind>]` variant (`spike` with `dir: \"down\"`) take the variant's kind, with its sprite and mask, as a placed instance does. Params that pick a class or an event set (water `variant`, vine `side`, killer `bounce`, ...) take the kind the loader derived with those values; an error when there is none. Returns the handle.");
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
        docs.add(
            "instances",
            "The live instances of a kind and of the kinds the loader derived from it (`water[variant=2]` is a `water`, as a GM object covers its children) as an array, in event order, as a snapshot at call time.",
        );
        engine.register_fn("instances", move |kind: ImmutableString| -> Res<Array> {
            s.with_world(|w, host| {
                let k = kind_id(host, &kind)?;
                let mut found: Vec<(u16, InstId)> = w
                    .iter_visible()
                    .filter(|x| host.def.kind_covers(k, x.kind))
                    .map(|x| (x.rank, x.id))
                    .collect();
                found.sort();
                s.charge(cost("instances").1.saturating_mul(found.len() as u64))?;
                Ok(found.into_iter().map(|(_, id)| Dynamic::from(Inst(id))).collect())
            })?
        });
        let s = Rc::clone(shared);
        docs.add("kid", "The kid's instance, or `()` when there is none.");
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
        docs.add(
            "emit",
            "Queues a named signal for the client (a sound cue, a flash); at most 64 bytes.",
        );
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
        register_audio(engine, shared, docs);
        register_rooms(engine, shared, docs);
        register_timelines(engine, shared, docs);
        let s = Rc::clone(shared);
        docs.add("kill_kid", "Kills the kid as a script death.");
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

// ---- The kid's fields (decision 0004) ----

fn not_a_kid(i: Inst) -> Box<rhai::EvalAltResult> {
    runtime_error(format!("instance {} is not a kid", i.0.0))
}

fn kid_state(w: &World, i: Inst) -> Res<&KidState> {
    match &live(w, i)?.state {
        KindState::Kid(s) => Ok(s),
        _ => Err(not_a_kid(i)),
    }
}

fn kid_state_mut(w: &mut World, i: Inst) -> Res<&mut KidState> {
    match &mut live_mut(w, i)?.state {
        KindState::Kid(s) => Ok(s),
        _ => Err(not_a_kid(i)),
    }
}

/// A kid field that reads as a number. Without `set` it is read only: the custom-variable
/// indexer refuses a write (`READ_ONLY_KID_FIELDS`).
fn kid_real_prop(
    engine: &mut Engine,
    s: &Rc<Shared>,
    docs: &mut Docs,
    name: &'static str,
    doc: &'static str,
    get: fn(&KidState) -> Real,
    set: Option<fn(&mut KidState, Real)>,
) {
    if set.is_some() {
        docs.add(name, doc);
    } else {
        docs.add_read_only(name, doc);
    }
    {
        let s = Rc::clone(s);
        engine.register_get(name, move |i: &mut Inst| -> Res<f64> {
            let i = *i;
            s.with_world(|w, _| kid_state(w, i).map(|k| get(k).get()))?
        });
    }
    // No setter for a read-only field: the guard in `index$set$` refuses the write.
    if let Some(set) = set {
        let s = Rc::clone(s);
        engine.register_set(name, move |i: &mut Inst, v: Dynamic| -> Res<()> {
            let v = Real::new(num(&v, name)?);
            let i = *i;
            s.with_world(|w, _| kid_state_mut(w, i).map(|k| set(k, v)))?
        });
    }
}

/// A kid flag, written as a bool. Without `set` it is read only: the custom-variable indexer
/// refuses a write.
fn kid_flag_prop(
    engine: &mut Engine,
    s: &Rc<Shared>,
    docs: &mut Docs,
    name: &'static str,
    doc: &'static str,
    get: fn(&KidState) -> bool,
    set: Option<fn(&mut KidState, bool)>,
) {
    if set.is_some() {
        docs.add(name, doc);
    } else {
        docs.add_read_only(name, doc);
    }
    {
        let s = Rc::clone(s);
        engine.register_get(name, move |i: &mut Inst| -> Res<bool> {
            let i = *i;
            s.with_world(|w, _| kid_state(w, i).map(get))?
        });
    }
    // No setter for a read-only field: the guard in `index$set$` refuses the write.
    if let Some(set) = set {
        let s = Rc::clone(s);
        engine.register_set(name, move |i: &mut Inst, v: bool| -> Res<()> {
            let i = *i;
            s.with_world(|w, _| kid_state_mut(w, i).map(|k| set(k, v)))?
        });
    }
}

/// An integer argument for a small counter, `0` to `255`; a non-integer or out-of-range value is
/// an error (decision 0004, `curJumps`).
fn int_byte(d: &Dynamic, what: &str) -> Res<u8> {
    let n = if let Ok(i) = d.as_int() {
        i
    } else if let Ok(f) = d.as_float() {
        if f.fract() != 0.0 {
            return Err(runtime_error(format!("{what}: expected an integer, got {f}")));
        }
        #[allow(
            clippy::cast_possible_truncation,
            reason = "range-checked below; a huge value saturates and fails"
        )]
        {
            f as i64
        }
    } else {
        return Err(runtime_error(format!(
            "{what}: expected an integer, got {}",
            d.type_name()
        )));
    };
    u8::try_from(n).map_err(|_| runtime_error(format!("{what} must be 0 to 255, got {n}")))
}

/// The kid's own fields on an `Inst` (decision 0004, "Kid fields a script may read and write").
/// Every field lives in `KidState`, so a write is hashed and replay-visible. An instance that is
/// not a kid has none of them: reading or writing one is a script error. `yflag` and `flipped`
/// stay read-only so a script cannot leave the mask and gravity disagreeing; flip by the
/// `set_kid_gravity` knob when one exists.
#[allow(clippy::too_many_lines, reason = "one registration per field")]
pub fn register_kid_api(engine: &mut Engine, shared: &Rc<Shared>, docs: &mut Docs) {
    let s = shared;
    kid_flag_prop(
        engine,
        s,
        docs,
        "djump",
        "The double-jump flag.",
        |k| k.djump,
        Some(|k, v| k.djump = v),
    );
    kid_flag_prop(
        engine,
        s,
        docs,
        "tjump",
        "The third-jump flag.",
        |k| k.tjump,
        Some(|k, v| k.tjump = v),
    );
    kid_flag_prop(
        engine,
        s,
        docs,
        "onPlatform",
        "The kid is on a platform.",
        |k| k.on_platform,
        Some(|k, v| k.on_platform = v),
    );
    kid_flag_prop(
        engine,
        s,
        docs,
        "ladder",
        "The kid is on a ladder.",
        |k| k.ladder,
        Some(|k, v| k.ladder = v),
    );
    kid_flag_prop(
        engine,
        s,
        docs,
        "frozen",
        "Input is blocked for the kid.",
        |k| k.frozen,
        Some(|k, v| k.frozen = v),
    );
    kid_flag_prop(
        engine,
        s,
        docs,
        "frozen2",
        "The movement action is skipped.",
        |k| k.frozen2,
        Some(|k, v| k.frozen2 = v),
    );
    kid_flag_prop(
        engine,
        s,
        docs,
        "muteki",
        "Kills are ignored while set.",
        |k| k.invincible,
        Some(|k, v| k.invincible = v),
    );
    kid_flag_prop(
        engine,
        s,
        docs,
        "ladderud",
        "The kid moved on a ladder this Step.",
        |k| k.ladder_ud,
        None,
    );
    kid_flag_prop(
        engine,
        s,
        docs,
        "ladderjump",
        "The kid left a ladder by jumping this Step.",
        |k| k.ladder_jump,
        None,
    );
    kid_flag_prop(
        engine,
        s,
        docs,
        "flipped",
        "The kid is the flipped variant (read only; part of the kind).",
        |k| k.flipped,
        None,
    );
    kid_flag_prop(
        engine,
        s,
        docs,
        "on_slope_last",
        "The kid was on a slope last Step.",
        |k| k.on_slope_last,
        None,
    );
    kid_flag_prop(
        engine,
        s,
        docs,
        "onVineL",
        "A left vine is within reach (family C).",
        |k| k.ext.on_vine_l,
        None,
    );
    kid_flag_prop(
        engine,
        s,
        docs,
        "onVineR",
        "A right vine is within reach (family C).",
        |k| k.ext.on_vine_r,
        None,
    );
    kid_real_prop(
        engine,
        s,
        docs,
        "tempHspeed",
        "The ice carry-over speed (`tempHspeed`).",
        |k| k.temp_hspeed,
        Some(|k, v| k.temp_hspeed = v),
    );
    kid_real_prop(
        engine,
        s,
        docs,
        "maxSpeed",
        "The run speed (`maxSpeed`).",
        |k| k.run_speed,
        Some(|k, v| k.run_speed = v),
    );
    kid_real_prop(
        engine,
        s,
        docs,
        "maxVspeed",
        "The vertical speed clamp (`maxVspeed`).",
        |k| k.max_vspeed,
        Some(|k, v| k.max_vspeed = v),
    );
    kid_real_prop(
        engine,
        s,
        docs,
        "maxFallSpeed",
        "The fall-speed clamp (`maxFallSpeed`, A4).",
        |k| k.max_fall_speed,
        Some(|k, v| k.max_fall_speed = v),
    );
    kid_real_prop(
        engine,
        s,
        docs,
        "scale",
        "The layer scale (`scale`, machinary; 1 elsewhere).",
        |k| k.scale,
        Some(|k, v| k.scale = v),
    );
    kid_real_prop(
        engine,
        s,
        docs,
        "yflag",
        "The gravity side, -1 or 1 (read only; flip through the gravity knob).",
        |k| Real::new(f64::from(k.yflag)),
        None,
    );
    kid_real_prop(
        engine,
        s,
        docs,
        "facing",
        "Facing, -1 or 1 (read only; write `image_xscale`).",
        |k| Real::new(f64::from(k.facing)),
        None,
    );
    kid_real_prop(
        engine,
        s,
        docs,
        "walk_h",
        "The last `walk` direction, -1, 0 or 1.",
        |k| Real::new(f64::from(k.ext.walk_h)),
        None,
    );
    kid_real_prop(
        engine,
        s,
        docs,
        "jump",
        "The ground jump speed (read only; arrays are #997).",
        |k| k.jump_speeds.first().copied().unwrap_or(Real::ZERO),
        None,
    );
    kid_real_prop(
        engine,
        s,
        docs,
        "jump2",
        "The first air jump speed (read only).",
        |k| k.jump_speeds.get(1).copied().unwrap_or(Real::ZERO),
        None,
    );
    kid_real_prop(
        engine,
        s,
        docs,
        "jump3",
        "The second air jump speed (read only).",
        |k| k.jump_speeds.get(2).copied().unwrap_or(Real::ZERO),
        None,
    );
    docs.add(
        "curJumps",
        "The air-jump counter, 0 to 255; a non-integer or out-of-range write is an error.",
    );
    {
        let s = Rc::clone(shared);
        engine.register_get("curJumps", move |i: &mut Inst| -> Res<f64> {
            let i = *i;
            s.with_world(|w, _| kid_state(w, i).map(|k| f64::from(k.cur_jumps)))?
        });
        let s = Rc::clone(shared);
        engine.register_set("curJumps", move |i: &mut Inst, v: Dynamic| -> Res<()> {
            let n = int_byte(&v, "curJumps")?;
            let i = *i;
            s.with_world(|w, _| kid_state_mut(w, i).map(|k| k.cur_jumps = n))?
        });
    }

    // Paths (#1530): `path_start`, `path_end` and the instance's `path_*` properties.
    super::paths::register_path_api(engine, shared, docs);
    // Views (#1531): the `view_*` reads and `set_view_*` writes over `World::views`.
    super::views::register_view_api(engine, shared, docs);
    // Strings (#1532): the GML string functions.
    super::strings::register_string_api(engine, docs);
    // `script_execute` (#1533): a package script function by literal name.
    super::execute::register_execute_api(engine, shared, docs);
}

/// `room_goto`, `room_goto_next`, `room_goto_previous` and `room_restart` (#731). Each sets the
/// world's one pending room change (`Ctx::request_room`; the last request wins) and returns:
/// the rest of the script still runs, then `events::dispatch` and the phase stop the tick as for
/// a built-in's request (CLONE_SPEC "Room change requests"). The change itself runs at tick
/// row 15 (`room::perform_change`), where next and previous resolve by room order.
fn register_rooms(engine: &mut Engine, shared: &Rc<Shared>, docs: &mut Docs) {
    let s = Rc::clone(shared);
    docs.add(
        "room_goto",
        "Requests a change to the room with that id (`\"stage2\"`) once the current script returns; the rest of the tick does not run. An unknown room is an error.",
    );
    engine.register_fn("room_goto", move |name: ImmutableString| -> Res<()> {
        s.with_ctx(|ctx| {
            let room = room_id(ctx.def, &name)?;
            ctx.request_room(RoomRequest::Goto(room));
            Ok(())
        })?
    });
    let simple: [(&str, RoomRequest, &str); 3] = [
        (
            "room_goto_next",
            RoomRequest::Next,
            "Requests a change to the next room in `room_order` once the current script returns; from the last room the change is an error.",
        ),
        (
            "room_goto_previous",
            RoomRequest::Previous,
            "Requests a change to the previous room in `room_order` once the current script returns; from the first room the change is an error.",
        ),
        (
            "room_restart",
            RoomRequest::Restart,
            "Requests the current room again (Room End, then the room rebuilt from its file) once the current script returns.",
        ),
    ];
    for (name, req, doc) in simple {
        let s = Rc::clone(shared);
        docs.add(name, doc);
        engine.register_fn(name, move || -> Res<()> {
            s.with_ctx(|ctx| ctx.request_room(req))
        });
    }
}

/// `start_timeline(i, timeline)` and `stop_timeline(i)` (#1386, census rank 25). A start is the
/// same `timeline::start_with_def` the kind's `timeline` key and the trigger action use, at
/// position 0, so a script-started timeline runs its moments on the same tick as a built-in's
/// (the row-3 clock `timeline::advance` implements).
fn register_timelines(engine: &mut Engine, shared: &Rc<Shared>, docs: &mut Docs) {
    let s = Rc::clone(shared);
    docs.add(
        "start_timeline",
        "`i.start_timeline(timeline)` starts a package timeline on `i` at position 0, replacing any running one; the timeline's `loop` flag is applied as the kind's `timeline` key and the trigger action apply it. The moments then run in the row-3 timeline phase, starting the tick after the call. An unknown timeline is an error; a literal is checked at load.",
    );
    engine.register_fn(
        "start_timeline",
        move |i: &mut Inst, name: ImmutableString| -> Res<()> {
            let i = *i;
            s.with_world(|w, host| {
                let tl = timeline_id(&host.def, &name)?;
                let inst = live_mut(w, i)?;
                timeline::start_with_def(&mut inst.timeline, &host.def, tl, Real::ZERO).map_err(sim_err)
            })?
        },
    );
    let s = Rc::clone(shared);
    docs.add(
        "stop_timeline",
        "`i.stop_timeline()` stops advancing `i`'s timeline (GM `timeline_running = false`); its position and speed stay. A no-op when no timeline runs.",
    );
    engine.register_fn("stop_timeline", move |i: &mut Inst| -> Res<()> {
        let i = *i;
        s.with_world(|w, _| {
            let inst = live_mut(w, i)?;
            if let Some(run) = inst.timeline.as_mut() {
                run.running = false;
            }
            Ok(())
        })?
    });
}

/// A package timeline by name (its index in `GameDef::timelines`).
fn timeline_id(def: &crate::def::GameDef, name: &str) -> Res<TimelineId> {
    def.timelines
        .iter()
        .position(|t| t.name == name)
        .and_then(|i| u32::try_from(i).ok())
        .map(TimelineId)
        .ok_or_else(|| runtime_error(format!("unknown timeline `{name}`")))
}

/// A room of the package by id (its path stem, as room params name it).
fn room_id(def: &crate::def::GameDef, name: &str) -> Res<RoomId> {
    def.rooms
        .iter()
        .position(|r| r.name == name)
        .and_then(|i| u32::try_from(i).ok())
        .map(RoomId)
        .ok_or_else(|| runtime_error(format!("unknown room `{name}`")))
}

/// A sound or music of the package (or a bundled engine pack) by name.
fn sound_id(def: &crate::def::GameDef, name: &str) -> Res<crate::def::SoundId> {
    def.sounds
        .iter()
        .position(|d| d.name == name)
        .and_then(|i| u32::try_from(i).ok())
        .map(crate::def::SoundId)
        .ok_or_else(|| runtime_error(format!("unknown sound `{name}`")))
}

/// `play_sound`, `play_music`, `stop_music` and `shake` (#402). Sounds go through the same
/// `sound` module as the built-ins, so MULTIMEDIA rules and `sound_isplaying` agree.
fn register_audio(engine: &mut Engine, shared: &Rc<Shared>, docs: &mut Docs) {
    let s = Rc::clone(shared);
    docs.add(
        "play_sound",
        "Plays a sound of the package once; `engine:<pack>/<id>` names a bundled pack sound. An unknown name is an error.",
    );
    engine.register_fn("play_sound", move |name: ImmutableString| -> Res<()> {
        s.with_ctx(|ctx| {
            let id = sound_id(ctx.def, &name)?;
            crate::sound::play(ctx, id, false).map_err(sim_err)
        })?
    });
    let s = Rc::clone(shared);
    docs.add(
        "play_music",
        "Plays a music track of the package in a loop; another playing music track stops or blocks it by the run's MULTIMEDIA rule. An unknown name is an error.",
    );
    engine.register_fn("play_music", move |name: ImmutableString| -> Res<()> {
        s.with_ctx(|ctx| {
            let id = sound_id(ctx.def, &name)?;
            crate::sound::play(ctx, id, true).map_err(sim_err)
        })?
    });
    let s = Rc::clone(shared);
    docs.add("stop_music", "Stops the music track that is playing, if any.");
    engine.register_fn("stop_music", move || -> Res<()> {
        s.with_ctx(|ctx| {
            if let Some(id) = ctx.w.sound.copies.iter().find(|c| c.multimedia).map(|c| c.sound) {
                crate::sound::stop(ctx, id);
            }
        })
    });
    let s = Rc::clone(shared);
    docs.add(
        "shake",
        "Shakes the screen for `ticks` ticks with `strength` pixels (a client effect; the sim reads nothing back).",
    );
    engine.register_fn("shake", move |ticks: Dynamic, strength: Dynamic| -> Res<()> {
        let (t, st) = (num(&ticks, "shake")?, num(&strength, "shake")?);
        if !t.is_finite() || !st.is_finite() || t < 0.0 || st < 0.0 || t > 65_535.0 {
            return Err(runtime_error(
                "shake: ticks (0 to 65535) and strength must be finite and not negative",
            ));
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "checked above"
        )]
        let ticks = crate::num::round(Real::new(t), &s.num).get() as u32;
        s.with_world(|w, _| {
            w.output.push(OutputEvent::Script(ScriptOutput::Shake {
                ticks,
                strength: Real::new(st),
            }));
        })
    });
}

fn host_tick(host: &super::host::Host) -> crate::rules::TickRules {
    host.rules.tick.clone()
}

fn spawn(s: &Rc<Shared>, kind: &str, x: Real, y: Real, params: Params) -> Res<Inst> {
    s.budget.count_spawn().map_err(runtime_error)?;
    s.charge(cost("instance_create").0)?;
    let k = s.with_world(|_, host| spawn_kind(host, kind, &params))??;
    let init = SpawnInit {
        params,
        ..SpawnInit::default()
    };
    let id = s.with_ctx(|ctx| ctx.spawn(k, x, y, init))?;
    id.map(Inst).map_err(sim_err)
}
