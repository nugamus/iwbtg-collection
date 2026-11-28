//! Classic's screen, the shelf (m5 `A4_Shelf`, `Shelf.dc.html`; #1618): one cover per game, not a
//! list of maps (a game's own difficulty select runs in the game). The picked game's art fills the
//! screen; on the left its title and what you did in it, on the right your progress and Play,
//! which opens the game as it starts (`PlayMode::Original`; docs/product.md, Two ways to play each
//! game). The client never shows or picks a save here.
//!
//! Left and Right move along the shelf (the covers slide, the picked one grows), Enter or a click
//! on the picked cover plays, Escape goes back to the main menu. Typing searches. The Played and
//! Finished chips keep the games you played or finished in Classic.
//!
//! The numbers are the stats book's Classic ones (#1619). The furthest point is a room until
//! packages declare sections, and the route line comes with m5 E7 (group 4).

use std::path::{Path, PathBuf};

use bevy::camera::visibility::RenderLayers;
use bevy::input_focus::InputFocus;
use bevy::picking::events::PointerClick;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui::IsDefaultUiCamera;
use bevy::ui_widgets::{Activate, Button};
use bevy::window::PrimaryWindow;
use iwbtg_sim::mods::ModSet;
use iwbtg_sim::stripped::PlayMode;

use super::AppScreen;
use super::select::library::{self, Library};
use crate::card::{COLLECTION_GAMES, Card, Variant, card, profile_name};
use crate::stats::{Book, SessionSummary};
use crate::ui::backdrop::Backdrop;
use crate::ui::fonts::Type;
use crate::ui::hints::hints;
use crate::ui::icons::{Icon, icon};
use crate::ui::mode::Mode;
use crate::ui::motion::{Ease, Height, OffsetX, Tween, Width};
use crate::ui::shape::{Band, Shape, Side, Veil};
use crate::ui::tokens;
use crate::ui::toolbar::toolbar;
use crate::ui::widgets::{ButtonKind, Chip, SearchField, button_with, chip, ranked_pill, search_field, text, text_leading};

/// The shelf's backdrop: a light dim and a little blur (`dim="0.22" blur="6"` of 40).
const DIM: f32 = 0.22;
const BLUR: f32 = 6.0 / 40.0;
/// A cover: 200 x 270, the picked one 236 x 318, 18 px apart, from x 40, bottoms on y 934.
const COVER: Vec2 = Vec2::new(200.0, 270.0);
const COVER_PICKED: Vec2 = Vec2::new(236.0, 318.0);
const COVER_GAP: f32 = 18.0;
const SHELF_X: f32 = 40.0;
const SHELF_TOP: f32 = 616.0;
/// The picked cover stays at least this many covers from the shelf's left end.
const LEAD: usize = 3;
/// `#b9adff`: the ring around the picked cover.
const RING: Color = Color::srgb_u8(0xb9, 0xad, 0xff);
/// `#e3e5ee` and `#dfe2ec`.
const BRIGHT: Color = Color::srgb_u8(0xe3, 0xe5, 0xee);
const ON_ART: Color = tokens::ON_ART;
/// The shade of the overlays: `rgba(8,9,14,...)`.
const SHADE: Color = Color::srgb_u8(0x08, 0x09, 0x0e);
/// Covers this far from the pick have their art loaded.
const ART_RADIUS: usize = 8;

pub struct ShelfPlugin {
    pub library: Option<PathBuf>,
}

impl Plugin for ShelfPlugin {
    fn build(&self, app: &mut App) {
        let folder = self.library.clone();
        app.init_resource::<ShelfState>()
            .add_systems(OnEnter(AppScreen::Shelf), move |world: &mut World| {
                enter(world, folder.as_deref())
            })
            .add_observer(on_activate)
            .add_observer(on_click)
            .add_systems(
                Update,
                (
                    search.run_if(crate::settings_screen::closed),
                    keys.run_if(crate::settings_screen::closed),
                    previews,
                    shelf_looks,
                    hero,
                    super::select::scale.run_if(any_with_component::<PrimaryWindow>),
                )
                    .chain()
                    .run_if(in_state(AppScreen::Shelf)),
            );
    }
}

/// What the shelf shows: the pick (an index into the library), the search and the chips. Kept
/// between visits, so back from a game the same cover is picked.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct ShelfState {
    pub pick: usize,
    pub search: String,
    pub played: bool,
    pub finished: bool,
}

/// One game's Classic facts from the stats book (`Book::game_mode`, #1619).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    /// Seconds played in Classic; every mode together when the book has no per-mode numbers
    /// for this game yet (`all_modes`).
    pub seconds: f64,
    pub deaths: u64,
    /// Finishes in Classic.
    pub finished: u32,
    /// The last finish's date, seconds since 1970.
    pub finished_at: Option<u64>,
    /// When a Classic run of it last ended.
    pub last_played: Option<u64>,
    /// The last Classic session.
    pub last_session: Option<SessionSummary>,
    /// The furthest room reached in Classic (package room id): sections come with E7.
    pub furthest: Option<String>,
    /// `seconds` and `deaths` are every mode together: the game was only played before the
    /// book kept modes apart.
    pub all_modes: bool,
}

