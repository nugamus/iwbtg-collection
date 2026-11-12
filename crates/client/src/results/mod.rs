//! The results screen (#41; docs/product.md, Results screen; docs/architecture/client/results.md):
//! one page after a cleared run, from the artboards `Results169`, `Results169Slower`, `Results43`
//! and `Results43Slower` (docs/design/mockups/m4).
//!
//! A clear that goes on a stats board (`stats.rs`: a run started as a new game) opens it: the
//! time, its local placement and what the run changed, the frozen leaderboard (`hud/leaderboard`),
//! the player card (`card/`), the splits against the PB and the deaths per stage. While it shows
//! the sim does not tick, the game and the HUD are hidden and the backdrop takes the results dim.
//! Back (or Escape) returns to the game; Retry (click, or hold R) starts a new run as the
//! quick-retry key does; Save replay copies the run's replay to `exports/` in the data area.
//! Watch replay waits for the replay viewer (#42), Upload replay for M7.
//!
//! [`Summary`] is everything the page shows, built from the stats book ([`Summary::new`]) or from
//! the artboards' sample (`--results-preview`, [`sample`]); `page.rs` draws it.

mod page;
pub mod sample;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::ui_widgets::Activate;
use bevy::window::PrimaryWindow;
use iwbtg_format::manifest::Manifest;
use iwbtg_sim::stripped::PlayMode;

use crate::art::ArtFiles;
use crate::card::{self, Card, RunChange};
use crate::hud::Suspended;
use crate::hud::leaderboard::{self, Entry};
use crate::stats::{Board, Cleared, Recorded, Run, Stats, fmt_ticks};
use crate::storage::Store;
use crate::ui::backdrop::{Backdrop, dim};
use crate::ui::tokens;
use crate::view::{Game, GameCamera, SimTick};

pub use page::Size;

/// How long R is held to retry, in seconds.
pub const HOLD_TO_RETRY: f32 = 0.5;
/// The key held to retry.
pub const RETRY_KEY: KeyCode = KeyCode::KeyR;

/// The artboards' two states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Scenario {
    /// `Results169`, `Results43`: a new PB.
    Pb,
    /// `Results169Slower`, `Results43Slower`: slower than the PB.
    Slower,
}

/// The results' command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug, Clone, Copy, Default)]
pub struct ResultsArgs {
    /// Screenshot checks: open the results at once with the artboards' sample run (pb or slower).
    #[arg(long, hide = true, value_name = "SCENARIO")]
    pub results_preview: Option<Scenario>,
}

/// One segment of the run (a row of the splits table, a bar of the deaths chart).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stage {
    pub name: String,
    /// The segment's own length and the run's time at its end, in ticks.
    pub segment: u64,
    pub split: u64,
    /// This split minus the PB's (negative: ahead); `None` without a PB to compare with.
    pub delta: Option<i64>,
    /// The best this segment has ever been.
    pub gold: bool,
    pub deaths: u32,
}

/// What the header says and how the run counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct About {
    /// The game's title and the line beside it (map, mode, mods).
    pub title: String,
    pub subtitle: String,
    pub rate: u32,
    /// Ranked: not Play original, no mod that keeps it off the boards.
    pub ranked: bool,
}

/// Everything the results page shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    pub about: About,
    /// When the run finished, seconds since 1970 (UTC).
    pub finished_at: u64,
    pub ticks: u64,
    pub deaths: u32,
    /// The PB before this run; `None` on a first clear.
    pub pb_before: Option<u64>,
    pub is_pb: bool,
    /// This run's place among your runs on the board, from 1, and how many runs that is.
    pub place: usize,
    pub runs: usize,
    pub stages: Vec<Stage>,
    pub sum_of_best: Option<u64>,
    /// The other runs on the board, for the frozen leaderboard.
    pub others: Vec<Entry>,
    pub card: Card,
    /// The run's replay in the data area.
    pub replay: Option<String>,
}

