//! The gameplay random generator (docs/architecture/sim/rng.md, N9, N10).
//!
//! The only randomness in the sim. Hidden consumers (blood, playerface) draw from it too
//! (Confirmed). Delphi LCG: `seed = seed * 134775813 + 1 mod 2^32` with wrapping arithmetic,
//! value `u = seed / 2^32` with the seed read as unsigned, one draw per call (N9; Confirmed
//! (recorder), CLONE_SPEC numbers: `traces/rely-the-needle-4-2/num-rng-a.csv` and `-b.csv`).
//! The start seed is UNKNOWN (the recorder fixes it); the sim takes it from the replay header.

use serde::{Deserialize, Serialize};

use crate::error::SimError;
use crate::num::Real;
use crate::rules::rng::StreamLayout;

/// The LCG multiplier, `0x08088405`.
const MULTIPLIER: u32 = 134_775_813;

/// `2^32`, the scale from a seed to `[0, 1)`.
const TWO_POW_32: f64 = 4_294_967_296.0;

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

    /// Advances once and returns `seed / 2^32` in `[0, 1)`, the seed read as unsigned (N9).
    pub fn next_unit(&mut self) -> Real {
        self.seed = self.seed.wrapping_mul(MULTIPLIER).wrapping_add(1);
        Real::new(f64::from(self.seed) / TWO_POW_32)
    }

    /// GML `random(x)`: `x * u`, one draw (Confirmed (recorder), num-rng-b). A negative `x` gives
    /// a value in `(x, 0]`. A non-finite `x` gives a non-finite result, which setters reject.
    pub fn random(&mut self, x: Real) -> Real {
        x * self.next_unit()
    }

    /// GML `random_range(a, b)`: `a + random(b - a)`, one draw (Inferred, N9; not traced).
    pub fn random_range(&mut self, a: Real, b: Real) -> Real {
        a + self.random(b - a)
    }

    /// GML `irandom(x)`: `floor((n + 1) * u)` with `n = floor(x)`, one draw. For an integer `x`
    /// this is the recorder's rule (`irandom(35) = floor(36 * u)`, Confirmed (recorder),
    /// num-rng-b). Flooring a fractional `x` first follows the manual's "inclusive when x is an
    /// integer" and its `irandom_range` rule; it is OPEN (N9), as is a negative `x` (here a value
    /// in `[n + 1, 0]`, computed by the same formula). A zero result is `+0.0`.
    ///
    /// Errors with `SimError::NonFinite` on a NaN or infinite `x`, before drawing.
    pub fn irandom(&mut self, x: Real) -> Result<Real, SimError> {
        if !x.is_finite() {
            return Err(SimError::non_finite("irandom argument"));
        }
        let n = x.get().floor();
        let u = self.next_unit().get();
        Ok(Real::new(((n + 1.0) * u).floor() + 0.0))
    }

    /// GML `irandom_range(a, b)`: `lo + floor((hi - lo + 1) * u)` with `lo = floor(a)` and
    /// `hi = floor(b)` (the manual rounds both bounds down and includes both), one draw.
    /// Inferred, not traced (N9); `a > b` uses the same formula and is OPEN. A zero result is
    /// `+0.0`.
    ///
    /// Errors with `SimError::NonFinite` on a NaN or infinite bound, before drawing.
    pub fn irandom_range(&mut self, a: Real, b: Real) -> Result<Real, SimError> {
        if !a.is_finite() || !b.is_finite() {
            return Err(SimError::non_finite("irandom_range argument"));
        }
        let lo = a.get().floor();
        let hi = b.get().floor();
        let u = self.next_unit().get();
        Ok(Real::new(lo + ((hi - lo + 1.0) * u).floor() + 0.0))
    }

    /// GML `choose` over `n` arguments: the index `floor(n * u)` of the chosen one, one draw
    /// (Confirmed (recorder) for 3 arguments, num-rng-b; other counts Inferred).
    ///
    /// GML always passes at least one argument; `n == 0` is `SimError::Invariant` and draws
    /// nothing.
    pub fn choose(&mut self, n: u32) -> Result<u32, SimError> {
        if n == 0 {
            return Err(SimError::invariant("choose with no arguments"));
        }
        let u = self.next_unit().get();
        let index = (f64::from(n) * u).floor();
        // `u < 1`, so the index is below `n`; the conversion and the bound are still checked.
        crate::num::to_int_trunc(Real::new(index))
            .ok()
            .and_then(|i| u32::try_from(i).ok())
            .filter(|&i| i < n)
            .ok_or_else(|| SimError::invariant("choose index out of range"))
    }
}

