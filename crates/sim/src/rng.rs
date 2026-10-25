//! The gameplay random generator (docs/plans/sim-core.md 3.3, N9, N10).
//!
//! The only randomness in the sim. Hidden consumers (blood, playerface) draw from it too
//! (Confirmed). Delphi-style LCG: `seed = seed * 134775813 + 1 mod 2^32` with wrapping
//! arithmetic, value `seed / 2^32` (Inferred, N9). One draw per call. WP0 declares; WP2 owns.
#![allow(unused_variables, reason = "WP0 stubs: WP2 fills the bodies")]

use serde::{Deserialize, Serialize};

use crate::error::SimError;
use crate::num::Real;

/// The generator state.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Rng {
    /// The LCG state.
    pub seed: u32,
}

impl Rng {
    /// A generator in state `seed` (the replay header's seed).
    pub const fn new(seed: u32) -> Rng {
        Rng { seed }
    }

    /// Advances once and returns `seed / 2^32` in `[0, 1)`.
    pub fn next_unit(&mut self) -> Real {
        unimplemented!("WP2: Rng::next_unit")
    }

    /// GML `random(x)`.
    pub fn random(&mut self, x: Real) -> Real {
        unimplemented!("WP2: Rng::random")
    }

    /// GML `random_range(a, b)`.
    pub fn random_range(&mut self, a: Real, b: Real) -> Real {
        unimplemented!("WP2: Rng::random_range")
    }

    /// GML `irandom(x)`.
    pub fn irandom(&mut self, x: Real) -> Result<Real, SimError> {
        unimplemented!("WP2: Rng::irandom")
    }

    /// GML `irandom_range(a, b)`.
    pub fn irandom_range(&mut self, a: Real, b: Real) -> Result<Real, SimError> {
        unimplemented!("WP2: Rng::irandom_range")
    }

    /// GML `choose` over `n` arguments: the index of the chosen one.
    pub fn choose(&mut self, n: u32) -> Result<u32, SimError> {
        unimplemented!("WP2: Rng::choose")
    }
}
