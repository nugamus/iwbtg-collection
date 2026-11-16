//! The drawing copy of one particle system: where its particles are, how they move, their color,
//! alpha, size and angle over their life (docs/architecture/client/particles.md).
//!
//! The sim keeps only what decides its RNG draws (`iwbtg_sim::particles`); everything here is
//! the client's, from the same `PartSystemDef`. It draws from its own generator, never the sim's,
//! so the particles on screen are where a particle of that system could be, not where the
//! original's are: the spec gives the draw count and the region point, not which draw sets which
//! value (CLONE_SPEC numbers, "Particle draw rules" 6 and the note on research#158).
//!
//! The motion rules are GM's `part_type_*` arguments as the format describes them
//! (docs/format/v0.md, Particles). What the spec does not cover is a choice of ours, marked
//! OPEN below and in the module page: wiggle as a sine of a fixed period, speed and size not going
//! below 0, gravity added to the motion vector after the speed and direction increments, colors
//! and alpha blended evenly over the life (two values: start to end; three: start, middle, end).

use iwbtg_sim::def::{PartLook, PartSystemDef, PartTypeDef};
use iwbtg_sim::particles::{PartColor, PartRange};
use iwbtg_sim::rng::Rng;

/// Ticks of one wiggle swing (OPEN: the spec does not say how wiggle moves; a value of ours).
const WIGGLE_PERIOD: f64 = 16.0;

/// One live particle.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Particle {
    /// Its type: an index into the definition's `types`.
    pub(crate) ty: usize,
    /// Room position (GM: x right, y down).
    pub(crate) x: f64,
    pub(crate) y: f64,
    /// Speed in pixels a tick and direction in degrees (counter-clockwise, 0 is right).
    speed: f64,
    direction: f64,
    /// Size factor (1 draws the shape or sprite at its own size).
    size: f64,
    /// Angle in degrees, before `orientation_relative`.
    orientation: f64,
    /// Ticks left and ticks it started with.
    life: f64,
    start_life: f64,
    /// Updates it has lived through.
    age: u32,
    /// A color fixed at its birth (`color_mix`, `color_rgb`, `color_hsv`), or `None` to follow
    /// the type's colors over the life.
    color: Option<[f32; 3]>,
    /// The sprite frame it started on.
    frame: u32,
    /// Wiggle phase, 0 to 1.
    phase: f64,
}

/// One particle system as the client draws it. `def` is the index of its `PartSystemDef` in
/// `GameDef::particle_systems`.
#[derive(Clone, Debug)]
pub(crate) struct DrawSystem {
    pub(crate) def: usize,
    /// Each emitter still streams (`destroy_emitters` stops them all).
    emitting: Vec<bool>,
    pub(crate) particles: Vec<Particle>,
    rng: Rng,
}

impl DrawSystem {
    /// Builds the system as the sim's Create does (`Particles::build`): the emitters with their
    /// streams, each emitter's burst in order, then the creates. `seed` starts its own generator.
    pub(crate) fn build(def_index: usize, def: &PartSystemDef, seed: u32) -> DrawSystem {
        let mut s = DrawSystem {
            def: def_index,
            emitting: vec![true; def.emitters.len()],
            particles: Vec::new(),
            rng: Rng::new(seed),
        };
        for em in &def.emitters {
            if let Some((ty, n)) = em.burst {
                let count = s.count(n);
                for _ in 0..count {
                    let (x, y) = s.region_point(em);
                    s.spawn(def, ty as usize, x, y);
                }
            }
        }
        for c in &def.creates {
            for _ in 0..c.number.max(0) {
                s.spawn(def, c.ty as usize, c.at[0].get(), c.at[1].get());
            }
        }
        s
    }

    /// `part_particles_clear`: the live particles go, with no death particles; streams go on.
    pub(crate) fn clear(&mut self) {
        self.particles.clear();
    }

