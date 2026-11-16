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
use iwbtg_sim::builtins::BuiltinState;
use iwbtg_sim::builtins::particle_system::Lifecycle;
use iwbtg_sim::def::GameDef;
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::room::RoomOutput;
use iwbtg_sim::world::{InstId, KindState, World as SimWorld};

use model::DrawSystem;

/// Draws particle systems (main.rs adds it).
pub(crate) struct ParticlesPlugin;

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        crate::shell::run_resource(app, |_| Tracker::default());
        draw::plugin(app);
        app.add_systems(
            FixedUpdate,
            follow_sim
                .after(crate::view::tick)
                .in_set(crate::view::SimTick),
        );
    }
}

/// What a `particle_system` instance does on its later events.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Watch {
    system: u32,
    on_room_start: Lifecycle,
    on_room_end: Lifecycle,
    on_destroy: Lifecycle,
}

/// The drawing copies of the sim's particle systems, by the sim's `PartSystemId`.
#[derive(Resource, Default, Debug)]
pub(crate) struct Tracker {
    /// Systems built from a package file. A sim system not built by a `particle_system` instance
    /// (a particles-mode `blood_emitter`, #766) has no entry and is not drawn here.
    pub(crate) systems: BTreeMap<u32, DrawSystem>,
    /// How many sim systems have been looked at.
    seen: usize,
    /// The `particle_system` instances alive after the last tick.
    watched: BTreeMap<InstId, Watch>,
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
    /// Brings the drawing copies up to the sim after a tick (or several).
    ///
    /// - **Build.** A new sim system that a `particle_system` instance holds is built from its
    ///   definition (bursts and creates included).
    /// - **destroy and destroy_emitters** are read from the sim's state (`World::particles`:
    ///   the system's and each emitter's `destroyed`), so they are exact.
    /// - **clear** leaves no trace in the sim's state, so it is inferred from the instance's
    ///   params: `on_room_end` for the instances alive before a room entry, `on_room_start` for
    ///   those alive after it (and after the first room's start, which `Sim::new` runs),
    ///   `on_destroy` for an instance gone on a tick without a room entry.
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
        // The first room's Room Start ran in `Sim::new`, before the first tick we see.
        let first = self.last_tick == 0 && self.seen == 0;
        let ticked = w.tick != self.last_tick;
        self.last_tick = w.tick;
        let entered = w
            .output
            .iter()
            .any(|e| matches!(e, OutputEvent::Room(RoomOutput::Entered { .. })));
        let now = watched(w);
        let before_len = self.systems.len();
        let mut changed = false;

        for s in self.seen..w.particles.systems.len() {
            let s = s as u32;
            if let Some(d) = def_of(w, s)
                && let Some(pd) = def.particle_systems.get(d)
            {
                self.systems.insert(s, DrawSystem::build(d, pd, seed(s)));
            }
        }
        self.seen = w.particles.systems.len();

        let mut clear = |systems: &mut BTreeMap<u32, DrawSystem>, s: u32| {
            if let Some(d) = systems.get_mut(&s) {
                d.clear();
                changed = true;
            }
        };
        if first && !entered {
            for x in now.values() {
                if x.on_room_start == Lifecycle::Clear {
                    clear(&mut self.systems, x.system);
                }
            }
        }
        if entered {
            for x in self.watched.values() {
                if x.on_room_end == Lifecycle::Clear {
                    clear(&mut self.systems, x.system);
                }
            }
            for x in now.values() {
                if x.on_room_start == Lifecycle::Clear {
                    clear(&mut self.systems, x.system);
                }
            }
        } else {
            for (id, x) in &self.watched {
                if !now.contains_key(id) && x.on_destroy == Lifecycle::Clear {
                    clear(&mut self.systems, x.system);
                }
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
        self.watched = now;
        if changed || self.systems.len() != before_len {
            self.generation += 1;
        }
    }
}

/// The `particle_system` instances alive in `w` that hold a system.
fn watched(w: &SimWorld) -> BTreeMap<InstId, Watch> {
    w.instances()
        .iter()
        .filter(|i| i.alive)
        .filter_map(|i| match &i.state {
            KindState::Builtin(BuiltinState::ParticleSystem(p)) => p.system.map(|s| {
                (
                    i.id,
                    Watch {
                        system: s.0,
                        on_room_start: p.on_room_start,
                        on_room_end: p.on_room_end,
                        on_destroy: p.on_destroy,
                    },
                )
            }),
            _ => None,
        })
        .collect()
}

/// The definition index of the live instance that holds sim system `s`. A system whose instance
/// is gone by the end of the tick that built it is not drawn (its `on_destroy` decides the
/// rest, which the client cannot see then).
fn def_of(w: &SimWorld, s: u32) -> Option<usize> {
    w.instances().iter().filter(|i| i.alive).find_map(|i| match &i.state {
        KindState::Builtin(BuiltinState::ParticleSystem(p)) if p.system.map(|x| x.0) == Some(s) => {
            p.def.index()
        }
        _ => None,
    })
}

/// After each tick: the drawing copies follow the sim.
fn follow_sim(game: NonSend<crate::view::Game>, mut tracker: ResMut<Tracker>) {
    tracker.follow(game.sim.def(), game.sim.world());
}

#[cfg(test)]
mod tests;
