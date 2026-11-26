//! Skipping a cutscene or dialogue region in a ranked run (#573; docs/product.md, Two ways to play
//! each game; docs/format/game.md, Stripped mode).
//!
//! While a region holds (`Sim::in_region`), the skip-cutscene key (a client key, Backspace by
//! default, bindings.rs) runs the region's ticks at once inside one fixed step, before the normal
//! tick: on the player's held input, until no region holds, the run stops or [`MAX_SKIP_SECONDS`]
//! of game time have run. Nothing is drawn for those ticks and their one-shot sounds are dropped;
//! the replay records every one of them, so it re-simulates to the same world as watching the
//! cutscene would. Regions never hold in Play original, so the key does nothing there.

use crate::time::Instant;

use bevy::prelude::*;
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::sim::Sim;
use iwbtg_sim::sound::SoundOutput;
use iwbtg_sim::world::SimStatus;

use crate::input::Collector;
use crate::replays::Recorder;
use crate::view::{Game, SimTick};

/// The most game time one press skips, in seconds of the game's tick rate. A region that still
/// holds after that (one that waits for the player to do something, say) runs on as usual; a
/// second press skips again. A cap, not a rule: `revisit` on #573.
pub const MAX_SKIP_SECONDS: u64 = 60;

/// Runs the skip key's requests.
pub struct SkipPlugin;

/// The skip's place in the module list (main.rs): its plugin.
pub struct Registration;

impl crate::registration::ClientModule for Registration {
    type Args = crate::registration::NoArgs;

    fn add(
        app: &mut bevy::prelude::App,
        _args: crate::registration::NoArgs,
        _startup: &crate::registration::Startup,
    ) {
        app.add_plugins(SkipPlugin);
    }
}

impl Plugin for SkipPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, skip.in_set(SimTick).before(crate::view::tick));
    }
}

/// What one skip did.
#[derive(Debug, Default, PartialEq)]
pub struct Skipped {
    /// Ticks run.
    pub ticks: u64,
    /// The output events of those ticks that change what keeps playing (music and the sound
    /// state), in order. One-shot sounds and script cues are left out.
    pub lasting: Vec<OutputEvent>,
}

/// Steps `sim` while a region holds and it runs, at most `cap` ticks. The first tick takes
/// `first` (the collector's frame, with its edges); the others hold what it holds, with no edges.
/// `record` gets every running tick's frame and splits, as `view::tick` gives them to the
/// replay recorder.
pub fn run_skip(
    sim: &mut Sim,
    first: InputFrame,
    cap: u64,
    mut record: impl FnMut(&Sim, InputFrame, &[iwbtg_sim::timing::Split]),
) -> Skipped {
    let mut out = Skipped::default();
    let mut frame = first;
    while out.ticks < cap && sim.world().status == SimStatus::Running && sim.in_region() {
        let step = sim.step(frame);
        record(sim, frame, &step.splits);
        out.ticks += 1;
        out.lasting
            .extend(sim.world().output.iter().filter(|e| lasts(e)).cloned());
        frame = InputFrame {
            held: frame.held,
            pressed: 0,
            released: 0,
        };
    }
    out
}

/// Whether an output event matters after its tick: every sound event but a one-shot play, so
/// music that starts, stops or changes during the skipped region still does.
fn lasts(e: &OutputEvent) -> bool {
    matches!(e, OutputEvent::Sound(s) if !matches!(s, SoundOutput::Play { looping: false, .. }))
}

