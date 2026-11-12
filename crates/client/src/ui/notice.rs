//! Notices: a short message in a corner of the window, out of the game, that fades after a few
//! seconds (docs/architecture/client/ui.md). Anything can post one with [`post`], from any
//! thread and before the app exists (settings load before it), so a module that has no `World`
//! at hand (storage) still reaches the player.
//!
//! A kind that says [`Kind::once`] shows once per session: the second storage failure in a
//! session only goes to the console. A new kind of notice (a dropped file that is not a package,
//! #648) is one more [`Kind`] variant and one `post` call where it happens.

use std::collections::HashSet;
use std::sync::Mutex;

use bevy::prelude::*;
use bevy::text::LineBreak;

use super::fonts::Type;
use super::motion::{Ease, OffsetX, Opacity, Tween};
use super::shape::{Band, Shape, Side, UiOpacity};
use super::tokens;
use super::widgets::text;

/// What a notice is about: its heading and whether it shows once per session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Saves, settings, stats or replays could not be kept (`storage::warn`).
    Storage,
}

impl Kind {
    /// The heading over the message.
    pub fn heading(self) -> &'static str {
        match self {
            Kind::Storage => "Not saved",
        }
    }

    /// What the card says under the heading. A storage message names files and OS errors the
    /// player can do nothing with: it stays in the console, the card says what it means.
    pub fn body(self, message: &str) -> String {
        match self {
            Kind::Storage => {
                let _ = message;
                "Saves and settings may not be kept this session.".to_string()
            }
        }
    }

    /// Shows only the first time in a session.
    pub fn once(self) -> bool {
        match self {
            Kind::Storage => true,
        }
    }
}

/// Posts a notice. Shown on the next frame of a running app (or its first), whatever thread
/// calls it.
pub fn post(kind: Kind, message: &str) {
    if let Ok(mut q) = QUEUE.lock() {
        q.push((kind, message.to_string()));
    }
}

/// Posted and not yet shown.
static QUEUE: Mutex<Vec<(Kind, String)>> = Mutex::new(Vec::new());

/// How long a notice stays before it fades, seconds.
pub const HOLD: f32 = 6.0;
/// Its widest, with the shear room, UI px.
const MAX_WIDTH: f32 = 340.0;
/// Its narrowest, UI px: in a small window it reaches over the game's edge rather than wrap
/// word by word. No wider than the narrowest leaderboard column outside a small window, less
/// the margins (`hud::layout::MIN_LEFT - 2 * MARGIN`), or it reaches over the game at 16:9
/// and 4:3 too (#668).
pub const MIN_WIDTH: f32 = 180.0;
/// Room left and right of a card for its shear, UI px (a card up to 120 px tall). Its drop
/// shadow fades out in the stack's margin.
const SHEAR_ROOM: f32 = 12.0;
/// Its distance from the corner of its area, UI px.
pub const MARGIN: f32 = 12.0;
/// The longest message shown; the rest is cut with `…` (the kit has no cut text yet, #636).
const MAX_CHARS: usize = 100;

/// Where notices may go, in UI px; the window when unset. The HUD sets it to the leaderboard's
/// column so a notice stays off the game (unless the column is narrower than [`MIN_WIDTH`]).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Default)]
pub struct Area(pub Option<Rect>);

/// The notices of this session: which kinds have shown.
#[derive(Resource, Default, Debug)]
pub struct Shown(HashSet<Kind>);

impl Shown {
    /// Whether a notice of `kind` should show now (and remembers it did).
    pub fn admit(&mut self, kind: Kind) -> bool {
        !kind.once() || self.0.insert(kind)
    }
}

/// The column the notices stack in, bottom up.
#[derive(Component)]
struct Stack;

/// One notice on screen and how long it has been there.
#[derive(Component)]
struct Notice {
    age: f32,
    fading: bool,
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Area>()
        .init_resource::<Shown>()
        .add_systems(Update, (place, show, age).chain());
}

/// Drains the queue into notices.
fn show(mut commands: Commands, mut shown: ResMut<Shown>, stack: Query<Entity, With<Stack>>) {
    let posted = match QUEUE.lock() {
        Ok(mut q) if !q.is_empty() => std::mem::take(&mut *q),
        _ => return,
    };
    let stack = match stack.iter().next() {
        Some(s) => s,
        None => spawn_stack(&mut commands),
    };
    for (kind, message) in posted {
        if shown.admit(kind) {
            let id = commands.spawn(notice(kind, &message)).id();
            commands.entity(stack).add_child(id);
        }
    }
}

fn spawn_stack(commands: &mut Commands) -> Entity {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(MARGIN),
                bottom: Val::Px(MARGIN),
                width: Val::Px(MAX_WIDTH),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Stretch,
                row_gap: Val::Px(10.0),
                ..default()
            },
            Pickable::IGNORE,
            // Over the HUD (10) and the pause overlay (20).
            GlobalZIndex(30),
            Stack,
        ))
        .id()
}

/// `message`, at most [`MAX_CHARS`] characters.
pub fn clip(message: &str) -> String {
    match message.char_indices().nth(MAX_CHARS) {
        Some((at, _)) => format!("{}…", message[..at].trim_end()),
        None => message.to_string(),
    }
}