    /// Stops emitter `i`'s stream (`part_emitter_destroy_all` or `part_emitter_destroy`, as the
    /// sim's state shows it); the live particles stay.
    pub(crate) fn stop_emitter(&mut self, i: usize) {
        if let Some(e) = self.emitting.get_mut(i) {
            *e = false;
        }
    }

    /// One particle update (the sim's runs once a tick after End Step): every live particle ages
    /// and moves, or dies with its death particles; then each streaming emitter makes its
    /// particles, in emitter order (CLONE_SPEC "Particle draw rules" 2).
    pub(crate) fn update(&mut self, def: &PartSystemDef) {
        let live = std::mem::take(&mut self.particles);
        let mut kept = Vec::with_capacity(live.len());
        let mut born = Vec::new();
        for mut p in live {
            let Some(t) = def.types.get(p.ty) else { continue };
            p.life -= 1.0;
            p.age += 1;
            if p.life <= 0.0 {
                if let Some(c) = t.draws.death {
                    born.push((c.ty.0 as usize, c.count, p.x, p.y));
                }
                continue;
            }
            if let Some(c) = t.draws.step {
                born.push((c.ty.0 as usize, c.count, p.x, p.y));
            }
            step(&mut p, &t.look);
            kept.push(p);
        }
        self.particles = kept;
        for (ty, n, x, y) in born {
            let count = self.count(n);
            for _ in 0..count {
                self.spawn(def, ty, x, y);
            }
        }
        for (i, em) in def.emitters.iter().enumerate() {
            let (Some((ty, n)), true) = (em.stream, self.emitting.get(i).copied().unwrap_or(false))
            else {
                continue;
            };
            let count = self.count(n);
            for _ in 0..count {
                let (x, y) = self.region_point(em);
                self.spawn(def, ty as usize, x, y);
            }
        }
    }

    /// How many particles a stream, burst or child count makes: `n > 0` makes `n`, `n < 0` one
    /// with chance `1 / (|n| + 1)` (CLONE_SPEC "Particle draw rules" 3 and 8).
    fn count(&mut self, n: i32) -> u32 {
        if n > 0 {
            n.unsigned_abs()
        } else {
            let chance = 1.0 / (f64::from(n.unsigned_abs()) + 1.0);
            u32::from(self.unit() < chance)
        }
    }

    fn unit(&mut self) -> f64 {
        self.rng.next_unit().get()
    }

    /// A value in `r` (its min when the range is empty or reversed).
    fn pick(&mut self, r: PartRange) -> f64 {
        if r.draws() {
            r.min.get() + self.unit() * (r.max.get() - r.min.get())
        } else {
            r.min.get()
        }
    }

    /// A point of the emitter's region in room coordinates: the sim's own region rule
    /// (`Region::draw_point`, in unit coordinates) on our generator, scaled to the rectangle.
    fn region_point(&mut self, em: &iwbtg_sim::def::PartEmitterDef) -> (f64, f64) {
        let (u, v) = em.region.draw_point(&mut self.rng);
        let [x0, x1, y0, y1] = em.rect.map(|r| r.get());
        (x0 + u.get() * (x1 - x0), y0 + v.get() * (y1 - y0))
    }

    /// A new particle of type `ty` at (`x`, `y`). A type that is not in the definition makes
    /// nothing, as in the sim's burst (item 8).
    fn spawn(&mut self, def: &PartSystemDef, ty: usize, x: f64, y: f64) {
        let Some(t) = def.types.get(ty) else { return };
        let d = &t.draws;
        let size = self.pick(d.size);
        let speed = self.pick(d.speed);
        let direction = self.pick(d.direction);
        let orientation = self.pick(d.orientation);
        let life = self.pick(d.life).max(1.0);
        let color = self.birth_color(t);
        let frame = if d.random_subimage {
            (self.unit() * 1_000_000.0) as u32
        } else {
            0
        };
        let phase = self.unit();
        self.particles.push(Particle {
            ty,
            x,
            y,
            speed,
            direction,
            size,
            orientation,
            life,
            start_life: life,
            age: 0,
            color,
            frame,
            phase,
        });
    }

