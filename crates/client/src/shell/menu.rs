//! The main menu (m5 `MainMenu.dc.html`: B6 initial, B7 top level, A1/A2 Play): the first screen
//! of the client started on a library (`--select`), and where Back and Escape lead from game
//! select.
//!
//! Three levels, after osu!'s button system in our look. **Initial:** the delicious fruit alone in
//! the middle and "Press any key". Any key or a click opens **Top**: the fruit slides left and
//! shrinks while the band of buttons opens to its right: Play, Browse, Replays, Settings, Exit.
//! **Play** swaps the band for the modes: Back, Time Attack, Classic and Race (later). The mode is
//! picked here, once (docs/product.md, Two ways to play each game); focusing a mode sets
//! [`Mode`], so the toolbar and the accent follow, and choosing it opens that mode's screen.
//! Hover or focus widens a button 1.5x and fills it with its own colour. Escape steps back a
//! level; at the top it exits.
//!
//! What is not built yet stays visible and does nothing: Browse (#50), Replays and Settings (#53)
//! open with their screens, Race is marked Later. Time Attack opens game select, Classic the
//! shelf (m5 A4, `shelf.rs`).

use bevy::camera::visibility::RenderLayers;
use bevy::input_focus::InputFocus;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;
use bevy::ui_widgets::{Activate, Button};
use bevy::window::PrimaryWindow;

use super::AppScreen;
use crate::card::{COLLECTION_GAMES, Card, Variant, card, profile_name};
use crate::ui::backdrop::Backdrop;
use crate::ui::fonts::Type;
use crate::ui::icons::{Icon, icon};
use crate::ui::mode::Mode;
use crate::ui::motion::{Ease, OffsetX, OffsetY, Scale, Tween, Width};
use crate::ui::shape::{Ink, Shape, UiOpacity};
use crate::ui::tokens;
use crate::ui::toolbar::toolbar;
use crate::ui::widgets::{rich, text, text_leading};

/// The menu's backdrop dim (`Backdrop` with `dim="0.42"`).
const DIM: f32 = 0.42;
/// The band of buttons: 150 px tall, buttons 210 px wide at rest, 6 px apart, from x 470.
const BAND_H: f32 = 150.0;
const BASE_W: f32 = 210.0;
const GAP: f32 = 6.0;
const ROW_X: f32 = 470.0;
/// The band's centre line: 470 px down (450 in the initial state).
const CY: f32 = 470.0;
/// The artboard's height: the middle of the page is laid out in it.
const DESIGN_H: f32 = 1080.0;
/// The artboard's width: the page is this wide at 16:9.
const DESIGN_W: f32 = 1920.0;
const CY_INITIAL: f32 = 450.0;
/// The fruit's pixel scale: 11x alone in the middle, 7.6x beside the band.
const FRUIT_SCALE_INITIAL: f32 = 11.0;
const FRUIT_SCALE: f32 = 7.6;
/// The fruit's own frame, px.
const FRUIT: Vec2 = Vec2::new(21.0, 24.0);
/// The fruit sits 150 px in beside the band.
const FRUIT_X: f32 = 150.0;
/// The fruit bobs 6 px up and back every 1.6 s (`mm-bob`); "Press any key" pulses over 2 s.
const BOB: f32 = 6.0;
const BOB_SECONDS: f32 = 1.6;
const PULSE_SECONDS: f32 = 2.0;
/// The fruit slides and shrinks into place and the buttons grow from zero (`motion::SCREEN`,
/// OutExpo); a button widens on focus (`motion::SELECT`, OutQuint: the player waits on it, so no
/// elastic overshoot, #1682).
const SLIDE_SECONDS: f32 = tokens::motion::SCREEN;
const GROW_SECONDS: f32 = tokens::motion::SCREEN;
const WIDEN_SECONDS: f32 = tokens::motion::SELECT;
/// The caption under the band: 520 px wide, 22 px below it, 10 px right of the focused button.
const CAPTION_W: f32 = 520.0;
/// Margins of the footer line and the player card.
const EDGE: f32 = 40.0;
const EDGE_BOTTOM: f32 = 28.0;
/// `#e3e5ee`: the caption and footer's bright text.
const BRIGHT: Color = Color::srgb_u8(0xe3, 0xe5, 0xee);
/// The band's shade: `rgba(8,9,14,...)`.
const SHADE: Color = Color::srgb_u8(0x08, 0x09, 0x0e);

/// Where the menu is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, clap::ValueEnum)]
pub enum Level {
    /// The fruit alone, "Press any key" (B6).
    #[default]
    Initial,
    /// Play, Browse, Replays, Settings, Exit (B7).
    Top,
    /// Back, Time Attack, Classic, Race (A1, A2).
    Play,
}

/// What the captions count: the games in the library and how many you cleared.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Facts {
    games: usize,
    cleared: usize,
}

/// The menu's state: its level and the focused button.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MainMenu {
    pub level: Level,
    /// The focused button of the band, an index into [`buttons`].
    pub focus: usize,
}

