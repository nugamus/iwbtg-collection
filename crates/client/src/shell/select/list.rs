//! The game select list (#149, `Main.dc.html`, right side): a sheared panel per game with its
//! own key art, and under the selected game a panel per map. The selected game and map slide
//! 25 px left; the list scrolls to keep the selection in view and dims while you work on the
//! left side ([`ListDim`], set by #148's left side).
//!
//! The search, group, sort and filters (#150, `filters.rs`) decide the rows ([`Shown`]). Each
//! game's panel and maps sit in a row placed by hand ([`place`]), not by flex layout, so a new
//! order slides every row to its place, a game that leaves fades out and one that comes back
//! fades in (OutQuint): the list never jumps.

use std::collections::HashMap;

use bevy::prelude::*;

use super::filters::{self, Filters, Row};
use super::library::{self, Library};
use crate::ui::fonts::Type;
use crate::ui::motion::{Ease, OffsetX, OffsetY, Opacity, Tween};
use crate::ui::shape::{Band, FadeMask, Ink, Shape, Side, UiOpacity, Veil};
use crate::ui::tokens::{self, star_color};
use crate::ui::widgets::{ellipsis_text, panel, text};

/// Game panels are 72 px tall, map panels 45, 3 px apart (`Main.dc.html`).
pub const GAME_HEIGHT: f32 = 72.0;
pub const MAP_HEIGHT: f32 = 45.0;
pub const GAP: f32 = 3.0;
/// A group header's row (#150; the artboard groups by game, so it shows none: `revisit`).
pub const HEADER_HEIGHT: f32 = 34.0;
/// Panels run 1000 px wide, past the window's right edge.
const WIDTH: f32 = 1000.0;
/// A game panel starts 860 px from the window's right edge (1060 at 1920 wide); map panels
/// 30 px further right.
const FROM_RIGHT: f32 = 860.0;
const MAP_INDENT: f32 = 30.0;
/// The selected game and map slide this far left.
pub const SLIDE: f32 = 25.0;
/// The first panel's top (under the toolbar, m5 A3).
pub const TOP: f32 = 150.0 + super::UNDER_TOOLBAR;
/// The list keeps the selection centred between [`TOP`] and the footer's top (1080 less the
/// footer's 26 px margin and the 108 px player card). Under the toolbar the window keeps m4's
/// height and runs that much further down, behind the footer, as m5 A3 draws it.
pub const BOTTOM: f32 = 1080.0 - 134.0 + super::UNDER_TOOLBAR;
/// Panels that scroll above [`TOP`] fade out over this many px.
const FADE: f32 = 30.0;
/// Room left of the panels for the slide, the shear and the shadows.
const LEFT_ROOM: f32 = 60.0;
/// The list's opacity while you work on the left side. The artboards do not show it (#149,
/// `revisit`).
pub const DIM: f32 = 0.5;

/// The game a Cleared mark's fruit belongs to, so an arriving preview can replace the
/// placeholder (#1593).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FruitMark(pub usize);

/// The game and map picked. #148's left side reads it.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub game: usize,
    pub map: usize,
}

/// True while the player works on the left side (tabs, ranking); the list dims. Nothing sets it
/// until the left side exists (#148).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListDim(pub bool);

/// Asks for the previews of the shown games around the pick (the visible window and its
/// neighbours); the library then loads them off the UI thread (#1593).
pub(super) fn want_previews(
    sel: Res<Selection>,
    shown: Res<Shown>,
    lib: Option<ResMut<Library>>,
) {
    let Some(mut lib) = lib else {
        return;
    };
    if !sel.is_changed() && !shown.is_changed() {
        return;
    }
    let order = shown.order();
    let Some(pos) = order.iter().position(|game| *game == sel.game) else {
        lib.want(&[]);
        return;
    };
    let start = pos.saturating_sub(library::PREVIEW_RADIUS);
    let end = (pos + library::PREVIEW_RADIUS + 1).min(order.len());
    lib.want(&order[start..end]);
}