/// The notice: a sheared dark card with an accent band on the left, a capital heading in the
/// accent colour and the message under it in soft ink, wrapped to the stack's width (the leaderboard column's in a game). It slides
/// in from the left and fades in (OutQuint, 400 ms).
fn notice(kind: Kind, message: &str) -> impl Bundle + use<> {
    let heading = Type {
        tracking: 0.12,
        ..Type::PILL_CAPS
    };
    (
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            padding: UiRect {
                left: Val::Px(22.0),
                right: Val::Px(14.0),
                top: Val::Px(10.0),
                bottom: Val::Px(11.0),
            },
            // The shear moves the bottom corners left and the top ones right by 0.1 of the
            // height: room for them inside the stack, so the card stays out of the game.
            margin: UiRect::horizontal(Val::Px(SHEAR_ROOM)),
            ..default()
        },
        // Solid, not the card's glass: a notice can float over other UI text (#664).
        Shape::fill(tokens::CARD_SOLID)
            .with_band(Band {
                side: Side::Left,
                width: 4.0,
                color: tokens::ACCENT,
                fade_to: 0.0,
            })
            .with_shadow(8.0, 24.0, tokens::black(0.5)),
        UiTransform::default(),
        UiOpacity(0.0),
        Tween::<Opacity>::new(0.0, 1.0, tokens::motion::STANDARD, Ease::OutQuint),
        Tween::<OffsetX>::new(-24.0, 0.0, tokens::motion::STANDARD, Ease::OutQuint),
        Pickable::IGNORE,
        Notice {
            age: 0.0,
            fading: false,
        },
        children![
            text(kind.heading(), heading, tokens::ACCENT),
            (
                text(&clip(&kind.body(message)), Type::barlow(14.0, 500), tokens::SOFT),
                TextLayout::linebreak(LineBreak::WordOrCharacter),
            ),
        ],
    )
}

/// Fades a notice after [`HOLD`] and removes it when it has faded. Real time: a paused game
/// still lets it go.
fn age(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut notices: Query<(Entity, &mut Notice, &mut Tween<Opacity>)>,
) {
    for (entity, mut notice, mut fade) in &mut notices {
        notice.age += time.delta_secs();
        if !notice.fading && notice.age >= HOLD {
            notice.fading = true;
            fade.retarget(0.0);
        } else if notice.fading && fade.finished() {
            commands.entity(entity).despawn();
        }
    }
}

/// Keeps the stack in the bottom left of its [`Area`].
fn place(
    area: Res<Area>,
    ui_scale: Res<UiScale>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut stack: Query<&mut Node, With<Stack>>,
) {
    let Some(window) = window.iter().next() else {
        return;
    };
    // The window in UI px.
    let size = window.size() / ui_scale.0.max(0.01);
    let r = area.0.unwrap_or(Rect::from_corners(Vec2::ZERO, size));
    let want = (
        Val::Px(r.min.x + MARGIN),
        Val::Px(size.y - r.max.y + MARGIN),
        Val::Px((r.width() - 2.0 * MARGIN).clamp(MIN_WIDTH, MAX_WIDTH)),
    );
    for mut node in &mut stack {
        if (node.left, node.bottom, node.width) != want {
            (node.left, node.bottom, node.width) = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_that_shows_once_shows_once_per_session() {
        let mut shown = Shown::default();
        assert!(shown.admit(Kind::Storage));
        assert!(!shown.admit(Kind::Storage));
    }

    #[test]
    fn a_second_storage_warning_spawns_no_second_notice() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Shown>()
            .add_systems(Update, show);
        post(Kind::Storage, "settings: disk full");
        app.update();
        post(Kind::Storage, "stats: disk full");
        app.update();
        let mut q = app.world_mut().query::<&Notice>();
        assert_eq!(q.iter(app.world()).count(), 1);
    }

    /// Bevy blends in linear light, so even the card glass's 0.6 % lets white text under a
    /// notice show at about 26/255 over its 14/255 fill (#664). A notice must be solid.
    #[test]
    fn a_notice_hides_the_ui_under_it() {
        let mut world = World::new();
        let id = world.spawn(notice(Kind::Storage, "x")).id();
        let shape = world.get::<Shape>(id).expect("a notice has a shape");
        assert_eq!(shape.fill.alpha(), 1.0);
    }

    /// The stack is its area less the margins, so the card, its shear and its shadow stay in
    /// the leaderboard's column (#668); only a column narrower than that takes [`MIN_WIDTH`].
    #[test]
    fn the_stack_fills_its_area_less_the_margins() {
        let width = |area: f32| {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .init_resource::<UiScale>()
                .insert_resource(Area(Some(Rect::new(0.0, 0.0, area, 900.0))))
                .add_systems(Update, place);
            app.world_mut().spawn((
                Window {
                    resolution: (1600, 900).into(),
                    ..default()
                },
                bevy::window::PrimaryWindow,
            ));
            spawn_stack(&mut app.world_mut().commands());
            app.update();
            let mut q = app.world_mut().query_filtered::<&Node, With<Stack>>();
            q.single(app.world()).unwrap().width
        };
        assert_eq!(width(204.0), Val::Px(180.0));
        assert_eq!(width(208.0), Val::Px(184.0));
        assert_eq!(width(124.0), Val::Px(MIN_WIDTH));
        assert_eq!(width(1000.0), Val::Px(MAX_WIDTH));
    }

    #[test]
    fn a_long_message_is_cut() {
        let long = "x".repeat(200);
        let cut = clip(&long);
        assert_eq!(cut.chars().count(), MAX_CHARS + 1);
        assert!(cut.ends_with('…'));
        assert_eq!(clip("short"), "short");
    }
}
