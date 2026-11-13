//! Particle systems' draws from the gameplay RNG (N10; docs/architecture/sim/rng.md).
//!
//! The rules are CLONE_SPEC numbers, Confirmed (recorder), "Particle draw rules" (research#158,
//! `traces/rely-the-needle-4-2/num-part-rng.md`). This module models only what the particle
//! system draws: how many values, in which order, and the unit point a region draw gives. It
//! keeps no particle positions, speeds or colors; drawing particles is the client's.
//!
//! - **Slot.** [`Particles::update`] runs once per tick, after End Step and before Draw.
//! - **Order.** Systems in creation order; inside a system, first the live particles (their step
//!   and death particles), then the emitters in emitter creation order.
//! - **Streams.** `n < 0`: one chance draw `u`, one particle when `u < 1 / (|n| + 1)`. `n > 0`:
//!   `n` particles, no chance draw.
//! - **Emitter particle** (stream or burst): the region's draws, then the type's draws.
//!   `part_particles_create` ([`Particles::create`]): the type's draws only.
//! - **Type draws:** see [`PartType::draws`].
//! - **Regions:** see [`Region::draw_point`].
//! - **Live particles** draw nothing themselves (wiggle included). A type with
//!   `part_type_step(T, k, T2)` costs `k` times T2's type draws in every update the particle is
//!   still alive after; `part_type_death(T, k, T2)` costs them in the update it dies.
//!
//! OPEN (each refused with `SimError::Unsupported`, never guessed): stream or burst number 0 and
//! negative bursts (not traced); which of a type's draws gives the life (only needed for a type
//! with step or death particles and a non-empty life range); step or death particles that
//! themselves have step or death particles; `part_system_automatic_update` off and
//! `part_system_update` (research#163); the real runner (research#164).

use serde::{Deserialize, Serialize};

use crate::error::SimError;
use crate::num::{self, Real};
use crate::rng::Rng;

/// A `min..max` setting of a particle type (size, speed, direction, orientation, life, or one
/// color channel). It draws once when `max > min`; an empty or reversed range draws nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct PartRange {
    /// Lower bound.
    pub min: Real,
    /// Upper bound.
    pub max: Real,
}

impl PartRange {
    /// A range from `min` to `max`.
    pub const fn new(min: f64, max: f64) -> PartRange {
        PartRange {
            min: Real::new(min),
            max: Real::new(max),
        }
    }

    /// The same value at both ends (draws nothing).
    pub const fn fixed(v: f64) -> PartRange {
        PartRange::new(v, v)
    }

    /// True when this range costs a draw: `max > min` (runs 5a-5p; reversed ranges draw nothing).
    pub fn draws(self) -> bool {
        self.max.get() > self.min.get()
    }
}

/// A particle type's color setting, as far as it draws.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum PartColor {
    /// `part_type_color1/2/3` (or no color call): no draws.
    #[default]
    Fixed,
    /// `part_type_color_mix`: 1 draw.
    Mix,
    /// `part_type_color_rgb`: 1 draw per channel whose max is above its min.
    Rgb([PartRange; 3]),
    /// `part_type_color_hsv`: 1 draw per channel whose max is above its min.
    Hsv([PartRange; 3]),
}

/// Step or death particles of a type: `count` particles of type `ty`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PartChild {
    /// Particles per update (step) or at death.
    pub count: u32,
    /// Their type.
    pub ty: PartTypeId,
}

/// A particle type, with the settings that decide its draws. Alpha, blend, gravity, wiggle and
/// fixed colors draw nothing and are not kept.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct PartType {
    /// `part_type_size` min and max.
    pub size: PartRange,
    /// `part_type_speed` min and max.
    pub speed: PartRange,
    /// `part_type_direction` min and max.
    pub direction: PartRange,
    /// `part_type_orientation` min and max.
    pub orientation: PartRange,
    /// `part_type_life` min and max, in ticks. The GM default (100) is the caller's.
    pub life: PartRange,
    /// The color setting.
    pub color: PartColor,
    /// `part_type_sprite` with random subimage.
    pub random_subimage: bool,
    /// `part_type_step(T, k, T2)`.
    pub step: Option<PartChild>,
    /// `part_type_death(T, k, T2)`.
    pub death: Option<PartChild>,
}