impl Summary {
    /// `run`, which `board.add` recorded as `rec` (the board already holds it).
    pub fn new(about: About, run: &Run, rec: &Recorded, board: &Board, card: Card) -> Summary {
        let mut others = board.runs.clone();
        match rec.place {
            Some(p) if p <= others.len() => {
                others.remove(p - 1);
            }
            _ => {}
        }
        let others = leaderboard::from_runs(&others, run.splits.len());
        let place = leaderboard::place(&others, run.ticks);
        let pb = &rec.pb_splits_before;
        let n = run.splits.len();
        let stages = run
            .segments()
            .into_iter()
            .enumerate()
            .map(|(i, segment)| Stage {
                name: stage_name(i, n),
                segment,
                split: run.splits[i],
                delta: (pb.len() == n).then(|| run.splits[i] as i64 - pb[i] as i64),
                gold: rec.gold.get(i).copied().unwrap_or(false),
                deaths: run.segment_deaths.get(i).copied().unwrap_or(0),
            })
            .collect();
        Summary {
            about,
            finished_at: run.finished_at,
            ticks: run.ticks,
            deaths: run.deaths,
            pb_before: rec.pb_before,
            is_pb: rec.is_pb,
            place,
            runs: others.len() + 1,
            stages,
            sum_of_best: rec.sum_of_best,
            others,
            card,
            replay: run.replay.clone(),
        }
    }

    /// The final time as the hero shows it: `38:12` and `.46`.
    pub fn hero(&self) -> (String, String) {
        let full = fmt_ticks(self.ticks, self.about.rate);
        match full.rfind('.') {
            Some(dot) => (full[..dot].to_owned(), full[dot..].to_owned()),
            None => (full, String::new()),
        }
    }

    /// `Cleared 3 Nov 2026, 21:14` (UTC, like the leaderboard's dates).
    pub fn cleared(&self) -> String {
        let s = self.finished_at;
        format!(
            "Cleared {}, {:02}:{:02}",
            leaderboard::date(s),
            s / 3600 % 24,
            s / 60 % 60
        )
    }

    /// The placement beside the time: `#1` in white for a PB, else in the soft colour; `None`
    /// for an unranked run (the Unranked pill instead).
    pub fn placement(&self) -> Option<(String, Color)> {
        self.about.ranked.then(|| {
            let color = if self.is_pb { tokens::INK } else { tokens::SOFT };
            (format!("#{}", self.place), color)
        })
    }

    /// The rank change under the time: the local placement (ranked only) and the PB.
    pub fn changes(&self) -> Vec<Change> {
        let t = |ticks| fmt_ticks(ticks, self.about.rate);
        let delta = |d: i64| leaderboard::fmt_delta(d, self.about.rate);
        let mut out = Vec::new();
        if self.about.ranked {
            let unit = if self.runs == 1 { "run" } else { "runs" };
            out.push(Change {
                label: "Local placement",
                value: format!("#{}", self.place),
                note: format!("of {} {unit}", self.runs),
                color: tokens::MUTED,
            });
        }
        let now = self.ticks as i64;
        out.push(match self.pb_before {
            None => Change {
                label: "Personal best",
                value: t(self.ticks),
                note: "First clear".to_owned(),
                color: tokens::AHEAD,
            },
            Some(before) if self.is_pb => Change {
                label: "Personal best",
                value: format!("{} → {}", t(before), t(self.ticks)),
                note: delta(now - before as i64),
                color: tokens::AHEAD,
            },
            Some(pb) => Change {
                label: "Personal best",
                value: format!("{} stands", t(pb)),
                note: format!("{} this run", delta(now - pb as i64)),
                color: tokens::BEHIND,
            },
        });
        out
    }

    /// The time the run could have taken with its best segments: the run minus the sum of best.
    pub fn time_save(&self) -> Option<u64> {
        self.sum_of_best.map(|s| self.ticks.saturating_sub(s))
    }
}

/// One item of the rank change: label, value, the note under it and its colour.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub label: &'static str,
    pub value: String,
    pub note: String,
    pub color: Color,
}

/// Segment `i` of `n`: `Stage 1`, `Stage 2`, ...; a game without stages has one, `Run`.
pub fn stage_name(i: usize, n: usize) -> String {
    if n <= 1 {
        "Run".to_owned()
    } else {
        format!("Stage {}", i + 1)
    }
}

