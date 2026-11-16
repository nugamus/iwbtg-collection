//! The loading card (#495; docs/product.md, Game select and loading;
//! docs/architecture/client/loading.md): between game select and the run, from the artboards
//! `Loading.dc.html` and `LoadingUnranked.dc.html` (docs/design/mockups/m4).
//!
//! It shows when the client opens a package: title, creator, map, your PB, the stage count and
//! the mods, with the amber Unranked pill and the reason when the run will not reach a board.
//! While it shows the sim does not tick, the game and the HUD are hidden and the backdrop takes
//! the loading dim. It stays at least [`MIN_SECONDS`] and then starts the run by itself; Escape
//! (or Start) goes back to the select list, else quits (the card says which). The package is already loaded when the
//! window opens, so the progress line fills over the minimum time.
//!
//! [`Info`] is what the card shows ([`Info::new`]); `page.rs` draws it.

mod page;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use iwbtg_format::manifest::{CreatorRole, Manifest};
use iwbtg_sim::mods::{Mod, ModKind, ModSet};
use iwbtg_sim::stripped::PlayMode;

use crate::art::ArtFiles;
use crate::hud::Suspended;
use crate::shell::{self, InRun, RUN_ONLY};
use crate::stats::{Book, Stats, fmt_ticks, map_key};
use crate::ui::backdrop::{Backdrop, dim};
use crate::ui::motion::ReducedMotion;
use crate::view::{ClockSpeed, Game, GameCamera, SimTick};

/// The card shows at least this long, seconds.
pub const MIN_SECONDS: f32 = 1.5;
/// The fruit shows each of its two frames this long (`ld-cherry 640ms steps(2)`).
const FRUIT_FRAME_SECONDS: f32 = 0.32;
/// The artboard's size: the page is laid out at its height and scaled to fit the window.
const DESIGN: Vec2 = Vec2::new(1920.0, 1080.0);
/// The narrowest the page gets before it scales by width instead (the card and some air).
const MIN_DESIGN_WIDTH: f32 = 1000.0;
/// `--loading-preview` holds the progress line where the artboard draws it.
const PREVIEW_PROGRESS: f32 = 0.64;
/// The artboard's sample PB (`38:12.46`), at 50 ticks a second.
const SAMPLE_PB: u64 = 50 * 2292 + 23;

/// The loading card's command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug, Clone, Copy, Default)]
pub struct LoadingArgs {
    /// Screenshot checks: keep the loading card up with the artboard's sample PB.
    #[arg(long, hide = true)]
    pub loading_preview: bool,
    /// With `--loading-preview`: draw the hint as when started with `--select` (`Esc back`), which
    /// a screenshot check cannot start.
    #[arg(long, hide = true, requires = "loading_preview")]
    pub loading_preview_back: bool,
}

/// `--loading-preview-back`: the card says `Esc back` without a select screen.
#[derive(Resource)]
struct PreviewBack;

/// One line of a tip: words, and whether they are a key (drawn bold).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tip(pub &'static [(&'static str, bool)]);

/// The tips the card picks from. Only keys that cannot be rebound, so the words stay true.
pub const TIPS: [Tip; 3] = [
    Tip(&[
        ("hold ", false),
        ("R", true),
        (" on the results screen to retry straight away.", false),
    ]),
    Tip(&[
        ("", false),
        ("F11", true),
        (" switches between a window and full screen.", false),
    ]),
    Tip(&[
        ("", false),
        ("[", true),
        (" and ", false),
        ("]", true),
        (" blur the background less and more.", false),
    ]),
];

/// Whether the run reaches a board, in words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Standing {
    /// `Sudden Death · ranked`.
    Ranked(String),
    /// The reason: `Slow motion 75% is a practice assist, so this run won't reach the leaderboard.`
    Unranked(String),
}

/// Everything the card shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    pub title: String,
    /// The authors, joined; `None` hides the `by` line.
    pub creator: Option<String>,
    /// The map's name, or the play mode for a package with one unnamed map.
    pub map: String,
    /// Your PB on this map's board for this mode and these mods.
    pub pb: Option<String>,
    pub stages: usize,
    /// The mods' codes, in table order.
    pub mods: Vec<&'static str>,
    pub standing: Standing,
}

