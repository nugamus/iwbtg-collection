//! Built-in `trigger` (mechanics `trigger`; docs/plans/sim-core.md 6.1; docs/format/builtins/
//! trigger.md): five models, state in `MechGlobals` or in the trigger instances. Tier 2. WP0
//! creates the file; WP6b owns it.
//!
//! A touch by either kid (the collision event) does, per `model`:
//!
//! - `shared_last`: when this instance's `triggered` differs from `trg`, it writes `trg` into
//!   `triggered` of every instance of its kind (and of the `group` kind; the original writes
//!   through the object name, which reaches all instances). Traps read the first instance
//!   (K6, OPEN: U-MECH-2). An instance's `triggered` starts as 0 (uninitialized reads as 0).
//! - `per_object`: sets `triggered = 1` on every instance of its kind (and of `group`).
//! - `global_last` (kaenbyou): `global.triggered = trg`, then it destroys itself. Its Create
//!   resets `global.triggered` to 0, except with `need_key`: that variant fires only while
//!   `global.triggered` equals the key.
//! - `flag_array`: `global.trigger[trg] = true` (`array = "stop"`: `global.stop[trg]`); `reset`
//!   clears the flag instead (poronkusema objTriggerReset). `need_key`: fires only while
//!   `global.trigger[key]` is set.
//! - `switch_array` (spook house): `global.switchTrigger[trg] = true`.
//!
//! After the model: `sets_frame` shows image frame 1, `one_shot_destroy` destroys the trigger.
//! The sound (`sound`) is the client's.
//!
//! OPEN, not done: who clears the arrays. The original clears them per game (poronkusema world
//! Room Start for ids 0-10, showdown roomChanger and loadGame, spook house world Room End for
//! the switches) and `game_restart` clears all globals (Inferred, U-MECH-3). The clearing
//! functions are on `MechGlobals`; wiring them to the world and restart belongs to those
//! built-ins.

use serde::{Deserialize, Serialize};

use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_error, param_kind,
    param_real, state_mut, state_of,
};
use crate::def::{KindDef, KindId, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::{self, Real};
use crate::world::{Ctx, Flow, InstId};

const MODELS: &[&str] = &[
    "shared_last",
    "per_object",
    "global_last",
    "flag_array",
    "switch_array",
];
const ARRAYS: &[&str] = &["trigger", "stop"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "trg",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "model",
        ty: ParamType::Enum(MODELS),
        required: false,
    },
    ParamSpec {
        name: "need_key",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "group",
        ty: ParamType::Kind,
        required: false,
    },
    ParamSpec {
        name: "array",
        ty: ParamType::Enum(ARRAYS),
        required: false,
    },
    ParamSpec {
        name: "reset",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "sound",
        ty: ParamType::Sound,
        required: false,
    },
    ParamSpec {
        name: "one_shot_destroy",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "sets_frame",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "size",
        ty: ParamType::Real,
        required: false,
    },
];

/// How a trigger tells its traps (the trap's `trigger_model` uses the same words).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Model {
    /// The object-wide `triggered` variable holds the last id.
    SharedLast,
    /// `triggered = 1` on every instance of the touched trigger's kind.
    PerObject,
    /// `global.triggered`.
    GlobalLast,
    /// `global.trigger[n]` (or `global.stop[n]`).
    FlagArray,
    /// `global.switchTrigger[n]`.
    SwitchArray,
}

impl Model {
    /// The model a param word names.
    pub(crate) fn parse(word: Option<&str>) -> Model {
        match word {
            Some("per_object") => Model::PerObject,
            Some("global_last") => Model::GlobalLast,
            Some("flag_array") => Model::FlagArray,
            Some("switch_array") => Model::SwitchArray,
            _ => Model::SharedLast,
        }
    }
}

/// The words a `model` or `trigger_model` param takes.
pub(crate) const MODEL_WORDS: &[&str] = MODELS;

/// A trigger's settings and state.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TriggerState {
    /// The id it fires.
    pub trg: Real,
    /// The model.
    pub model: Model,
    /// Fires only while this key is set.
    pub need_key: Option<Real>,
    /// Another kind whose instances share `triggered`.
    pub group: Option<KindId>,
    /// `flag_array` writes `global.stop` instead of `global.trigger`.
    pub stop_array: bool,
    /// `flag_array` clears the flag instead of setting it.
    pub reset: bool,
    /// Show image frame 1 when touched.
    pub sets_frame: bool,
    /// Destroy when touched.
    pub one_shot_destroy: bool,
    /// The `triggered` variable (`shared_last`, `per_object`).
    pub triggered: Real,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: the kid touching it.
pub fn event_set(_kind: &KindDef) -> EventSet {
    let mut set = EventSet::default();
    set.collisions.push(super::warp::KIDS);
    set
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let model = Model::parse(param_enum(params, "model", MODELS)?);
    let need_key = param_real(params, "need_key")?;
    if need_key.is_some() && !matches!(model, Model::GlobalLast | Model::FlagArray) {
        return Err(param_error(
            "need_key",
            "only the global_last and flag_array models have key variants",
        ));
    }
    param_bool(params, "reset")?;
    Ok(BuiltinState::Trigger(TriggerState {
        trg: param_real(params, "trg")?.unwrap_or(Real::ZERO),
        model,
        need_key,
        group: param_kind(params, "group")?,
        stop_array: param_enum(params, "array", ARRAYS)? == Some("stop"),
        reset: param_bool(params, "reset")?.unwrap_or(false),
        sets_frame: param_bool(params, "sets_frame")?.unwrap_or(false),
        one_shot_destroy: param_bool(params, "one_shot_destroy")?.unwrap_or(false),
        triggered: Real::ZERO,
    }))
}