/// The bar label of a stage: `Stage 3` as `3`.
pub fn bar_label(name: &str) -> &str {
    name.strip_prefix("Stage ").unwrap_or(name)
}

/// A button on the page.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Back,
    Retry,
    SaveReplay,
}

/// The results' state. `summary` is `Some` while the page shows.
#[derive(Resource, Default)]
pub struct Results {
    summary: Option<Summary>,
    /// Bumped whenever what the page shows changes, so it is drawn again.
    generation: u32,
    /// The clear last shown, so each clear opens the page once.
    seen: Option<Cleared>,
    preview: Option<Scenario>,
    /// Seconds R has been held.
    held: f32,
    picked: Option<Action>,
    /// The replay was saved (the button says so).
    saved: bool,
}

/// Whether the results page is closed (a run condition: the sim ticks, the pause key works).
pub fn closed(results: Option<Res<Results>>) -> bool {
    results.is_none_or(|r| r.summary.is_none())
}

/// A retry (`retry.rs`) starts a new run: the page closes.
pub(crate) fn reset(world: &mut World) {
    if let Some(mut r) = world.get_resource_mut::<Results>() {
        r.summary = None;
        r.held = 0.0;
        r.picked = None;
    }
}

pub struct ResultsPlugin(pub ResultsArgs);

impl Plugin for ResultsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Results {
            preview: self.0.results_preview,
            ..default()
        })
        .configure_sets(FixedUpdate, SimTick.run_if(closed))
        .add_observer(on_activate)
        .add_systems(
            Update,
            (open, keys, act, show).chain().after(crate::screen::apply),
        );
    }
}

/// The header's title: the package's `title`, else the game's id.
fn game_title(files: Option<&ArtFiles>, id: &str) -> String {
    files
        .and_then(|f| f.0.get("game.toml"))
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|t| Manifest::parse(t).ok())
        .map(|m| m.title.into_inner())
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| id.to_owned())
}

/// Opens the page for a new clear on a board (or the preview at once).
fn open(
    game: NonSend<Game>,
    stats: Option<Res<Stats>>,
    files: Option<Res<ArtFiles>>,
    mods: Option<Res<crate::mods::RunMods>>,
    mut results: ResMut<Results>,
) {
    if results.summary.is_some() {
        return;
    }
    if let Some(s) = results.preview.take() {
        show_summary(&mut results, sample::summary(s));
        return;
    }
    let Some(stats) = stats else {
        return;
    };
    let (Some(cleared), Some(rec)) = (&stats.cleared, &stats.last) else {
        return;
    };
    if results.seen.as_ref() == Some(cleared) {
        return;
    }
    results.seen = Some(cleared.clone());
    let Some(board) = stats
        .book
        .map(&cleared.map)
        .and_then(|m| m.boards.get(&cleared.board))
    else {
        return;
    };
    let sim = &game.sim;
    let def = sim.def();
    let mods = mods.map_or(iwbtg_sim::mods::ModSet::NONE, |m| m.0);
    let original = sim.options().mode == PlayMode::Original;
    let title = game_title(files.as_deref(), &def.id);
    let mut subtitle = Vec::new();
    if def.maps.len() > 1 {
        subtitle.push(sim.map_id().to_owned());
    }
    subtitle.push(if original { "Play original" } else { "Ranked" }.to_owned());
    if mods != iwbtg_sim::mods::ModSet::NONE {
        subtitle.push(mods.codes());
    }
    let about = About {
        title: title.clone(),
        subtitle: subtitle.join(" · "),
        rate: def.tick_rate,
        ranked: !original && mods.unranked_by().is_none(),
    };
    let run = &cleared.run;
    let card = Card {
        run: Some(RunChange::new(
            &stats.book,
            &def.id,
            &title,
            def.maps.len().max(1),
            run,
            def.tick_rate,
        )),
        ..Card::from_book(&stats.book, &card::profile_name(), card::COLLECTION_GAMES)
    };
    let summary = Summary::new(about, run, rec, board, card);
    show_summary(&mut results, summary);
}