/// What the run is: its mode, mods and slow motion speed (percent).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunKind {
    pub original: bool,
    pub mods: ModSet,
    pub slow_speed: u8,
}

impl Info {
    /// The card for a run of map `map_id` of game `id` (`manifest` is the package's, when it
    /// reads), with your PB from `book`.
    pub fn new(
        manifest: Option<&Manifest>,
        id: &str,
        map_id: &str,
        run: RunKind,
        book: &Book,
        rate: u32,
    ) -> Info {
        let title = manifest
            .map(|m| m.title.get_ref().clone())
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| id.to_owned());
        let creator = manifest.and_then(authors);
        let mode = if run.original {
            "Play original"
        } else {
            "Ranked mode"
        };
        let (map, stages) = manifest
            .and_then(|m| {
                let named = !m.maps.is_empty();
                m.map_list().into_iter().find(|v| v.id == map_id).map(|v| {
                    let name = if named { v.name.to_owned() } else { mode.to_owned() };
                    (name, v.stages.len() + 1)
                })
            })
            .unwrap_or_else(|| (mode.to_owned(), 1));
        let pb = book
            .board(&map_key(id, map_id), !run.original, &run.mods.codes())
            .and_then(|b| b.pb())
            .map(|r| fmt_ticks(r.ticks, rate));
        Info {
            title,
            creator,
            map,
            pb,
            stages,
            mods: run.mods.iter().map(|d| d.code).collect(),
            standing: standing(run),
        }
    }
}

/// The package's authors (`author` and `co-author`), joined with `, `.
fn authors(m: &Manifest) -> Option<String> {
    let names: Vec<&str> = m
        .creators
        .get_ref()
        .iter()
        .filter(|c| matches!(c.role, CreatorRole::Author | CreatorRole::CoAuthor))
        .map(|c| c.name.get_ref().as_str())
        .collect();
    (!names.is_empty()).then(|| names.join(", "))
}

/// Ranked with its mods named, or unranked with the reason: Play original first, then the first
/// mod that keeps the run off the boards.
pub fn standing(run: RunKind) -> Standing {
    const OFF: &str = "so this run won’t reach the leaderboard.";
    if run.original {
        return Standing::Unranked(format!("Play original keeps your own stats only, {OFF}"));
    }
    if let Some(d) = run.mods.unranked_by() {
        let speed = if d.id == Mod::SlowMotion {
            format!(" {}%", run.slow_speed)
        } else {
            String::new()
        };
        let what = match d.kind {
            ModKind::Practice => "a practice assist",
            _ => "not ranked",
        };
        return Standing::Unranked(format!("{}{speed} is {what}, {OFF}", d.name));
    }
    let names: Vec<&str> = run.mods.iter().map(|d| d.name).collect();
    if names.is_empty() {
        Standing::Ranked("No mods · ranked".to_owned())
    } else {
        Standing::Ranked(format!("{} · ranked", names.join(" · ")))
    }
}

/// The card's state.
#[derive(Resource)]
pub struct Loading {
    open: bool,
    /// Seconds shown.
    shown: f32,
    /// `--loading-preview`: never starts the run, shows the sample PB.
    preview: bool,
    tip: usize,
    info: Option<Info>,
    /// Whether the page was up last frame (`show` suspends the game on the change).
    was_open: bool,
}

impl Loading {
    fn new(open: bool, preview: bool, tip: usize) -> Loading {
        Loading {
            open,
            shown: 0.0,
            preview,
            tip,
            info: None,
            was_open: false,
        }
    }

    /// How far the progress line is filled, 0 to 1.
    fn progress(&self) -> f32 {
        if self.preview {
            PREVIEW_PROGRESS
        } else {
            (self.shown / MIN_SECONDS).clamp(0.0, 1.0)
        }
    }
}

/// Whether the card is gone (a run condition: the sim ticks, the pause key works).
pub fn closed(loading: Option<Res<Loading>>) -> bool {
    loading.is_none_or(|l| !l.open)
}