impl PartType {
    /// The draws one particle of this type costs: 1, plus 1 for each of size, speed, direction,
    /// orientation and life whose max is above its min, plus 1 per such rgb or hsv channel, plus 1
    /// for color mix, plus 1 for a sprite with random subimage (spec rule 5).
    pub fn draws(&self) -> u32 {
        let ranges = [self.size, self.speed, self.direction, self.orientation, self.life];
        let mut n = 1 + count(&ranges);
        n += match self.color {
            PartColor::Fixed => 0,
            PartColor::Mix => 1,
            PartColor::Rgb(c) | PartColor::Hsv(c) => count(&c),
        };
        n + u32::from(self.random_subimage)
    }
}

fn count(ranges: &[PartRange]) -> u32 {
    let n = ranges.iter().filter(|r| r.draws()).count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// `ps_shape_*`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum RegionShape {
    /// `ps_shape_rectangle`: never redraws.
    #[default]
    Rectangle,
    /// `ps_shape_ellipse`: redraws while `(2x - 1)^2 + (2y - 1)^2 > 1`.
    Ellipse,
    /// `ps_shape_diamond`: redraws while `x + y > 1.5`.
    Diamond,
    /// `ps_shape_line`: never redraws; gaussian and invgaussian both draw `g(x)`, `g(y)`.
    Line,
}

/// `ps_distr_*`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum RegionDistr {
    /// `ps_distr_linear`: x, then y.
    #[default]
    Linear,
    /// `ps_distr_gaussian`: `g(x)`, then `g(y)`.
    Gaussian,
    /// `ps_distr_invgaussian`: `g(x)`, `g(y)`, then `t1`, `t2`.
    InvGaussian,
}

/// An emitter's region. The draws use unit coordinates (0..1 across the region), so the
/// region's size never changes the draw count (runs 6m-6r) and is not kept here.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Region {
    /// The shape.
    pub shape: RegionShape,
    /// The distribution.
    pub distr: RegionDistr,
}

impl Region {
    /// Draws one point in unit coordinates (spec rule 6). The position probes of runs 6s-6u put
    /// every decoded particle within 1/256 of this point.
    pub fn draw_point(self, rng: &mut Rng) -> (Real, Real) {
        loop {
            let (x, y) = match (self.distr, self.shape) {
                (RegionDistr::Linear, _) => {
                    let x = rng.next_unit();
                    (x, rng.next_unit())
                }
                (RegionDistr::Gaussian, _) | (RegionDistr::InvGaussian, RegionShape::Line) => {
                    let x = gauss(rng);
                    (x, gauss(rng))
                }
                (RegionDistr::InvGaussian, _) => {
                    let gx = gauss(rng);
                    let gy = gauss(rng);
                    let t1 = rng.next_unit();
                    let t2 = rng.next_unit();
                    if t1.get() < 0.5 {
                        (t2, far_side(gy))
                    } else {
                        (far_side(gx), t2)
                    }
                }
            };
            if self.inside(x, y) {
                return (x, y);
            }
        }
    }

    fn inside(self, x: Real, y: Real) -> bool {
        let (x, y) = (x.get(), y.get());
        match self.shape {
            RegionShape::Rectangle | RegionShape::Line => true,
            RegionShape::Ellipse => {
                let (dx, dy) = (2.0 * x - 1.0, 2.0 * y - 1.0);
                dx * dx + dy * dy <= 1.0
            }
            RegionShape::Diamond => x + y <= 1.5,
        }
    }
}

/// `g`: draws pairs `(a, b)` until `b < exp(-18 (a - 0.5)^2)` and returns `a` (spec rule 6). The
/// spec gives the formula, not the runner's operation order; a different rounding could only
/// change a pair whose `b` lies within an ulp of the bound.
fn gauss(rng: &mut Rng) -> Real {
    loop {
        let a = rng.next_unit();
        let b = rng.next_unit();
        let d = a.get() - 0.5;
        if b.get() < num::exp(Real::new(-18.0 * (d * d))).get() {
            return a;
        }
    }
}

/// `s(v)`: `v + 0.5` for `v < 0.5`, else `v - 0.5`.
fn far_side(v: Real) -> Real {
    if v.get() < 0.5 {
        Real::new(v.get() + 0.5)
    } else {
        Real::new(v.get() - 0.5)
    }
}

/// A particle type in [`Particles::types`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub struct PartTypeId(pub u32);