fn show_summary(results: &mut Results, summary: Summary) {
    results.summary = Some(summary);
    results.generation = results.generation.wrapping_add(1);
    results.saved = false;
    results.held = 0.0;
}

/// A click (or Enter on a focused button) picks its action.
fn on_activate(activate: On<Activate>, actions: Query<&Action>, mut results: ResMut<Results>) {
    if let Ok(action) = actions.get(activate.entity)
        && results.summary.is_some()
    {
        results.picked = Some(*action);
    }
}

/// Escape goes back; holding R retries (the Retry button shows pressed while it is held).
fn keys(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut results: ResMut<Results>,
    buttons: Query<(Entity, &Action, Has<Pressed>)>,
) {
    if results.summary.is_none() {
        results.held = 0.0;
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        results.picked = Some(Action::Back);
    }
    let holding = keys.pressed(RETRY_KEY);
    results.held = if holding {
        results.held + time.delta_secs()
    } else {
        0.0
    };
    if results.held >= HOLD_TO_RETRY {
        results.held = 0.0;
        results.picked = Some(Action::Retry);
    }
    for (entity, action, pressed) in &buttons {
        if *action == Action::Retry && pressed != holding {
            if holding {
                commands.entity(entity).insert(Pressed);
            } else {
                commands.entity(entity).remove::<Pressed>();
            }
        }
    }
}

/// Does what was picked.
fn act(world: &mut World) {
    let Some(action) = world.resource_mut::<Results>().picked.take() else {
        return;
    };
    match action {
        Action::Back => {
            world.resource_mut::<Results>().summary = None;
            // Keys pressed on the page never reach the game.
            if let Some(mut c) = world.get_resource_mut::<crate::input::Collector>() {
                c.take_frame();
            }
        }
        Action::Retry => crate::retry::request(world),
        Action::SaveReplay => {
            let mut results = world.resource_mut::<Results>();
            let Some(name) = results.summary.as_ref().and_then(|s| s.replay.clone()) else {
                return;
            };
            match save_replay(&name) {
                Ok(to) => {
                    eprintln!("results: replay saved to {to}");
                    results.saved = true;
                    results.generation = results.generation.wrapping_add(1);
                }
                Err(e) => crate::storage::warn(&format!("results: replay not saved: {e}")),
            }
        }
    }
}

/// Copies the replay `name` (in the data area) to `exports/`, where it is kept whatever the
/// replay pruning does. Returns where it went.
fn save_replay(name: &str) -> Result<String, String> {
    let store = Store::data().ok_or("there is no data folder")?;
    let bytes = store
        .read(name)?
        .ok_or_else(|| format!("{} is missing", store.describe(name)))?;
    let to = export_name(name);
    store.write(&to, &bytes)?;
    Ok(store.describe(&to))
}

/// `replays/kaenbyou/1793740440-0.iwbr` saves as `exports/kaenbyou-1793740440-0.iwbr`.
fn export_name(replay: &str) -> String {
    let rest = replay.strip_prefix("replays/").unwrap_or(replay);
    format!("exports/{}", rest.replace('/', "-"))
}

/// The page's root and what it was drawn for.
#[derive(Component)]
struct Root {
    size: Size,
    generation: u32,
}

/// The camera the page draws on: UI only, over everything.
#[derive(Component)]
struct ResultsCamera;

type RootParts = (Entity, &'static Root, &'static mut Node);