impl Facts {
    /// `game`'s facts in `book`.
    pub fn of(book: &Book, game: &str) -> Facts {
        let g = book.game_mode(game, false);
        let maps = || book.maps.values().filter(|m| m.game == game);
        let any_modes = maps().any(|m| !m.modes.is_empty());
        let classic_clears: u32 = maps()
            .flat_map(|m| m.boards.iter())
            .filter(|(k, _)| k.starts_with("original/"))
            .map(|(_, b)| b.clears)
            .sum();
        if !any_modes {
            // A book from before #1619: its totals are every mode together.
            return Facts {
                seconds: maps().map(|m| m.play_seconds).sum(),
                deaths: maps().map(|m| m.deaths).sum(),
                finished: classic_clears,
                all_modes: maps().next().is_some(),
                ..Facts::default()
            };
        }
        Facts {
            seconds: g.play_seconds,
            deaths: g.deaths,
            finished: g.finishes.max(classic_clears),
            finished_at: g.finished_at,
            last_played: g.last_played,
            last_session: g.last_session,
            furthest: g.furthest.map(|f| f.room),
            all_modes: false,
        }
    }

    pub fn played(&self) -> bool {
        self.seconds > 0.0 || self.last_played.is_some()
    }

    /// The cover's line: `Finished`, `Played` or `Not started`.
    pub fn status(&self) -> &'static str {
        if self.finished > 0 {
            "Finished"
        } else if self.played() {
            "Played"
        } else {
            "Not started"
        }
    }

    /// The note under time played and deaths: `in Classic`, or `all modes` for an old book.
    pub fn scope(&self) -> &'static str {
        if self.all_modes { "all modes" } else { "in Classic" }
    }
}

/// The shelf's facts per game, read from the book when the screen opens.
#[derive(Resource, Default)]
struct AllFacts(Vec<Facts>);

/// The games the search and chips leave, in library order.
pub fn shown(lib: &Library, facts: &[Facts], state: &ShelfState) -> Vec<usize> {
    let words: Vec<String> = state
        .search
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    (0..lib.games.len())
        .filter(|&i| {
            let g = &lib.games[i];
            let hay = format!("{} {}", g.title, g.creator).to_lowercase();
            let f = facts.get(i).cloned().unwrap_or_default();
            words.iter().all(|w| hay.contains(w))
                && (!state.played || f.played())
                && (!state.finished || f.finished > 0)
        })
        .collect()
}

#[derive(Component)]
struct ShelfCamera;
/// The row the covers sit in; it slides to keep the pick in view.
#[derive(Component)]
struct Row;
/// A cover: its game's index in the library.
#[derive(Component, Clone, Copy)]
struct Cover(usize);
/// A cover's title and status line.
#[derive(Component)]
struct CoverTitle;
#[derive(Component)]
struct CoverStatus;

/// The hero's texts the pick rewrites: its lines, each stat column's value and the line under
/// it, the avatar's initial.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum HeroText {
    Line,
    Title,
    By,
    Progress,
    /// `last played 2 days ago` beside Your progress.
    LastPlayed,
    Value(usize),
    Sub(usize),
    Initial,
}
/// The creator's avatar.
#[derive(Component)]
struct Avatar;

/// The buttons.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShelfButton {
    Play,
    Details,
    Back,
    Played,
    Finished,
}