/// A system in [`Particles::systems`], in creation order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub struct PartSystemId(pub u32);

/// An emitter of one system, in its creation order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub struct EmitterId {
    /// Its system.
    pub system: PartSystemId,
    /// Its index in the system's emitters.
    pub index: u32,
}

/// `part_emitter_stream` of an emitter.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PartStream {
    /// The type it makes.
    pub ty: PartTypeId,
    /// The stream number: `n > 0` particles per tick, `n < 0` one with chance `1 / (|n| + 1)`.
    pub number: i32,
}

/// One emitter.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Emitter {
    /// Its region.
    pub region: Region,
    /// Its stream, if any.
    pub stream: Option<PartStream>,
    /// `part_emitter_destroy` was called: it no longer runs.
    pub destroyed: bool,
}

/// A live particle the model has to age: one whose type has step or death particles. Others
/// draw nothing after their spawn and are not kept.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
struct Live {
    ty: PartTypeId,
    life: Real,
}

/// One particle system.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct PartSystem {
    /// Emitters in creation order.
    pub emitters: Vec<Emitter>,
    live: Vec<Live>,
    /// `part_system_destroy` was called: it no longer runs.
    pub destroyed: bool,
}

/// All particle types and systems of a run.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Particles {
    /// Types by id.
    pub types: Vec<PartType>,
    /// Systems in creation order.
    pub systems: Vec<PartSystem>,
}

impl Particles {
    /// `part_type_create`: adds a type.
    pub fn add_type(&mut self, ty: PartType) -> PartTypeId {
        self.types.push(ty);
        PartTypeId(index_u32(self.types.len() - 1))
    }

    /// `part_system_create`: adds a system after every existing one.
    pub fn add_system(&mut self) -> PartSystemId {
        self.systems.push(PartSystem::default());
        PartSystemId(index_u32(self.systems.len() - 1))
    }

    /// `part_system_destroy`.
    pub fn destroy_system(&mut self, s: PartSystemId) -> Result<(), SimError> {
        let sys = self.system_mut(s)?;
        sys.destroyed = true;
        sys.live.clear();
        Ok(())
    }

    /// `part_emitter_create` plus `part_emitter_region`: adds an emitter after the system's others.
    pub fn add_emitter(&mut self, s: PartSystemId, region: Region) -> Result<EmitterId, SimError> {
        let sys = self.system_mut(s)?;
        sys.emitters.push(Emitter {
            region,
            ..Emitter::default()
        });
        Ok(EmitterId {
            system: s,
            index: index_u32(sys.emitters.len() - 1),
        })
    }

    /// `part_emitter_destroy`.
    pub fn destroy_emitter(&mut self, e: EmitterId) -> Result<(), SimError> {
        self.emitter_mut(e)?.destroyed = true;
        Ok(())
    }

    /// `part_emitter_stream`. Number 0 is refused (not traced; OPEN).
    pub fn stream(&mut self, e: EmitterId, ty: PartTypeId, number: i32) -> Result<(), SimError> {
        self.ty(ty)?;
        if number == 0 {
            return Err(SimError::unsupported(
                "part_emitter_stream of 0 (not traced; N10, research#158)",
            ));
        }
        self.emitter_mut(e)?.stream = Some(PartStream { ty, number });
        Ok(())
    }

    /// `part_emitter_burst` (game code, at once): `number` times the region's draws, then the
    /// type's. A number below 1 is refused (not traced; OPEN).
    pub fn burst(
        &mut self,
        rng: &mut Rng,
        e: EmitterId,
        ty: PartTypeId,
        number: i32,
    ) -> Result<(), SimError> {
        let region = self.emitter_mut(e)?.region;
        let n = u32::try_from(number).ok().filter(|n| *n > 0).ok_or_else(|| {
            SimError::unsupported(format!(
                "part_emitter_burst of {number} (not traced; N10, research#158)"
            ))
        })?;
        for _ in 0..n {
            self.spawn(rng, e.system, Some(region), ty)?;
        }
        Ok(())
    }

    /// `part_particles_create` (game code, at once): `number` times the type's draws, no region.
    /// A number below 1 makes nothing.
    pub fn create(
        &mut self,
        rng: &mut Rng,
        s: PartSystemId,
        ty: PartTypeId,
        number: i32,
    ) -> Result<(), SimError> {
        self.system_mut(s)?;
        for _ in 0..number.max(0) {
            self.spawn(rng, s, None, ty)?;
        }
        Ok(())
    }