    /// The color a particle keeps for its life, when its type picks one at birth.
    fn birth_color(&mut self, t: &PartTypeDef) -> Option<[f32; 3]> {
        match t.draws.color {
            PartColor::Fixed => None,
            PartColor::Mix => {
                let a = rgb(t.look.colors.first().copied().unwrap_or(0xff_ffff));
                let b = rgb(t.look.colors.get(1).copied().unwrap_or(0xff_ffff));
                let m = self.unit() as f32;
                Some(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * m))
            }
            PartColor::Rgb(c) => Some(c.map(|r| (self.pick(r) / 255.0).clamp(0.0, 1.0) as f32)),
            PartColor::Hsv(c) => {
                let [h, s, v] = c.map(|r| (self.pick(r) / 255.0).clamp(0.0, 1.0));
                Some(hsv(h, s, v))
            }
        }
    }
}

/// One update's motion and changes of a live particle (OPEN rules in the module docs).
fn step(p: &mut Particle, look: &PartLook) {
    let [size_inc, _] = look.size_change.map(|r| r.get());
    let [speed_inc, _] = look.speed_change.map(|r| r.get());
    let [dir_inc, _] = look.direction_change.map(|r| r.get());
    let [orient_inc, _] = look.orientation_change.map(|r| r.get());
    p.size = (p.size + size_inc).max(0.0);
    p.speed = (p.speed + speed_inc).max(0.0);
    p.direction += dir_inc;
    p.orientation += orient_inc;
    let [g, g_dir] = look.gravity.map(|r| r.get());
    let speed = p.speed + wiggle(p, look.speed_change[1].get());
    let direction = p.direction + wiggle(p, look.direction_change[1].get());
    let (mut vx, mut vy) = polar(speed, direction);
    if g != 0.0 {
        let (gx, gy) = polar(g, g_dir);
        vx += gx;
        vy += gy;
        // The gravity stays in the motion: speed and direction follow the new vector.
        p.speed = vx.hypot(vy);
        if p.speed > 0.0 {
            p.direction = (-vy).atan2(vx).to_degrees();
        }
    }
    p.x += vx;
    p.y += vy;
}

/// `speed` pixels in `direction` degrees (counter-clockwise on screen, y down in the room).
fn polar(speed: f64, direction: f64) -> (f64, f64) {
    let (sin, cos) = direction.to_radians().sin_cos();
    (speed * cos, -speed * sin)
}

/// The wiggle offset of `amount` at the particle's age.
fn wiggle(p: &Particle, amount: f64) -> f64 {
    if amount == 0.0 {
        return 0.0;
    }
    amount * (std::f64::consts::TAU * (f64::from(p.age) / WIGGLE_PERIOD + p.phase)).sin()
}

/// `0xRRGGBB` as sRGB channels 0 to 1.
pub(crate) fn rgb(c: u32) -> [f32; 3] {
    [(c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff].map(|v| v as f32 / 255.0)
}

/// HSV (each 0 to 1) to sRGB.
fn hsv(h: f64, s: f64, v: f64) -> [f32; 3] {
    let h6 = (h * 6.0) % 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i as u32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [r as f32, g as f32, b as f32]
}

/// One to three values blended evenly over the life: `t` is 0 at birth and 1 at death.
fn over_life<const N: usize>(values: &[[f32; N]], t: f32) -> Option<[f32; N]> {
    let mix = |a: [f32; N], b: [f32; N], k: f32| std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k);
    match values {
        [] => None,
        [a] => Some(*a),
        [a, b] => Some(mix(*a, *b, t)),
        [a, b, c, ..] => Some(if t < 0.5 {
            mix(*a, *b, t * 2.0)
        } else {
            mix(*b, *c, t * 2.0 - 1.0)
        }),
    }
}