/// What a band button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Play,
    Browse,
    Replays,
    Settings,
    Exit,
    Back,
    Mode(Mode),
    Race,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Action::Play => "Play",
            Action::Browse => "Browse",
            Action::Replays => "Replays",
            Action::Settings => "Settings",
            Action::Exit => "Exit",
            Action::Back => "Back",
            Action::Mode(m) => m.label(),
            Action::Race => "Race",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Action::Play => Icon::MenuPlay,
            Action::Browse => Icon::MenuBrowse,
            Action::Replays => Icon::MenuReplays,
            Action::Settings => Icon::MenuSettings,
            Action::Exit => Icon::MenuExit,
            Action::Back => Icon::MenuBack,
            Action::Mode(Mode::TimeAttack) => Icon::MenuTimeAttack,
            Action::Mode(Mode::Classic) => Icon::MenuClassic,
            Action::Race => Icon::MenuRace,
        }
    }

    /// The line under the label: what the mode is for, or that Race comes later.
    fn sub(self) -> Option<&'static str> {
        match self {
            Action::Mode(Mode::TimeAttack) => Some("Leaderboards"),
            Action::Mode(Mode::Classic) => Some("The game as made"),
            Action::Race => Some("Later"),
            _ => None,
        }
    }

    /// Race is not built: it shows faded and takes no focus.
    fn usable(self) -> bool {
        self != Action::Race
    }

    /// The width at rest, and focused or hovered.
    fn widths(self) -> (f32, f32) {
        match self {
            Action::Back => (BASE_W * 0.62, BASE_W * 0.62 * 1.5),
            Action::Mode(_) => (BASE_W * 1.15, BASE_W * 1.55),
            _ => (BASE_W, BASE_W * 1.5),
        }
    }

    /// The fill at rest, and focused or hovered: the button's own colour.
    fn fills(self) -> (Color, Color) {
        match self {
            Action::Play => (tokens::white(0.1), tokens::ACCENT),
            Action::Mode(m) => (
                tokens::over(m.accent(), if m == Mode::Classic { 0.24 } else { 0.22 }, tokens::GROUND),
                m.accent(),
            ),
            Action::Back => (tokens::white(0.08), tokens::white(0.18)),
            Action::Race => (tokens::white(0.06), tokens::white(0.06)),
            _ => (tokens::white(0.1), tokens::white(0.18)),
        }
    }

    /// Focused, a coloured button lifts on a glow of its colour (`0 10px 30px`, .35).
    fn glow(self) -> Option<Color> {
        match self {
            Action::Play => Some(tokens::ACCENT),
            Action::Mode(m) => Some(m.accent()),
            _ => None,
        }
    }
}

/// The band's buttons at `level`.
pub fn buttons(level: Level) -> &'static [Action] {
    match level {
        Level::Initial => &[],
        Level::Top => &[
            Action::Play,
            Action::Browse,
            Action::Replays,
            Action::Settings,
            Action::Exit,
        ],
        Level::Play => &[
            Action::Back,
            Action::Mode(Mode::TimeAttack),
            Action::Mode(Mode::Classic),
            Action::Race,
        ],
    }
}

/// The index of `action` at `level`.
fn index_of(level: Level, action: Action) -> usize {
    buttons(level).iter().position(|a| *a == action).unwrap_or(0)
}

/// The menu's flags: where a `--screenshot` check opens it.
pub struct MenuPlugin {
    /// Open at this level (`--menu-preview`), the Play level with Classic focused for `classic`.
    pub preview: Option<MenuPreview>,
}

/// `--menu-preview`: the menu's states, as the artboards draw them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum MenuPreview {
    /// B6.
    Initial,
    /// B7.
    Top,
    /// A1: Play open, Time Attack focused.
    Play,
    /// A2: Play open, Classic focused.
    Classic,
}

impl MenuPreview {
    fn state(self) -> (MainMenu, Mode) {
        let play = |m: Mode| MainMenu {
            level: Level::Play,
            focus: index_of(Level::Play, Action::Mode(m)),
        };
        match self {
            MenuPreview::Initial => (MainMenu::default(), Mode::TimeAttack),
            MenuPreview::Top => (
                MainMenu {
                    level: Level::Top,
                    focus: 0,
                },
                Mode::TimeAttack,
            ),
            MenuPreview::Play => (play(Mode::TimeAttack), Mode::TimeAttack),
            MenuPreview::Classic => (play(Mode::Classic), Mode::Classic),
        }
    }
}

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        let (menu, mode) = self.preview.map_or((MainMenu::default(), None), |p| {
            let (menu, mode) = p.state();
            (menu, Some(mode))
        });
        app.insert_resource(menu);
        if let Some(mode) = mode {
            app.insert_resource(mode);
        }
        app.add_systems(OnEnter(AppScreen::Menu), enter)
            .add_observer(on_activate)
            .add_systems(
                Update,
                (
                    keys.run_if(crate::settings_screen::closed),
                    hover,
                    follow_mode,
                    rebuild,
                    looks,
                    caption,
                    bob,
                )
                    .chain()
                    .run_if(in_state(AppScreen::Menu)),
            )
            // Laid out at the artboard's 1080 px height, scaled to the window as game select is.
            .add_systems(
                Update,
                super::select::scale
                    .run_if(in_state(AppScreen::Menu))
                    .run_if(any_with_component::<PrimaryWindow>),
            );
    }
}

/// The menu's root, built once per visit (it goes when the menu is left).
#[derive(Component)]
struct Root {
    /// The level its slots show.
    level: Level,
}

/// The camera the menu draws with.
#[derive(Component)]
struct MenuCamera;

/// A button of the band: its place in [`buttons`].
#[derive(Component, Clone, Copy, Debug)]
pub struct BandButton(pub usize);

/// The caption under the band, and its second line.
#[derive(Component)]
struct Caption;
#[derive(Component)]
struct CaptionLine;
#[derive(Component)]
struct CaptionSub;

/// The fruit's bobbing layer, and "Press any key".
#[derive(Component)]
struct Bob;
#[derive(Component)]
struct Pulse;

/// The glow behind the fruit, whose colour follows the mode.
#[derive(Component)]
struct ModeGlow;

