//! The notification list behind the toolbar's bell (m5 `D2_Notifications`, `Notices.dc.html`):
//! every notice of this session is kept here ([`super::notice::post`] adds it), newest first. The
//! bell shows how many arrived since the list was last opened; a click on it opens the panel on
//! the right: `Notifications`, `Clear all`, and the list, each with its kind's icon tile, heading,
//! time and message. Escape or a click outside closes it. Progress (downloads, uploads) has no
//! producer yet, so the panel's In progress part is not drawn (#165, online phase).

use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};

use super::fonts::Type;
use super::icons::icon;
use super::notice::Kind;
use super::toolbar::ToolbarButton;
use super::tokens;
use super::widgets::text;
use crate::locale;

/// One notification.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub kind: Kind,
    pub message: String,
    /// When it arrived, seconds since 1970.
    pub at: u64,
}

/// This session's notifications and the panel's state.
#[derive(Resource, Debug, Default)]
pub struct Notifications {
    /// Newest last.
    pub items: Vec<Item>,
    /// Arrived since the panel was last opened.
    pub unread: usize,
    pub open: bool,
    generation: u64,
}

impl Notifications {
    /// Keeps a notice (`notice::show`).
    pub fn push(&mut self, item: Item) {
        self.items.push(item);
        if !self.open {
            self.unread += 1;
        }
        self.generation = self.generation.wrapping_add(1);
    }

    fn toggle(&mut self) {
        self.open = !self.open;
        if self.open {
            self.unread = 0;
        }
        self.generation = self.generation.wrapping_add(1);
    }
}

/// The panel's width: 460 px.
const WIDTH: f32 = 460.0;
/// `#10121a`.
const PANEL: Color = Color::srgb_u8(0x10, 0x12, 0x1a);

/// `--notice-preview list`: the list opens once the first notices are in.
static OPEN_AT_START: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Opens the list when the first notices arrive (the screenshot preview).
pub fn open_at_start() {
    OPEN_AT_START.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn preview(mut list: ResMut<Notifications>) {
    if !list.items.is_empty() && OPEN_AT_START.swap(false, std::sync::atomic::Ordering::Relaxed) && !list.open {
        list.toggle();
    }
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Notifications>()
        .add_observer(on_activate)
        .add_systems(Update, (preview, keys, badge, draw).chain());
}

/// The panel's parts that take clicks.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    ClearAll,
    Close,
    /// A row whose kind opens something (`Kind::opens`).
    Open(Kind),
}

fn on_activate(
    activate: On<Activate>,
    bells: Query<&ToolbarButton>,
    hits: Query<&Hit>,
    list: Option<ResMut<Notifications>>,
    mut commands: Commands,
) {
    let Some(mut list) = list else {
        return;
    };
    if bells.get(activate.entity) == Ok(&ToolbarButton::Notifications) {
        list.toggle();
        return;
    }
    match hits.get(activate.entity) {
        Ok(Hit::ClearAll) => {
            list.items.clear();
            list.generation = list.generation.wrapping_add(1);
        }
        Ok(Hit::Close) => list.toggle(),
        Ok(&Hit::Open(kind)) => {
            list.toggle();
            commands.trigger(super::notice::Opened { kind });
        }
        Err(_) => {}
    }
}

/// Escape closes the panel.
fn keys(keys: Res<ButtonInput<KeyCode>>, mut list: ResMut<Notifications>) {
    if list.open && keys.just_pressed(KeyCode::Escape) {
        list.toggle();
    }
}

/// The unread count on the bell.
#[derive(Component)]
struct Badge;