/// Adds the card. `skip` (a `--screenshot` check without `--loading-preview`) starts the run at
/// once, so the other screens' checks see the game.
pub struct LoadingPlugin {
    pub args: LoadingArgs,
    pub skip: bool,
}

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        let preview = self.args.loading_preview;
        let skip = self.skip;
        if self.args.loading_preview_back {
            app.insert_resource(PreviewBack);
        }
        // Every run opens with the card (and a new tip).
        shell::run_resource(app, move |_| {
            let open = preview || !skip;
            let tip = if preview || skip {
                0
            } else {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs() as usize % TIPS.len())
            };
            Loading::new(open, preview, tip)
        });
        app.configure_sets(FixedUpdate, SimTick.run_if(closed))
            .add_systems(
                Update,
                (prepare, keys, advance, show, animate)
                    .chain()
                    .after(crate::screen::apply)
                    .in_set(InRun),
            );
    }
}

/// Reads what the card shows, once.
fn prepare(
    game: NonSend<Game>,
    files: Option<Res<ArtFiles>>,
    stats: Option<Res<Stats>>,
    mods: Option<Res<crate::mods::RunMods>>,
    speed: Option<Res<ClockSpeed>>,
    mut loading: ResMut<Loading>,
) {
    if !loading.open || loading.info.is_some() {
        return;
    }
    let sim = &game.sim;
    let def = sim.def();
    let manifest = files
        .as_ref()
        .and_then(|f| f.0.get("game.toml"))
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|t| Manifest::parse(t).ok());
    let mods = mods.map_or(ModSet::NONE, |m| m.0);
    let run = RunKind {
        original: sim.options().mode == PlayMode::Original,
        mods,
        slow_speed: speed.map_or(100, |s| (s.0 * 100.0).round() as u8),
    };
    let empty = Book::default();
    let book = stats.as_ref().map_or(&empty, |s| &s.book);
    let mut info = Info::new(manifest.as_ref(), &def.id, sim.map_id(), run, book, def.tick_rate);
    if loading.preview {
        info.pb = Some(fmt_ticks(SAMPLE_PB, 50));
    }
    loading.info = Some(info);
}

/// Escape (or Start) leaves the run: back to the select screen, or quits when the client was
/// started on one package (`shell::leave`).
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    collector: Option<Res<crate::input::Collector>>,
    loading: Res<Loading>,
    mut commands: Commands,
    mut pad_before: Local<bool>,
) {
    let pad = collector.is_some_and(|c| c.pad_down(crate::pause::PAUSE_PAD));
    let pad_pressed = pad && !*pad_before;
    *pad_before = pad;
    if loading.open && (keys.just_pressed(crate::pause::PAUSE_KEY) || pad_pressed) {
        commands.queue(crate::shell::leave);
    }
}

/// Counts the time shown and starts the run once the minimum has passed.
fn advance(
    time: Res<Time>,
    mut loading: ResMut<Loading>,
    collector: Option<ResMut<crate::input::Collector>>,
) {
    if !loading.open {
        return;
    }
    loading.shown += time.delta_secs();
    if loading.preview || loading.shown < MIN_SECONDS {
        return;
    }
    loading.open = false;
    eprintln!("loading: run started after {:.1} s", loading.shown);
    // Keys pressed on the card never reach the game.
    if let Some(mut c) = collector {
        c.take_frame();
        c.take_quick_retry();
        c.take_skip();
        c.take_advance();
    }
}

/// The page's root and the key art it was drawn with.
#[derive(Component)]
struct Root {
    art: Option<Handle<Image>>,
}

/// The camera the page draws on: UI only, over everything.
#[derive(Component)]
struct LoadingCamera;