/// Opening the shelf: the library (read once, kept with game select's), the facts, the page.
fn enter(world: &mut World, folder: Option<&Path>) {
    let paths = folder.map(super::select::packages).unwrap_or_default();
    let keys: Vec<library::PathKey> = paths.iter().map(|p| library::path_key(p)).collect();
    let fresh = world
        .get_resource::<Library>()
        .is_none_or(|l| !l.matches(folder, &paths, &keys));
    if fresh {
        let lib = world.resource_scope(|_, mut images: Mut<Assets<Image>>| {
            Library::load(folder, &paths, &mut images)
        });
        world.insert_resource(lib);
    }
    let empty = Book::default();
    let facts: Vec<Facts> = {
        let book = world
            .get_resource::<crate::stats::Stats>()
            .map_or(&empty, |s| &s.book);
        let lib = world.resource::<Library>();
        lib.games.iter().map(|g| Facts::of(book, &g.id)).collect()
    };
    let player = {
        let book = world
            .get_resource::<crate::stats::Stats>()
            .map_or(&empty, |s| &s.book);
        Card::from_book(book, &profile_name(), COLLECTION_GAMES)
    };
    world.insert_resource(AllFacts(facts));
    if world.get_resource::<crate::mods::RunMods>().is_none() {
        world.insert_resource(crate::mods::RunMods(ModSet::NONE));
    }
    let n = world.resource::<Library>().games.len();
    {
        let mut state = world.resource_mut::<ShelfState>();
        state.pick = state.pick.min(n.saturating_sub(1));
        state.set_changed();
    }
    let search = world.resource::<ShelfState>().search.clone();
    let (played, finished) = {
        let s = world.resource::<ShelfState>();
        (s.played, s.finished)
    };
    {
        let mut backdrop = world.resource_mut::<Backdrop>();
        backdrop.dim = DIM;
        backdrop.blur = BLUR;
    }
    for mut w in world
        .query_filtered::<&mut Window, With<PrimaryWindow>>()
        .iter_mut(world)
    {
        w.title = super::select::TITLE.to_owned();
    }
    let camera = world
        .spawn((
            Camera2d,
            Camera {
                order: 10,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            RenderLayers::none(),
            IsDefaultUiCamera,
            ShelfCamera,
            DespawnOnExit(AppScreen::Shelf),
        ))
        .id();
    let mode = world.get_resource::<Mode>().cloned().unwrap_or_default();
    let mut commands = world.commands();
    page(&mut commands, camera, mode, &player, &search, played, finished);
    world.flush();
    rebuild_covers(world);
}

/// The page around the covers.
fn page(
    commands: &mut Commands,
    camera: Entity,
    mode: Mode,
    player: &Card,
    search: &str,
    played: bool,
    finished: bool,
) {
    let root = (
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        UiTargetCamera(camera),
        DespawnOnExit(AppScreen::Shelf),
    );
    let player = player.clone();
    let search = search.to_owned();
    commands.spawn(root).with_children(move |p| {
        // The overlays: darker on the left for the hero, and up from the bottom for the shelf.
        p.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundGradient::from(LinearGradient::to_right(vec![
                ColorStop::new(tokens::dark(SHADE, 0.82), Val::Percent(0.0)),
                ColorStop::new(tokens::dark(SHADE, 0.35), Val::Percent(45.0)),
                ColorStop::new(tokens::dark(SHADE, 0.55), Val::Percent(100.0)),
            ])),
        ));
        p.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                height: Val::Px(600.0),
                ..default()
            },
            BackgroundGradient::from(LinearGradient::to_top(vec![
                ColorStop::new(tokens::dark(SHADE, 0.96), Val::Percent(30.0)),
                ColorStop::new(tokens::dark(SHADE, 0.0), Val::Percent(100.0)),
            ])),
        ));
        p.spawn(hero_block());
        p.spawn(progress_block());
        p.spawn(library_row(&search, played, finished));
        // The shelf: clipped at the window's edges, the row inside slides.
        p.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(SHELF_TOP - 12.0),
                height: Val::Px(COVER_PICKED.y + 24.0),
                overflow: Overflow::clip_x(),
                ..default()
            },
            children![(
                Row,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(SHELF_X),
                    top: Val::Px(12.0),
                    height: Val::Px(COVER_PICKED.y),
                    align_items: AlignItems::FlexEnd,
                    column_gap: Val::Px(COVER_GAP),
                    ..default()
                },
                UiTransform::default(),
                Tween::<OffsetX>::at_rest(0.0, tokens::motion::SELECT, Ease::OutQuint),
            )],
        ));
        p.spawn(footer(player));
        p.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                ..default()
            },
            children![toolbar(mode)],
        ));
    });
}

fn label(content: &str) -> impl Bundle + use<> {
    text(content, Type::LABEL, tokens::MUTED)
}

/// The picked game: what it is, its title, its creator, your facts in it.
fn hero_block() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(64.0),
            top: Val::Px(104.0),
            // 1000 px wide at 1920; on a narrower window it stops short of Your progress (660 px
            // wide, 50 px from the right edge) and a long title wraps.
            right: Val::Px(50.0 + 660.0 + 40.0),
            max_width: Val::Px(1000.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(14.0),
            ..default()
        },
        children![
            (
                Node {
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(14.0),
                    ..default()
                },
                children![
                    ranked_pill(),
                    (text("", Type::barlow(16.0, 600), BRIGHT), HeroText::Line),
                ],
            ),
            (
                text_leading("", Type::condensed(92.0, 700).italic(), tokens::INK, 0.95),
                TextShadow {
                    offset: Vec2::new(0.0, 4.0),
                    color: tokens::black(0.55),
                },
                HeroText::Title,
            ),
            (
                Node {
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(10.0),
                    ..default()
                },
                children![
                    (
                        Avatar,
                        Node {
                            width: Val::Px(26.0),
                            height: Val::Px(26.0),
                            border_radius: BorderRadius::all(Val::Px(13.0)),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(Color::hsl(48.0, 0.7, 0.52)),
                        children![(
                            text("", Type::barlow(12.0, 700), tokens::ON_LIGHT),
                            HeroText::Initial
                        )],
                    ),
                    (text("", Type::barlow(18.0, 400), ON_ART), HeroText::By),
                ],
            ),
            (
                Node {
                    column_gap: Val::Px(18.0),
                    margin: UiRect::top(Val::Px(14.0)),
                    width: Val::Px(860.0),
                    ..default()
                },
                Children::spawn(SpawnIter(
                    ["Time played", "Deaths", "Finished", "Furthest", "Last session"]
                        .into_iter()
                        .enumerate()
                        .map(stat_column)
                )),
            ),
        ],
    )
}

