//! The replay viewer (#42): watch a stored `.iwbr` replay with play/pause, speed and scrub.
//!
//! A replay is never mutated: the viewer decodes it, rebuilds the sim from its header (the same
//! setup `iwbtg replay verify` uses) and steps the recorded input frames again. The replayed
//! `Sim` is swapped into [`crate::view::Game`] while the viewer is open, so the ordinary renderer
//! draws the frame, and the run's own sim is put back on close.
//!
//! Scrubbing is O(ticks) from tick 0 without help, so the viewer keeps keyframes: a clone of the
//! sim every [`KEYFRAME_INTERVAL`] ticks, at most [`MAX_KEYFRAMES`] of them (the oldest non-zero
//! ones go first). A seek re-simulates from the nearest keyframe at or before the target. The
//! trailer's checkpoint hashes are checked at every checkpoint the re-sim reaches, and the final
//! hash at the last tick, so a replay that does not verify stops the viewer instead of showing a
//! silent desync.
//!
//! The entry point is the results page's Watch replay button (`results.rs`); the viewer runs
//! while the shell is on [`RunScreen::Results`], hides the results page and keeps the HUD
//! suspended. Keys: Space play/pause, Left/Right 5 s (Shift 30 s), Home/End, `,`/`.` speed,
//! Escape close; a click on the progress bar scrubs there.
//!
//! `--viewer-preview` (hidden) is the screenshot check: it records a short run of the loaded
//! package, stores it like any replay, opens it and seeks to the middle, so `--screenshot` shows
//! a real saved replay being watched.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::camera::visibility::RenderLayers;
use bevy::picking::events::PointerClick;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
use iwbtg_sim::def::GameDef;
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::mods::ModSet;
use iwbtg_sim::replay::{
    CHECKPOINT_INTERVAL, Header, MAGIC, REPLAY_FORMAT, Replay, SAVE_SLOT_FORMAT, StartMode, Trailer,
};
use iwbtg_sim::room::StartPoint;
use iwbtg_sim::save::{Checkpoint, SaveSlot};
use iwbtg_sim::sim::{Sim, SimOptions};
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::stripped::PlayMode;
use iwbtg_sim::world::SimStatus;

use crate::shell::{self, InRun, LoadedPackage, RunScreen};
use crate::stats::fmt_ticks;
use crate::storage::{self, Store};
use crate::ui::backdrop::{self, Backdrop};
use crate::ui::fonts::Type;
use crate::ui::tokens;
use crate::ui::widgets::text;
use crate::view::{Game, GameCamera};

/// Ticks between the sim clones a seek starts from. 1000 ticks is 20 s at 50 Hz; a seek re-runs
/// at most that far, and the trailer's 500-tick checkpoints still verify every step.
pub const KEYFRAME_INTERVAL: u64 = 1000;
/// How many keyframes are kept. The first (tick 0) always stays; when the cache is full the oldest
/// non-zero keyframe goes, so playback keeps recent history and a far backward seek starts over.
pub const MAX_KEYFRAMES: usize = 96;
/// Roughly how many bytes the kept sim clones may take. A clone's weight is its world, measured
/// by serializing it (postcard), and a clone is about twice that in memory.
pub const KEYFRAME_BUDGET: usize = 32 * 1024 * 1024;
/// Always keep at least this many keyframes, even for a room whose world is very large.
pub const MIN_KEYFRAMES: usize = 8;
/// The playback speeds the `,` and `.` keys cycle through.
pub const SPEEDS: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];
/// How far an arrow key seeks, in ticks: 5 s at 50 Hz.
pub const SEEK: u64 = 250;
/// How far Shift + arrow seeks: 30 s at 50 Hz.
pub const SEEK_LONG: u64 = 1500;
/// The most ticks one frame advances, so a stalled frame cannot run the sim for seconds.
pub const MAX_STEPS_PER_FRAME: u64 = 240;
/// Ticks the `--viewer-preview` run records before opening the viewer at the middle.
pub const PREVIEW_TICKS: u64 = 520;

/// What the viewer shows and the pending input it has collected. `Send`: the sim itself is in
/// [`ViewerSim`], a non-send resource.
#[derive(Resource, Default)]
pub struct Viewer {
    open: bool,
    /// Close on the next update (Escape).
    close: bool,
    /// Scrub to this tick on the next update (an arrow key or a click on the bar).
    seek: Option<u64>,
    tick: u64,
    total: u64,
    rate: u32,
    playing: bool,
    /// Index into [`SPEEDS`].
    speed: usize,
    /// Ticks carried between frames at a fractional playback rate.
    accum: f64,
    /// Set when the re-sim no longer matches the replay's recorded hashes.
    error: Option<String>,
    ui: Option<Ui>,
}