/// Entering the menu: the camera, the backdrop's dim and the page for the current level.
#[allow(clippy::too_many_arguments, reason = "the screen's parts")]
fn enter(
    mut commands: Commands,
    mut backdrop: ResMut<Backdrop>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    mut menu: ResMut<MainMenu>,
    mode: Option<Res<Mode>>,
    shell: Option<Res<super::Shell>>,
    stats: Option<Res<crate::stats::Stats>>,
    shot: Option<Res<crate::view::ScreenshotTo>>,
    mut art: Local<Option<Option<Handle<Image>>>>,
    mut images: ResMut<Assets<Image>>,
) {
    let paths = shell
        .as_ref()
        .and_then(|s| s.library.as_deref())
        .map(super::select::packages)
        .unwrap_or_default();
    let cleared = stats.map_or(0, |s| s.book.totals().games_cleared);
    commands.insert_resource(Facts {
        games: paths.len(),
        cleared,
    });
    // Behind the menu, a game from the library: its key art, picked once a session (the first
    // in a `--screenshot` check, so the picture is the same every time).
    let art = art
        .get_or_insert_with(|| {
            let start = if shot.is_some() {
                0
            } else {
                chrono::Local::now().timestamp().unsigned_abs() as usize
            };
            pick_art(&paths, start).map(|image| images.add(image))
        })
        .clone();
    commands.spawn((
        Camera2d,
        Camera {
            order: 10,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        RenderLayers::none(),
        IsDefaultUiCamera,
        MenuCamera,
        DespawnOnExit(AppScreen::Menu),
    ));
    backdrop.art = art;
    backdrop.dim = DIM;
    for mut w in &mut window {
        w.title = super::select::TITLE.to_owned();
    }
    // Back from game select: the Play level, the mode focused. Change detection rebuilds.
    if menu.level == Level::Play {
        let mode = mode.as_deref().copied().unwrap_or_default();
        menu.focus = index_of(Level::Play, Action::Mode(mode));
    }
    menu.set_changed();
}

/// The key art of a game in `paths`, trying up to [`ART_TRIES`] of them from `start` on (a game
/// without art of its own is skipped).
fn pick_art(paths: &[std::path::PathBuf], start: usize) -> Option<Image> {
    let n = paths.len();
    (0..n.min(ART_TRIES)).find_map(|i| {
        let (_, files) = super::select::library::read(&paths[(start + i) % n]);
        files.as_ref().and_then(super::select::library::backdrop_art)
    })
}

/// How many packages the menu reads looking for key art.
const ART_TRIES: usize = 6;

/// The parts of the page that change with the level. Each is a slot (a full-page node that takes
/// no pointer) whose children are built from the level alone (`ui::live`): a level change
/// rebuilds these, and nothing else (UI foundation, #1684).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Glow,
    Row,
    Fruit,
    Wordmark,
}

/// A part shown only at the initial level (`Initial`), or only past it (`Open`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum ShownAt {
    Initial,
    Open,
}

/// A full-page node for a part of the page: it takes no pointer, so the parts under it stay
/// clickable.
fn page_node() -> (Node, Pickable) {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        Pickable::IGNORE,
    )
}

/// Builds the page once on entering, then follows the level: the slots are rebuilt for the new
/// level and the parts of the other level hidden. A focus change touches none of it (`looks` and
/// `caption` update the buttons in place). From the initial level the fruit slides and shrinks
/// into place; the band's buttons grow from nothing.
fn rebuild(world: &mut World, mut fruit: Local<Option<Handle<Image>>>) {
    if !world.is_resource_changed::<MainMenu>() {
        return;
    }
    let level = world.resource::<MainMenu>().level;
    let root = world.query::<(Entity, &Root)>().iter(world).next().map(|(e, r)| (e, r.level));
    let from = match root {
        Some((_, built)) if built == level => return,
        Some((e, built)) => {
            world.entity_mut(e).insert(Root { level });
            Some(built)
        }
        None => {
            let Some(camera) = world.query_filtered::<Entity, With<MenuCamera>>().iter(world).next() else {
                return;
            };
            if fruit.is_none() {
                *fruit = Some(
                    world
                        .resource_mut::<Assets<Image>>()
                        .add(crate::pause::to_bevy(&crate::pause::bundled_fruit())),
                );
            }
            spawn_page(world, camera, level);
            None
        }
    };
    let mode = world.get_resource::<Mode>().copied().unwrap_or_default();
    let focus = world.resource::<MainMenu>().focus;
    let animate = !world.get_resource::<crate::ui::motion::ReducedMotion>().is_some_and(|r| r.0);
    let slide = animate && from == Some(Level::Initial);
    let grow = animate && from.is_some();
    let fruit = fruit.clone().unwrap_or_default();
    let page_w = page_width(world);
    let slots: Vec<(Entity, Slot)> = world.query::<(Entity, &Slot)>().iter(world).map(|(e, s)| (e, *s)).collect();
    for (e, slot) in slots {
        let fruit = fruit.clone();
        crate::ui::live::refresh(world, e, &level, |c, level| {
            let level = *level;
            match slot {
                Slot::Glow => {
                    c.spawn(mode_glow(level, mode));
                }
                Slot::Row if level != Level::Initial => {
                    c.spawn(row(level, focus, grow));
                }
                Slot::Row => {}
                Slot::Fruit => {
                    c.spawn(fruit_block(level, fruit, slide, page_w));
                }
                Slot::Wordmark => {
                    c.spawn(wordmark(level, slide, page_w));
                }
            }
        });
    }
    let initial = level == Level::Initial;
    let mut parts = world.query::<(&ShownAt, &mut Visibility)>();
    for (at, mut v) in parts.iter_mut(world) {
        let want = if (*at == ShownAt::Initial) == initial {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *v != want {
            *v = want;
        }
    }
}

/// The page: built once per visit to the menu. The slots are filled in by [`rebuild`].
fn spawn_page(world: &mut World, camera: Entity, level: Level) {
    let mode = world.get_resource::<Mode>().copied().unwrap_or_default();
    let empty = crate::stats::Book::default();
    let player = {
        let book = world.get_resource::<crate::stats::Stats>().map_or(&empty, |s| &s.book);
        Card::from_book(book, &profile_name(), COLLECTION_GAMES)
    };
    let shown = |at: ShownAt| {
        (
            at,
            if (at == ShownAt::Initial) == (level == Level::Initial) {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
        )
    };
    world
        .spawn((
            Root { level },
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            UiTargetCamera(camera),
            DespawnOnExit(AppScreen::Menu),
        ))
        .with_children(|page| {
            // The middle of the artboard (glow, band, fruit, caption) stays centred when a
            // narrow window makes the page taller than 1080 design px (#1692).
            page.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    top: Val::Percent(50.0),
                    height: Val::Px(DESIGN_H),
                    margin: UiRect::top(Val::Px(-DESIGN_H / 2.0)),
                    ..default()
                },
                Pickable::IGNORE,
            ))
            .with_children(|page| {
                page.spawn((Slot::Glow, page_node()));
                page.spawn((band(), shown(ShownAt::Open)));
                page.spawn((Slot::Row, page_node()));
                page.spawn((caption_block(), shown(ShownAt::Open)));
                page.spawn((Slot::Fruit, page_node()));
                page.spawn((Slot::Wordmark, page_node()));
                page.spawn((press_any_key(), shown(ShownAt::Initial)));
            });
            page.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Px(0.0),
                    ..default()
                },
                shown(ShownAt::Open),
                children![toolbar(mode)],
            ));
            page.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(EDGE),
                    bottom: Val::Px(EDGE_BOTTOM),
                    ..default()
                },
                shown(ShownAt::Open),
                children![card(&player, Variant::Footer)],
            ));
            page.spawn(footer());
        });
}

