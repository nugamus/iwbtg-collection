//! Kid death per family: sound, blood emitter, destroy, deaths +1, death-only save, GAMEOVER
//! (docs/plans/sim-core.md 5.3 "Death"). Called through `kid::api::kill`. WP5a owns it.
//!
//! Only the family-independent core is here so far: a kill is ignored while `muteki` is set
//! (A1 Death step 2), otherwise the kid is destroyed. The `[death]` order (sound, blood emitter,
//! deaths +1, death-only save, GAMEOVER, the Select-role special case) is WP5a.4 (#122).

use super::api::KillCause;
use crate::error::SimError;
use crate::world::{Ctx, Flow, InstId, KindState};

/// Kills the kid `kid`. `Flow::Halt` when the destroy halted the sim; an error when `kid` is not
/// a kid. A kid that no longer exists is left alone.
pub fn kill(ctx: &mut Ctx, kid: InstId, cause: KillCause) -> Result<Flow, SimError> {
    let Some(inst) = ctx.w.get(kid) else {
        return Ok(Flow::Continue);
    };
    let KindState::Kid(s) = &inst.state else {
        return Err(SimError::invariant(format!(
            "kid::death::kill: instance {} is not a kid",
            kid.0
        )));
    };
    if s.invincible {
        return Ok(Flow::Continue);
    }
    Ok(ctx.destroy(kid))
}