impl Viewer {
    /// Whether the viewer is showing (the results page uses it to hide itself).
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }
}

/// How many keyframes a replay's world fits in [`KEYFRAME_BUDGET`], between [`MIN_KEYFRAMES`]
/// and [`MAX_KEYFRAMES`]. `0` (a world that would not serialize) takes the cap.
pub fn keyframe_cap(world_bytes: usize) -> usize {
    if world_bytes == 0 {
        return MAX_KEYFRAMES;
    }
    (KEYFRAME_BUDGET / world_bytes.saturating_mul(2)).clamp(MIN_KEYFRAMES, MAX_KEYFRAMES)
}

/// The viewer's entities, dropped on close.
struct Ui {
    root: Entity,
    camera: Entity,
}

/// The viewed replay and the sims that re-simulate it: non-send, like [`Game`], because `Sim`
/// shares the script host by `Rc`.
pub(crate) struct ViewerSim {
    replay: Replay,
    /// The run's sim while the viewer shows the replay; put back on close.
    live: Sim,
    /// The replay's sim at tick 0, to restart even if its keyframe was evicted.
    initial: Sim,
    /// `(tick, sim clone)` sorted by tick, tick 0 first.
    keyframes: Vec<(u64, Sim)>,
    /// How many keyframes this replay's world fits in [`KEYFRAME_BUDGET`].
    max_keyframes: usize,
    /// The trailer's checkpoint hashes by tick.
    checkpoints: BTreeMap<u64, [u8; 16]>,
}

impl ViewerSim {
    /// The sim of the run that was playing when the viewer opened.
    pub(crate) fn live_sim(&self) -> &Sim {
        &self.live
    }

    /// Re-simulates the recorded frames from the nearest keyframe at or before `target`, leaving
    /// `sim` and `tick` at `target`. Every checkpoint crossed is checked against the trailer.
    fn seek(&mut self, sim: &mut Sim, tick: &mut u64, target: u64) -> Result<(), String> {
        let target = target.min(self.total());
        if target < *tick {
            let (at, saved) = self
                .keyframes
                .iter()
                .rev()
                .find(|(at, _)| *at <= target)
                .map(|(at, saved)| (*at, saved.clone()))
                .unwrap_or_else(|| (0, self.initial.clone()));
            *sim = saved;
            *tick = at;
        }
        self.advance(sim, tick, target - *tick)
    }

    /// Steps `ticks` recorded frames (clamped to the replay's end), checking hashes on the way.
    fn advance(&mut self, sim: &mut Sim, tick: &mut u64, ticks: u64) -> Result<(), String> {
        let end = tick.saturating_add(ticks).min(self.total());
        while *tick < end {
            let frame = self.replay.inputs[*tick as usize];
            let step = sim.step(frame);
            let before = *tick;
            *tick = sim.world().tick;
            if *tick == before {
                return Err(match &step.status {
                    SimStatus::Halted(e) => format!("the sim halted at tick {before}: {e}"),
                    _ => format!("the sim stopped at tick {before}"),
                });
            }
            if let Some(want) = self.checkpoints.get(tick) {
                let mut got = [0u8; 16];
                got.copy_from_slice(&sim.state_hash()[..16]);
                if got != *want {
                    return Err(format!(
                        "the state hash does not match the replay at tick {tick}"
                    ));
                }
            }
            if *tick == self.total() && sim.state_hash() != self.replay.trailer.final_hash {
                return Err("the final state hash does not match the replay".to_owned());
            }
            if (*tick).is_multiple_of(KEYFRAME_INTERVAL) {
                self.remember(*tick, sim);
            }
            if step.status == SimStatus::Ended {
                break;
            }
        }
        Ok(())
    }

    /// The replay's length in ticks (one input frame per tick).
    fn total(&self) -> u64 {
        self.replay.inputs.len() as u64
    }

    /// Keeps a keyframe at `tick`, dropping the oldest non-zero one past the cap (tick 0 stays).
    fn remember(&mut self, tick: u64, sim: &Sim) {
        if self.keyframes.last().is_some_and(|(at, _)| *at >= tick) {
            return;
        }
        self.keyframes.push((tick, sim.clone()));
        while self.keyframes.len() > self.max_keyframes {
            self.keyframes.remove(1);
        }
    }
}

/// The viewer's command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug, Clone, Copy, Default)]
pub struct ViewerArgs {
    /// Screenshot checks: store a short run of this package, open it in the viewer and scrub to
    /// its middle.
    #[arg(long, hide = true)]
    pub viewer_preview: bool,
}

/// The viewer's place in the module list (main.rs): its flags and plugin.
pub struct Registration;

impl crate::registration::ClientModule for Registration {
    type Args = ViewerArgs;