/// Draws previews as they arrive. A game whose preview has not arrived (or was evicted) keeps
/// its dark placeholder panel and the placeholder fruit (#1593).
pub(super) fn previews(
    mut lib: Option<ResMut<Library>>,
    mut images: ResMut<Assets<Image>>,
    mut seen: Local<u64>,
    mut panels: Query<(&GamePanel, &mut Shape)>,
    mut marks: Query<(&FruitMark, &mut ImageNode)>,
) {
    let Some(lib) = lib.as_mut() else {
        return;
    };
    lib.poll(&mut images);
    if lib.revision() == *seen {
        return;
    }
    *seen = lib.revision();
    for (game, mut shape) in &mut panels {
        shape.image = lib.art.get(game.0).cloned().flatten();
    }
    for (mark, mut image) in &mut marks {
        image.image = lib
            .fruit
            .get(mark.0)
            .cloned()
            .flatten()
            .unwrap_or_else(|| lib.placeholder_fruit.clone());
    }
}

/// The rows the list shows now, from the library and the [`Filters`] ([`filters::rows`]).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct Shown {
    pub rows: Vec<Row>,
}

impl Shown {
    /// The games shown, in order.
    pub fn order(&self) -> Vec<usize> {
        filters::order(&self.rows)
    }
}

/// A game's panel, by index in the library.
#[derive(Component, Clone, Copy, Debug)]
pub struct GamePanel(pub usize);

/// A map's panel: game and map index.
#[derive(Component, Clone, Copy, Debug)]
pub struct MapPanel(pub usize, pub usize);

/// A game's row: its panel and, under it, its maps. Placed by [`arrange`].
#[derive(Component, Clone, Copy, Debug)]
pub struct GameRow(pub usize);

/// A group header's row, by its name.
#[derive(Component, Clone, Debug)]
pub(super) struct HeaderRow(String);

/// A header on its way out: despawned once faded.
#[derive(Component)]
pub(super) struct Leaving;

/// The holder of a game's map panels, shown only for the selected game.
#[derive(Component)]
pub(super) struct Maps(usize);

/// The name on a map panel (bold when selected).
#[derive(Component)]
pub(super) struct MapName(usize, usize);

/// The column that scrolls.
#[derive(Component)]
pub(super) struct Column;

/// The list's frame, which dims.
#[derive(Component)]
pub(super) struct Frame;

/// The veil over a game panel's art: darker on the left, where the title is.
fn veil(selected: bool) -> Veil {
    let ink = |a: f32| Color::srgba(8.0 / 255.0, 9.0 / 255.0, 14.0 / 255.0, a);
    if selected {
        Veil::across(ink(0.82), ink(0.45), ink(0.0), 0.45)
    } else {
        Veil::across(ink(0.9), ink(0.6), ink(0.2), 0.45)
    }
}

fn game_shape(art: Option<Handle<Image>>, selected: bool) -> Shape {
    let shape = match art {
        Some(art) => Shape::fill(tokens::GROUND).with_image(art),
        None => Shape::fill(tokens::GROUND),
    };
    style_game(shape.with_veil(veil(selected)), selected)
}

/// The selected game's white outline and deeper shadow.
fn style_game(mut shape: Shape, selected: bool) -> Shape {
    shape.veil = Some(veil(selected));
    shape.ring = None;
    if selected {
        shape
            .with_ring(2.0, Color::srgba(1.0, 1.0, 1.0, 0.55))
            .with_shadow(10.0, 30.0, tokens::black(0.5))
    } else {
        shape.with_shadow(4.0, 12.0, tokens::black(0.25))
    }
}

/// A map panel's glass: dark, or light when selected, with its shadow.
fn style_map(mut shape: Shape, selected: bool) -> Shape {
    shape.shadow = None;
    if selected {
        shape.fill = tokens::white(0.26);
        shape.with_shadow(6.0, 18.0, tokens::black(0.4))
    } else {
        shape.fill = tokens::dark(tokens::CARD_SOLID, 0.82);
        shape
    }
}

/// The colour of a star rating; a map without one (#111) is muted.
fn stars_color(stars: Option<f32>) -> Color {
    stars.map_or(tokens::MUTED, star_color)
}

/// An upright rounded kit shape, so it fades with the list (`UiOpacity`), unlike a
/// `BackgroundColor`.
fn upright(fill: Color, radius: f32) -> Shape {
    Shape {
        shear: 0.0,
        radius,
        ..Shape::fill(fill)
    }
}

/// The colour dot of one map on a game panel.
fn dot(color: Color) -> impl Bundle {
    (
        Node {
            width: Val::Px(10.0),
            height: Val::Px(10.0),
            ..default()
        },
        upright(color, 3.0).with_ring(2.0, tokens::black(0.35)),
    )
}

