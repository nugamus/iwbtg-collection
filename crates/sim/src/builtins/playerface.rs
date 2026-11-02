//! Built-in `playerface` (numbers hidden RNG consumers; docs/plans/sim-core.md 6.1;
//! docs/format/builtins/playerface.md). Tier 2. WP0 creates the file; WP6b owns it.
//!
//! The face on the death screen. Create sets `image_index = irandom(3)`: one draw from the
//! gameplay RNG, made even when the face is never shown (11 of the 20 games, CLONE_SPEC numbers
//! "Hidden RNG consumers"). The draw is the whole behavior; showing the frame is the client's.

use super::{BuiltinState, ParamError, ParamSpec};
use crate::def::{KindDef, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId};

/// The params this built-in accepts: none.
pub fn param_specs() -> &'static [ParamSpec] {
    &[]
}

/// What a kind with this base handles: nothing (Create always runs).
pub fn event_set(_kind: &KindDef) -> EventSet {
    EventSet::default()
}

/// A new instance's state: none.
pub fn init_state(_params: &Params) -> Result<BuiltinState, ParamError> {
    Ok(BuiltinState::Stateless)
}

/// Create draws the frame: `image_index = irandom(3)`.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    if *ev == EventKind::Create {
        let frame = ctx.w.rng.irandom(Real::new(3.0))?;
        if let Some(inst) = ctx.w.get_mut(me) {
            inst.img.set_index(frame)?;
        }
    }
    Ok(Flow::Continue)
}