    fn add(
        app: &mut bevy::prelude::App,
        args: ViewerArgs,
        _startup: &crate::registration::Startup,
    ) {
        app.add_plugins(ViewerPlugin {
            preview: args.viewer_preview,
        });
    }
}

pub struct ViewerPlugin {
    preview: bool,
}

impl Plugin for ViewerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Viewer>()
            .add_observer(scrub_click)
            .add_systems(
                Update,
                (preview, keys, close, drive)
                    .chain()
                    .before(crate::view::follow_view)
                    .in_set(InRun),
            )
            .add_systems(
                Update,
                (ensure_ui, sync_ui).chain().in_set(InRun),
            );
        if self.preview {
            app.init_resource::<Preview>();
        }
        // Leaving the page (Back, Retry, the run closing) puts the live sim back first.
        app.add_systems(OnExit(RunScreen::Results), close_on_exit);
    }
}

/// Opens the replay `name` (in the data area) for the run's package. Called by the results
/// page's Watch replay button; a replay that is missing, for another package or another sim
/// build is one warning and the page stays.
pub(crate) fn open(world: &mut World, name: &str) {
    let Some(store) = Store::data() else {
        storage::warn("replay viewer: there is no data folder");
        return;
    };
    open_from(world, &store, name);
}

/// [`open`] on a caller's store (the tests use a folder of their own).
fn open_from(world: &mut World, store: &Store, name: &str) {
    if world.resource::<Viewer>().open {
        return;
    }
    let (def, hash) = {
        let loaded = world.resource::<LoadedPackage>();
        (Arc::clone(&loaded.pkg.def), loaded.pkg.gameplay_hash)
    };
    let bytes = match store.read(name) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            storage::warn(&format!("replay viewer: {} is missing", store.describe(name)));
            return;
        }
        Err(e) => {
            storage::warn(&format!("replay viewer: {e}"));
            return;
        }
    };
    let replay = match Replay::decode(&bytes) {
        Ok(replay) => replay,
        Err(e) => {
            storage::warn(&format!("replay viewer: {name}: {e}"));
            return;
        }
    };
    if let Err(e) = compatible(&replay, &def, hash) {
        storage::warn(&format!("replay viewer: {name}: {e}"));
        return;
    }
    let sim = match replay_sim(&replay, &def) {
        Ok(sim) => sim,
        Err(e) => {
            storage::warn(&format!("replay viewer: {name}: {e}"));
            return;
        }
    };
    let total = replay.inputs.len() as u64;
    let rate = def.tick_rate.max(1);
    let checkpoints = replay.trailer.checkpoints.iter().copied().collect();
    // The keyframe count follows the room's world size, so a large room does not cost
    // MAX_KEYFRAMES clones (docs/architecture/client/replays.md, "Replay viewer").
    let max_keyframes = keyframe_cap(postcard::to_allocvec(sim.world()).map_or(0, |b| b.len()));
    let initial = sim.clone();
    let live = {
        let mut game = world.non_send_mut::<Game>();
        std::mem::replace(&mut game.sim, sim)
    };
    world.insert_non_send(ViewerSim {
        replay,
        live,
        initial: initial.clone(),
        keyframes: vec![(0, initial)],
        max_keyframes,
        checkpoints,
    });
    {
        let mut viewer = world.resource_mut::<Viewer>();
        *viewer = Viewer {
            open: true,
            tick: 0,
            total,
            rate,
            playing: true,
            speed: 2,
            ..default()
        };
    }
    cover(world, false);
}

/// Whether the replay may be replayed by this package and build: the same checks as
/// `iwbtg replay verify`.
fn compatible(replay: &Replay, def: &GameDef, hash: [u8; 32]) -> Result<(), String> {
    let h = &replay.header;
    if h.package_id != def.id {
        return Err(format!(
            "the replay is for package `{}`, not `{}`",
            h.package_id, def.id
        ));
    }
    if h.gameplay_hash != hash {
        return Err("the replay was recorded on a different build of this package".to_owned());
    }
    if h.sim_version != iwbtg_sim::SIM_VERSION {
        return Err(format!(
            "the replay is sim_version {}, this build is {}",
            h.sim_version,
            iwbtg_sim::SIM_VERSION
        ));
    }
    if h.engine_rev != def.engine_rev {
        return Err(format!(
            "the replay is engine_rev {}, the package is {}",
            h.engine_rev, def.engine_rev
        ));
    }
    Ok(())
}