/// The upright star pill of a map panel: 24 px, at least 64 wide, 8 px corners.
fn map_stars(stars: Option<f32>) -> impl Bundle {
    let label = stars.map_or_else(|| "★ –".to_owned(), |s| format!("★ {s:.2}"));
    (
        Node {
            height: Val::Px(24.0),
            min_width: Val::Px(64.0),
            padding: UiRect::horizontal(Val::Px(9.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_shrink: 0.0,
            ..default()
        },
        upright(stars_color(stars), 8.0),
        children![text(&label, Type::STARS, tokens::ON_LIGHT)],
    )
}

/// A row's node: placed at the column's top left and moved by its `OffsetY` tween.
fn row_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(LEFT_ROOM),
        top: Val::Px(FADE),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(GAP),
        ..default()
    }
}

/// Spawns the list for `lib` with `sel` picked and `shown` rows; its parts take their selected
/// look in `looks` and their places in `arrange`.
pub fn spawn(commands: &mut Commands, camera: Entity, lib: &Library, sel: Selection, shown: &Shown) {
    let maps: Vec<usize> = lib.games.iter().map(|g| g.maps.len()).collect();
    let placed = place(&shown.rows, &maps, sel);
    let column = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            UiTransform::default(),
            Tween::<OffsetY>::at_rest(placed.scroll(), tokens::motion::PANEL, Ease::OutQuint),
            Column,
        ))
        .id();
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(TOP - FADE),
                bottom: Val::Px(0.0),
                right: Val::Px(-(WIDTH - FROM_RIGHT)),
                width: Val::Px(WIDTH + LEFT_ROOM),
                overflow: Overflow::clip_y(),
                ..default()
            },
            FadeMask {
                top: FADE,
                bottom: 0.0,
            },
            UiOpacity(1.0),
            Tween::<Opacity>::at_rest(1.0, tokens::motion::PANEL, Ease::OutQuint),
            Frame,
            UiTargetCamera(camera),
            DespawnOnExit(super::AppScreen::Select),
        ))
        .add_child(column);
    for (i, row) in shown.rows.iter().enumerate() {
        if let Row::Header(name, count) = row {
            let header = header(name, *count, placed.ys[i], 1.0);
            let id = commands.spawn(header).id();
            commands.entity(column).add_child(id);
        }
    }
    for (g, game) in lib.games.iter().enumerate() {
        let records = lib.records.get(g).cloned().unwrap_or_default();
        let at = shown.rows.iter().position(|r| *r == Row::Game(g));
        let (y, opacity) = at.map_or((0.0, 0.0), |i| (placed.ys[i], 1.0));
        let group = commands
            .spawn((
                row_node(),
                GameRow(g),
                UiTransform::from_translation(Val2::px(0.0, y)),
                Tween::<OffsetY>::at_rest(y, tokens::motion::PANEL, Ease::OutQuint),
                UiOpacity(opacity),
                Tween::<Opacity>::at_rest(opacity, tokens::motion::PANEL, Ease::OutQuint),
                if at.is_some() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            ))
            .id();
        commands.entity(column).add_child(group);
        let dots: Vec<Color> = game.maps.iter().map(|m| stars_color(m.stars)).collect();
        let fruit = lib.fruit[g]
            .clone()
            .unwrap_or_else(|| lib.placeholder_fruit.clone());
        let game_panel = commands
            .spawn((
                panel(WIDTH, GAME_HEIGHT, game_shape(lib.art[g].clone(), false)),
                GamePanel(g),
                UiTransform::default(),
                Tween::<OffsetX>::at_rest(0.0, tokens::motion::SELECT, Ease::OutQuint),
                children![
                    (
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(2.0),
                            width: Val::Px(520.0),
                            min_width: Val::Px(0.0),
                            flex_shrink: 0.0,
                            ..default()
                        },
                        children![
                            ellipsis_text(&game.title, Type::barlow(23.0, 700), tokens::INK),
                            text(&game.creator, Type::barlow(14.0, 400), tokens::ON_ART),
                        ],
                    ),
                    (
                        Node {
                            column_gap: Val::Px(5.0),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        Children::spawn(SpawnIter(dots.into_iter().map(dot))),
                    ),
                ],
            ))
            .id();
        if records.cleared {
            let mark = commands
                .spawn((
                    Node {
                        column_gap: Val::Px(6.0),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    children![
                        (
                            Node {
                                width: Val::Px(14.0),
                                height: Val::Px(16.0),
                                ..default()
                            },
                            ImageNode::new(fruit),
                            Ink(Color::WHITE),
                            FruitMark(g),
                        ),
                        text("Cleared", Type::PILL_CAPS, Color::WHITE),
                    ],
                ))
                .id();
            commands.entity(game_panel).add_child(mark);
        }
        let maps = commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(GAP),
                    display: Display::None,
                    ..default()
                },
                Maps(g),
            ))
            .id();
        commands.entity(group).add_children(&[game_panel, maps]);
        for (m, map) in game.maps.iter().enumerate() {
            let pb = records.pbs.get(m).cloned().flatten();
            let (pb_text, pb_color) = match pb {
                Some(t) => (t, tokens::INK),
                None => ("No PB".to_owned(), tokens::MUTED),
            };
            let shape = Shape::fill(Color::NONE).with_band(Band {
                side: Side::Left,
                width: 6.0,
                color: stars_color(map.stars),
                fade_to: 0.0,
            });
            let mut node = panel(WIDTH, MAP_HEIGHT, style_map(shape, false));
            node.0.margin = UiRect::left(Val::Px(MAP_INDENT));
            node.0.padding = UiRect {
                left: Val::Px(26.0),
                right: Val::Px(22.0),
                ..default()
            };
            let panel = commands
                .spawn((
                    node,
                    MapPanel(g, m),
                    UiTransform::default(),
                    Tween::<OffsetX>::at_rest(0.0, tokens::motion::SELECT, Ease::OutQuint),
                    children![
                        map_stars(map.stars),
                        (
                            Node {
                                width: Val::Px(300.0),
                                flex_shrink: 0.0,
                                ..default()
                            },
                            children![(text(&map.name, Type::BODY, tokens::INK), MapName(g, m))],
                        ),
                        text(&pb_text, Type::barlow(15.0, 600), pb_color),
                    ],
                ))
                .id();
            commands.entity(maps).add_child(panel);
        }
    }
}