fn stat_column((i, key): (usize, &'static str)) -> impl Bundle {
    (
        Node {
            width: Val::Px((860.0 - 4.0 * 18.0) / 5.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            ..default()
        },
        children![
            label(key),
            (text("", Type::barlow(24.0, 600), tokens::INK), HeroText::Value(i)),
            (text("", Type::barlow(13.0, 400), tokens::MUTED), HeroText::Sub(i)),
        ],
    )
}

/// Your progress and Play.
fn progress_block() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            // 1210 px in at 1920 wide: held to the right edge, so narrower windows keep it whole.
            right: Val::Px(50.0),
            top: Val::Px(96.0),
            width: Val::Px(660.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(14.0),
            ..default()
        },
        children![
            (
                Node {
                    padding: UiRect::new(Val::Px(4.0), Val::Px(8.0), Val::Px(0.0), Val::Px(0.0)),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Baseline,
                    ..default()
                },
                children![
                    text("Your progress", Type::barlow(20.0, 700), tokens::INK),
                    (
                        text("", Type::barlow(14.0, 400), tokens::MUTED),
                        HeroText::LastPlayed
                    ),
                ],
            ),
            (
                Node {
                    padding: UiRect::left(Val::Px(4.0)),
                    ..default()
                },
                children![(
                    text("", Type::barlow(14.0, 400), tokens::SOFT),
                    HeroText::Progress
                )],
            ),
            (
                Node {
                    column_gap: Val::Px(10.0),
                    margin: UiRect::top(Val::Px(8.0)),
                    padding: UiRect::left(Val::Px(6.0)),
                    ..default()
                },
                Children::spawn(SpawnWith(|c: &mut ChildSpawner| {
                    let mut play = c.spawn((
                        button_with(
                            ButtonKind::Primary,
                            64.0,
                            28.0,
                            (
                                Node {
                                    align_items: AlignItems::Baseline,
                                    column_gap: Val::Px(10.0),
                                    ..default()
                                },
                                children![
                                    text("Play", Type::barlow(23.0, 700), tokens::INK),
                                    text(
                                        "opens the game's title screen",
                                        Type::barlow(14.0, 600),
                                        tokens::INK.with_alpha(0.9)
                                    ),
                                ],
                            ),
                        ),
                        ShelfButton::Play,
                    ));
                    // Play takes the row's width beside Details.
                    if let Some(mut node) = play.get_mut::<Node>() {
                        node.flex_grow = 1.0;
                    }
                    c.spawn((
                        button_with(
                            ButtonKind::Secondary,
                            64.0,
                            24.0,
                            text("Details", Type::barlow(17.0, 700), tokens::INK),
                        ),
                        ShelfButton::Details,
                    ));
                })),
            ),
            (
                Node {
                    padding: UiRect::new(Val::Px(10.0), Val::Px(0.0), Val::Px(2.0), Val::Px(0.0)),
                    ..default()
                },
                children![text_leading(
                    "New game, Continue and the file select are the game's own, exactly as in the original. Its saves stay in the game's save folder; the client never picks a save for you.",
                    Type::barlow(14.0, 400),
                    tokens::MUTED,
                    1.5
                )],
            ),
        ],
    )
}

/// `Library`, the search and the chips over the shelf.
fn library_row(search: &str, played: bool, finished: bool) -> impl Bundle {
    let placeholder = "Search games".to_owned();
    let search = search.to_owned();
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(64.0),
            right: Val::Px(64.0),
            top: Val::Px(552.0),
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.0),
            ..default()
        },
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            c.spawn((
                Node {
                    padding: UiRect::right(Val::Px(10.0)),
                    ..default()
                },
                children![text("Library", Type::barlow(20.0, 700), tokens::INK)],
            ));
            c.spawn((
                Node {
                    width: Val::Px(380.0),
                    ..default()
                },
                children![search_field(&placeholder, &search)],
            ));
            c.spawn((chip("Played", played), ShelfButton::Played));
            c.spawn((chip("Finished", finished), ShelfButton::Finished));
        })),
    )
}

/// Back and Details, the keys, the player card.
fn footer(player: Card) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(64.0),
            right: Val::Px(64.0),
            bottom: Val::Px(26.0),
            align_items: AlignItems::FlexEnd,
            column_gap: Val::Px(12.0),
            ..default()
        },
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            c.spawn((
                button_with(
                    ButtonKind::Secondary,
                    52.0,
                    26.0,
                    (
                        Node {
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(10.0),
                            ..default()
                        },
                        children![
                            icon(Icon::Back, 14.0, tokens::INK),
                            text("Back", Type::BUTTON, tokens::INK)
                        ],
                    ),
                ),
                ShelfButton::Back,
            ));
            c.spawn((
                button_with(
                    ButtonKind::Secondary,
                    52.0,
                    24.0,
                    text("Details", Type::BUTTON, tokens::INK),
                ),
                ShelfButton::Details,
            ));
            c.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
            c.spawn((
                Node {
                    align_self: AlignSelf::Center,
                    ..default()
                },
                children![hints(&[
                    ("Enter", "Play"),
                    ("← →", "Pick"),
                    ("Type", "Search"),
                    ("Esc", "Back"),
                ])],
            ));
            c.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
            c.spawn(card(&player, Variant::Footer));
        })),
    )
}