/// The skip key's system: a press while a region holds skips it.
fn skip(
    mut game: NonSendMut<Game>,
    mut collector: ResMut<Collector>,
    mut audio: ResMut<crate::audio::Queue>,
    mut replay: Option<ResMut<Recorder>>,
    mut stats: Option<ResMut<crate::stats::Stats>>,
    menu: Option<Res<crate::pause::Menu>>,
    fixed: Option<Res<Time<Fixed>>>,
) {
    if !collector.take_skip() || !game.sim.in_region() {
        return;
    }
    let cap = MAX_SKIP_SECONDS * u64::from(game.sim.def().tick_rate.max(1));
    let first = collector.take_frame();
    let replay_name = replay
        .as_deref()
        .and_then(|r| r.current_name())
        .map(str::to_owned);
    let tick_seconds = fixed.map_or(0.0, |f| f.timestep().as_secs_f64());
    let skipped = run_skip(&mut game.sim, first, cap, |sim, frame, splits| {
        if let Some(r) = replay.as_deref_mut() {
            r.record(sim, frame, splits);
        }
        // Every skipped tick reaches the stats, not only the state after the skip (#1018): its
        // play time, deaths and splits are the same as when the cutscene is watched.
        if let Some(stats) = stats.as_deref_mut() {
            stats.observe(sim, menu.as_deref(), tick_seconds, replay_name.as_deref());
        }
    });
    audio.push(Instant::now(), &skipped.lasting);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use iwbtg_sim::def::{GameDef, KindId, RoomId};
    use iwbtg_sim::input::Action;
    use iwbtg_sim::room::StartPoint;
    use iwbtg_sim::sim::SimOptions;
    use iwbtg_sim::sound::DllMode;
    use iwbtg_sim::stripped::{PlayMode, Region, RegionCondition};

    use super::*;

    /// The demo package with a cutscene that holds while a bullet exists.
    fn def() -> Arc<GameDef> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/input.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut def = (*pkg.def).clone();
        let bullet = def
            .kinds
            .iter()
            .position(|k| k.name == "kid_bullet")
            .and_then(|i| u32::try_from(i).ok())
            .map(KindId)
            .expect("kid_bullet");
        def.stripped.cutscenes.push(Region {
            id: "bullet".into(),
            holds: RegionCondition::Kind(bullet),
        });
        Arc::new(def)
    }

    fn sim(def: &Arc<GameDef>, mode: PlayMode) -> Sim {
        let options = SimOptions {
            start: StartPoint::NewGame { difficulty: 0 },
            seed: 7,
            dll_mode: DllMode::Model,
            mode,
            map: None,
        };
        Sim::new(Arc::clone(def), options).unwrap()
    }

    const RIGHT: InputFrame = InputFrame {
        held: Action::Right.bit(),
        pressed: 0,
        released: 0,
    };

    /// Plays to a shot: 20 ticks of nothing, then shoot.
    fn to_the_shot(sim: &mut Sim, frames: &mut Vec<InputFrame>) {
        for _ in 0..20 {
            sim.step(InputFrame::EMPTY);
            frames.push(InputFrame::EMPTY);
        }
        let shoot = InputFrame {
            held: Action::Shoot.bit(),
            pressed: Action::Shoot.bit(),
            released: 0,
        };
        sim.step(shoot);
        frames.push(shoot);
    }

    /// A skip runs exactly the frozen ticks on the held input; the frames it records replay to
    /// the same world as playing the cutscene tick by tick; the timer does not move.
    #[test]
    fn a_skip_runs_the_region_and_replays_like_watching_it() {
        let def = def();
        let mut skipped = sim(&def, PlayMode::Stripped);
        let mut frames = Vec::new();
        to_the_shot(&mut skipped, &mut frames);
        assert!(skipped.in_region(), "the bullet flies");
        let ranked = skipped.ranked_ticks();
        let first = InputFrame {
            held: Action::Right.bit(),
            pressed: Action::Right.bit(),
            released: 0,
        };
        let out = run_skip(&mut skipped, first, 10_000, |_, f, _| frames.push(f));
        assert!(out.ticks > 1, "{out:?}");
        assert!(!skipped.in_region(), "the region is over");
        assert_eq!(
            skipped.ranked_ticks(),
            ranked,
            "a region's ticks are not ranked time"
        );
        assert_eq!(frames.len() as u64, 21 + out.ticks, "every tick run is recorded");
        assert_eq!(frames[21], first, "the first tick keeps the edges");
        assert!(frames[22..].iter().all(|f| *f == RIGHT), "then only what is held");

        // Watching: the same frames one tick at a time.
        let mut watched = sim(&def, PlayMode::Stripped);
        for f in &frames {
            watched.step(*f);
        }
        assert_eq!(watched.state_hash(), skipped.state_hash());
        assert_eq!(watched.world().tick, skipped.world().tick);
    }

    #[test]
    fn a_skip_stops_at_the_cap_and_does_nothing_outside_a_region() {
        let def = def();
        let mut s = sim(&def, PlayMode::Stripped);
        let mut frames = Vec::new();
        to_the_shot(&mut s, &mut frames);
        let out = run_skip(&mut s, RIGHT, 3, |_, _, _| {});
        assert_eq!(out.ticks, 3);
        assert!(s.in_region(), "the cap left the rest");

        // No region in Play original: nothing runs.
        let mut s = sim(&def, PlayMode::Original);
        to_the_shot(&mut s, &mut Vec::new());
        let tick = s.world().tick;
        assert_eq!(run_skip(&mut s, RIGHT, 10_000, |_, _, _| {}).ticks, 0);
        assert_eq!(s.world().tick, tick);
    }

    /// The system: the skip key in a region runs it out in one go; without the key, or outside a
    /// region, nothing extra runs.
    #[test]
    fn the_skip_key_runs_the_region_out() {
        use bevy::ecs::system::RunSystemOnce;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/input.pkg");
        let mut world = crate::view::test_world(&crate::package::open(&dir, false).unwrap());
        world.insert_resource(crate::audio::Queue::new(0));
        let def = def();
        let mut s = sim(&def, PlayMode::Stripped);
        to_the_shot(&mut s, &mut Vec::new());
        world.non_send_mut::<Game>().sim = s;
        let tick = |w: &World| w.non_send::<Game>().sim.world().tick;
        let before = tick(&world);
        world.run_system_once(skip).unwrap();
        assert_eq!(tick(&world), before, "no key, no skip");
        world
            .resource_mut::<Collector>()
            .press(iwbtg_sim::input::vk::BACKSPACE);
        world.run_system_once(skip).unwrap();
        assert!(tick(&world) > before + 1);
        assert!(!world.non_send::<Game>().sim.in_region());
        let after = tick(&world);
        world
            .resource_mut::<Collector>()
            .press(iwbtg_sim::input::vk::BACKSPACE);
        world.run_system_once(skip).unwrap();
        assert_eq!(tick(&world), after, "outside a region the key does nothing");
    }

    /// A copy of the `clear.pkg` fixture in a temp folder, with `(file, from, to)` edits.
    fn package_variant(name: &str, edits: &[(&str, &str, &str)]) -> crate::package::Package {
        fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
            std::fs::create_dir_all(to).unwrap();
            for entry in std::fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                let target = to.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy_dir(&entry.path(), &target);
                } else {
                    std::fs::copy(entry.path(), target).unwrap();
                }
            }
        }
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let dst = std::env::temp_dir().join(format!("iwbtg-skip-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dst);
        copy_dir(&src, &dst);
        for (file, from, to) in edits {
            let path = dst.join(file);
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(text.contains(from), "{file} does not contain {from:?}");
            std::fs::write(&path, text.replacen(from, to, 1)).unwrap();
        }
        crate::package::open(&dst, false).unwrap()
    }

    /// `clear.pkg` with the block and spike removed, so holding Right clears it without jumps:
    /// the stage room has no obstacle, and the goal room has no spike.
    fn open_clear_def() -> Arc<GameDef> {
        let pkg = package_variant(
            "open",
            &[
                (
                    "rooms/stage.toml",
                    "{ kind = \"block\", x = 320, y = 544, run = [1, 1], step = [32, 32] },",
                    "",
                ),
                (
                    "rooms/goal.toml",
                    "{ kind = \"spike\", x = 352, y = 544, dir = \"up\" },",
                    "",
                ),
            ],
        );
        Arc::clone(&pkg.def)
    }

    fn room_index(def: &GameDef, name: &str) -> RoomId {
        let i = def.rooms.iter().position(|r| r.name == name).unwrap();
        RoomId(u32::try_from(i).unwrap())
    }

    const HOLD_RIGHT: InputFrame = InputFrame {
        held: Action::Right.bit(),
        pressed: 0,
        released: 0,
    };

    /// #1018: a skip feeds the stats every simulated tick, so the same frames give the same
    /// book watched or skipped. A stage fires inside the skip (in the frozen region), every
    /// tick is play time, and the region's ticks stay out of the ranked time.
    #[test]
    fn a_skip_gives_the_stats_the_same_book_as_watching() {
        use crate::stats::{Stats, map_key};

        let mut def = (*open_clear_def()).clone();
        let stage = room_index(&def, "stage");
        let goal = room_index(&def, "goal");
        def.stripped.cutscenes.push(Region {
            id: "stage".into(),
            holds: RegionCondition::Room(stage),
        });
        // Two stages fire on the step that enters the goal room, inside the skip. The loader
        // always gives the package a map (even without `[[maps]]`), and the sim reads that
        // chain, so set it there (`def.timing` is only the map-less fallback).
        let stages = vec![
            iwbtg_sim::timing::TimingTrigger::EnterRoom(goal),
            iwbtg_sim::timing::TimingTrigger::EnterRoom(goal),
        ];
        for map in &mut def.maps {
            map.timing.stages = stages.clone();
        }
        let def = Arc::new(def);
        let dt = 1.0 / 50.0;

        // Skipped: the region holds until the goal room; then walk to the clear.
        let mut skipped_sim = sim(&def, PlayMode::Stripped);
        let mut skipped = Stats::open(None);
        skipped.open_current(&skipped_sim, None, None, false);
        let first = InputFrame {
            pressed: Action::Right.bit(),
            ..HOLD_RIGHT
        };
        let mut frames = Vec::new();
        let out = run_skip(&mut skipped_sim, first, 10_000, |s, f, _| {
            frames.push(f);
            skipped.observe(s, None, dt, None);
        });
        assert!(out.ticks > 1, "the stage room was skipped: {out:?}");
        assert!(!skipped_sim.in_region(), "the skip left the region");
        for _ in 0..2000 {
            if skipped_sim.world().timing.ended {
                break;
            }
            skipped_sim.step(HOLD_RIGHT);
            frames.push(HOLD_RIGHT);
            skipped.observe(&skipped_sim, None, dt, None);
        }
        assert!(skipped_sim.world().timing.ended, "the skipped run cleared");

        // Watched: the same frames one tick at a time.
        let mut watched_sim = sim(&def, PlayMode::Stripped);
        let mut watched = Stats::open(None);
        watched.open_current(&watched_sim, None, None, false);
        for f in &frames {
            watched_sim.step(*f);
            watched.observe(&watched_sim, None, dt, None);
        }

        let id = map_key(&def.id, "main");
        let (a, b) = (
            skipped.book.map(&id).unwrap(),
            watched.book.map(&id).unwrap(),
        );
        assert_eq!(a.played_ticks, frames.len() as u64, "every tick is play time");
        assert_eq!(a.played_ticks, b.played_ticks);
        assert_eq!(a.deaths, b.deaths);
        let (sc, wc) = (
            skipped.cleared.as_ref().unwrap(),
            watched.cleared.as_ref().unwrap(),
        );
        assert_eq!((&sc.map, &sc.board), (&wc.map, &wc.board));
        assert_eq!(
            (sc.run.ticks, &sc.run.splits, &sc.run.segment_deaths),
            (wc.run.ticks, &wc.run.splits, &wc.run.segment_deaths),
            "the same run is recorded"
        );
        assert_eq!(sc.run.death_log, wc.run.death_log);
        assert_eq!(wc.run.splits.len(), 3, "both skipped stages and the end");
        assert_eq!(
            wc.run.splits[0], wc.run.splits[1],
            "both stage triggers fire in the frozen region"
        );
        assert!(
            wc.run.ticks < a.played_ticks,
            "the region's ticks stay out of ranked time"
        );
    }

    /// #1018: a death inside a skip counts once and in the same segment as when watched.
    #[test]
    fn a_death_inside_a_skip_counts_like_a_watched_one() {
        use crate::stats::{Stats, map_key};

        // The kid starts in the goal room, whose spike kills it while the region holds.
        let pkg = package_variant(
            "spike",
            &[(
                "game.toml",
                "new_game_room = \"stage\"",
                "new_game_room = \"goal\"",
            )],
        );
        let mut def = (*pkg.def).clone();
        let goal = room_index(&def, "goal");
        def.stripped.cutscenes.push(Region {
            id: "goal".into(),
            holds: RegionCondition::Room(goal),
        });
        let def = Arc::new(def);
        let dt = 1.0 / 50.0;
        let first = InputFrame {
            pressed: Action::Right.bit(),
            ..HOLD_RIGHT
        };

        let mut skipped_sim = sim(&def, PlayMode::Stripped);
        let mut skipped = Stats::open(None);
        skipped.open_current(&skipped_sim, None, None, false);
        let mut frames = Vec::new();
        let out = run_skip(&mut skipped_sim, first, 200, |s, f, _| {
            frames.push(f);
            skipped.observe(s, None, dt, None);
        });
        assert!(out.ticks > 1, "the skip ran some ticks: {out:?}");

        let mut watched_sim = sim(&def, PlayMode::Stripped);
        let mut watched = Stats::open(None);
        watched.open_current(&watched_sim, None, None, false);
        for f in &frames {
            watched_sim.step(*f);
            watched.observe(&watched_sim, None, dt, None);
        }

        assert!(skipped.session.deaths > 0, "the spike killed the kid");
        assert_eq!(skipped.session.deaths, watched.session.deaths);
        let id = map_key(&def.id, "main");
        assert_eq!(skipped.book.map(&id).unwrap().played_ticks, frames.len() as u64);
        assert_eq!(
            skipped.book.map(&id).unwrap().deaths,
            watched.book.map(&id).unwrap().deaths
        );
        assert_eq!(skipped.session.segment_deaths, watched.session.segment_deaths);
    }

    #[test]
    fn only_lasting_sound_events_are_kept() {
        use iwbtg_sim::def::SoundId;
        use iwbtg_sim::sound::SoundTarget;
        let target = SoundTarget::Builtin(SoundId(0));
        let play = |looping| {
            OutputEvent::Sound(SoundOutput::Play {
                target,
                looping,
                length_ticks: 0,
                tick: 0,
            })
        };
        assert!(!lasts(&play(false)), "a one-shot sound");
        assert!(lasts(&play(true)), "music");
        assert!(lasts(&OutputEvent::Sound(SoundOutput::Stop {
            target,
            tick: 0
        })));
        assert!(lasts(&OutputEvent::Sound(SoundOutput::StopAll {
            adapter: false,
            tick: 0,
        })));
    }
}