/// A named stream a consumer draws from (decision 0005).
///
/// Under [`StreamLayout::Shared`] (`gm8`) every stream is the one gameplay generator; under
/// [`StreamLayout::Split`] (`standard`) each has its own state. Consumers must be assigned a
/// stream explicitly, so a new draw cannot silently join the gameplay stream.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stream {
    /// The original's generator: gameplay decisions, and the particle update until #760.
    Gameplay,
    /// Visual effects whose values a player does not act on: blood, blood emitters, playerface.
    Effects,
    /// The Draw slot's own draws (`standard`); no sim consumer today.
    Draw,
    /// A script's `random()` family, in `standard`.
    Script,
}

/// Per-stream salts for the SplitMix64 finalizer. Fixed, so the replay header needs only the
/// seed: the layout comes from the ruleset and the other streams are derived from the seed.
const EFFECTS_SALT: u64 = 0xA076_1D64_78BD_642F;
const DRAW_SALT: u64 = 0xE703_7ED1_A0B4_28DB;
const SCRIPT_SALT: u64 = 0x8EBC_6AF0_9C88_C6E3;

/// A deterministic 32-bit stream seed from the replay seed and a salt (SplitMix64's finalizer).
fn derive(seed: u32, salt: u64) -> u32 {
    let mut z = u64::from(seed) ^ salt;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    // The finalizer's low 32 bits are the stream seed; the LCG state is 32-bit.
    let low = (z ^ (z >> 31)) & u64::from(u32::MAX);
    u32::try_from(low).expect("masked to 32 bits")
}

/// The named streams' state. Hashed (docs/architecture/sim/hash.md).
///
/// [`World::rng`](crate::world::World) is the `gameplay` stream in both layouts; this struct
/// holds the three streams a split layout adds. In [`StreamLayout::Shared`] they are seeded but
/// never read, so the `gm8` draw order and values are exactly the recorder's.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RngStreams {
    /// Whether consumers share the gameplay generator or draw from their named stream.
    pub layout: StreamLayout,
    /// The `effects` stream (`standard`): blood, blood emitters, playerface.
    pub effects: Rng,
    /// The `draw` stream (`standard`): the Draw slot's own draws.
    pub draw: Rng,
    /// The `script` stream (`standard`): `random()`, `irandom()`, `choose()` and friends.
    pub script: Rng,
}

impl RngStreams {
    /// The streams for a run seeded with `seed`. `gameplay` (the world's `rng`) is the raw seed;
    /// the other three are derived with fixed salts, so a replay stores only the seed.
    pub fn seeded(seed: u32, layout: StreamLayout) -> RngStreams {
        RngStreams {
            layout,
            effects: Rng::new(derive(seed, EFFECTS_SALT)),
            draw: Rng::new(derive(seed, DRAW_SALT)),
            script: Rng::new(derive(seed, SCRIPT_SALT)),
        }
    }
}

impl Default for RngStreams {
    /// The absent ruleset is `gm8`: shared, with the derived streams unused. A world restored
    /// from an older saved state gets these values; nothing reads them under `Shared`.
    fn default() -> RngStreams {
        RngStreams::seeded(0, StreamLayout::Shared)
    }
}

impl crate::world::World {
    /// The generator `stream` draws from in this world. Under [`StreamLayout::Shared`] every
    /// stream is the gameplay generator ([`World::rng`](crate::world::World::rng)), which keeps
    /// the recorder's one-stream order; under [`StreamLayout::Split`] each stream has its own.
    ///
    /// Particles still call this with [`Stream::Gameplay`] until #760 moves them to `effects`.
    pub fn stream_mut(&mut self, stream: Stream) -> &mut Rng {
        match (self.streams.layout, stream) {
            (StreamLayout::Shared, _) | (StreamLayout::Split, Stream::Gameplay) => &mut self.rng,
            (StreamLayout::Split, Stream::Effects) => &mut self.streams.effects,
            (StreamLayout::Split, Stream::Draw) => &mut self.streams.draw,
            (StreamLayout::Split, Stream::Script) => &mut self.streams.script,
        }
    }
}