/// How a particle looks this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Look {
    /// sRGB color and alpha, each 0 to 1.
    pub(crate) rgba: [f32; 4],
    /// Size factor (times the type's scale for x and y).
    pub(crate) size: f32,
    /// Angle in degrees, counter-clockwise.
    pub(crate) angle: f32,
    /// Sprite frame (before the frame count wraps it).
    pub(crate) frame: u32,
}

impl Particle {
    /// Its look for type `t`: colors and alpha over the life, size and angle with wiggle, the
    /// sprite frame (`animate` steps one frame a tick; `stretch` spreads the frames over the
    /// life; `frames` is the sprite's frame count).
    pub(crate) fn look(&self, t: &PartTypeDef, frames: u32) -> Look {
        let k = if self.start_life > 0.0 {
            (f64::from(self.age) / self.start_life).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let colors: Vec<[f32; 3]> = t.look.colors.iter().map(|&c| rgb(c)).collect();
        let color = self
            .color
            .or_else(|| {
                if t.draws.color == PartColor::Fixed {
                    over_life(&colors, k)
                } else {
                    None
                }
            })
            .unwrap_or([1.0; 3]);
        let alphas: Vec<[f32; 1]> = t.look.alpha.iter().map(|a| [a.get() as f32]).collect();
        let alpha = over_life(&alphas, k).map_or(1.0, |[a]| a.clamp(0.0, 1.0));
        let size = (self.size + wiggle(self, t.look.size_change[1].get())).max(0.0) as f32;
        let mut angle = self.orientation + wiggle(self, t.look.orientation_change[1].get());
        if t.look.orientation_relative {
            angle += self.direction;
        }
        let frames = frames.max(1);
        let frame = match t.look.sprite {
            Some((_, true, true)) => ((k * frames as f32) as u32).min(frames - 1),
            Some((_, true, false)) => self.frame.wrapping_add(self.age),
            _ => self.frame,
        } % frames;
        Look {
            rgba: [color[0], color[1], color[2], alpha],
            size,
            angle: angle as f32,
            frame,
        }
    }
}

#[cfg(test)]
mod tests {
    use iwbtg_sim::def::{PartCreateDef, PartEmitterDef};
    use iwbtg_sim::num::Real;
    use iwbtg_sim::particles::{PartChild, PartType, PartTypeId, Region};

    use super::*;

    fn ty(life: f64) -> PartTypeDef {
        PartTypeDef {
            name: "t".into(),
            draws: PartType {
                size: PartRange::fixed(1.0),
                life: PartRange::fixed(life),
                ..PartType::default()
            },
            look: PartLook {
                scale: [Real::ONE; 2],
                gravity: [Real::ZERO, Real::new(270.0)],
                ..PartLook::default()
            },
        }
    }

    fn rect(x0: f64, x1: f64, y0: f64, y1: f64) -> [Real; 4] {
        [x0, x1, y0, y1].map(Real::new)
    }

    #[test]
    fn bursts_and_creates_land_where_the_definition_puts_them() {
        let def = PartSystemDef {
            types: vec![ty(10.0)],
            emitters: vec![PartEmitterDef {
                rect: rect(100.0, 200.0, 50.0, 60.0),
                region: Region::default(),
                stream: None,
                burst: Some((0, 40)),
            }],
            creates: vec![PartCreateDef {
                at: [Real::new(7.0), Real::new(9.0)],
                ty: 0,
                number: 3,
            }],
            ..PartSystemDef::default()
        };
        let s = DrawSystem::build(0, &def, 1);
        assert_eq!(s.particles.len(), 43);
        for p in &s.particles[..40] {
            assert!((100.0..=200.0).contains(&p.x) && (50.0..=60.0).contains(&p.y), "{p:?}");
        }
        for p in &s.particles[40..] {
            assert_eq!((p.x, p.y), (7.0, 9.0));
        }
    }