/// Respawns the covers for the games shown, after the search, a chip or the pick changed.
fn rebuild_covers(world: &mut World) {
    let shown = {
        let lib = world.resource::<Library>();
        let facts = &world.resource::<AllFacts>().0;
        shown(lib, facts, world.resource::<ShelfState>())
    };
    // The pick stays among the games shown.
    {
        let mut state = world.resource_mut::<ShelfState>();
        if !shown.contains(&state.pick)
            && let Some(&first) = shown.first()
        {
            state.pick = first;
        }
    }
    let Ok(row) = world.query_filtered::<Entity, With<Row>>().single(world) else {
        return;
    };
    let old: Vec<Entity> = world
        .query_filtered::<Entity, With<Cover>>()
        .iter(world)
        .collect();
    for e in old {
        world.entity_mut(e).despawn();
    }
    let pick = world.resource::<ShelfState>().pick;
    let covers: Vec<_> = {
        let lib = world.resource::<Library>();
        let facts = &world.resource::<AllFacts>().0;
        shown
            .iter()
            .map(|&i| {
                (
                    i,
                    lib.games[i].title.clone(),
                    facts.get(i).cloned().unwrap_or_default(),
                    lib.cover[i].clone(),
                    lib.fruit[i].clone().unwrap_or(lib.placeholder_fruit.clone()),
                )
            })
            .collect()
    };
    world.entity_mut(row).with_children(|r| {
        for (i, title, facts, art, fruit) in covers {
            r.spawn(cover(i, &title, facts, i == pick, art, fruit));
        }
    });
    world.resource_mut::<ShelfState>().set_changed();
}

/// A cover's shape: its art under the dark veil, the band at its foot, the ring when picked.
fn cover_shape(facts: Facts, picked: bool, art: Option<Handle<Image>>) -> Shape {
    let band = if picked {
        tokens::CLASSIC
    } else if facts.finished > 0 {
        tokens::white(0.5)
    } else if facts.played() {
        tokens::CLASSIC.with_alpha(0.6)
    } else {
        Color::NONE
    };
    let mut shape = Shape::fill(tokens::CARD_SOLID)
        .with_veil(Veil {
            stops: [
                tokens::dark(SHADE, 0.1),
                tokens::dark(SHADE, 0.1),
                tokens::dark(SHADE, 0.94),
            ],
            mid: 0.4,
            vertical: true,
        })
        .with_band(Band {
            side: Side::Bottom,
            width: 4.0,
            color: band,
            fade_to: 0.0,
        });
    if let Some(art) = art {
        shape = shape.with_image(art);
    }
    if picked {
        shape.ring = Some((3.0, RING));
        shape.with_shadow(18.0, 40.0, tokens::black(0.55))
    } else {
        shape.with_shadow(8.0, 20.0, tokens::black(0.35))
    }
}

fn cover(
    index: usize,
    title: &str,
    facts: Facts,
    picked: bool,
    art: Option<Handle<Image>>,
    fruit: Handle<Image>,
) -> impl Bundle {
    let size = if picked { COVER_PICKED } else { COVER };
    let finished = facts.finished > 0;
    let status_ink = if facts.played() { BRIGHT } else { tokens::MUTED };
    let status = facts.status().to_owned();
    let title = title.to_owned();
    (
        Cover(index),
        Node {
            width: Val::Px(size.x),
            height: Val::Px(size.y),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexEnd,
            row_gap: Val::Px(4.0),
            padding: UiRect::new(Val::Px(26.0), Val::Px(18.0), Val::Px(0.0), Val::Px(18.0)),
            ..default()
        },
        cover_shape(facts, picked, art),
        // The picked cover grows and the others make room over `motion::SELECT` (#1685): a pick
        // never jumps the row.
        Tween::<Width>::at_rest(size.x, tokens::motion::SELECT, Ease::OutQuint),
        Tween::<Height>::at_rest(size.y, tokens::motion::SELECT, Ease::OutQuint),
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            c.spawn((
                text_leading(
                    &title,
                    Type::barlow(if picked { 21.0 } else { 18.0 }, 700),
                    tokens::INK,
                    1.1,
                ),
                CoverTitle,
            ));
            c.spawn((
                Node {
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    ..default()
                },
                CoverStatus,
            ))
            .with_children(|line| {
                if finished {
                    line.spawn((
                        Node {
                            width: Val::Px(11.0),
                            height: Val::Px(13.0),
                            ..default()
                        },
                        ImageNode {
                            image: fruit,
                            ..default()
                        },
                    ));
                }
                line.spawn(text(&status, Type::barlow(13.0, 600), status_ink));
            });
        })),
    )
}

/// Typing goes to the search field; a new search rebuilds the covers.
fn search(
    fields: Query<&EditableText, With<SearchField>>,
    mut state: ResMut<ShelfState>,
    mut commands: Commands,
) {
    for field in &fields {
        let text = field.value().to_string();
        if state.search != text {
            state.search = text;
            commands.queue(rebuild_covers);
        }
    }
}

/// Left and Right move along the shelf (the search field's cursor while it holds text), Enter
/// plays, Escape clears the search, else goes back to the main menu.
#[allow(clippy::too_many_arguments, reason = "the screen's keys")]
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    lib: Res<Library>,
    facts: Res<AllFacts>,
    focus: Res<InputFocus>,
    buttons: Query<(), With<Button>>,
    mut fields: Query<&mut EditableText, With<SearchField>>,
    mut state: ResMut<ShelfState>,
    mut next: ResMut<NextState<AppScreen>>,
    mut commands: Commands,
) {
    let order = shown(&lib, &facts.0, &state);
    let typing = !state.search.is_empty();
    let at = order.iter().position(|&i| i == state.pick);
    if !typing {
        if keys.just_pressed(KeyCode::ArrowRight)
            && let Some(&i) = at.and_then(|p| order.get(p + 1))
        {
            state.pick = i;
        }
        if keys.just_pressed(KeyCode::ArrowLeft)
            && let Some(&i) = at.and_then(|p| p.checked_sub(1)).and_then(|p| order.get(p))
        {
            state.pick = i;
        }
    }
    let on_button = focus.get().is_some_and(|e| buttons.contains(e));
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) && !on_button {
        play(&mut commands, &lib, state.pick);
    }
    if keys.just_pressed(KeyCode::Escape) {
        if typing {
            for mut f in &mut fields {
                f.clear();
            }
        } else {
            next.set(AppScreen::Menu);
        }
    }
}