/// A group header at `y`, starting at `opacity`: the name in small capitals and the count,
/// muted, on no panel (the artboard draws none: `revisit`).
fn header(name: &str, count: usize, y: f32, opacity: f32) -> impl Bundle + use<> {
    let mut node = row_node();
    node.height = Val::Px(HEADER_HEIGHT);
    node.flex_direction = FlexDirection::Row;
    node.align_items = AlignItems::Center;
    node.column_gap = Val::Px(10.0);
    node.padding = UiRect::left(Val::Px(26.0));
    let label = Type {
        tracking: 0.14,
        ..Type::LABEL
    };
    (
        node,
        HeaderRow(name.to_owned()),
        UiTransform::from_translation(Val2::px(0.0, y)),
        Tween::<OffsetY>::at_rest(y, tokens::motion::PANEL, Ease::OutQuint),
        UiOpacity(opacity),
        Tween::<Opacity>::new(opacity, 1.0, tokens::motion::PANEL, Ease::OutQuint),
        children![
            text(name, Type { size: 13.0, ..label }, tokens::SOFT),
            text(&count.to_string(), Type::barlow(13.0, 600), tokens::MUTED),
        ],
    )
}

/// Where the rows go: each row's top below the first row's, the selection's centre and the
/// list's height.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    /// By row, the row's top.
    pub ys: Vec<f32>,
    pub centre: f32,
    pub height: f32,
}

impl Placement {
    /// How far the list scrolls (0 or less) to put the selection in the middle of the room
    /// between [`TOP`] and [`BOTTOM`], without leaving room above the first row or below the
    /// last.
    pub fn scroll(&self) -> f32 {
        let middle = (BOTTOM - TOP) / 2.0;
        let lowest = ((BOTTOM - TOP) - self.height).min(0.0);
        (middle - self.centre).clamp(lowest, 0.0)
    }
}

