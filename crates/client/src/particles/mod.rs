//! Particle systems on screen (docs/architecture/client/particles.md, #815).
//!
//! The sim builds each `particle_system` instance's system from its `particles/<id>.toml` and
//! keeps only what decides its RNG draws. This module keeps a drawing copy of each such system
//! ([`model::DrawSystem`]), follows the sim's lifecycle calls (build, clear, destroy_emitters,
//! destroy) after every tick, runs its own particle update once per tick and draws the particles
//! ([`draw`]). It only reads the sim: its generator is its own, so no tick, replay or hash changes.

mod draw;
pub(crate) mod model;
mod shapes;

use std::collections::BTreeMap;

use bevy::prelude::*;
use iwbtg_sim::def::GameDef;
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::particles::PartSystemId;
use iwbtg_sim::room::RoomOutput;
use iwbtg_sim::world::World as SimWorld;

use model::DrawSystem;

/// Draws particle systems (main.rs adds it).
pub(crate) struct ParticlesPlugin;

/// Particles' place in the module list (main.rs): their plugin.
pub struct Registration;

impl crate::registration::ClientModule for Registration {
    type Args = crate::registration::NoArgs;

    fn add(
        app: &mut bevy::prelude::App,
        _args: crate::registration::NoArgs,
        _startup: &crate::registration::Startup,
    ) {
        app.add_plugins(ParticlesPlugin);
    }
}

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        crate::shell::run_resource(app, |_| Tracker::default());
        draw::plugin(app);
        app.add_systems(
            FixedUpdate,
            follow_sim.after(crate::view::tick).in_set(crate::view::SimTick),
        );
    }
}

/// The drawing copies of the sim's particle systems, by the sim's `PartSystemId`.
#[derive(Resource, Default, Debug)]
pub(crate) struct Tracker {
    /// Systems built from a package file. A sim system not built by a `particle_system` instance
    /// (a particles-mode `blood_emitter`, #766) has no entry and is not drawn here.
    pub(crate) systems: BTreeMap<u32, DrawSystem>,
    /// How many sim systems have been looked at.
    seen: usize,
    /// The sim's `clear` count (`World::particle_clears`) each drawing copy has followed.
    clears: BTreeMap<u32, u32>,
    /// The sim's tick when last looked at.
    last_tick: u64,
    /// Bumped whenever a system changes, so the meshes are rebuilt only then.
    pub(crate) generation: u64,
}

/// Our generator's start for sim system `s`: fixed, so a screenshot looks the same every time.
fn seed(s: u32) -> u32 {
    0x9e37_79b9 ^ s.wrapping_mul(0x85eb_ca6b)
}

impl Tracker {
    /// The depth sim system `s` draws at: its own file's `depth`, so each system of a
    /// `system` list draws at its own (0 for a system not drawn here).
    pub(crate) fn depth(&self, def: &GameDef, s: u32) -> f64 {
        self.systems
            .get(&s)
            .and_then(|d| def.particle_systems.get(d.def))
            .map_or(0.0, |d| d.depth.get())
    }

    /// Brings the drawing copies up to the sim after a tick (or several).
    ///
    /// - **Build.** A new sim system that a `particle_system` instance built is built from its
    ///   definition (`World::particle_defs`, so also when the instance is already gone; bursts
    ///   and creates included).
    /// - **destroy and destroy_emitters** are read from the sim's state (`World::particles`:
    ///   the system's and each emitter's `destroyed`), so they are exact.
    /// - **clear** is read from the sim's count of them (`World::particle_clears`, #883): a
    ///   copy whose count moved since the last look is cleared, so it is exact too, also for an
    ///   instance gone before the client saw it. Several clears in one tick are one clear here,
    ///   as no particle is made between them.
    /// - **Update.** One particle update per tick, skipped on a tick that entered a room (the sim
    ///   skips it on a room request tick, CLONE_SPEC "Particle draw rules" 9).
    ///
    /// A sim that went back in time (a retry builds a new one) starts everything over.
    pub(crate) fn follow(&mut self, def: &GameDef, w: &SimWorld) {
        if w.tick < self.last_tick || w.particles.systems.len() < self.seen {
            *self = Tracker {
                generation: self.generation + 1,
                ..Tracker::default()
            };
        }
        let ticked = w.tick != self.last_tick;
        self.last_tick = w.tick;
        // The sim's first step keeps `Sim::new`'s room-start output (#466): that entry is the
        // first room's, not one made on this tick.
        let entries = w
            .output
            .iter()
            .filter(|e| matches!(e, OutputEvent::Room(RoomOutput::Entered { .. })))
            .count();
        let entered = entries > usize::from(w.tick == 1);
        let before_len = self.systems.len();
        let mut changed = false;

        for s in self.seen..w.particles.systems.len() {
            let s = s as u32;
            if let Some(d) = w.particle_defs.get(&PartSystemId(s)).and_then(|d| d.index())
                && let Some(pd) = def.particle_systems.get(d)
            {
                self.systems.insert(s, DrawSystem::build(d, pd, seed(s)));
            }
        }
        self.seen = w.particles.systems.len();

        for (&s, d) in &mut self.systems {
            let n = w.particle_clears.get(&PartSystemId(s)).copied().unwrap_or(0);
            // A new copy starts at 0, so a clear made on the tick that built it counts.
            if self.clears.get(&s).copied().unwrap_or(0) != n {
                d.clear();
                self.clears.insert(s, n);
                changed = true;
            }
        }

        self.systems.retain(|&s, d| {
            let Some(sim) = w.particles.systems.get(s as usize) else {
                return false;
            };
            if sim.destroyed {
                return false;
            }
            for (i, e) in sim.emitters.iter().enumerate() {
                if e.destroyed {
                    d.stop_emitter(i);
                }
            }
            true
        });

        if ticked && !entered {
            for d in self.systems.values_mut() {
                if let Some(pd) = def.particle_systems.get(d.def) {
                    d.update(pd);
                    changed = true;
                }
            }
        }
        self.clears.retain(|s, _| self.systems.contains_key(s));
        if changed || self.systems.len() != before_len {
            self.generation += 1;
        }
    }
}

/// After each tick: the drawing copies follow the sim.
fn follow_sim(game: NonSend<crate::view::Game>, mut tracker: ResMut<Tracker>) {
    tracker.follow(game.sim.def(), game.sim.world());
}

#[cfg(test)]
mod tests;
