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
use iwbtg_sim::def::{GameDef, RoomId};
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
            follow_sim.after(crate::view::tick).in_set(crate::view::SimTick),
        );
    }
}

/// What a `particle_system` instance does on its later events.
#[derive(Clone, Debug, PartialEq)]
struct Watch {
    /// The sim systems it built, in its `system` list order.
    systems: Vec<u32>,
    on_room_start: Lifecycle,
    /// Rooms where its Room Start does nothing.
    except: Vec<RoomId>,
    /// Its Room Start destroys it after the call (so its Destroy runs too).
    destroys_instance: bool,
    on_room_end: Lifecycle,
    on_destroy: Lifecycle,
    /// A persistent instance gone after a room entry was destroyed by its own Room Start.
    persistent: bool,
}

impl Watch {
    fn room_start_runs_in(&self, room: RoomId) -> bool {
        !self.except.contains(&room)
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
    /// - **Build.** A new sim system that a `particle_system` instance holds is built from its
    ///   definition (bursts and creates included).
    /// - **destroy and destroy_emitters** are read from the sim's state (`World::particles`:
    ///   the system's and each emitter's `destroyed`), so they are exact.
    /// - **clear** leaves no trace in the sim's state, so it is inferred from the instance's
    ///   params: `on_room_end` for the instances alive before a room entry, `on_room_start` for
    ///   those alive after it (and after the first room's start, which `Sim::new` runs) unless
    ///   the room is in `on_room_start_except`, `on_destroy` for an instance gone on a tick
    ///   without a room entry. A persistent instance with `on_room_start_destroys_instance` gone
    ///   after entering a room its Room Start runs in made its `on_room_start` and `on_destroy`
    ///   calls. Every call reaches each system the instance built.
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
        // The sim's first step keeps `Sim::new`'s room-start output (#466): that entry is the
        // first room's, not one made on this tick.
        let entries = w
            .output
            .iter()
            .filter(|e| matches!(e, OutputEvent::Room(RoomOutput::Entered { .. })))
            .count();
        let entered = entries > usize::from(w.tick == 1);
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

        let mut clear = |systems: &mut BTreeMap<u32, DrawSystem>, x: &Watch| {
            for s in &x.systems {
                if let Some(d) = systems.get_mut(s) {
                    d.clear();
                    changed = true;
                }
            }
        };
        let room = w.room;
        if first {
            for x in now.values() {
                if x.on_room_start == Lifecycle::Clear && x.room_start_runs_in(room) {
                    clear(&mut self.systems, x);
                }
            }
        }
        if entered {
            for x in self.watched.values() {
                if x.on_room_end == Lifecycle::Clear {
                    clear(&mut self.systems, x);
                }
            }
            for x in now.values() {
                if x.on_room_start == Lifecycle::Clear && x.room_start_runs_in(room) {
                    clear(&mut self.systems, x);
                }
            }
            for (id, x) in &self.watched {
                let destroyed_by_room_start =
                    !now.contains_key(id) && x.persistent && x.destroys_instance && x.room_start_runs_in(room);
                if destroyed_by_room_start
                    && (x.on_room_start == Lifecycle::Clear || x.on_destroy == Lifecycle::Clear)
                {
                    clear(&mut self.systems, x);
                }
            }
        } else {
            for (id, x) in &self.watched {
                if !now.contains_key(id) && x.on_destroy == Lifecycle::Clear {
                    clear(&mut self.systems, x);
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
            KindState::Builtin(BuiltinState::ParticleSystem(p)) if !p.systems.is_empty() => Some((
                i.id,
                Watch {
                    systems: p.systems.iter().map(|s| s.0).collect(),
                    on_room_start: p.on_room_start,
                    except: p.on_room_start_except.clone(),
                    destroys_instance: p.on_room_start_destroys_instance,
                    on_room_end: p.on_room_end,
                    on_destroy: p.on_destroy,
                    persistent: i.persistent,
                },
            )),
            _ => None,
        })
        .collect()
}

/// The definition index of sim system `s`: the `system` list entry of the live instance that
/// built it. A system whose instance is gone by the end of the tick that built it is not drawn
/// (its `on_destroy` decides the rest, which the client cannot see then).
fn def_of(w: &SimWorld, s: u32) -> Option<usize> {
    w.instances()
        .iter()
        .filter(|i| i.alive)
        .find_map(|i| match &i.state {
            KindState::Builtin(BuiltinState::ParticleSystem(p)) => {
                let k = p.systems.iter().position(|x| x.0 == s)?;
                p.defs.get(k)?.index()
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