/// Places `rows` with the selected game's maps open (`maps[g]`: the number of maps of game
/// `g`). A selection that is not shown counts as the top.
pub fn place(rows: &[Row], maps: &[usize], sel: Selection) -> Placement {
    let mut ys = Vec::with_capacity(rows.len());
    let mut y = 0.0;
    let mut centre = None;
    for row in rows {
        if !ys.is_empty() {
            y += GAP;
        }
        ys.push(y);
        match row {
            Row::Header(..) => y += HEADER_HEIGHT,
            Row::Game(g) if *g == sel.game => {
                let open = maps.get(*g).copied().unwrap_or(0);
                centre = Some(if open == 0 {
                    y + GAME_HEIGHT / 2.0
                } else {
                    y + GAME_HEIGHT
                        + GAP
                        + sel.map.min(open - 1) as f32 * (MAP_HEIGHT + GAP)
                        + MAP_HEIGHT / 2.0
                });
                y += GAME_HEIGHT + open as f32 * (MAP_HEIGHT + GAP);
            }
            Row::Game(_) => y += GAME_HEIGHT,
        }
    }
    Placement {
        ys,
        centre: centre.unwrap_or(0.0),
        height: y,
    }
}

type GameParts<'a> = (&'a GamePanel, &'a mut Shape, &'a mut Tween<OffsetX>);
type MapParts<'a> = (&'a MapPanel, &'a mut Shape, &'a mut Tween<OffsetX>);

/// Gives the selected game and map their look when the selection changes: the game slides left
/// with a white outline and opens its maps, the map slides and brightens.
pub(super) fn looks(
    sel: Res<Selection>,
    mut games: Query<GameParts, Without<MapPanel>>,
    mut maps: Query<MapParts, Without<GamePanel>>,
    mut holders: Query<(&Maps, &mut Node)>,
    mut names: Query<(&MapName, &mut TextFont)>,
) {
    if !sel.is_changed() {
        return;
    }
    for (p, mut shape, mut tween) in &mut games {
        let on = p.0 == sel.game;
        *shape = style_game(shape.clone(), on);
        tween.retarget(if on { -SLIDE } else { 0.0 });
    }
    for (p, mut shape, mut tween) in &mut maps {
        let on = p.0 == sel.game && p.1 == sel.map;
        *shape = style_map(shape.clone(), on);
        tween.retarget(if on { -SLIDE } else { 0.0 });
    }
    for (holder, mut node) in &mut holders {
        node.display = if holder.0 == sel.game {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (name, mut font) in &mut names {
        let on = name.0 == sel.game && name.1 == sel.map;
        font.weight = Type::barlow(17.0, if on { 700 } else { 500 }).font().weight;
    }
}

/// The rows again when the filters or the library change; a selected game that is no longer
/// shown hands the selection to the first one that is.
pub(super) fn refilter(
    filters: Res<Filters>,
    lib: Res<Library>,
    lists: Res<super::collections::Collections>,
    mut shown: ResMut<Shown>,
    mut sel: ResMut<Selection>,
) {
    if !filters.is_changed() && !lib.is_changed() && !lists.is_changed() {
        return;
    }
    let rows = filters::rows(&lib.games, &lib.records, &filters, &lists);
    if shown.rows != rows {
        shown.rows = rows;
    }
    let order = shown.order();
    if !order.contains(&sel.game)
        && let Some(&first) = order.first()
    {
        *sel = Selection { game: first, map: 0 };
    }
}

type RowParts<'a> = (
    &'a GameRow,
    &'a mut Tween<OffsetY>,
    &'a mut Tween<Opacity>,
    &'a mut Visibility,
);
type HeaderParts<'a> = (
    Entity,
    &'a HeaderRow,
    &'a mut Tween<OffsetY>,
    &'a mut Tween<Opacity>,
    Has<Leaving>,
);