/// The sim a replay's header sets up, at tick 0: the same start, seed, play mode and map the
/// recorder read from the run (`crates/tools/src/replay_cmd.rs`, `replay_input`).
fn replay_sim(replay: &Replay, def: &Arc<GameDef>) -> Result<Sim, String> {
    let h = &replay.header;
    let start = match &h.start_mode {
        StartMode::NewGame => StartPoint::NewGame { difficulty: 0 },
        // The checkpoint travels to the sim unresolved: the sim looks its room up when it
        // starts (#1238).
        StartMode::FromSave(bytes) if h.replay_format > SAVE_SLOT_FORMAT => StartPoint::FromSave(
            Checkpoint::decode(bytes)
                .map_err(|e| format!("the replay's checkpoint does not decode: {e}"))?,
        ),
        StartMode::FromSave(bytes) => {
            let slot = SaveSlot::decode(bytes)
                .map_err(|e| format!("the replay's save slot does not decode: {e}"))?;
            StartPoint::FromSave(Checkpoint::from_slot(def, &slot).map_err(|e| {
                format!("the replay's save slot does not resolve: {e}")
            })?)
        }
    };
    let seed = u32::try_from(h.rng_seed)
        .map_err(|_| format!("the replay's seed {} does not fit the sim", h.rng_seed))?;
    let options = SimOptions {
        start,
        seed,
        dll_mode: DllMode::Model,
        mode: h.play_mode,
        map: Some(h.map.clone()),
    };
    Sim::new(Arc::clone(def), options).map_err(|e| format!("the replay's game did not start: {e}"))
}

/// Hides the game for the results page or shows it for the viewer. The HUD stays suspended
/// either way: the viewer draws its own bar.
fn cover(world: &mut World, covered: bool) {
    let mut cameras = world.query_filtered::<&mut Camera, With<GameCamera>>();
    for mut camera in cameras.iter_mut(world) {
        camera.is_active = !covered;
    }
    if let Some(mut backdrop) = world.get_resource_mut::<Backdrop>() {
        backdrop.dim = if covered {
            backdrop::dim::RESULTS
        } else {
            backdrop::dim::GAMEPLAY
        };
    }
}

/// Escape closes; Space toggles; arrows seek (Shift seeks 30 s); Home/End go to the ends; `,`
/// and `.` change speed.
fn keys(keys: Res<ButtonInput<KeyCode>>, mut viewer: ResMut<Viewer>) {
    if !viewer.open {
        return;
    }
    let Viewer {
        open: _,
        close,
        seek,
        tick,
        total,
        speed,
        playing,
        ..
    } = &mut *viewer;
    if keys.just_pressed(KeyCode::Escape) {
        *close = true;
        return;
    }
    if keys.just_pressed(KeyCode::Space) {
        *playing = !*playing;
    }
    let long = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let step = if long { SEEK_LONG } else { SEEK };
    if keys.just_pressed(KeyCode::ArrowLeft) {
        *seek = Some(tick.saturating_sub(step));
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        *seek = Some((*tick + step).min(*total));
    }
    if keys.just_pressed(KeyCode::Home) {
        *seek = Some(0);
    }
    if keys.just_pressed(KeyCode::End) {
        *seek = Some(*total);
    }
    if keys.just_pressed(KeyCode::Comma) {
        *speed = speed.saturating_sub(1);
    }
    if keys.just_pressed(KeyCode::Period) {
        *speed = (*speed + 1).min(SPEEDS.len() - 1);
    }
}

/// Applies a pending seek and advances the replayed sim at the chosen speed.
pub(crate) fn drive(
    time: Res<Time>,
    mut game: NonSendMut<Game>,
    mut viewing: Option<NonSendMut<ViewerSim>>,
    mut viewer: ResMut<Viewer>,
) {
    if !viewer.open {
        return;
    }
    let Some(viewing) = viewing.as_deref_mut() else {
        return;
    };
    let mut tick = viewer.tick;
    if let Some(target) = viewer.seek.take() {
        viewer.playing = false;
        viewer.accum = 0.0;
        match viewing.seek(&mut game.sim, &mut tick, target) {
            Ok(()) => {}
            Err(e) => fail(&mut viewer, e),
        }
        viewer.tick = tick;
    }
    if !viewer.playing || viewer.error.is_some() {
        viewer.accum = 0.0;
        return;
    }
    viewer.accum +=
        f64::from(time.delta_secs()) * f64::from(viewer.rate) * f64::from(SPEEDS[viewer.speed]);
    let ticks = viewer.accum.floor() as u64;
    viewer.accum -= ticks as f64;
    if ticks == 0 {
        return;
    }
    match viewing.advance(&mut game.sim, &mut tick, ticks.min(MAX_STEPS_PER_FRAME)) {
        Ok(()) => {}
        Err(e) => fail(&mut viewer, e),
    }
    viewer.tick = tick;
    if viewer.tick >= viewer.total {
        viewer.playing = false;
    }
}