    /// The particle update of one tick (after End Step, before Draw; spec rules 1-3 and 7).
    pub fn update(&mut self, rng: &mut Rng) -> Result<(), SimError> {
        for s in 0..self.systems.len() {
            if self.systems[s].destroyed {
                continue;
            }
            self.age(rng, s)?;
            for e in 0..self.systems[s].emitters.len() {
                let em = &self.systems[s].emitters[e];
                let (Some(stream), false) = (em.stream, em.destroyed) else {
                    continue;
                };
                let region = em.region;
                let n = if stream.number > 0 {
                    stream.number.unsigned_abs()
                } else {
                    let chance = 1.0 / (f64::from(stream.number.unsigned_abs()) + 1.0);
                    u32::from(rng.next_unit().get() < chance)
                };
                for _ in 0..n {
                    self.spawn(rng, PartSystemId(index_u32(s)), Some(region), stream.ty)?;
                }
            }
        }
        Ok(())
    }

    /// Ages the live particles of system `s` by one: each loses 1 life; one at or below 0 dies
    /// and makes its death particles, the others make their step particles.
    fn age(&mut self, rng: &mut Rng, s: usize) -> Result<(), SimError> {
        let live = std::mem::take(&mut self.systems[s].live);
        let mut kept = Vec::with_capacity(live.len());
        for mut p in live {
            p.life = Real::new(p.life.get() - 1.0);
            let t = &self.types[usize::try_from(p.ty.0).unwrap_or(usize::MAX)];
            let dies = p.life.get() <= 0.0;
            let child = if dies { t.death } else { t.step };
            if let Some(c) = child {
                let draws = self.ty(c.ty)?.draws();
                for _ in 0..c.count * draws {
                    rng.next_unit();
                }
            }
            if !dies {
                kept.push(p);
            }
        }
        self.systems[s].live = kept;
        Ok(())
    }

    /// One new particle: the region's draws (emitters only), then the type's.
    fn spawn(
        &mut self,
        rng: &mut Rng,
        s: PartSystemId,
        region: Option<Region>,
        ty: PartTypeId,
    ) -> Result<(), SimError> {
        let t = self.ty(ty)?.clone();
        let tracked = t.step.is_some() || t.death.is_some();
        if tracked {
            for c in [t.step, t.death].into_iter().flatten() {
                let ct = self.ty(c.ty)?;
                if ct.step.is_some() || ct.death.is_some() {
                    return Err(SimError::unsupported(
                        "step or death particles with their own step or death particles (not traced; N10)",
                    ));
                }
            }
            if t.life.max.get() < t.life.min.get() || t.life.draws() {
                return Err(SimError::unsupported(
                    "a particle type with step or death particles and a random or reversed life: \
                     which type draw gives the life is not in the spec (N10)",
                ));
            }
        }
        if let Some(r) = region {
            r.draw_point(rng);
        }
        for _ in 0..t.draws() {
            rng.next_unit();
        }
        if tracked {
            self.system_mut(s)?.live.push(Live { ty, life: t.life.min });
        }
        Ok(())
    }

    fn ty(&self, ty: PartTypeId) -> Result<&PartType, SimError> {
        usize::try_from(ty.0)
            .ok()
            .and_then(|i| self.types.get(i))
            .ok_or_else(|| SimError::unsupported(format!("particle type {} does not exist", ty.0)))
    }

    fn system_mut(&mut self, s: PartSystemId) -> Result<&mut PartSystem, SimError> {
        usize::try_from(s.0)
            .ok()
            .and_then(|i| self.systems.get_mut(i))
            .filter(|sys| !sys.destroyed)
            .ok_or_else(|| SimError::unsupported(format!("particle system {} does not exist", s.0)))
    }

    fn emitter_mut(&mut self, e: EmitterId) -> Result<&mut Emitter, SimError> {
        usize::try_from(e.index)
            .ok()
            .and_then(|i| self.system_mut(e.system).ok()?.emitters.get_mut(i))
            .filter(|em| !em.destroyed)
            .ok_or_else(|| SimError::unsupported(format!("particle emitter {} does not exist", e.index)))
    }
}

fn index_u32(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}