/// The wide glow over the page, centred on the fruit: the accent at .12 (.16 when the fruit is
/// alone, .14 in Classic), gone at 38 % of the way to the far corner.
fn mode_glow(level: Level, mode: Mode) -> impl Bundle {
    let (cx, cy) = fruit_centre(level);
    // CSS `radial-gradient(ellipse at x y, c 0%, transparent 38%)`: the far corner's ellipse,
    // 38 % of it. About 908 x 328 px from (230, 470) on a 1920 x 1080 page.
    let (rx, ry) = (908.0, 328.0);
    let (left, margin) = place_x(level, cx, 2.0 * ry);
    (
        ModeGlow,
        Node {
            position_type: PositionType::Absolute,
            left,
            margin,
            top: Val::Px(cy - ry),
            width: Val::Px(2.0 * ry),
            height: Val::Px(2.0 * ry),
            ..default()
        },
        UiTransform {
            scale: Vec2::new(rx / ry, 1.0),
            ..default()
        },
        BackgroundGradient::from(glow_gradient(glow_color(level, mode))),
    )
}

fn glow_color(level: Level, mode: Mode) -> Color {
    let a = match (level, mode) {
        (Level::Initial, _) => 0.16,
        (_, Mode::Classic) => 0.14,
        (_, Mode::TimeAttack) => 0.12,
    };
    let accent = if level == Level::Initial {
        tokens::ACCENT
    } else {
        mode.accent()
    };
    tokens::over(accent, a, tokens::GROUND)
}

fn glow_gradient(color: Color) -> RadialGradient {
    RadialGradient::new(
        UiPosition::CENTER,
        RadialGradientShape::ClosestSide,
        vec![
            ColorStop::new(color, Val::Percent(0.0)),
            ColorStop::new(color.with_alpha(0.0), Val::Percent(100.0)),
        ],
    )
}

/// The band behind the buttons: full width, `rgba(8,9,14,.25)` at the left to .78 at 30 %.
fn band() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(CY - BAND_H / 2.0),
            height: Val::Px(BAND_H),
            ..default()
        },
        BackgroundGradient::from(LinearGradient::to_right(vec![
            ColorStop::new(tokens::dark(SHADE, 0.25), Val::Percent(0.0)),
            ColorStop::new(tokens::dark(SHADE, 0.78), Val::Percent(30.0)),
            ColorStop::new(tokens::dark(SHADE, 0.78), Val::Percent(100.0)),
        ])),
    )
}

/// The row of buttons for `level`, `focus` focused; `grow`: each grows from zero width.
fn row(level: Level, focus: usize, grow: bool) -> impl Bundle {
    let actions = buttons(level);
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(ROW_X),
            top: Val::Px(CY - BAND_H / 2.0),
            height: Val::Px(BAND_H),
            column_gap: Val::Px(GAP),
            ..default()
        },
        Children::spawn(SpawnIter(
            actions
                .iter()
                .enumerate()
                .map(move |(i, a)| band_button(i, *a, i == focus, grow))
                .collect::<Vec<_>>()
                .into_iter(),
        )),
    )
}

fn band_button(index: usize, action: Action, focused: bool, grow: bool) -> impl Bundle {
    let (rest_w, focus_w) = action.widths();
    let w = if focused && action.usable() { focus_w } else { rest_w };
    let width = if grow {
        Tween::<Width>::new(0.0, w, GROW_SECONDS, Ease::OutExpo)
    } else {
        Tween::<Width>::at_rest(w, WIDEN_SECONDS, Ease::OutQuint)
    };
    let label = Type::barlow(21.0, 700);
    (
        BandButton(index),
        Button,
        Hovered::default(),
        Node {
            width: Val::Px(w),
            height: Val::Px(BAND_H),
            flex_shrink: 0.0,
            ..default()
        },
        button_shape(action, focused),
        UiOpacity(if action.usable() { 1.0 } else { 0.45 }),
        width,
        // The content is clipped to the box (a button growing from nothing), the shape is not:
        // its shear reaches past the box.
        children![(
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexEnd,
                row_gap: Val::Px(8.0),
                padding: UiRect::new(Val::Px(26.0), Val::Px(0.0), Val::Px(0.0), Val::Px(18.0)),
                overflow: Overflow::clip_x(),
                ..default()
            },
            Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
                c.spawn(icon(action.icon(), 32.0, Color::WHITE));
                c.spawn((text(action.label(), label, Color::WHITE), TextLayout::no_wrap()));
                if let Some(sub) = action.sub() {
                    c.spawn((
                        text(sub, Type::barlow(14.0, 600), Color::WHITE.with_alpha(0.86)),
                        TextLayout::no_wrap(),
                    ));
                }
            })),
        )],
    )
}

fn button_shape(action: Action, focused: bool) -> Shape {
    let (rest, own) = action.fills();
    let on = focused && action.usable();
    let shape = Shape::fill(if on { own } else { rest });
    match action.glow() {
        Some(c) if on => shape.with_shadow(10.0, 30.0, tokens::over(c, 0.35, tokens::GROUND)),
        _ => shape,
    }
}

/// The caption under the band: what the focused button is for, and a second line.
fn caption_block() -> impl Bundle {
    (
        Caption,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(ROW_X + 10.0),
            top: Val::Px(CY + BAND_H / 2.0 + 22.0),
            width: Val::Px(CAPTION_W),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            ..default()
        },
        children![
            (
                text_leading("", Type::barlow(17.0, 400), BRIGHT, 1.4),
                CaptionLine
            ),
            (text("", Type::barlow(14.0, 400), tokens::MUTED), CaptionSub),
        ],
    )
}

/// The fruit's centre at `level`, px.
fn fruit_centre(level: Level) -> (f32, f32) {
    let size = fruit_size(level);
    match level {
        Level::Initial => (960.0, CY_INITIAL),
        _ => (FRUIT_X + size.x / 2.0, CY - 6.0),
    }
}

fn fruit_size(level: Level) -> Vec2 {
    let s = if level == Level::Initial {
        FRUIT_SCALE_INITIAL
    } else {
        FRUIT_SCALE
    };
    (FRUIT * s).round()
}