/// Keeps a red badge with the unread count on every bell.
fn badge(
    mut commands: Commands,
    list: Res<Notifications>,
    bells: Query<(Entity, &ToolbarButton, Option<&Children>)>,
    badges: Query<(Entity, &Children), With<Badge>>,
    mut texts: Query<&mut Text>,
) {
    for (bell, button, children) in &bells {
        if *button != ToolbarButton::Notifications {
            continue;
        }
        let have = children.and_then(|c| c.iter().find(|e| badges.contains(*e)));
        match (have, list.unread) {
            (Some(b), 0) => commands.entity(b).despawn(),
            (Some(b), n) => {
                if let Ok((_, kids)) = badges.get(b)
                    && let Some(t) = kids.iter().next()
                    && let Ok(mut t) = texts.get_mut(t)
                {
                    let want = n.min(99).to_string();
                    if t.0 != want {
                        t.0 = want;
                    }
                }
            }
            (None, 0) => {}
            (None, n) => {
                let b = commands
                    .spawn((
                        Badge,
                        Node {
                            position_type: PositionType::Absolute,
                            top: Val::Px(8.0),
                            right: Val::Px(8.0),
                            min_width: Val::Px(16.0),
                            height: Val::Px(16.0),
                            padding: UiRect::horizontal(Val::Px(4.0)),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(tokens::ACCENT),
                        Pickable::IGNORE,
                        children![text(&n.min(99).to_string(), Type::barlow(11.0, 700), Color::WHITE)],
                    ))
                    .id();
                commands.entity(bell).add_child(b);
            }
        }
    }
}

/// The open panel.
#[derive(Component)]
struct Root(u64);

/// `21:16` in the player's time zone.
fn clock(at: u64) -> String {
    let local = at as i64 + i64::from(crate::hud::leaderboard::local_offset(at));
    let m = local.rem_euclid(86_400) / 60;
    format!("{}:{:02}", m / 60, m % 60)
}

/// Builds the panel again when the list changes, and takes it down when it closes.
fn draw(mut commands: Commands, list: Res<Notifications>, roots: Query<(Entity, &Root)>) {
    let mut current = false;
    for (e, r) in &roots {
        if list.open && r.0 == list.generation {
            current = true;
        } else {
            commands.entity(e).despawn();
        }
    }
    if !list.open || current {
        return;
    }
    let items: Vec<Item> = list.items.iter().rev().cloned().collect();
    commands.spawn((
        Root(list.generation),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(super::toolbar::HEIGHT),
            right: Val::Px(0.0),
            bottom: Val::Px(0.0),
            ..default()
        },
        GlobalZIndex(55),
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            // The screen under it at 35%: a click closes.
            c.spawn((
                Button,
                Hit::Close,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    right: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    ..default()
                },
                BackgroundColor(tokens::dark(Color::srgb_u8(5, 6, 10), 0.35)),
            ));
            c.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(0.0),
                    top: Val::Px(0.0),
                    bottom: Val::Px(0.0),
                    width: Val::Px(WIDTH),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    padding: UiRect::all(Val::Px(22.0)),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                BackgroundColor(PANEL),
                BoxShadow::new(tokens::black(0.5), Val::Px(-20.0), Val::ZERO, Val::ZERO, Val::Px(60.0)),
            ))
            .with_children(|c| {
                c.spawn(Node {
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Baseline,
                    ..default()
                })
                .with_children(|c| {
                    c.spawn(text(locale::text("notif_title"), Type::barlow(24.0, 700), tokens::INK));
                    if !items.is_empty() {
                        c.spawn((
                            Button,
                            Hit::ClearAll,
                            Node::default(),
                            children![text(locale::text("notif_clear_all"), Type::barlow(14.0, 700), tokens::MUTED)],
                        ));
                    }
                });
                if items.is_empty() {
                    c.spawn((
                        Node {
                            padding: UiRect::top(Val::Px(10.0)),
                            ..default()
                        },
                        children![text(locale::text("notif_empty"), Type::barlow(15.0, 400), tokens::MUTED)],
                    ));
                    return;
                }
                c.spawn((
                    Node {
                        padding: UiRect::top(Val::Px(10.0)),
                        ..default()
                    },
                    children![text(locale::text("notif_today"), Type::LABEL, tokens::MUTED)],
                ));
                for item in &items {
                    row(c, item);
                }
            });
        })),
    ));
}

fn row(c: &mut ChildSpawner, item: &Item) {
    let tone = item.kind.tone();
    let mut row = c.spawn((
        Node {
            column_gap: Val::Px(14.0),
            align_items: AlignItems::Center,
            padding: UiRect::axes(Val::Px(14.0), Val::Px(12.0)),
            border_radius: BorderRadius::all(Val::Px(10.0)),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(tokens::white(0.04)),
    ));
    // A row that opens something (the update dialog) takes clicks, and its edge says so.
    if item.kind.opens() {
        row.insert((
            Button,
            Hit::Open(item.kind),
            BorderColor::all(tone.with_alpha(0.6)),
        ));
        row.entry::<Node>().and_modify(|mut n| n.border = UiRect::all(Val::Px(1.0)));
    }
    row.with_children(|c| {
        c.spawn((
            Node {
                width: Val::Px(44.0),
                height: Val::Px(44.0),
                flex_shrink: 0.0,
                border_radius: BorderRadius::all(Val::Px(10.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(tokens::over(tone, 0.3, PANEL)),
            children![icon(item.kind.icon(), 22.0, Color::WHITE)],
        ));
        c.spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(3.0),
            flex_grow: 1.0,
            min_width: Val::Px(0.0),
            ..default()
        })
        .with_children(|c| {
            c.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|c| {
                c.spawn(text(item.kind.heading(), super::notice::HEADING, tone));
                c.spawn(text(&clock(item.at), Type::barlow(12.0, 400), tokens::MUTED));
            });
            c.spawn(text(
                &item.kind.body(&item.message),
                Type::barlow(15.0, 400),
                tokens::ROW_NAME,
            ));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unread_counts_until_the_list_opens() {
        let mut n = Notifications::default();
        let item = Item {
            kind: Kind::Drop,
            message: "a.zip is not a game package".into(),
            at: 0,
        };
        n.push(item.clone());
        n.push(item.clone());
        assert_eq!(n.unread, 2);
        n.toggle();
        assert_eq!(n.unread, 0, "opening reads them");
        n.push(item);
        assert_eq!(n.unread, 0, "arrived while open");
        assert_eq!(n.items.len(), 3);
    }
}
