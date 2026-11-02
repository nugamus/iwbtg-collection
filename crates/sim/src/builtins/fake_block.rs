//! Built-in `fake_block` (mechanics `fake_block`; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/fake_block.md). Tier 2. WP0 creates the file; WP6b owns it.
//!
//! Looks like a block but is not solid, or vanishes. Create applies `solid` (default the
//! kind data) and `visible`. `vanish_on`:
//!
//! - `touch`: a kid touching it destroys it (KOT blockVis; the sound is the client's).
//! - `approach` (default): each Step, a kid within `approach_distance` (2) destroys it (WAN
//!   blockFake, LR blockNotInvis, SHO blockKieru01).
//! - `group_fade` (WAN oHiddenWall): a kid touch makes every instance of the kind fade: each
//!   Step `image_alpha -= 0.05`, destroyed at 0 or below.
//! - `never`: stays.
//!
//! `blood_solid_when_no_kid` (KOT niseblock, blockVis): while no kid exists it is solid, so
//! blood sticks. `stops_bullets` destroys a kid bullet that touches it. The class that makes
//! moving platforms reverse (`moving_platform` `marker_reverse`) is the loader's, set from
//! `reverses_platforms`.
//!
//! OPEN, not done: `tile_delete` (spook house oPatFakeBlock: deletes the tile under it and
//! makes 10 particles at random offsets up to 12 px; the draw order is not in the spec) is
//! refused at load; the WAN blockFake test that runs only while player2 does not exist.

use serde::{Deserialize, Serialize};

use super::warp::KIDS;
use super::{
    BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_enum, param_error, param_real,
    state_mut, state_of,
};
use crate::collide::Target;
use crate::collide::query::distance_to_object;
use crate::def::{ClassMask, KindDef, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::{self, Real};
use crate::world::{Ctx, Flow, InstId};

const VANISH: &[&str] = &["touch", "approach", "group_fade", "tile_delete", "never"];

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "solid",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "visible",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "vanish_on",
        ty: ParamType::Enum(VANISH),
        required: false,
    },
    ParamSpec {
        name: "approach_distance",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "blood_solid_when_no_kid",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "stops_bullets",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "reverses_platforms",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "sound",
        ty: ParamType::Sound,
        required: false,
    },
];

/// When it vanishes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Vanish {
    /// A kid touches it.
    Touch,
    /// A kid comes near.
    Approach,
    /// A touch makes the whole kind fade.
    GroupFade,
    /// Never.
    Never,
}

/// A fake block's settings and state.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct FakeBlockState {
    /// When it vanishes.
    pub vanish: Vanish,
    /// The `approach` distance.
    pub approach: Real,
    /// Solid while no kid exists.
    pub blood_solid: bool,
    /// `solid` override.
    pub solid: Option<bool>,
    /// `visible` override.
    pub visible: Option<bool>,
    /// Fading (`group_fade`).
    pub fading: bool,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

fn flag(kind: &KindDef, name: &str) -> bool {
    matches!(kind.params.0.get(name), Some(ParamValue::Bool(true)))
}

/// What a kind with this base handles.
pub fn event_set(kind: &KindDef) -> EventSet {
    let mut set = EventSet {
        step: true,
        ..EventSet::default()
    };
    if matches!(kind.params.0.get("vanish_on"), Some(ParamValue::Str(w)) if w == "touch" || w == "group_fade")
    {
        set.collisions.push(KIDS);
    }
    if flag(kind, "stops_bullets") {
        set.collisions.push(ClassMask::KID_BULLET);
    }
    set
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let vanish = match param_enum(params, "vanish_on", VANISH)? {
        Some("touch") => Vanish::Touch,
        Some("group_fade") => Vanish::GroupFade,
        Some("never") => Vanish::Never,
        Some("tile_delete") => {
            return Err(param_error(
                "vanish_on",
                "tile_delete (spook house oPatFakeBlock) is not supported yet (OPEN: particle draw order)",
            ));
        }
        _ => Vanish::Approach,
    };
    param_bool(params, "stops_bullets")?;
    param_bool(params, "reverses_platforms")?;
    Ok(BuiltinState::FakeBlock(FakeBlockState {
        vanish,
        approach: param_real(params, "approach_distance")?.unwrap_or(Real::new(2.0)),
        blood_solid: param_bool(params, "blood_solid_when_no_kid")?.unwrap_or(false),
        solid: param_bool(params, "solid")?,
        visible: param_bool(params, "visible")?,
        fading: false,
    }))
}

/// Runs one event on a fake block.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::FakeBlock(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Create => {
            if let Some(inst) = ctx.w.get_mut(me) {
                if let Some(v) = s.solid {
                    inst.solid = v;
                }
                if let Some(v) = s.visible {
                    inst.visible = v;
                }
            }
        }
        EventKind::Step => step(ctx, me, &s)?,
        EventKind::Collision { with, other } => {
            if with.intersects(ClassMask::KID_BULLET) {
                ctx.destroy(*other);
            } else if with.intersects(KIDS) {
                match s.vanish {
                    Vanish::Touch => {
                        ctx.destroy(me);
                    }
                    Vanish::GroupFade => fade_all(ctx, me),
                    Vanish::Approach | Vanish::Never => {}
                }
            }
        }
        _ => {}
    }
    Ok(Flow::Continue)
}

fn fade_all(ctx: &mut Ctx, me: InstId) {
    let Some(kind) = ctx.w.get(me).map(|i| i.kind) else {
        return;
    };
    let ids: Vec<InstId> = ctx
        .w
        .iter_alive()
        .filter(|i| i.kind == kind)
        .map(|i| i.id)
        .collect();
    for id in ids {
        if let Some(BuiltinState::FakeBlock(t)) = state_mut(ctx, id) {
            t.fading = true;
        }
    }
}

fn step(ctx: &mut Ctx, me: InstId, s: &FakeBlockState) -> Result<(), SimError> {
    if s.blood_solid
        && !ctx.w.iter_alive().any(|i| i.classes.intersects(KIDS))
        && let Some(inst) = ctx.w.get_mut(me)
    {
        inst.solid = true;
    }
    match s.vanish {
        Vanish::Approach => {
            if let Some(d) = distance_to_object(ctx, me, Target::Class(KIDS))?
                && num::gm_lt(d, s.approach, &ctx.rules.num)
            {
                ctx.destroy(me);
            }
        }
        Vanish::GroupFade if s.fading => {
            let rules = ctx.rules;
            let Some(inst) = ctx.w.get_mut(me) else {
                return Ok(());
            };
            let a = inst.img.alpha() - Real::new(0.05);
            inst.img.set_alpha(a)?;
            if num::gm_le(a, Real::ZERO, &rules.num) {
                ctx.destroy(me);
            }
        }
        _ => {}
    }
    Ok(())
}
