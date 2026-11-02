//! CleanMem DLL calls: no-ops (docs/plans/sim-core.md 7). CleanMem only trims the process
//! working set; it has no effect on game state or audio (CLONE_SPEC sound, CleanMem).

use super::DllMode;
use crate::num::Real;
use crate::world::Ctx;

/// `halo_shg_clean`: 1 (success), or 0 in [`DllMode::Stub`]. Does nothing.
pub fn clean(ctx: &Ctx) -> Real {
    if ctx.dll_mode == DllMode::Stub {
        Real::ZERO
    } else {
        Real::ONE
    }
}

/// `halo_shg_get_mem`: a fixed positive byte count (1 MiB), or 0 in [`DllMode::Stub`].
pub fn get_mem(ctx: &Ctx) -> Real {
    if ctx.dll_mode == DllMode::Stub {
        Real::ZERO
    } else {
        Real::new(1_048_576.0)
    }
}