/// The page's width in design px: the window's width over the UI scale. The page is 1080 tall
/// and at least `select::MIN_WIDTH` wide, so it is 1920 only at 16:9.
fn page_width(world: &mut World) -> f32 {
    let scale = world.get_resource::<UiScale>().map_or(1.0, |s| s.0);
    let width = world
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .iter(world)
        .next()
        .map(|w| w.width());
    match width {
        Some(w) if w >= 1.0 && scale > 0.0 => w / scale,
        _ => DESIGN_W,
    }
}

/// Where a box `w` wide centred at artboard x `cx` goes: `left` and its left margin. The middle
/// of the page (B6, the fruit alone) is the middle of the window at any width, so there the box
/// keeps to 50 % instead of to x 960, which is the middle only at 16:9; beside the band it keeps
/// to the left edge as the artboard does.
fn place_x(level: Level, cx: f32, w: f32) -> (Val, UiRect) {
    if level == Level::Initial {
        (Val::Percent(50.0), UiRect::left(Val::Px(cx - DESIGN_W / 2.0 - w / 2.0)))
    } else {
        (Val::Px(cx - w / 2.0), UiRect::default())
    }
}

/// The delicious fruit with its red halo and a hard shadow 1 fruit-pixel below; `slide`: it
/// comes from the middle of the page (`page_w` wide), shrinking.
fn fruit_block(level: Level, fruit: Handle<Image>, slide: bool, page_w: f32) -> impl Bundle {
    let size = fruit_size(level);
    let (cx, cy) = fruit_centre(level);
    let pixel = size.x / FRUIT.x;
    let frame = Rect::new(0.0, 0.0, FRUIT.x, FRUIT.y);
    let (dx, dy, scale) = slide_from(level, page_w);
    let halo = 230.0 * size.x / 160.0;
    let (left, margin) = place_x(level, cx, size.x);
    (
        Node {
            position_type: PositionType::Absolute,
            left,
            margin,
            top: Val::Px(cy - size.y / 2.0),
            width: Val::Px(size.x),
            height: Val::Px(size.y),
            ..default()
        },
        UiTransform::default(),
        Tween::<OffsetX>::new(if slide { dx } else { 0.0 }, 0.0, SLIDE_SECONDS, Ease::OutExpo),
        Tween::<OffsetY>::new(if slide { dy } else { 0.0 }, 0.0, SLIDE_SECONDS, Ease::OutExpo),
        Tween::<Scale>::new(if slide { scale } else { 1.0 }, 1.0, SLIDE_SECONDS, Ease::OutExpo),
        children![(
            Bob,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            UiTransform::default(),
            children![
                (
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(size.x / 2.0 - halo / 2.0),
                        top: Val::Px(size.y / 2.0 - halo / 2.0),
                        width: Val::Px(halo),
                        height: Val::Px(halo),
                        ..default()
                    },
                    BackgroundGradient::from(glow_gradient(tokens::over(
                        tokens::ACCENT,
                        0.32,
                        tokens::GROUND
                    ))),
                ),
                (
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(pixel.round()),
                        width: Val::Px(size.x),
                        height: Val::Px(size.y),
                        ..default()
                    },
                    ImageNode {
                        image: fruit.clone(),
                        rect: Some(frame),
                        color: tokens::black(0.35),
                        ..default()
                    },
                ),
                (
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(size.x),
                        height: Val::Px(size.y),
                        ..default()
                    },
                    ImageNode {
                        image: fruit,
                        rect: Some(frame),
                        ..default()
                    },
                ),
            ],
        )],
    )
}

/// How far the fruit at `level` starts from where it ends when it leaves the middle of a page
/// `page_w` wide: x, y and scale (1 when it does not move).
fn slide_from(level: Level, page_w: f32) -> (f32, f32, f32) {
    if level == Level::Initial {
        return (0.0, 0.0, 1.0);
    }
    let (_, fy) = fruit_centre(Level::Initial);
    let (tx, ty) = fruit_centre(level);
    (
        page_w / 2.0 - tx,
        fy - ty,
        fruit_size(Level::Initial).x / fruit_size(level).x,
    )
}

/// The working title and its logo-slot pill under the fruit (72 px alone, 40 px beside the band).
fn wordmark(level: Level, slide: bool, page_w: f32) -> impl Bundle {
    let initial = level == Level::Initial;
    let size = fruit_size(level);
    let (cx, cy) = fruit_centre(level);
    let (w, top, font) = if initial {
        (700.0, cy + size.y / 2.0 + 34.0, 72.0)
    } else {
        (420.0, CY + BAND_H / 2.0 + 22.0, 40.0)
    };
    let (dx, dy) = if slide {
        // From under the fruit in the middle (B6) to under the band.
        let from = fruit_centre(Level::Initial);
        let from_top = from.1 + fruit_size(Level::Initial).y / 2.0 + 34.0;
        (page_w / 2.0 - cx, from_top - top)
    } else {
        (0.0, 0.0)
    };
    let (left, margin) = place_x(level, cx, w);
    (
        Node {
            position_type: PositionType::Absolute,
            left,
            margin,
            top: Val::Px(top),
            width: Val::Px(w),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(8.0),
            ..default()
        },
        UiTransform::default(),
        Tween::<OffsetX>::new(dx, 0.0, SLIDE_SECONDS, Ease::OutExpo),
        Tween::<OffsetY>::new(dy, 0.0, SLIDE_SECONDS, Ease::OutExpo),
        children![
            (
                text(
                    "IWBTG Collection",
                    Type::condensed(font, 700).italic(),
                    tokens::INK
                ),
                TextLayout::no_wrap(),
                TextShadow {
                    offset: Vec2::new(0.0, 4.0),
                    color: tokens::black(0.5),
                },
            ),
            (
                Node {
                    height: Val::Px(22.0),
                    padding: UiRect::horizontal(Val::Px(10.0)),
                    align_items: AlignItems::Center,
                    ..default()
                },
                Shape {
                    radius: 8.0,
                    ..Shape::fill(tokens::white(0.12))
                },
                children![text(
                    "Working title · logo slot (#201)",
                    Type::barlow(12.0, 600),
                    tokens::SOFT
                )],
            ),
        ],
    )
}