/// Plays game `index` of `lib` as it starts.
fn play(commands: &mut Commands, lib: &Library, index: usize) {
    let Some(game) = lib.games.get(index) else {
        return;
    };
    let path = game.path.clone();
    let map = game.maps.first().map(|m| m.id.clone());
    commands.queue(move |world: &mut World| {
        if let Some(mut practice) = world.get_resource_mut::<crate::practice::Practice>() {
            practice.0 = false;
        }
        super::start_map(world, &path, map.as_deref(), PlayMode::Original, None);
    });
}

/// The buttons: Play, Back, the chips (Details opens with group 12).
#[allow(clippy::too_many_arguments, reason = "the screen's buttons")]
fn on_activate(
    activate: On<Activate>,
    buttons: Query<&ShelfButton>,
    lib: Option<Res<Library>>,
    state: Option<ResMut<ShelfState>>,
    mut chips: Query<&mut Chip>,
    next: Option<ResMut<NextState<AppScreen>>>,
    screen: Option<Res<State<AppScreen>>>,
    mut commands: Commands,
) {
    let (Ok(button), Some(lib), Some(mut state), Some(mut next)) =
        (buttons.get(activate.entity), lib, state, next)
    else {
        return;
    };
    if screen.is_none_or(|s| *s.get() != AppScreen::Shelf) {
        return;
    }
    match button {
        ShelfButton::Play => play(&mut commands, &lib, state.pick),
        ShelfButton::Back => next.set(AppScreen::Menu),
        ShelfButton::Played | ShelfButton::Finished => {
            let on = if *button == ShelfButton::Played {
                state.played = !state.played;
                state.played
            } else {
                state.finished = !state.finished;
                state.finished
            };
            if let Ok(mut chip) = chips.get_mut(activate.entity) {
                chip.on = on;
            }
            commands.queue(rebuild_covers);
        }
        ShelfButton::Details => {}
    }
}

/// A click on a cover picks it; on the picked one, plays.
fn on_click(
    click: On<PointerClick>,
    covers: Query<&Cover>,
    lib: Option<Res<Library>>,
    state: Option<ResMut<ShelfState>>,
    mut commands: Commands,
) {
    let (Ok(cover), Some(lib), Some(mut state)) = (covers.get(click.entity), lib, state) else {
        return;
    };
    if state.pick == cover.0 {
        play(&mut commands, &lib, cover.0);
    } else {
        state.pick = cover.0;
    }
}

/// Asks for the covers around the pick and puts each one on its cover when it arrives.
fn previews(
    mut lib: ResMut<Library>,
    state: Res<ShelfState>,
    facts: Res<AllFacts>,
    mut images: ResMut<Assets<Image>>,
    mut covers: Query<(&Cover, &mut Shape)>,
    mut last: Local<Option<(usize, u64)>>,
) {
    let order = shown(&lib, &facts.0, &state);
    let at = order.iter().position(|&i| i == state.pick).unwrap_or(0);
    let lo = at.saturating_sub(ART_RADIUS);
    let hi = (at + ART_RADIUS + 1).min(order.len());
    if state.is_changed() {
        lib.want(&order[lo..hi]);
    }
    lib.poll(&mut images);
    let key = (state.pick, lib.revision());
    if *last == Some(key) && !state.is_changed() {
        return;
    }
    *last = Some(key);
    for (cover, mut shape) in &mut covers {
        let want = lib.cover.get(cover.0).cloned().flatten();
        if shape.image != want {
            shape.image = want;
        }
    }
}

/// A cover's parts that follow the pick.
type CoverParts = (
    &'static Cover,
    &'static mut Tween<Width>,
    &'static mut Tween<Height>,
    &'static mut Shape,
    &'static Children,
);

/// The picked cover grows and gets the ring; the row slides to keep it in view.
fn shelf_looks(
    state: Res<ShelfState>,
    lib: Res<Library>,
    facts: Res<AllFacts>,
    mut covers: Query<CoverParts>,
    mut titles: Query<&mut TextFont, With<CoverTitle>>,
    mut row: Query<&mut Tween<OffsetX>, With<Row>>,
) {
    if !state.is_changed() {
        return;
    }
    for (cover, mut w, mut h, mut shape, children) in &mut covers {
        let picked = cover.0 == state.pick;
        let size = if picked { COVER_PICKED } else { COVER };
        if (w.to - size.x).abs() > f32::EPSILON {
            w.retarget(size.x);
            h.retarget(size.y);
        }
        let f = facts.0.get(cover.0).cloned().unwrap_or_default();
        let mut want = cover_shape(f, picked, shape.image.clone());
        want.image = shape.image.clone();
        if *shape != want {
            *shape = want;
        }
        for c in children.iter() {
            if let Ok(mut font) = titles.get_mut(c) {
                *font = Type::barlow(if picked { 21.0 } else { 18.0 }, 700).font();
            }
        }
    }
    let order = shown(&lib, &facts.0, &state);
    let at = order.iter().position(|&i| i == state.pick).unwrap_or(0);
    let first = at.saturating_sub(LEAD);
    for mut tween in &mut row {
        tween.retarget(-(first as f32) * (COVER.x + COVER_GAP));
    }
}