/// Shows or hides the page: the game camera, the HUD, the backdrop's dim and the UI scale, and
/// draws it again when the key art arrives.
#[allow(clippy::too_many_arguments, reason = "one pass over what the page covers")]
fn show(
    mut commands: Commands,
    mut loading: ResMut<Loading>,
    fruit: Option<Res<crate::pause::Fruit>>,
    shell: Option<Res<shell::Shell>>,
    preview_back: Option<Res<PreviewBack>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut ui_scale: ResMut<UiScale>,
    mut roots: Query<(Entity, &Root, &mut Node)>,
    cameras: Query<Entity, With<LoadingCamera>>,
    mut game_cameras: Query<&mut Camera, (With<GameCamera>, Without<LoadingCamera>)>,
    mut suspended: ResMut<Suspended>,
    mut backdrop: ResMut<Backdrop>,
) {
    let open = loading.open && loading.info.is_some();
    if loading.was_open != open {
        loading.was_open = open;
        suspended.0 = open;
        for mut camera in &mut game_cameras {
            camera.is_active = !open;
        }
    }
    let Some(info) = loading.info.as_ref().filter(|_| open) else {
        for (entity, _, _) in &roots {
            commands.entity(entity).despawn();
        }
        for entity in &cameras {
            commands.entity(entity).despawn();
        }
        if backdrop.dim == dim::LOADING {
            backdrop.dim = dim::GAMEPLAY;
        }
        return;
    };
    if backdrop.dim != dim::LOADING {
        backdrop.dim = dim::LOADING;
    }
    let win = window.size();
    if win.x < 1.0 || win.y < 1.0 {
        return;
    }
    let scale = (win.y / DESIGN.y).min(win.x / MIN_DESIGN_WIDTH);
    if (ui_scale.0 - scale).abs() > 1e-4 {
        ui_scale.0 = scale;
    }
    let (w, h) = (Val::Px(win.x / scale), Val::Px(win.y / scale));
    let mut current = false;
    for (entity, root, mut node) in &mut roots {
        if root.art == backdrop.art {
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
                LoadingCamera,
                RUN_ONLY,
            ))
            .id(),
    };
    let look = page::Look {
        art: backdrop.art.clone(),
        fruit: fruit.map(|f| f.0.clone()),
        back: preview_back.is_some() || shell.is_some_and(|s| s.goes_back()),
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
            art: backdrop.art.clone(),
        },
        UiTargetCamera(camera),
        RUN_ONLY,
        children![page::page(info, &TIPS[loading.tip % TIPS.len()], &look)],
    ));
}