fn press_any_key() -> impl Bundle {
    let (_, cy) = fruit_centre(Level::Initial);
    let top = cy + fruit_size(Level::Initial).y / 2.0 + 190.0;
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(top),
            justify_content: JustifyContent::Center,
            ..default()
        },
        UiOpacity(1.0),
        Pulse,
        children![text("Press any key", Type::barlow(17.0, 600), BRIGHT)],
    )
}

/// The build and what the project is, bottom left.
fn footer() -> impl Bundle {
    let small = Type::barlow(13.0, 400);
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(EDGE),
            bottom: Val::Px(EDGE_BOTTOM),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            ..default()
        },
        children![
            rich(vec![
                (
                    // The version, and the CI build it is when CI compiled it (#1675).
                    match crate::updater::BUILD_COMMIT {
                        Some(commit) => format!(
                            "v{} · {}",
                            env!("CARGO_PKG_VERSION"),
                            crate::locale::format("menu_build", &[("commit", &crate::updater::short(commit))])
                        ),
                        None => format!("v{}", env!("CARGO_PKG_VERSION")),
                    },
                    Type::barlow(13.0, 700),
                    BRIGHT
                ),
                (
                    " · A fan project, not affiliated with Kayin · non-commercial".to_owned(),
                    small,
                    tokens::MUTED
                ),
            ]),
            text("Credits · Report a bug", small, tokens::MUTED),
        ],
    )
}

/// What the caption says for the focused button at `level`: what it is, and what you have.
/// Classic's progress comes from the games' save files, which nothing reads yet (m5 A4), so its
/// second line counts the games only.
fn caption_for(level: Level, focus: usize, mode: Mode, facts: Facts) -> (&'static str, String) {
    let games = format!("{} game{}", facts.games, if facts.games == 1 { "" } else { "s" });
    match level {
        Level::Initial => ("", String::new()),
        Level::Top => ("Pick Time Attack or Classic next.", "Enter plays · Esc exits".to_owned()),
        Level::Play => {
            let shown = match buttons(level).get(focus) {
                Some(Action::Mode(m)) => *m,
                _ => mode,
            };
            match shown {
                Mode::TimeAttack => (
                    "The game stripped to the run: no title or menus, the timer starts on your first input, and a clear can reach the leaderboards.",
                    format!("{games} · {} cleared", facts.cleared),
                ),
                Mode::Classic => (
                    "The game as it was made: its title screen, its saves, its own difficulty select. Your stats stay yours; no leaderboard.",
                    games,
                ),
            }
        }
    }
}

/// The keys: any key leaves the initial level; Left and Right move the focus over the usable
/// buttons, Enter or Space chooses, Escape steps back a level (at the top, exits).
#[allow(clippy::too_many_arguments, reason = "the screen's keys")]
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    focus: Option<Res<InputFocus>>,
    widgets: Query<(), With<Button>>,
    mut menu: ResMut<MainMenu>,
    mut mode: Option<ResMut<Mode>>,
    mut next: ResMut<NextState<AppScreen>>,
    mut commands: Commands,
) {
    if menu.level == Level::Initial {
        let clicked = mouse.is_some_and(|m| m.get_just_pressed().next().is_some());
        if keys.get_just_pressed().next().is_some() || clicked {
            *menu = MainMenu {
                level: Level::Top,
                focus: 0,
            };
        }
        return;
    }
    let actions = buttons(menu.level);
    let step = |from: usize, by: isize| -> usize {
        let n = actions.len() as isize;
        let mut i = from as isize;
        for _ in 0..n {
            i += by;
            if i < 0 || i >= n {
                return from;
            }
            if actions[i as usize].usable() {
                return i as usize;
            }
        }
        from
    };
    if keys.just_pressed(KeyCode::ArrowRight) {
        let f = step(menu.focus, 1);
        if f != menu.focus {
            menu.focus = f;
        }
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        let f = step(menu.focus, -1);
        if f != menu.focus {
            menu.focus = f;
        }
    }
    // A focused mode is the mode: the toolbar and the accent follow it.
    if let (Some(Action::Mode(m)), Some(mode)) = (actions.get(menu.focus), mode.as_mut())
        && **mode != *m
    {
        **mode = *m;
    }
    // Enter on a focused widget (a toolbar tab) is the widget's.
    let on_widget = focus.and_then(|f| f.get()).is_some_and(|e| widgets.contains(e));
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) && !on_widget
        && let Some(action) = actions.get(menu.focus).copied()
    {
        choose(action, &mut menu, mode.as_deref_mut(), &mut next, &mut commands);
    }
    if keys.just_pressed(KeyCode::Escape) {
        match menu.level {
            Level::Play => {
                *menu = MainMenu {
                    level: Level::Top,
                    focus: 0,
                };
            }
            _ => {
                commands.write_message(AppExit::Success);
            }
        }
    }
}

/// What choosing `action` does.
fn choose(
    action: Action,
    menu: &mut MainMenu,
    mode: Option<&mut Mode>,
    next: &mut NextState<AppScreen>,
    commands: &mut Commands,
) {
    match action {
        Action::Play => {
            let m = mode.map_or(Mode::TimeAttack, |m| *m);
            *menu = MainMenu {
                level: Level::Play,
                focus: index_of(Level::Play, Action::Mode(m)),
            };
        }
        Action::Back => {
            *menu = MainMenu {
                level: Level::Top,
                focus: 0,
            };
        }
        Action::Mode(m) => {
            if let Some(mode) = mode {
                *mode = m;
            }
            next.set(super::mode_home(Some(m)));
        }
        Action::Exit => {
            commands.write_message(AppExit::Success);
        }
        Action::Settings => {
            commands.queue(|w: &mut World| {
                if let Some(mut o) = w.get_resource_mut::<crate::settings_screen::Overlay>() {
                    o.show();
                }
            });
        }
        // Their screens come with their groups (#50, replays); Race is phase 8.
        Action::Browse | Action::Replays | Action::Race => {}
    }
}

/// A click on a band button chooses it.
fn on_activate(
    activate: On<Activate>,
    band: Query<&BandButton>,
    mut menu: Option<ResMut<MainMenu>>,
    mut mode: Option<ResMut<Mode>>,
    next: Option<ResMut<NextState<AppScreen>>>,
    mut commands: Commands,
) {
    let (Ok(button), Some(menu), Some(mut next)) = (band.get(activate.entity), menu.as_mut(), next)
    else {
        return;
    };
    let Some(action) = buttons(menu.level).get(button.0).copied() else {
        return;
    };
    if !action.usable() {
        return;
    }
    choose(action, menu, mode.as_deref_mut(), &mut next, &mut commands);
}