/// Stops the viewer on a replay that no longer reproduces its hashes.
fn fail(viewer: &mut Viewer, error: String) {
    storage::warn(&format!("replay viewer: {error}"));
    viewer.error = Some(error);
    viewer.playing = false;
}

/// Closes the viewer when Escape asked for it.
fn close(world: &mut World) {
    if !world.resource::<Viewer>().close {
        return;
    }
    close_now(world, true);
}

/// Leaving the results page closes the viewer too; the page's own exit re-covers the game.
fn close_on_exit(world: &mut World) {
    if world.resource::<Viewer>().open {
        close_now(world, false);
    }
}

/// Puts the run's live sim back, drops the viewer's entities and (on a normal close) hides the
/// game behind the results page again.
fn close_now(world: &mut World, cover_results: bool) {
    let (was_open, ui) = {
        let mut viewer = world.resource_mut::<Viewer>();
        let was_open = viewer.open;
        viewer.open = false;
        viewer.close = false;
        viewer.seek = None;
        viewer.error = None;
        (was_open, viewer.ui.take())
    };
    if let Some(ui) = ui {
        world.despawn(ui.root);
        world.despawn(ui.camera);
    }
    if !was_open {
        return;
    }
    if let Some(viewing) = world.remove_non_send::<ViewerSim>() {
        let mut game = world.non_send_mut::<Game>();
        game.sim = viewing.live;
    }
    if cover_results {
        cover(world, true);
    }
}

/// The test hooks for the save-file isolation regression (`savefile.rs`): open on a store of the
/// test's own, ask for a seek, read the progress and close without the results page.
#[cfg(test)]
impl Viewer {
    pub(crate) fn seek_for_test(&mut self, tick: u64) {
        self.seek = Some(tick);
    }

    pub(crate) fn progress_for_test(&self) -> (u64, u64) {
        (self.tick, self.total)
    }
}

#[cfg(test)]
pub(crate) fn open_for_test(world: &mut World, store: &Store, name: &str) {
    open_from(world, store, name);
}

#[cfg(test)]
pub(crate) fn close_for_test(world: &mut World) {
    close_now(world, false);
}

/// A click on the progress bar scrubs there: the bar's normalized x is -0.5 to 0.5.
fn scrub_click(
    click: On<PointerClick>,
    bars: Query<&RelativeCursorPosition, With<ScrubBar>>,
    mut viewer: ResMut<Viewer>,
) {
    let Ok(position) = bars.get(click.entity) else {
        return;
    };
    let Some(at) = position.normalized else {
        return;
    };
    if !viewer.open {
        return;
    }
    let fraction = (at.x + 0.5).clamp(0.0, 1.0);
    viewer.seek = Some((f64::from(fraction) * viewer.total as f64).round() as u64);
}

/// The viewer's camera, root, bar and text.
#[derive(Component)]
struct ViewerCamera;
#[derive(Component)]
struct ViewerRoot;
#[derive(Component)]
struct ScrubBar;
#[derive(Component)]
struct ScrubFill;
#[derive(Component)]
enum UiText {
    Time,
    State,
    Speed,
}

/// Spawns the viewer's bottom bar once per opening.
fn ensure_ui(mut commands: Commands, mut viewer: ResMut<Viewer>) {
    if !viewer.open || viewer.ui.is_some() {
        return;
    }
    let camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: 21,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            RenderLayers::none(),
            ViewerCamera,
            crate::shell::RUN_ONLY,
        ))
        .id();
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexEnd,
                padding: UiRect::all(Val::Px(24.0)),
                ..default()
            },
            UiTargetCamera(camera),
            ViewerRoot,
            crate::shell::RUN_ONLY,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Percent(100.0),
                    padding: UiRect::all(Val::Px(16.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(10.0),
                    border_radius: BorderRadius::all(Val::Px(12.0)),
                    ..default()
                },
                BackgroundColor(tokens::GROUND.with_alpha(0.86)),
            ))
            .with_children(|panel| {
                panel
                    .spawn((
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(8.0),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(tokens::RULE_STRONG),
                        RelativeCursorPosition::default(),
                        Hovered::default(),
                        ScrubBar,
                    ))
                    .with_children(|bar| {
                        bar.spawn((
                            Node {
                                width: Val::Percent(0.0),
                                height: Val::Percent(100.0),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(tokens::ACCENT),
                            ScrubFill,
                        ));
                    });
                panel
                    .spawn(Node {
                        width: Val::Percent(100.0),
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(16.0),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn(Node {
                            column_gap: Val::Px(12.0),
                            align_items: AlignItems::Baseline,
                            ..default()
                        })
                        .with_children(|left| {
                            left.spawn((
                                text("0:00.00", Type::barlow(18.0, 700), tokens::INK),
                                TextLayout::no_wrap(),
                                UiText::Time,
                            ));
                            left.spawn((
                                text("Playing", Type::barlow(14.0, 600), tokens::SOFT),
                                TextLayout::no_wrap(),
                                UiText::State,
                            ));
                        });
                        row.spawn(Node {
                            column_gap: Val::Px(14.0),
                            align_items: AlignItems::Baseline,
                            ..default()
                        })
                        .with_children(|right| {
                            right.spawn((
                                text("1x", Type::barlow(14.0, 700), tokens::INK),
                                TextLayout::no_wrap(),
                                UiText::Speed,
                            ));
                            right.spawn((
                                text(
                                    "Space play/pause · ←/→ 5 s · Shift ←/→ 30 s · , . speed · Esc close",
                                    Type::barlow(12.0, 500),
                                    tokens::MUTED,
                                ),
                                TextLayout::no_wrap(),
                            ));
                        });
                    });
            });
        })
        .id();
    viewer.ui = Some(Ui { root, camera });
}