/// Steps the fruit between its two frames (held on the first with reduced motion) and fills the
/// progress line.
fn animate(
    loading: Res<Loading>,
    reduced: Option<Res<ReducedMotion>>,
    mut fruits: Query<&mut ImageNode, With<page::FruitFrame>>,
    mut fills: Query<&mut Node, With<page::Progress>>,
) {
    let still = reduced.is_some_and(|r| r.0);
    let frame = if still {
        0
    } else {
        (loading.shown / FRUIT_FRAME_SECONDS) as u32 % 2
    };
    let size = crate::pause::FRUIT;
    let x = (frame * size.x) as f32;
    let want = Some(Rect::new(x, 0.0, x + size.x as f32, size.y as f32));
    for mut image in &mut fruits {
        if image.rect != want {
            image.rect = want;
        }
    }
    let width = Val::Percent(loading.progress() * 100.0);
    for mut node in &mut fills {
        if node.width != width {
            node.width = width;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn run(original: bool, codes: &str) -> RunKind {
        RunKind {
            original,
            mods: ModSet::parse(codes).unwrap(),
            slow_speed: 75,
        }
    }

    /// The artboards' two lines, and the other ways a run is ranked or not.
    #[test]
    fn the_mods_line_says_whether_the_run_is_ranked_and_why() {
        assert_eq!(
            standing(run(false, "SL")),
            Standing::Unranked(
                "Slow motion 75% is a practice assist, so this run won’t reach the leaderboard.".into()
            )
        );
        assert_eq!(
            standing(run(false, "SD")),
            Standing::Ranked("Sudden Death · ranked".into())
        );
        assert_eq!(
            standing(run(false, "")),
            Standing::Ranked("No mods · ranked".into())
        );
        assert_eq!(
            standing(run(false, "SD+NB")),
            Standing::Ranked("Sudden Death · No background · ranked".into())
        );
        assert_eq!(
            standing(run(false, "SD,FA")),
            Standing::Unranked(
                "Frame advance is a practice assist, so this run won’t reach the leaderboard.".into()
            )
        );
        let Standing::Unranked(why) = standing(run(true, "SD")) else {
            panic!("Play original is never ranked");
        };
        assert!(why.starts_with("Play original"), "{why}");
    }

    /// The clear fixture: its title and author, the play mode for its one map, a stage, no PB
    /// until a clear on this board, and the PB once there is one.
    #[test]
    fn the_card_reads_the_package_and_your_pb() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let text = std::fs::read_to_string(dir.join("game.toml")).unwrap();
        let manifest = Manifest::parse(&text).unwrap();
        let mut book = Book::default();
        let info = Info::new(Some(&manifest), "demo-clear", "main", run(false, "SD"), &book, 50);
        assert_eq!(info.title, "Clear fixture");
        assert_eq!(info.creator.as_deref(), Some("IWBTG Collection"), "authors only");
        assert_eq!(info.map, "Ranked mode");
        assert_eq!(info.stages, 2, "one stage split");
        assert_eq!(info.pb, None);
        assert_eq!(info.mods, ["SD"]);

        let board = book
            .map_mut(&map_key("demo-clear", "main"), "demo-clear", 50)
            .boards
            .entry(crate::stats::board_key(true, "SD"))
            .or_default();
        board.add(crate::stats::Run {
            ticks: SAMPLE_PB,
            deaths: 0,
            splits: vec![100, SAMPLE_PB],
            segment_deaths: vec![0, 0],
            finished_at: 0,
            replay: None,
            death_log: Default::default(),
        });
        let info = Info::new(Some(&manifest), "demo-clear", "main", run(false, "SD"), &book, 50);
        assert_eq!(info.pb.as_deref(), Some("38:12.46"));
        let other = Info::new(Some(&manifest), "demo-clear", "main", run(false, ""), &book, 50);
        assert_eq!(other.pb, None, "another board");
        let bare = Info::new(None, "demo-clear", "main", run(true, ""), &book, 50);
        assert_eq!(
            (bare.title.as_str(), bare.map.as_str()),
            ("demo-clear", "Play original")
        );
        assert_eq!(bare.creator, None);
    }

    /// The key hint says `Esc back` when the client has a select screen, else `Esc quit`.
    #[test]
    fn the_key_hint_says_back_with_a_select_screen() {
        let book = Book::default();
        let info = Info::new(None, "demo", "main", run(true, ""), &book, 50);
        for (back, word) in [(true, " back"), (false, " quit")] {
            let mut app = App::new();
            let look = page::Look {
                art: None,
                fruit: None,
                back,
            };
            let tip = &TIPS[0];
            app.world_mut().spawn(page::page(&info, tip, &look));
            let spans: Vec<String> = app
                .world_mut()
                .query::<&TextSpan>()
                .iter(app.world())
                .map(|s| s.0.clone())
                .collect();
            assert!(spans.iter().any(|s| s == word), "{back}: {spans:?}");
            let other = if back { " quit" } else { " back" };
            assert!(!spans.iter().any(|s| s == other), "{back}: {spans:?}");
        }
    }

    /// The card stays for the minimum time and then lets the sim tick; the preview never starts.
    #[test]
    fn the_card_starts_the_run_after_the_minimum_time() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .insert_resource(Loading::new(true, false, 0))
            .add_systems(Update, advance);
        let step = |app: &mut App, secs: f32| {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs_f32(secs));
            app.update();
            app.world_mut().run_system_cached(closed).unwrap()
        };
        assert!(!step(&mut app, 0.5));
        assert!(!step(&mut app, 0.9));
        assert!(step(&mut app, 0.2), "1.6 s shown");

        let mut app = App::new();
        app.init_resource::<Time>()
            .insert_resource(Loading::new(true, true, 0))
            .add_systems(Update, advance);
        assert!(!step(&mut app, 5.0), "the preview holds");
        assert!((app.world().resource::<Loading>().progress() - PREVIEW_PROGRESS).abs() < 1e-6);
    }

    /// Escape on the card quits; after it, Escape is the pause key's again.
    #[test]
    fn escape_on_the_card_quits() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .add_message::<AppExit>()
            .insert_resource(Loading::new(true, false, 0))
            .add_systems(Update, keys);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        let exits = app.world().resource::<Messages<AppExit>>();
        assert_eq!(exits.len(), 1);

        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .add_message::<AppExit>()
            .insert_resource(Loading::new(false, false, 0))
            .add_systems(Update, keys);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert!(app.world().resource::<Messages<AppExit>>().is_empty());
    }
}