/// Hovering a usable button focuses it. Not in a `--screenshot` check: where the desktop's
/// pointer happens to be must not change the picture.
fn hover(
    hovered: Query<(&BandButton, &Hovered), Changed<Hovered>>,
    shot: Option<Res<crate::view::ScreenshotTo>>,
    mut menu: ResMut<MainMenu>,
    mut mode: Option<ResMut<Mode>>,
) {
    if shot.is_some() {
        return;
    }
    for (button, hovered) in &hovered {
        let usable = buttons(menu.level)
            .get(button.0)
            .is_some_and(|a| a.usable());
        if hovered.get() && usable && menu.focus != button.0 {
            menu.focus = button.0;
            if let (Some(Action::Mode(m)), Some(mode)) =
                (buttons(menu.level).get(button.0), mode.as_mut())
            {
                **mode = *m;
            }
        }
    }
}

/// A mode picked in the toolbar moves the focus to it on the Play level; the glow follows.
fn follow_mode(
    mode: Option<Res<Mode>>,
    mut menu: ResMut<MainMenu>,
    mut glows: Query<&mut BackgroundGradient, With<ModeGlow>>,
) {
    let Some(mode) = mode else {
        return;
    };
    if !mode.is_changed() {
        return;
    }
    if menu.level == Level::Play {
        let want = index_of(Level::Play, Action::Mode(*mode));
        if menu.focus != want && matches!(buttons(Level::Play)[menu.focus], Action::Mode(_)) {
            menu.focus = want;
        }
    }
    for mut g in &mut glows {
        *g = BackgroundGradient::from(glow_gradient(glow_color(menu.level, *mode)));
    }
}

/// Each band button widens and fills when focused.
fn looks(
    menu: Res<MainMenu>,
    mut buttons_q: Query<(&BandButton, &mut Shape, &mut Tween<Width>)>,
) {
    if !menu.is_changed() {
        return;
    }
    let actions = buttons(menu.level);
    for (button, mut shape, mut width) in &mut buttons_q {
        let Some(action) = actions.get(button.0).copied() else {
            continue;
        };
        let focused = button.0 == menu.focus;
        let want = button_shape(action, focused);
        if *shape != want {
            *shape = want;
        }
        let (rest, focus) = action.widths();
        let w = if focused && action.usable() { focus } else { rest };
        if width.finished() || width.to > 0.0 {
            if width.finished() {
                width.ease = Ease::OutQuint;
                width.duration = WIDEN_SECONDS;
            }
            width.retarget(w);
        }
    }
}

/// The caption's first line: its text and colour.
type CaptionText = (&'static mut Text, &'static mut Ink);

/// The caption sits under the focused button and says what it is for.
fn caption(
    menu: Res<MainMenu>,
    mode: Option<Res<Mode>>,
    facts: Option<Res<Facts>>,
    mut boxes: Query<&mut Node, With<Caption>>,
    mut line: Query<CaptionText, (With<CaptionLine>, Without<CaptionSub>)>,
    mut sub: Query<&mut Text, (With<CaptionSub>, Without<CaptionLine>)>,
    added: Query<(), Added<Caption>>,
) {
    let mode_changed = mode.as_ref().is_some_and(|m| m.is_changed());
    if !menu.is_changed() && added.is_empty() && !mode_changed {
        return;
    }
    let mode = mode.as_deref().copied().unwrap_or_default();
    let actions = buttons(menu.level);
    // Left of the focused button; at the top level (no mode button) under the first.
    let anchor = match menu.level {
        Level::Play => menu.focus,
        _ => 0,
    };
    let x = ROW_X
        + actions
            .iter()
            .take(anchor)
            .enumerate()
            .map(|(i, a)| {
                let (rest, focus) = a.widths();
                (if i == menu.focus { focus } else { rest }) + GAP
            })
            .sum::<f32>();
    for mut node in &mut boxes {
        node.left = Val::Px(x + 10.0);
    }
    let facts = facts.as_deref().copied().unwrap_or_default();
    let (first, second) = caption_for(menu.level, menu.focus, mode, facts);
    for (mut t, _) in &mut line {
        t.0 = Type::barlow(17.0, 400).set(first);
    }
    for mut t in &mut sub {
        t.0 = Type::barlow(14.0, 400).set(&second);
    }
}