    #[test]
    fn streams_flow_until_the_emitters_stop_and_particles_die_at_their_life() {
        let def = PartSystemDef {
            types: vec![ty(3.0)],
            emitters: vec![PartEmitterDef {
                rect: rect(0.0, 0.0, 0.0, 0.0),
                stream: Some((0, 2)),
                ..PartEmitterDef::default()
            }],
            ..PartSystemDef::default()
        };
        let mut s = DrawSystem::build(0, &def, 1);
        assert!(s.particles.is_empty());
        for _ in 0..5 {
            s.update(&def);
        }
        // Life 3: the particles of the last three updates are alive.
        assert_eq!(s.particles.len(), 6);
        s.stop_emitter(0);
        s.update(&def);
        assert_eq!(s.particles.len(), 4, "no new ones; the oldest two died");
        s.clear();
        assert!(s.particles.is_empty());
    }

    #[test]
    fn a_negative_stream_is_a_chance() {
        let def = PartSystemDef {
            types: vec![ty(1000.0)],
            emitters: vec![PartEmitterDef {
                stream: Some((0, -9)),
                ..PartEmitterDef::default()
            }],
            ..PartSystemDef::default()
        };
        let mut s = DrawSystem::build(0, &def, 7);
        for _ in 0..1000 {
            s.update(&def);
        }
        // One in ten on average.
        assert!((60..=140).contains(&s.particles.len()), "{}", s.particles.len());
    }

    #[test]
    fn death_particles_appear_where_a_particle_dies() {
        let mut parent = ty(2.0);
        parent.draws.speed = PartRange::fixed(1.0);
        parent.draws.death = Some(PartChild {
            count: 3,
            ty: PartTypeId(1),
        });
        let def = PartSystemDef {
            types: vec![parent, ty(50.0)],
            creates: vec![PartCreateDef {
                at: [Real::new(10.0), Real::new(10.0)],
                ty: 0,
                number: 1,
            }],
            ..PartSystemDef::default()
        };
        let mut s = DrawSystem::build(0, &def, 1);
        s.update(&def);
        assert_eq!((s.particles[0].x, s.particles[0].y), (11.0, 10.0), "speed 1 to the right");
        s.update(&def);
        assert_eq!(s.particles.len(), 3);
        assert!(s.particles.iter().all(|p| p.ty == 1 && p.x == 11.0));
    }

    #[test]
    fn gravity_pulls_down_and_colors_and_alpha_blend_over_the_life() {
        let mut t = ty(4.0);
        t.look.gravity = [Real::new(0.5), Real::new(270.0)];
        t.look.colors = vec![0xff_0000, 0x00_00ff];
        t.look.alpha = vec![Real::new(1.0), Real::new(0.0)];
        let def = PartSystemDef {
            types: vec![t],
            creates: vec![PartCreateDef {
                at: [Real::ZERO, Real::ZERO],
                ty: 0,
                number: 1,
            }],
            ..PartSystemDef::default()
        };
        let mut s = DrawSystem::build(0, &def, 1);
        let start = s.particles[0].look(&def.types[0], 1);
        assert_eq!(start.rgba, [1.0, 0.0, 0.0, 1.0]);
        s.update(&def);
        s.update(&def);
        let p = &s.particles[0];
        assert!(p.y > 1.0 && p.x.abs() < 1e-9, "falls straight down: {p:?}");
        let mid = p.look(&def.types[0], 1);
        assert_eq!(mid.rgba, [0.5, 0.0, 0.5, 0.5]);
    }

    #[test]
    fn the_same_seed_draws_the_same_picture() {
        let mut t = ty(30.0);
        t.draws.speed = PartRange::new(1.0, 3.0);
        t.draws.direction = PartRange::new(0.0, 360.0);
        let def = PartSystemDef {
            types: vec![t],
            emitters: vec![PartEmitterDef {
                rect: rect(0.0, 800.0, 0.0, 608.0),
                stream: Some((0, 3)),
                ..PartEmitterDef::default()
            }],
            ..PartSystemDef::default()
        };
        let run = || {
            let mut s = DrawSystem::build(0, &def, 42);
            for _ in 0..20 {
                s.update(&def);
            }
            s.particles
        };
        assert_eq!(run(), run());
    }
}