/// Slides each row to its place when the rows or the selection change: shown games and headers
/// move there, games no longer shown fade out where they are, games shown again fade in at
/// their new place; headers come and go the same way. The list scrolls to the selection.
#[allow(clippy::too_many_arguments, reason = "one pass over the list's rows")]
pub(super) fn arrange(
    mut commands: Commands,
    shown: Res<Shown>,
    sel: Res<Selection>,
    lib: Res<Library>,
    mut games: Query<RowParts, (Without<HeaderRow>, Without<Column>)>,
    mut headers: Query<HeaderParts, (Without<GameRow>, Without<Column>)>,
    mut column: Query<(Entity, &mut Tween<OffsetY>), With<Column>>,
) {
    if !shown.is_changed() && !sel.is_changed() {
        return;
    }
    let maps: Vec<usize> = lib.games.iter().map(|g| g.maps.len()).collect();
    let placed = place(&shown.rows, &maps, *sel);
    let mut at: HashMap<&Row, f32> = HashMap::new();
    for (row, y) in shown.rows.iter().zip(&placed.ys) {
        at.insert(row, *y);
    }
    for (row, mut y, mut opacity, mut visibility) in &mut games {
        match at.get(&Row::Game(row.0)) {
            Some(&to) => {
                if opacity.to <= 0.0 && opacity.value() <= 0.0 {
                    // Coming back: appear in place rather than slide from where it left.
                    *y = Tween::at_rest(to, y.duration, y.ease);
                } else {
                    y.retarget(to);
                }
                opacity.retarget(1.0);
                if *visibility != Visibility::Inherited {
                    *visibility = Visibility::Inherited;
                }
            }
            None => opacity.retarget(0.0),
        }
    }
    let mut kept: Vec<String> = Vec::new();
    for (entity, row, mut y, mut opacity, leaving) in &mut headers {
        let to = shown.rows.iter().zip(&placed.ys).find_map(|(r, y)| match r {
            Row::Header(name, _) if *name == row.0 => Some(*y),
            _ => None,
        });
        match to {
            Some(to) if !leaving => {
                y.retarget(to);
                kept.push(row.0.clone());
            }
            Some(_) => {}
            None => {
                opacity.retarget(0.0);
                commands.entity(entity).insert(Leaving);
            }
        }
    }
    let Ok((column, mut scroll)) = column.single_mut() else {
        return;
    };
    scroll.retarget(placed.scroll());
    for (row, y) in shown.rows.iter().zip(&placed.ys) {
        if let Row::Header(name, count) = row
            && !kept.contains(name)
        {
            let id = commands.spawn(header(name, *count, *y, 0.0)).id();
            commands.entity(column).add_child(id);
        }
    }
}

/// Hides a game row once it has faded out (so it takes no clicks) and despawns a header that
/// left.
pub(super) fn settle(
    mut commands: Commands,
    mut games: Query<(&Tween<Opacity>, &mut Visibility), With<GameRow>>,
    leaving: Query<(Entity, &Tween<Opacity>), With<Leaving>>,
) {
    for (opacity, mut visibility) in &mut games {
        if opacity.finished() && opacity.to <= 0.0 && *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
        }
    }
    for (entity, opacity) in &leaving {
        if opacity.finished() {
            commands.entity(entity).despawn();
        }
    }
}

/// The list dims while [`ListDim`] is on.
pub(super) fn dim(state: Res<ListDim>, mut frame: Query<&mut Tween<Opacity>, With<Frame>>) {
    if !state.is_changed() {
        return;
    }
    for mut tween in &mut frame {
        tween.retarget(if state.0 { DIM } else { 1.0 });
    }
}