/// The array index an id names (`floor`).
fn key(ctx: &Ctx, id: Real) -> Result<i64, SimError> {
    num::to_int_round(num::floor(id, &ctx.rules.num)).map_err(|_| SimError::overflow("trigger id"))
}

/// Runs one event on a trigger.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::Trigger(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Create => {
            if s.model == Model::GlobalLast && s.need_key.is_none() {
                ctx.w.engine.mech.global_triggered = Real::ZERO;
            }
            Ok(Flow::Continue)
        }
        EventKind::Collision { with, .. } if with.intersects(super::warp::KIDS) => touch(ctx, me, &s),
        _ => Ok(Flow::Continue),
    }
}

fn touch(ctx: &mut Ctx, me: InstId, s: &TriggerState) -> Result<Flow, SimError> {
    match s.model {
        Model::SharedLast => {
            if !num::gm_eq(s.triggered, s.trg, &ctx.rules.num) {
                write_group(ctx, me, s, s.trg);
            }
        }
        Model::PerObject => write_group(ctx, me, s, Real::ONE),
        Model::GlobalLast => {
            if let Some(k) = s.need_key
                && !num::gm_eq(ctx.w.engine.mech.global_triggered, k, &ctx.rules.num)
            {
                return Ok(Flow::Continue);
            }
            ctx.w.engine.mech.global_triggered = s.trg;
            ctx.destroy(me);
            return Ok(Flow::Continue);
        }
        Model::FlagArray => {
            if let Some(k) = s.need_key
                && !ctx.w.engine.mech.trigger_flags.contains(&key(ctx, k)?)
            {
                return Ok(Flow::Continue);
            }
            let id = key(ctx, s.trg)?;
            let set = if s.stop_array {
                &mut ctx.w.engine.mech.stop_flags
            } else {
                &mut ctx.w.engine.mech.trigger_flags
            };
            if s.reset {
                set.remove(&id);
            } else {
                set.insert(id);
            }
        }
        Model::SwitchArray => {
            let id = key(ctx, s.trg)?;
            ctx.w.engine.mech.switches.insert(id);
        }
    }
    if s.sets_frame
        && let Some(inst) = ctx.w.get_mut(me)
    {
        inst.img.set_index(Real::ONE)?;
    }
    if s.one_shot_destroy {
        ctx.destroy(me);
    }
    Ok(Flow::Continue)
}

/// Writes `v` into `triggered` of every instance of this trigger's kind and of its `group`.
fn write_group(ctx: &mut Ctx, me: InstId, s: &TriggerState, v: Real) {
    let Some(own) = ctx.w.get(me).map(|i| i.kind) else {
        return;
    };
    let ids: Vec<InstId> = ctx
        .w
        .iter_alive()
        .filter(|i| i.kind == own || Some(i.kind) == s.group)
        .map(|i| i.id)
        .collect();
    for id in ids {
        if let Some(BuiltinState::Trigger(t)) = state_mut(ctx, id) {
            t.triggered = v;
        }
    }
}

/// The `triggered` value a trap reads through the object name: the first instance (lowest id)
/// of any of `kinds`, 0 when there is none.
pub fn read_shared(ctx: &Ctx, kinds: &[KindId]) -> Real {
    for inst in ctx.w.iter_alive() {
        if kinds.contains(&inst.kind)
            && let Some(BuiltinState::Trigger(t)) = state_of(ctx, inst.id)
        {
            return t.triggered;
        }
    }
    Real::ZERO
}

/// Whether a trap with id `trg` and `model` sees its trigger fired. `kinds` are the trigger
/// kinds the trap reads (`shared_last`, `per_object`).
pub fn fired(ctx: &Ctx, model: Model, trg: Real, kinds: &[KindId]) -> Result<bool, SimError> {
    let r = &ctx.rules.num;
    Ok(match model {
        Model::SharedLast => num::gm_eq(read_shared(ctx, kinds), trg, r),
        Model::PerObject => !num::gm_eq(read_shared(ctx, kinds), Real::ZERO, r),
        Model::GlobalLast => num::gm_eq(ctx.w.engine.mech.global_triggered, trg, r),
        Model::FlagArray => ctx.w.engine.mech.trigger_flags.contains(&key(ctx, trg)?),
        Model::SwitchArray => ctx.w.engine.mech.switches.contains(&key(ctx, trg)?),
    })
}

/// Whether `global.stop[trg]` is set (the move-to trap's gate).
pub fn stop_set(ctx: &Ctx, trg: Real) -> Result<bool, SimError> {
    Ok(ctx.w.engine.mech.stop_flags.contains(&key(ctx, trg)?))
}