/// The hero, the progress line and the backdrop follow the pick.
#[allow(clippy::too_many_arguments, reason = "the hero's parts")]
fn hero(
    state: Res<ShelfState>,
    lib: Res<Library>,
    facts: Res<AllFacts>,
    mut texts: Query<(&HeroText, &mut Text)>,
    added: Query<(), Added<HeroText>>,
    mut avatar: Query<&mut BackgroundColor, With<Avatar>>,
    mut backdrop: ResMut<Backdrop>,
    mut last: Local<Option<usize>>,
) {
    // The picked game's art fills the screen, from the library's previews (built off the UI
    // thread: a pick never reads a package on it); until it arrives the last art stays.
    if let Some(handle) = lib.backdrop.get(state.pick).cloned().flatten()
        && backdrop.art.as_ref() != Some(&handle)
    {
        backdrop.art = Some(handle);
    }
    // The texts once per pick, and again when the page is new (back on the shelf).
    if *last == Some(state.pick) && added.is_empty() {
        return;
    }
    *last = Some(state.pick);
    let Some(game) = lib.games.get(state.pick) else {
        return;
    };
    let f = facts.0.get(state.pick).cloned().unwrap_or_default();
    let maps = game.maps.len();
    let line = if maps > 1 {
        format!("{maps} difficulties, picked in the game")
    } else {
        "One route".to_owned()
    };
    let by = if game.creator.is_empty() {
        String::new()
    } else {
        format!("by {}", game.creator)
    };
    let now = crate::stats::now_seconds();
    let progress = progress_line(&f);
    let last_played = f
        .last_played
        .map(|t| format!("last played {}", crate::card::ago(t, now)))
        .unwrap_or_default();
    let stats = hero_stats(&f, now);
    let first = game
        .creator
        .chars()
        .next()
        .map_or_else(|| "?".to_owned(), |c| c.to_uppercase().collect());
    for (part, mut t) in &mut texts {
        let (s, style) = match *part {
            HeroText::Line => (line.as_str(), Type::barlow(16.0, 600)),
            HeroText::Title => (game.title.as_str(), Type::condensed(92.0, 700).italic()),
            HeroText::By => (by.as_str(), Type::barlow(18.0, 400)),
            HeroText::Progress => (progress.as_str(), Type::barlow(14.0, 400)),
            HeroText::LastPlayed => (last_played.as_str(), Type::barlow(14.0, 400)),
            HeroText::Value(i) => (
                stats.get(i).map_or("", |(v, _)| v.as_str()),
                Type::barlow(24.0, 600),
            ),
            HeroText::Sub(i) => (
                stats.get(i).map_or("", |(_, d)| d.as_str()),
                Type::barlow(13.0, 400),
            ),
            HeroText::Initial => (first.as_str(), Type::barlow(12.0, 700)),
        };
        t.0 = style.set(s);
    }
    let hue = game.creator.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b))) % 360;
    for mut bg in &mut avatar {
        bg.0 = Color::hsl(hue as f32, 0.7, 0.52);
    }
}

/// The line under Your progress: the furthest room and the last finish (m5 A4).
fn progress_line(f: &Facts) -> String {
    if !f.played() {
        return "Not started yet. Play opens the game's own title screen.".to_owned();
    }
    let furthest = f
        .furthest
        .as_deref()
        .map_or_else(String::new, |r| format!("Furthest: {r} · "));
    let finished = match (f.finished, f.finished_at) {
        (0, _) => "not finished yet".to_owned(),
        (n, Some(at)) => format!(
            "finished {}, {}",
            times(n).to_lowercase(),
            crate::hud::leaderboard::date_short(at)
        ),
        (n, None) => format!("finished {}", times(n).to_lowercase()),
    };
    format!("{furthest}{finished} · sections come with the package")
}

/// The five numbers on the left (m5 A4): time played, deaths, finished, furthest, last session.
fn hero_stats(f: &Facts, now: u64) -> [(String, String); 5] {
    let finished = if f.finished == 0 {
        ("Not yet".to_owned(), String::new())
    } else {
        (
            times(f.finished),
            f.finished_at
                .map(crate::hud::leaderboard::date_short)
                .unwrap_or_default(),
        )
    };
    let furthest = match &f.furthest {
        Some(room) => (room.clone(), "room reached".to_owned()),
        None => ("—".to_owned(), String::new()),
    };
    let session = match f.last_session {
        Some(s) => (
            clock(s.seconds),
            format!(
                "{} deaths · {}",
                crate::card::grouped(s.deaths),
                crate::card::ago(s.ended_at, now)
            ),
        ),
        None => ("—".to_owned(), String::new()),
    };
    [
        (clock(f.seconds), f.scope().to_owned()),
        (crate::card::grouped(f.deaths), f.scope().to_owned()),
        finished,
        furthest,
        session,
    ]
}

