//! The tracker on the particle demo: builds, lifecycle calls, retries, and the sim untouched.

use std::path::Path;
use std::sync::Arc;

use iwbtg_sim::def::{GameDef, ParamValue};
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::particles::PartSystemId;
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::sim::{Sim, SimOptions};
use iwbtg_sim::sound::DllMode;

use super::*;

fn demo() -> Arc<GameDef> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/particles.pkg");
    crate::package::open(&dir, false).unwrap().def
}

fn sim(def: Arc<GameDef>) -> Sim {
    let options = SimOptions {
        start: StartPoint::NewGame { difficulty: 0 },
        seed: 0,
        dll_mode: DllMode::Model,
        mode: iwbtg_sim::stripped::PlayMode::Original,
        map: None,
    };
    Sim::new(def, options).unwrap()
}

/// The sim system and drawing copy of the particle file `name`.
fn system<'a>(def: &GameDef, t: &'a Tracker, name: &str) -> (u32, &'a DrawSystem) {
    let d = def.particle_systems.iter().position(|p| p.name == name).unwrap();
    let (s, sys) = t.systems.iter().find(|(_, s)| s.def == d).unwrap();
    (*s, sys)
}

#[test]
fn the_demo_builds_both_systems_and_they_flow_every_tick() {
    let def = demo();
    let mut sim = sim(Arc::clone(&def));
    let mut t = Tracker::default();
    sim.step(InputFrame::EMPTY);
    t.follow(&def, sim.world());
    assert_eq!(t.systems.len(), 2);
    let (_, snow) = system(&def, &t, "snow");
    // The burst of 120 and one update of the stream of 2.
    assert_eq!(snow.particles.len(), 122);
    let (_, fountain) = system(&def, &t, "fountain");
    assert_eq!(
        fountain.particles.len(),
        12 + 3,
        "the star burst and one stream update"
    );
    let g = t.generation;
    for _ in 0..50 {
        sim.step(InputFrame::EMPTY);
        t.follow(&def, sim.world());
    }
    assert!(t.generation > g);
    let (_, fountain) = system(&def, &t, "fountain");
    let sparks = def.particle_systems[fountain.def]
        .types
        .iter()
        .position(|ty| ty.name == "spark")
        .unwrap();
    let up = fountain
        .particles
        .iter()
        .filter(|p| p.ty == sparks && p.y < 470.0)
        .count();
    assert!(up > 50, "the sparks rise over the block: {up}");
}

#[test]
fn following_the_sim_never_changes_it() {
    let def = demo();
    let (mut watched, mut alone) = (sim(Arc::clone(&def)), sim(Arc::clone(&def)));
    let mut t = Tracker::default();
    for _ in 0..100 {
        watched.step(InputFrame::EMPTY);
        alone.step(InputFrame::EMPTY);
        t.follow(&def, watched.world());
    }
    assert_eq!(watched.state_hash(), alone.state_hash());
}

#[test]
fn destroy_and_destroy_emitters_come_from_the_sims_state() {
    let def = demo();
    let mut sim = sim(Arc::clone(&def));
    let mut t = Tracker::default();
    sim.step(InputFrame::EMPTY);
    t.follow(&def, sim.world());
    let (snow, _) = system(&def, &t, "snow");
    let (fountain, _) = system(&def, &t, "fountain");
    let mut w = sim.world().clone();
    w.particles.destroy_all_emitters(PartSystemId(snow)).unwrap();
    w.particles.destroy_system(PartSystemId(fountain)).unwrap();
    w.tick += 1;
    let before = system(&def, &t, "snow").1.particles.len();
    t.follow(&def, &w);
    assert!(!t.systems.contains_key(&fountain), "destroyed: gone");
    let after = system(&def, &t, "snow").1.particles.len();
    assert_eq!(after, before, "no stream any more, and no flake dies this early");
}

#[test]
fn a_room_start_clear_takes_the_burst_away() {
    let mut def = (*demo()).clone();
    let k = def.kinds.iter_mut().find(|k| k.name == "@snowfall").unwrap();
    k.params
        .0
        .insert("on_room_start".into(), ParamValue::Str("clear".into()));
    k.events.room_start = true;
    let def = Arc::new(def);
    let mut sim = sim(Arc::clone(&def));
    let mut t = Tracker::default();
    sim.step(InputFrame::EMPTY);
    t.follow(&def, sim.world());
    let (_, snow) = system(&def, &t, "snow");
    assert_eq!(
        snow.particles.len(),
        2,
        "the burst was cleared; one stream update is left"
    );
}

