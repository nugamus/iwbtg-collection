//! Built-in `game_over` (GAMEOVER; kid/A1.md Death, kid/A2.md, kid/B.md; docs/plans/sim-core.md
//! 6.1; docs/format/builtins/game_over.md). Tier 1. WP0 creates the file; WP6a owns it.
//!
//! Create places it relative to view 0 (`offset_x`, `offset_y`: rely 400, 304; POR 480, 270;
//! A2 and B 400, 300) and, with a `delay` (A2 and B: 30), hides it until Alarm 0. The kid's death
//! creates it. The fades run in the Draw slot's `Late` handler (plan: "Late handler for fades"),
//! which only visible instances get: the image alpha rises 0.01 a tick up to 1, the black overlay
//! alpha 0.015 up to 0.6, and the bob phase by pi/100 (rely: the text bobs by 32 times the sine
//! of the phase). The overlay and the bob are draw-only values the client reads with
//! [`overlay_alpha`] and [`bob_offset`]. OPEN: whether the original updates them in Step or Draw
//! (the spec names Create, Step and Draw without saying which); no value read in the sim depends
//! on the choice.

use serde::{Deserialize, Serialize};

use super::{BuiltinState, ParamError, ParamSpec, ParamType, param_bool, param_real, state_mut, state_of};
use crate::alarm;
use crate::def::{KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::{self, Real};
use crate::world::{Ctx, Flow, InstId, KindState, World};

const SPECS: &[ParamSpec] = &[
    ParamSpec {
        name: "offset_x",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "offset_y",
        ty: ParamType::Real,
        required: false,
    },
    ParamSpec {
        name: "view_relative",
        ty: ParamType::Bool,
        required: false,
    },
    ParamSpec {
        name: "delay",
        ty: ParamType::Real,
        required: false,
    },
];

/// A GAMEOVER's settings and fade state.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct GameOverState {
    /// Offset from view 0.
    pub offset: (Real, Real),
    /// Position is view 0 plus the offset (else the spawn position stays).
    pub view_relative: bool,
    /// Ticks hidden after Create (0 shows it at once).
    pub delay: Real,
    /// The black overlay's alpha.
    pub overlay: Real,
    /// The bob phase, radians.
    pub phase: Real,
}

/// The params this built-in accepts (`builtins::param_specs` routes here).
pub fn param_specs() -> &'static [ParamSpec] {
    SPECS
}

/// What a kind with this base handles: alarm 0 and the Draw-slot handler.
pub fn event_set(_kind: &KindDef) -> EventSet {
    EventSet {
        late: true,
        alarms: 1,
        ..EventSet::default()
    }
}

/// A new instance's state from its params.
pub fn init_state(params: &Params) -> Result<BuiltinState, ParamError> {
    let r = |name: &str, d: f64| Ok::<Real, ParamError>(param_real(params, name)?.unwrap_or(Real::new(d)));
    let delay = r("delay", 0.0)?;
    // gm-compare: ok (load-time param validation)
    if delay.get() < 0.0 {
        return Err(super::param_error("delay", "must be 0 or more"));
    }
    Ok(BuiltinState::GameOver(GameOverState {
        offset: (r("offset_x", 400.0)?, r("offset_y", 304.0)?),
        view_relative: param_bool(params, "view_relative")?.unwrap_or(true),
        delay,
        overlay: Real::ZERO,
        phase: Real::ZERO,
    }))
}

/// Runs one event on a GAMEOVER.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let Some(BuiltinState::GameOver(s)) = state_of(ctx, me) else {
        return Ok(Flow::Continue);
    };
    let s = s.clone();
    match ev {
        EventKind::Create => {
            let (vx, vy) = (ctx.w.views[0].x, ctx.w.views[0].y);
            let rules = &ctx.rules.num;
            let Some(inst) = ctx.w.get_mut(me) else {
                return Ok(Flow::Continue);
            };
            if s.view_relative {
                inst.m.set_x(vx + s.offset.0)?;
                inst.m.set_y(vy + s.offset.1)?;
            }
            inst.img.set_alpha(Real::ZERO)?;
            // gm-compare: ok (a delay param of 0 means no delay, not a GML test)
            if s.delay.get() > 0.0 {
                inst.visible = false;
                alarm::set(inst, 0, s.delay, rules)?;
            }
            Ok(Flow::Continue)
        }
        EventKind::Alarm(0) => {
            if let Some(inst) = ctx.w.get_mut(me) {
                inst.visible = true;
            }
            Ok(Flow::Continue)
        }
        EventKind::Late => {
            if let Some(inst) = ctx.w.get_mut(me) {
                let a = num::gm_min(inst.img.alpha() + Real::new(0.01), Real::ONE, &ctx.rules.num);
                inst.img.set_alpha(a)?;
            }
            let rules = &ctx.rules.num;
            if let Some(BuiltinState::GameOver(st)) = state_mut(ctx, me) {
                st.overlay = num::gm_min(st.overlay + Real::new(0.015), Real::new(0.6), rules);
                st.phase += Real::new(std::f64::consts::PI / 100.0);
            }
            Ok(Flow::Continue)
        }
        _ => Ok(Flow::Continue),
    }
}

fn state(w: &World, id: InstId) -> Option<&GameOverState> {
    match &w.get(id)?.state {
        KindState::Builtin(BuiltinState::GameOver(s)) => Some(s),
        _ => None,
    }
}

/// The black overlay's alpha (0 for anything that is not a GAMEOVER).
pub fn overlay_alpha(w: &World, id: InstId) -> Real {
    state(w, id).map_or(Real::ZERO, |s| s.overlay)
}

/// The vertical bob of the text, `32 * sin(phase)` (0 for anything that is not a GAMEOVER).
pub fn bob_offset(w: &World, id: InstId) -> Real {
    state(w, id).map_or(Real::ZERO, |s| Real::new(32.0) * num::sin(s.phase))
}