/// `Once`, `Twice`, `3 times`.
pub fn times(n: u32) -> String {
    match n {
        1 => "Once".to_owned(),
        2 => "Twice".to_owned(),
        n => format!("{n} times"),
    }
}

/// `h:mm:ss`.
pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{Board, MapStats};

    /// The shelf on the test fixtures, without a window.
    fn screen() -> App {
        use bevy::state::app::StatesPlugin;
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
            .init_asset::<Image>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<InputFocus>()
            .init_resource::<UiScale>()
            .init_resource::<Backdrop>()
            .insert_state(AppScreen::Shelf)
            .add_plugins(ShelfPlugin {
                library: Some(fixtures),
            });
        app.update();
        app.update();
        app
    }

    fn tap(app: &mut App, key: KeyCode) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        app.update();
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.release(key);
        input.clear();
        app.update();
    }

    /// Walking the shelf respawns nothing, and the picked cover grows by tween, not by a jump
    /// (UI foundation, #1685).
    #[test]
    fn walking_the_shelf_updates_it_in_place() {
        use crate::ui::latency::node_set;
        let mut app = screen();
        let before = node_set(app.world_mut());
        for key in [KeyCode::ArrowRight, KeyCode::ArrowRight, KeyCode::ArrowLeft] {
            tap(&mut app, key);
            assert_eq!(node_set(app.world_mut()), before, "{key:?}");
        }
        let pick = app.world().resource::<ShelfState>().pick;
        let mut q = app.world_mut().query::<(&Cover, &Tween<Width>, &Tween<Height>)>();
        for (cover, w, h) in q.iter(app.world()) {
            let want = if cover.0 == pick { COVER_PICKED } else { COVER };
            assert_eq!((w.to, h.to), (want.x, want.y), "cover {}", cover.0);
        }
    }

    /// One cover per game; Right and Left move the pick along the shelf; Escape goes back to the
    /// main menu.
    #[test]
    fn the_arrows_walk_the_shelf_and_escape_goes_back() {
        let mut app = screen();
        let games = app.world().resource::<Library>().games.len();
        let covers = app
            .world_mut()
            .query::<&Cover>()
            .iter(app.world())
            .count();
        assert!(games > 2);
        assert_eq!(covers, games, "a cover per game");
        assert_eq!(app.world().resource::<ShelfState>().pick, 0);
        tap(&mut app, KeyCode::ArrowRight);
        assert_eq!(app.world().resource::<ShelfState>().pick, 1);
        tap(&mut app, KeyCode::ArrowLeft);
        tap(&mut app, KeyCode::ArrowLeft);
        assert_eq!(app.world().resource::<ShelfState>().pick, 0, "the shelf ends there");
        tap(&mut app, KeyCode::Escape);
        assert_eq!(
            *app.world().resource::<State<AppScreen>>().get(),
            AppScreen::Menu
        );
    }

    /// Enter plays the picked game as it starts: the original game, not the stripped one.
    #[test]
    fn enter_plays_the_game_as_it_starts() {
        let mut app = screen();
        tap(&mut app, KeyCode::Enter);
        let pending = app.world().resource::<super::super::PendingStart>();
        assert_eq!(pending.mode, PlayMode::Original);
    }

    /// The search keeps the games whose title or creator has every word; the chips keep the
    /// played or finished ones.
    #[test]
    fn the_search_and_chips_narrow_the_shelf() {
        let app = screen();
        let lib = app.world().resource::<Library>();
        let facts = vec![Facts::default(); lib.games.len()];
        let all = shown(lib, &facts, &ShelfState::default());
        assert_eq!(all.len(), lib.games.len());
        let title = lib.games[all[1]].title.to_lowercase();
        let word = title.split_whitespace().next().unwrap().to_owned();
        let found = shown(
            lib,
            &facts,
            &ShelfState {
                search: word.clone(),
                ..ShelfState::default()
            },
        );
        assert!(found.contains(&all[1]) && found.iter().all(|&i| lib.games[i].title.to_lowercase().contains(&word) || lib.games[i].creator.to_lowercase().contains(&word)));
        let played = shown(
            lib,
            &facts,
            &ShelfState {
                played: true,
                ..ShelfState::default()
            },
        );
        assert!(played.is_empty(), "nothing played yet");
    }

    #[test]
    fn facts_count_classic_finishes_only() {
        let mut book = Book::default();
        let mut m = MapStats {
            game: "g".into(),
            deaths: 12,
            play_seconds: 90.0,
            ..MapStats::default()
        };
        m.boards.insert(
            "original/NM".into(),
            Board {
                clears: 2,
                ..Board::default()
            },
        );
        m.boards.insert(
            "ranked/NM".into(),
            Board {
                clears: 5,
                ..Board::default()
            },
        );
        book.maps.insert("g/main".into(), m);
        let f = Facts::of(&book, "g");
        assert_eq!((f.deaths, f.finished), (12, 2));
        assert_eq!(f.status(), "Finished");
        assert_eq!(Facts::of(&book, "other").status(), "Not started");
        assert_eq!(times(1), "Once");
        assert_eq!(clock(3725.0), "1:02:05");
    }
}