/// The fruit bobs; "Press any key" pulses between .55 and 1.
fn bob(
    time: Res<Time>,
    reduced: Option<Res<crate::ui::motion::ReducedMotion>>,
    mut fruit: Query<&mut UiTransform, With<Bob>>,
    mut pulse: Query<&mut UiOpacity, With<Pulse>>,
) {
    let still = reduced.is_some_and(|r| r.0);
    let t = time.elapsed_secs();
    let wave = |period: f32| 0.5 - 0.5 * (t / period * std::f32::consts::TAU).cos();
    let y = if still { 0.0 } else { -BOB * wave(BOB_SECONDS) };
    for mut tr in &mut fruit {
        tr.translation.y = Val::Px(y);
    }
    let a = if still { 1.0 } else { 0.55 + 0.45 * wave(PULSE_SECONDS) };
    for mut o in &mut pulse {
        o.0 = a;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Backdrop>()
            .init_resource::<Mode>()
            .init_resource::<Assets<Image>>()
            .insert_state(AppScreen::Menu)
            .add_plugins(MenuPlugin { preview: None });
        app.update();
        app
    }

    fn press(app: &mut App, key: KeyCode) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        app.update();
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.release(key);
        input.clear();
        app.update();
    }

    fn menu(app: &App) -> MainMenu {
        *app.world().resource::<MainMenu>()
    }

    /// Any key opens the top level; Play opens the modes with the current mode focused; Escape
    /// steps back.
    #[test]
    fn the_levels_open_and_close_with_the_keys() {
        let mut app = app();
        assert_eq!(menu(&app).level, Level::Initial);
        press(&mut app, KeyCode::KeyQ);
        assert_eq!(menu(&app), MainMenu { level: Level::Top, focus: 0 });
        press(&mut app, KeyCode::Enter);
        assert_eq!(menu(&app).level, Level::Play);
        assert_eq!(
            buttons(Level::Play)[menu(&app).focus],
            Action::Mode(Mode::TimeAttack)
        );
        press(&mut app, KeyCode::Escape);
        assert_eq!(menu(&app).level, Level::Top);
        press(&mut app, KeyCode::Escape);
        assert_eq!(app.should_exit(), Some(AppExit::Success), "Escape at the top exits");
    }

    /// Focusing Classic picks Classic; Race takes no focus; choosing a mode opens its screen.
    #[test]
    fn a_focused_mode_is_the_mode_and_choosing_it_opens_its_screen() {
        let mut app = app();
        press(&mut app, KeyCode::Space);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::ArrowRight);
        assert_eq!(*app.world().resource::<Mode>(), Mode::Classic);
        press(&mut app, KeyCode::ArrowRight);
        assert_eq!(
            buttons(Level::Play)[menu(&app).focus],
            Action::Mode(Mode::Classic),
            "Race is not built: the focus stays"
        );
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            *app.world().resource::<State<AppScreen>>().get(),
            AppScreen::Shelf,
            "Classic's screen is the shelf"
        );
        assert_eq!(*app.world().resource::<Mode>(), Mode::Classic);
    }

    /// The page is built once: moving the focus spawns and despawns nothing, and a level change
    /// rebuilds only the level's slots (UI foundation, #1684).
    #[test]
    fn the_page_updates_in_place() {
        use crate::ui::latency::node_set;
        let mut app = app();
        let root = app.world_mut().query_filtered::<Entity, With<Root>>().single(app.world()).expect("the page is built");
        let lasting = |app: &mut App| -> Vec<Entity> {
            let mut q = app.world_mut().query_filtered::<Entity, (With<ShownAt>, Without<Slot>)>();
            let mut v: Vec<Entity> = q.iter(app.world()).collect();
            v.sort();
            v
        };
        let parts = lasting(&mut app);
        assert_eq!(parts.len(), 5, "band, caption, press any key, toolbar, card");
        press(&mut app, KeyCode::KeyQ);
        let top = node_set(app.world_mut());
        for key in [KeyCode::ArrowRight, KeyCode::ArrowRight, KeyCode::ArrowLeft] {
            press(&mut app, key);
            assert_eq!(node_set(app.world_mut()), top, "{key:?} moved the focus in place");
        }
        assert_eq!(menu(&app).focus, 1);
        press(&mut app, KeyCode::ArrowLeft);
        press(&mut app, KeyCode::Enter);
        assert_eq!(menu(&app).level, Level::Play);
        press(&mut app, KeyCode::Escape);
        assert_eq!(menu(&app).level, Level::Top);
        assert!(app.world().get_entity(root).is_ok(), "the same page");
        assert_eq!(lasting(&mut app), parts, "the lasting parts were kept");
        let open = |app: &mut App, at: ShownAt| {
            let mut q = app.world_mut().query::<(&ShownAt, &Visibility)>();
            q.iter(app.world()).filter(|(a, _)| **a == at).all(|(_, v)| *v == Visibility::Inherited)
        };
        assert!(open(&mut app, ShownAt::Open) && !open(&mut app, ShownAt::Initial));
    }

    /// The caption follows the focus, and sits under the focused mode.
    #[test]
    fn the_caption_says_what_the_focused_mode_is() {
        let facts = Facts {
            games: 20,
            cleared: 13,
        };
        let (line, sub) = caption_for(Level::Play, 2, Mode::TimeAttack, facts);
        assert!(line.starts_with("The game as it was made"), "{line}");
        assert_eq!(sub, "20 games");
        let (line, sub) = caption_for(Level::Play, 0, Mode::TimeAttack, facts);
        assert!(line.starts_with("The game stripped"), "Back keeps the mode's caption");
        assert_eq!(sub, "20 games · 13 cleared");
    }

    /// Every level's band, its widest button focused, ends inside the narrowest page the shell
    /// lays out, with the shear's overhang and the right margin (#1692).
    #[test]
    fn the_widest_band_fits_the_narrowest_page() {
        for level in [Level::Top, Level::Play] {
            let actions = buttons(level);
            let widest = (0..actions.len())
                .map(|focus| {
                    actions
                        .iter()
                        .enumerate()
                        .map(|(i, a)| if i == focus { a.widths().1 } else { a.widths().0 })
                        .sum::<f32>()
                })
                .fold(0.0, f32::max);
            let end = ROW_X + widest + GAP * (actions.len() - 1) as f32 + BAND_H * 0.1;
            assert!(
                end + EDGE <= super::super::select::MIN_WIDTH,
                "{level:?} ends at {end}"
            );
        }
    }

    /// The artboard's numbers: the fruit 160 x 182 at (150, 373) beside the band, 231 x 264 in
    /// the middle.
    #[test]
    fn the_fruit_sits_where_the_artboard_puts_it() {
        assert_eq!(fruit_size(Level::Top), Vec2::new(160.0, 182.0));
        let (cx, cy) = fruit_centre(Level::Top);
        assert_eq!((cx - 80.0, cy - 91.0), (150.0, 373.0));
        assert_eq!(fruit_size(Level::Initial), Vec2::new(231.0, 264.0));
    }

    /// The fruit alone keeps to the middle of the window at any width (B6), not to x 960, which
    /// is the middle only at 16:9; leaving it, the fruit slides from that middle.
    #[test]
    fn the_fruit_alone_stays_in_the_middle_at_any_width() {
        let size = fruit_size(Level::Initial);
        let (cx, _) = fruit_centre(Level::Initial);
        let (left, margin) = place_x(Level::Initial, cx, size.x);
        assert_eq!(left, Val::Percent(50.0));
        assert_eq!(margin.left, Val::Px(-size.x / 2.0));
        let (left, margin) = place_x(Level::Top, fruit_centre(Level::Top).0, 160.0);
        assert_eq!((left, margin.left), (Val::Px(150.0), Val::Px(0.0)));
        let (tx, _) = fruit_centre(Level::Top);
        assert_eq!(slide_from(Level::Top, 1920.0).0, 960.0 - tx);
        assert_eq!(slide_from(Level::Top, 3000.0).0, 1500.0 - tx);
    }
}