/// Updates the time, the state, the speed and how far the bar is filled.
fn sync_ui(
    viewer: Res<Viewer>,
    mut texts: Query<(&UiText, &mut Text)>,
    mut fills: Query<&mut Node, With<ScrubFill>>,
) {
    if !viewer.open {
        return;
    }
    let total = viewer.total.max(1);
    for (kind, mut body) in &mut texts {
        let want = match kind {
            UiText::Time => format!(
                "{} / {}",
                fmt_ticks(viewer.tick, viewer.rate),
                fmt_ticks(viewer.total, viewer.rate)
            ),
            UiText::State => viewer.error.as_ref().map_or_else(
                || if viewer.playing { "Playing" } else { "Paused" }.to_owned(),
                |_| "Does not verify".to_owned(),
            ),
            UiText::Speed => format!("{}x", SPEEDS[viewer.speed]),
        };
        if body.0 != want {
            body.0 = want;
        }
    }
    let percent = viewer.tick as f32 / total as f32 * 100.0;
    for mut node in &mut fills {
        if node.width != Val::Percent(percent) {
            node.width = Val::Percent(percent);
        }
    }
}

/// The `--viewer-preview` state: 0 wants the results screen, 1 opens the stored replay there.
#[derive(Resource, Default)]
struct Preview {
    stage: u8,
}

/// `--viewer-preview`: go to the results screen (whose `OnEnter` hides the game), then store a
/// short run of the loaded package, open it and scrub to its middle.
fn preview(world: &mut World) {
    let Some(stage) = world.get_resource::<Preview>().map(|p| p.stage) else {
        return;
    };
    match stage {
        0 => {
            if shell::run_screen(world) == Some(RunScreen::Playing) {
                shell::go(world, RunScreen::Results);
                world.resource_mut::<Preview>().stage = 1;
            }
        }
        1 if shell::run_screen(world) == Some(RunScreen::Results) => {
            world.resource_mut::<Preview>().stage = 2;
            if let Some(name) = preview_replay(world) {
                open(world, &name);
                let middle = world.resource::<Viewer>().total / 2;
                world.resource_mut::<Viewer>().seek = Some(middle);
            }
        }
        _ => {}
    }
}