/// A `system` list builds one drawing copy per file from the one instance, and each draws at its
/// own file's depth (rely objEffect's two systems, client#826).
#[test]
fn a_system_list_draws_each_system_at_its_own_depth() {
    let mut def = (*demo()).clone();
    let id = |name: &str| {
        let i = def.particle_systems.iter().position(|p| p.name == name).unwrap();
        ParamValue::Particles(iwbtg_sim::def::PartSystemDefId(i as u32))
    };
    let list = ParamValue::List(vec![id("snow"), id("fountain")]);
    // Only the snowfall instance stays; it builds both files.
    let fountain = def.kinds.iter().position(|k| k.name == "@fountain").unwrap();
    for r in &mut def.rooms {
        r.instances.retain(|i| i.kind.index() != Some(fountain));
    }
    let def = {
        let mut def = def;
        let k = def.kinds.iter_mut().find(|k| k.name == "@snowfall").unwrap();
        k.params.0.insert("system".into(), list);
        Arc::new(def)
    };
    let mut sim = sim(Arc::clone(&def));
    let mut t = Tracker::default();
    sim.step(InputFrame::EMPTY);
    t.follow(&def, sim.world());
    let (snow, _) = system(&def, &t, "snow");
    let (fountain, _) = system(&def, &t, "fountain");
    assert_eq!(t.depth(&def, snow), 10.0);
    assert_eq!(t.depth(&def, fountain), -10.0);
}

/// In a room listed in `on_room_start_except` the Room Start clear does not happen
/// (go-the-spook-house oRainSys, client#826).
#[test]
fn an_excepted_room_keeps_the_burst() {
    let mut def = (*demo()).clone();
    let stage = def.rooms.iter().position(|r| r.name == "stage").unwrap();
    let k = def.kinds.iter_mut().find(|k| k.name == "@snowfall").unwrap();
    k.params
        .0
        .insert("on_room_start".into(), ParamValue::Str("clear".into()));
    k.params.0.insert(
        "on_room_start_except".into(),
        ParamValue::List(vec![ParamValue::Room(iwbtg_sim::def::RoomId(stage as u32))]),
    );
    k.events.room_start = true;
    let def = Arc::new(def);
    let mut sim = sim(Arc::clone(&def));
    let mut t = Tracker::default();
    sim.step(InputFrame::EMPTY);
    t.follow(&def, sim.world());
    let (_, snow) = system(&def, &t, "snow");
    assert_eq!(
        snow.particles.len(),
        122,
        "the burst of 120 stays, plus one stream update"
    );
}

/// An instance whose Room Start destroys it in the first room is gone before the client first
/// looks, yet its systems still stream in the sim, so they are drawn (`World::particle_defs`,
/// go-the-spook-house oRainSys outside its excepted rooms, #866).
#[test]
fn a_system_whose_instance_destroyed_itself_in_the_first_room_is_drawn() {
    let mut def = (*demo()).clone();
    let k = def.kinds.iter_mut().find(|k| k.name == "@snowfall").unwrap();
    k.params
        .0
        .insert("on_room_start_destroys_instance".into(), ParamValue::Bool(true));
    k.events.room_start = true;
    let def = Arc::new(def);
    let mut sim = sim(Arc::clone(&def));
    let mut t = Tracker::default();
    sim.step(InputFrame::EMPTY);
    t.follow(&def, sim.world());
    let snowfall = def.kinds.iter().position(|k| k.name == "@snowfall").unwrap();
    assert!(
        !sim.world()
            .instances()
            .iter()
            .any(|i| i.alive && i.kind.index() == Some(snowfall)),
        "the instance destroyed itself"
    );
    let (_, snow) = system(&def, &t, "snow");
    assert_eq!(
        snow.particles.len(),
        122,
        "the burst of 120 plus one stream update"
    );
    let before = snow.particles.len();
    for _ in 0..5 {
        sim.step(InputFrame::EMPTY);
        t.follow(&def, sim.world());
    }
    let (_, snow) = system(&def, &t, "snow");
    assert!(snow.particles.len() > before, "it keeps streaming");
}

#[test]
fn a_retry_starts_the_particles_over() {
    let def = demo();
    let mut first = sim(Arc::clone(&def));
    let mut t = Tracker::default();
    for _ in 0..30 {
        first.step(InputFrame::EMPTY);
        t.follow(&def, first.world());
    }
    let mut again = sim(Arc::clone(&def));
    again.step(InputFrame::EMPTY);
    t.follow(&def, again.world());
    let mut fresh = Tracker::default();
    fresh.follow(&def, again.world());
    let (_, a) = system(&def, &t, "snow");
    let (_, b) = system(&def, &fresh, "snow");
    assert_eq!(a.particles, b.particles);
}
