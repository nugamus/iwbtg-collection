//! Built-in `spike` (mechanics `spike`; collision spikes Confirmed; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/spike.md). Tier 1. WP0 creates the file; WP6a owns it.
//!
//! A spike is a killer: its class is `ClassMask::KILLER` (set by the loader) and the kid's
//! "Collision with killer" handler kills, so the spike has no kill code. The mask comes from the
//! sprite (precise) and `dir` picks the sprite, both data. What the sim does:
//!
//! - Create applies the `visible` and `depth` overrides.
//! - `auto_rotate` (family B, kid/B.md autoSpikeSprite): the sprite is the game's sprSpikeM, a
//!   sprite whose origin is its center, so Create sets `image_angle` from `dir` (up 0, down 180,
//!   left 90, right 270) and shifts the spike by the fixed per-direction offset.
//! - `destroys_bullets` (eat-the-new-year): a Collision with the kid's bullet destroys the
//!   bullet. It is read from the kind (the handler is declared per kind).

use serde::{Deserialize, Serialize};

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_real, state_of};
use crate::def::{ClassMask, KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId};

const DIRS: &[&str] = &["up", "down", "left", "right"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "dir",
        ty: ParamType::Enum(DIRS),
        required: false,
    },
    ParamSpec {
        name: "size",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "depth",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "visible",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "destroys_bullets",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "auto_rotate",
        ty: ParamType::Bool,
        required: false,
    },
];

/// A spike's per-instance settings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SpikeState {
    /// `visible` override.
    pub visible: Option<bool>,
    /// `depth` override.
    pub depth: Option<Real>,
    /// Set the angle and shift from `dir` in Create (family B).
    pub rotate: Option<Rotation>,
}

/// What `auto_rotate` does to one spike.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Rotation {
    /// `image_angle`.
    pub angle: Real,
    /// Shift added to x.
    pub dx: Real,
    /// Shift added to y.
    pub dy: Real,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: a collision with the bullet class when it has
/// `destroys_bullets`.
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet::default();
    if matches!(
        kind.params.0.get("destroys_bullets"),
        Some(ParamValue::Bool(true))
    ) {
        set.collisions.push(ClassMask::KID_BULLET);
    }
    set
}

/// (angle, dx, dy) of autoSpikeSprite for a direction and size (kid/B.md).
fn rotation(dir: &str, size: f64) -> Result<Rotation, ParamError> {
    let big = (size - 32.0).abs() < 1e-9;
    let mini = (size - 16.0).abs() < 1e-9;
    let (angle, dx, dy) = match (dir, big, mini) {
        ("up", true, _) => (0.0, 16.0, 16.0),
        ("down", true, _) => (180.0, 15.0, 15.0),
        ("left", true, _) => (90.0, 16.0, 15.0),
        ("right", true, _) => (270.0, 15.0, 16.0),
        ("up", _, true) => (0.0, 8.0, 8.0),
        ("down", _, true) => (180.0, 7.0, 7.0),
        ("left", _, true) => (90.0, 8.0, 7.0),
        ("right", _, true) => (270.0, 7.0, 8.0),
        _ => return Err(super::param_error("size", "auto_rotate needs size 32 or 16")),
    };
    Ok(Rotation {
        angle: Real::new(angle),
        dx: Real::new(dx),
        dy: Real::new(dy),
    })
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let dir = param_enum(params, "dir", DIRS)?;
    param_bool(params, "destroys_bullets")?;
    let size = param_real(params, "size")?.unwrap_or(Real::new(32.0));
    let rotate = if param_bool(params, "auto_rotate")?.unwrap_or(false) {
        let dir = dir.ok_or_else(|| super::param_error("dir", "auto_rotate needs `dir`"))?;
        Some(rotation(dir, size.get())?)
    } else {
        None
    };
    Ok(BuiltinState::Spike(SpikeState {
        visible: param_bool(params, "visible")?,
        depth: param_real(params, "depth")?,
        rotate,
    }))
}

/// Runs one event on a spike: Create applies the overrides and the rotation; a Collision with a
/// bullet destroys the bullet.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    match ev {
        EventKind::Create => {
            let Some(BuiltinState::Spike(s)) = state_of(ctx, me) else {
                return Ok(Flow::Continue);
            };
            let s = s.clone();
            let Some(inst) = ctx.w.get_mut(me) else {
                return Ok(Flow::Continue);
            };
            if let Some(v) = s.visible {
                inst.visible = v;
            }
            if let Some(v) = s.depth {
                inst.depth = v;
            }
            if let Some(r) = s.rotate {
                inst.img.set_angle(r.angle)?;
                let (x, y) = (inst.m.x() + r.dx, inst.m.y() + r.dy);
                inst.m.set_x(x)?;
                inst.m.set_y(y)?;
            }
            Ok(Flow::Continue)
        }
        EventKind::Collision { with, other } if with.intersects(ClassMask::KID_BULLET) => {
            Ok(ctx.destroy(*other))
        }
        _ => Ok(Flow::Continue),
    }
}