/// Records [`PREVIEW_TICKS`] ticks of the loaded package (right held, jump tapped) into
/// `replays/<id>/viewer-preview.iwbr` and returns its name, so a `--screenshot` check opens a
/// real stored replay. This is the screenshot check's own run, never a player's: the hidden flag
/// is only passed by checks.
fn preview_replay(world: &mut World) -> Option<String> {
    let (def, hash) = {
        let loaded = world.resource::<LoadedPackage>();
        (Arc::clone(&loaded.pkg.def), loaded.pkg.gameplay_hash)
    };
    let store = Store::data()?;
    let options = SimOptions {
        start: StartPoint::NewGame { difficulty: 0 },
        seed: 0,
        dll_mode: DllMode::Model,
        mode: PlayMode::Original,
        map: None,
    };
    let mut sim = Sim::new(Arc::clone(&def), options).ok()?;
    let mut inputs = Vec::with_capacity(PREVIEW_TICKS as usize);
    let mut checkpoints = Vec::new();
    for t in 0..PREVIEW_TICKS {
        let jump = t % 40 == 5;
        let frame = InputFrame {
            held: Action::Right.bit() | if jump { Action::Jump.bit() } else { 0 },
            pressed: if jump { Action::Jump.bit() } else { 0 },
            released: if t % 40 == 6 { Action::Jump.bit() } else { 0 },
        };
        let step = sim.step(frame);
        inputs.push(frame);
        if step.tick > 0
            && step.tick.is_multiple_of(CHECKPOINT_INTERVAL)
            && checkpoints.last().map(|c: &(u64, [u8; 16])| c.0) != Some(step.tick)
        {
            let mut checkpoint = [0u8; 16];
            checkpoint.copy_from_slice(&sim.state_hash()[..16]);
            checkpoints.push((step.tick, checkpoint));
        }
        if step.status != SimStatus::Running {
            break;
        }
    }
    let replay = Replay {
        header: Header {
            magic: MAGIC,
            replay_format: REPLAY_FORMAT,
            sim_version: iwbtg_sim::SIM_VERSION,
            package_id: def.id.clone(),
            gameplay_hash: hash,
            engine_rev: def.engine_rev,
            rng_seed: 0,
            start_mode: StartMode::NewGame,
            play_mode: PlayMode::Original,
            map: sim.map_id().to_owned(),
            mods: ModSet::NONE,
            client_build_id: format!("iwbtg-client {} viewer-preview", env!("CARGO_PKG_VERSION")),
        },
        trailer: Trailer {
            total_ticks: inputs.len() as u64,
            splits: Vec::new(),
            ranked_ticks: sim.ranked_ticks(),
            pauses: Vec::new(),
            checkpoints,
            final_hash: sim.state_hash(),
        },
        inputs,
    };
    let bytes = replay.encode().ok()?;
    let name = format!("replays/{}/viewer-preview.iwbr", def.id);
    store.write(&name, &bytes).ok()?;
    let mut names: Vec<String> = store
        .read_text(crate::replays::INDEX)
        .ok()
        .flatten()
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    if !names.contains(&name) {
        names.push(name.clone());
        let mut index = names.join("\n");
        index.push('\n');
        store.write_text(crate::replays::INDEX, &index).ok()?;
    }
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> crate::package::Package {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/input.pkg");
        crate::package::open(&dir, false).unwrap()
    }

    /// A ranked demo run recorded the way `replays.rs` records one: 600 ticks, right held with a
    /// jump tap every 40.
    fn recorded(pkg: &crate::package::Package) -> Replay {
        let options = SimOptions {
            start: StartPoint::NewGame { difficulty: 0 },
            seed: 7,
            dll_mode: DllMode::Model,
            mode: PlayMode::Stripped,
            map: None,
        };
        let mut sim = Sim::new(Arc::clone(&pkg.def), options).unwrap();
        let mut inputs = Vec::new();
        let mut checkpoints = Vec::new();
        for t in 0..600u64 {
            let jump = t % 40 == 5;
            let frame = InputFrame {
                held: Action::Right.bit() | if jump { Action::Jump.bit() } else { 0 },
                pressed: if jump { Action::Jump.bit() } else { 0 },
                released: if t % 40 == 6 { Action::Jump.bit() } else { 0 },
            };
            let step = sim.step(frame);
            inputs.push(frame);
            if step.tick > 0
                && step.tick.is_multiple_of(CHECKPOINT_INTERVAL)
                && checkpoints.last().map(|c: &(u64, [u8; 16])| c.0) != Some(step.tick)
            {
                let mut checkpoint = [0u8; 16];
                checkpoint.copy_from_slice(&sim.state_hash()[..16]);
                checkpoints.push((step.tick, checkpoint));
            }
        }
        let header = Header {
            magic: MAGIC,
            replay_format: REPLAY_FORMAT,
            sim_version: iwbtg_sim::SIM_VERSION,
            package_id: pkg.def.id.clone(),
            gameplay_hash: pkg.gameplay_hash,
            engine_rev: pkg.def.engine_rev,
            rng_seed: 7,
            start_mode: StartMode::NewGame,
            play_mode: PlayMode::Stripped,
            map: sim.map_id().to_owned(),
            mods: ModSet::NONE,
            client_build_id: "test".to_owned(),
        };
        Replay {
            header,
            trailer: Trailer {
                total_ticks: inputs.len() as u64,
                splits: Vec::new(),
                ranked_ticks: sim.ranked_ticks(),
                pauses: Vec::new(),
                checkpoints,
                final_hash: sim.state_hash(),
            },
            inputs,
        }
    }

    /// A viewer state on a fresh sim of `replay`, with `live` a second clone (tests never swap).
    fn viewing(replay: Replay, def: &Arc<GameDef>) -> (ViewerSim, Sim) {
        let sim = replay_sim(&replay, def).unwrap();
        let checkpoints = replay.trailer.checkpoints.iter().copied().collect();
        (
            ViewerSim {
                initial: sim.clone(),
                live: sim.clone(),
                keyframes: vec![(0, sim.clone())],
                max_keyframes: MAX_KEYFRAMES,
                checkpoints,
                replay,
            },
            sim,
        )
    }

    /// The header rebuilds the run: playing every frame again reaches the recorded final hash.
    #[test]
    fn a_replay_rebuilt_from_its_header_reaches_the_recorded_hash() {
        let pkg = demo();
        let replay = recorded(&pkg);
        let (mut viewing, mut sim) = viewing(replay.clone(), &pkg.def);
        let mut tick = 0;
        viewing
            .seek(&mut sim, &mut tick, viewing.total())
            .expect("the replay verifies");
        assert_eq!(tick, 600);
        assert_eq!(sim.state_hash(), replay.trailer.final_hash);
    }

    /// Scrubbing lands on the same state whether it arrives from the start, a keyframe or back;
    /// the keyframe cache stays bounded.
    #[test]
    fn scrubbing_from_keyframes_lands_on_the_same_state() {
        let pkg = demo();
        let replay = recorded(&pkg);
        let (mut viewing, mut sim) = viewing(replay, &pkg.def);
        let mut tick = 0;
        viewing.seek(&mut sim, &mut tick, 250).unwrap();
        let at_250 = sim.state_hash();
        viewing.seek(&mut sim, &mut tick, 520).unwrap();
        viewing.seek(&mut sim, &mut tick, 100).unwrap();
        viewing.seek(&mut sim, &mut tick, 250).unwrap();
        assert_eq!(sim.state_hash(), at_250, "a backward seek is exact");
        assert_eq!(tick, 250);
        viewing.seek(&mut sim, &mut tick, 600).unwrap();
        assert!(viewing.keyframes.len() <= MAX_KEYFRAMES);
        assert_eq!(viewing.keyframes[0].0, 0, "tick 0 always stays");
    }

    /// A checkpoint hash that does not match the re-sim stops the viewer at that tick.
    #[test]
    fn a_desynced_checkpoint_stops_the_viewer() {
        let pkg = demo();
        let mut replay = recorded(&pkg);
        replay.trailer.checkpoints[0].1 = [0xff; 16];
        let (mut viewing, mut sim) = viewing(replay, &pkg.def);
        let mut tick = 0;
        let error = viewing.seek(&mut sim, &mut tick, 600).unwrap_err();
        assert!(error.contains("does not match"), "{error}");
        assert_eq!(tick, CHECKPOINT_INTERVAL);
    }

    /// A replay for another package or another sim build is refused before anything is swapped.
    #[test]
    fn a_replay_from_another_build_is_refused() {
        let pkg = demo();
        let mut replay = recorded(&pkg);
        assert!(compatible(&replay, &pkg.def, pkg.gameplay_hash).is_ok());
        replay.header.gameplay_hash = [0; 32];
        assert!(compatible(&replay, &pkg.def, pkg.gameplay_hash).is_err());
        replay.header.gameplay_hash = pkg.gameplay_hash;
        replay.header.sim_version = iwbtg_sim::SIM_VERSION.wrapping_add(1);
        assert!(compatible(&replay, &pkg.def, pkg.gameplay_hash).is_err());
    }

    /// The drive system advances at the replay's tick rate while playing and holds still when
    /// paused.
    #[test]
    fn playback_advances_at_the_replay_tick_rate() {
        let pkg = demo();
        let replay = recorded(&pkg);
        let total = replay.inputs.len() as u64;
        let (viewing, sim) = viewing(replay, &pkg.def);
        let mut app = App::new();
        app.init_resource::<Time>()
            .insert_resource(Viewer {
                open: true,
                total,
                rate: 50,
                playing: true,
                speed: 2,
                ..Viewer::default()
            })
            .add_systems(Update, drive);
        app.world_mut()
            .insert_non_send(crate::view::Game { sim: sim.clone() });
        app.world_mut().insert_non_send(viewing);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(1));
        app.update();
        assert_eq!(app.world().resource::<Viewer>().tick, 50);
        app.world_mut().resource_mut::<Viewer>().playing = false;
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(1));
        app.update();
        assert_eq!(app.world().resource::<Viewer>().tick, 50, "paused");
    }

    /// The keyframe count shrinks with a larger world and never leaves its bounds.
    #[test]
    fn the_keyframe_count_follows_the_world_size() {
        assert_eq!(keyframe_cap(0), MAX_KEYFRAMES);
        assert_eq!(keyframe_cap(1024), MAX_KEYFRAMES);
        assert_eq!(keyframe_cap(1024 * 1024), 16);
        assert_eq!(keyframe_cap(100 * 1024 * 1024), MIN_KEYFRAMES);
    }

    /// The speed keys stay in range and the label matches the array.
    #[test]
    fn speeds_are_bounded() {
        assert_eq!(SPEEDS[2], 1.0);
        assert_eq!(format!("{}x", SPEEDS[2]), "1x");
    }
}