/// Shows or hides the page: the game camera, the HUD, the backdrop's dim, the UI scale (the
/// page is laid out at its artboard's size and scaled to fit the window), and draws it again
/// when its size class or content changed.
#[allow(clippy::too_many_arguments, reason = "one pass over what the page covers")]
fn show(
    mut commands: Commands,
    results: Res<Results>,
    fruit: Option<Res<crate::pause::Fruit>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut ui_scale: ResMut<UiScale>,
    mut roots: Query<RootParts>,
    cameras: Query<Entity, With<ResultsCamera>>,
    mut game_cameras: Query<&mut Camera, (With<GameCamera>, Without<ResultsCamera>)>,
    mut suspended: ResMut<Suspended>,
    mut backdrop: ResMut<Backdrop>,
) {
    let open = results.summary.is_some();
    if suspended.0 != open {
        suspended.0 = open;
    }
    for mut camera in &mut game_cameras {
        if camera.is_active == open {
            camera.is_active = !open;
        }
    }
    let Some(summary) = &results.summary else {
        for (entity, _, _) in &roots {
            commands.entity(entity).despawn();
        }
        for entity in &cameras {
            commands.entity(entity).despawn();
        }
        if backdrop.dim == dim::RESULTS {
            backdrop.dim = dim::GAMEPLAY;
        }
        return;
    };
    if backdrop.dim != dim::RESULTS {
        backdrop.dim = dim::RESULTS;
    }
    let win = window.size();
    if win.x < 1.0 || win.y < 1.0 {
        return;
    }
    let size = Size::for_window(win);
    let design = size.design();
    let scale = (win.x / design.x).min(win.y / design.y);
    if (ui_scale.0 - scale).abs() > 1e-4 {
        ui_scale.0 = scale;
    }
    let (w, h) = (Val::Px(win.x / scale), Val::Px(win.y / scale));
    let mut current = false;
    for (entity, root, mut node) in &mut roots {
        if root.size == size && root.generation == results.generation {
            current = true;
            if (node.width, node.height) != (w, h) {
                (node.width, node.height) = (w, h);
            }
        } else {
            commands.entity(entity).despawn();
        }
    }
    if current {
        return;
    }
    let camera = match cameras.iter().next() {
        Some(c) => c,
        None => commands
            .spawn((
                Camera2d,
                Camera {
                    order: 20,
                    clear_color: ClearColorConfig::None,
                    ..default()
                },
                RenderLayers::none(),
                ResultsCamera,
            ))
            .id(),
    };
    let look = page::Look {
        size,
        fruit: fruit.map(|f| f.0.clone()),
        saved: results.saved,
    };
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: w,
            height: h,
            ..default()
        },
        Root {
            size,
            generation: results.generation,
        },
        UiTargetCamera(camera),
        children![page::page(summary, &look)],
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(splits: &[u64], deaths: &[u32]) -> Run {
        Run {
            ticks: *splits.last().unwrap(),
            deaths: deaths.iter().sum(),
            splits: splits.to_vec(),
            segment_deaths: deaths.to_vec(),
            finished_at: 1_793_740_440,
            replay: None,
            death_log: Default::default(),
        }
    }

    fn about(ranked: bool) -> About {
        About {
            title: "Demo".into(),
            subtitle: "Ranked".into(),
            rate: 50,
            ranked,
        }
    }

    fn summary_after(board: &mut Board, r: &Run, ranked: bool) -> Summary {
        let rec = board.add(r.clone());
        Summary::new(about(ranked), r, &rec, board, crate::card::preview::sample())
    }

    /// The issue's user check: a run, then a faster one. The second is a new PB at #1, its
    /// splits ahead (negative) of the first, gold where it beat its best.
    #[test]
    fn a_faster_second_clear_is_a_new_pb_with_gold_segments() {
        let mut board = Board::default();
        let first = summary_after(&mut board, &run(&[100, 250, 400], &[1, 2, 3]), true);
        assert!(first.is_pb);
        assert_eq!(first.pb_before, None);
        assert_eq!((first.place, first.runs), (1, 1));
        assert!(first.others.is_empty(), "the run itself is not a rival");
        assert!(first.stages.iter().all(|s| s.gold && s.delta.is_none()));
        assert_eq!(first.changes()[1].note, "First clear");

        // 90, 170, 120: faster overall, the middle segment slower.
        let second = summary_after(&mut board, &run(&[90, 260, 380], &[0, 4, 0]), true);
        assert!(second.is_pb);
        assert_eq!((second.place, second.runs), (1, 2));
        assert_eq!(second.others.len(), 1);
        assert_eq!(second.others[0].ticks, 400);
        let golds: Vec<bool> = second.stages.iter().map(|s| s.gold).collect();
        assert_eq!(golds, [true, false, true]);
        let deltas: Vec<Option<i64>> = second.stages.iter().map(|s| s.delta).collect();
        assert_eq!(deltas, [Some(-10), Some(10), Some(-20)]);
        assert_eq!(second.stages[1].segment, 170);
        assert_eq!(second.stages[1].deaths, 4);
        assert_eq!(
            second.stages.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["Stage 1", "Stage 2", "Stage 3"]
        );
        let c = second.changes();
        assert_eq!((c[0].value.as_str(), c[0].note.as_str()), ("#1", "of 2 runs"));
        assert_eq!(c[1].value, "0:08.00 → 0:07.60");
        assert_eq!((c[1].note.as_str(), c[1].color), ("−0.40", tokens::AHEAD));
        // Best segments 90, 150, 120.
        assert_eq!(second.sum_of_best, Some(360));
        assert_eq!(second.time_save(), Some(20));
    }

    #[test]
    fn a_slower_clear_is_not_a_pb_and_says_how_far_behind() {
        let mut board = Board::default();
        summary_after(&mut board, &run(&[100, 200], &[0, 0]), true);
        let s = summary_after(&mut board, &run(&[80, 260], &[0, 0]), true);
        assert!(!s.is_pb);
        assert_eq!((s.place, s.runs), (2, 2));
        assert_eq!(s.placement(), Some(("#2".to_owned(), tokens::SOFT)));
        let c = &s.changes()[1];
        assert_eq!(c.value, "0:04.00 stands");
        assert_eq!((c.note.as_str(), c.color), ("+1.20 this run", tokens::BEHIND));
        assert_eq!(s.stages[0].delta, Some(-20));
        assert!(s.stages[0].gold);
    }

    #[test]
    fn an_unranked_clear_shows_no_placement() {
        let mut board = Board::default();
        let s = summary_after(&mut board, &run(&[100], &[2]), false);
        assert_eq!(s.placement(), None);
        let c = s.changes();
        assert_eq!(c.len(), 1, "only the PB");
        assert_eq!(c[0].label, "Personal best");
        assert_eq!(s.stages[0].name, "Run");
    }

    #[test]
    fn the_header_and_hero_read_as_the_artboard_writes_them() {
        let mut board = Board::default();
        let s = summary_after(&mut board, &run(&[50 * 2292 + 23], &[0]), true);
        assert_eq!(s.hero(), ("38:12".to_owned(), ".46".to_owned()));
        assert_eq!(s.cleared(), "Cleared 3 Nov 2026, 21:14");
        assert_eq!(bar_label("Stage 6"), "6");
        assert_eq!(bar_label("Boss"), "Boss");
        assert_eq!(
            export_name("replays/kaenbyou/1793740440-0.iwbr"),
            "exports/kaenbyou-1793740440-0.iwbr"
        );
    }

    /// Escape closes the page; holding R for the hold time retries, a short tap does not.
    #[test]
    fn escape_goes_back_and_holding_r_retries() {
        use std::time::Duration;
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Time>()
            .insert_resource(Results::default())
            .add_systems(Update, keys);
        let mut board = Board::default();
        let s = summary_after(&mut board, &run(&[100], &[0]), true);
        show_summary(&mut app.world_mut().resource_mut::<Results>(), s);
        let step = |app: &mut App, held: bool, secs: f32| {
            {
                let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
                input.clear();
                if held {
                    input.press(RETRY_KEY);
                } else {
                    input.release(RETRY_KEY);
                }
            }
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs_f32(secs));
            app.update();
            app.world_mut().resource_mut::<Results>().picked.take()
        };
        assert_eq!(step(&mut app, true, 0.2), None);
        assert_eq!(step(&mut app, false, 0.2), None, "let go: the hold starts over");
        assert_eq!(step(&mut app, true, 0.3), None);
        assert_eq!(step(&mut app, true, 0.3), Some(Action::Retry));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert_eq!(app.world().resource::<Results>().picked, Some(Action::Back));
    }
}