/// The next selection after a key, among the games shown (`order`, `maps[g]`: the number of
/// maps of game `g`): Down and Up step through the maps, on into the next or previous game;
/// Right and Left step through the games (each opens on its first map). Wraps. A selection
/// that is not shown goes to the first game shown.
pub fn step(order: &[usize], maps: &[usize], sel: Selection, key: KeyCode) -> Selection {
    let n = order.len();
    let Some(at) = order.iter().position(|&g| g == sel.game) else {
        return order.first().map_or(sel, |&game| Selection { game, map: 0 });
    };
    let count = |i: usize| maps.get(order[i]).copied().unwrap_or(0).max(1);
    let first = |i: usize| Selection {
        game: order[i],
        map: 0,
    };
    match key {
        KeyCode::ArrowRight => first((at + 1) % n),
        KeyCode::ArrowLeft => first((at + n - 1) % n),
        KeyCode::ArrowDown if sel.map + 1 < count(at) => Selection {
            map: sel.map + 1,
            ..sel
        },
        KeyCode::ArrowDown => first((at + 1) % n),
        KeyCode::ArrowUp if sel.map > 0 => Selection {
            map: sel.map - 1,
            ..sel
        },
        KeyCode::ArrowUp => {
            let i = (at + n - 1) % n;
            Selection {
                game: order[i],
                map: count(i) - 1,
            }
        }
        _ => sel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(n: usize) -> Vec<Row> {
        (0..n).map(Row::Game).collect()
    }

    #[test]
    fn keys_walk_maps_then_games_and_wrap() {
        let maps = [3, 0, 2];
        let order = [0, 1, 2];
        let s = |game, map| Selection { game, map };
        let step = |sel, key| step(&order, &maps, sel, key);
        assert_eq!(step(s(0, 0), KeyCode::ArrowDown), s(0, 1));
        assert_eq!(step(s(0, 2), KeyCode::ArrowDown), s(1, 0));
        assert_eq!(
            step(s(1, 0), KeyCode::ArrowDown),
            s(2, 0),
            "a game without maps is one stop"
        );
        assert_eq!(step(s(2, 1), KeyCode::ArrowDown), s(0, 0));
        assert_eq!(
            step(s(0, 0), KeyCode::ArrowUp),
            s(2, 1),
            "up wraps to the last map"
        );
        assert_eq!(step(s(2, 0), KeyCode::ArrowUp), s(1, 0));
        assert_eq!(step(s(0, 2), KeyCode::ArrowRight), s(1, 0));
        assert_eq!(step(s(0, 2), KeyCode::ArrowLeft), s(2, 0));
        assert_eq!(super::step(&[], &[], s(0, 0), KeyCode::ArrowDown), s(0, 0));
    }

    #[test]
    fn keys_follow_the_shown_order_and_skip_hidden_games() {
        let maps = [1, 1, 1, 1];
        let order = [3, 0];
        let s = |game, map| Selection { game, map };
        assert_eq!(step(&order, &maps, s(3, 0), KeyCode::ArrowDown), s(0, 0));
        assert_eq!(step(&order, &maps, s(0, 0), KeyCode::ArrowRight), s(3, 0));
        assert_eq!(step(&order, &maps, s(3, 0), KeyCode::ArrowUp), s(0, 0));
        assert_eq!(
            step(&order, &maps, s(1, 0), KeyCode::ArrowDown),
            s(3, 0),
            "a hidden selection goes to the first game shown"
        );
    }

    #[test]
    fn the_artboard_list_needs_no_scroll() {
        // Main.dc.html (and m5 A3): Kaenbyou, the fourth of seven games, open on its third of five
        // maps.
        let maps = [3, 2, 3, 5, 3, 2, 2];
        let sel = Selection { game: 3, map: 2 };
        let placed = place(&all(7), &maps, sel);
        assert_eq!(placed.centre, 3.0 * 75.0 + 75.0 + 2.0 * 48.0 + 22.5);
        assert_eq!(placed.height, 7.0 * 75.0 - 3.0 + 5.0 * 48.0);
        assert_eq!(placed.ys[4], 4.0 * 75.0 + 5.0 * 48.0, "games under it make room");
        assert_eq!(placed.scroll(), 0.0);
    }

    #[test]
    fn a_long_list_centres_the_selection_and_stops_at_its_end() {
        let maps = [1; 20];
        let middle = (BOTTOM - TOP) / 2.0;
        let sel = Selection { game: 10, map: 0 };
        let placed = place(&all(20), &maps, sel);
        assert_eq!(placed.scroll(), middle - placed.centre);
        let last = place(&all(20), &maps, Selection { game: 19, map: 0 });
        assert_eq!(
            last.scroll(),
            (BOTTOM - TOP) - last.height,
            "the last panel ends at the bottom"
        );
        assert_eq!(place(&all(20), &maps, Selection::default()).scroll(), 0.0);
    }

    #[test]
    fn headers_and_a_new_order_move_the_rows() {
        let maps = [2, 1, 1];
        let rows = vec![
            Row::Header("A".into(), 2),
            Row::Game(2),
            Row::Game(0),
            Row::Header("B".into(), 1),
            Row::Game(1),
        ];
        let placed = place(&rows, &maps, Selection { game: 0, map: 1 });
        let after_header = HEADER_HEIGHT + GAP;
        assert_eq!(placed.ys[1], after_header);
        assert_eq!(placed.ys[2], after_header + GAME_HEIGHT + GAP);
        assert_eq!(
            placed.centre,
            placed.ys[2] + GAME_HEIGHT + GAP + (MAP_HEIGHT + GAP) + MAP_HEIGHT / 2.0
        );
        assert_eq!(
            placed.ys[3],
            placed.ys[2] + GAME_HEIGHT + 2.0 * (MAP_HEIGHT + GAP) + GAP,
            "the open game's maps push the next header down"
        );
    }
}
