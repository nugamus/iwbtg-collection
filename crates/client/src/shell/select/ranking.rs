//! Game select's Details and Ranking tabs under the stage strip (#151; m5
//! `A3_SelectTimeAttack`, m4 `Main.dc.html`, `SelectNoClears.dc.html`).
//!
//! Ranking lists the board of the picked map with the mods set now: in the Local scope your
//! own runs from the stats book, fastest first, #1 in gold and your PB marked; the Global scope
//! waits for the online boards (#484) and says so. Details holds the folder, the keys and the
//! collections until its own design lands (#498).
//!
//! The list is a `ui::live` part keyed on the rows it shows, so moving through the game list
//! rebuilds it only when the board differs.

use bevy::prelude::*;
use bevy::ui_widgets::Activate;

use super::AppScreen;
use super::library::Library;
use super::list::Selection;
use crate::ui::fonts::Type;
use crate::ui::tokens;
use crate::ui::widgets::{ButtonKind, button_with, ellipsis_text, text};

/// The tabs' top, design px.
pub(super) const TOP: f32 = 566.0;
/// A ranking row's height and the gap under it.
const ROW_H: f32 = 48.0;
const ROW_GAP: f32 = 6.0;
/// The most rows the tab shows (the board keeps more).
const MAX_ROWS: usize = 6;

/// Which tab shows.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    Details,
    #[default]
    Ranking,
}

/// Whose runs the Ranking tab lists.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    Global,
    #[default]
    Local,
}

/// One row of the Ranking tab.
#[derive(Clone, Debug, PartialEq)]
pub struct RankRow {
    pub rank: usize,
    pub name: String,
    pub time: String,
    pub deaths: String,
    pub date: String,
    pub pb: bool,
}

/// What the Ranking tab shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Board {
    Rows(Vec<RankRow>),
    /// Nobody has cleared the map on this board.
    Empty,
    /// The Global scope, before the online boards.
    Offline,
}

/// The rows for `runs` (fastest first): at most [`MAX_ROWS`], your name on each.
pub fn rows(runs: &[crate::stats::Run], rate: u32, name: &str) -> Board {
    if runs.is_empty() {
        return Board::Empty;
    }
    Board::Rows(
        runs.iter()
            .take(MAX_ROWS)
            .enumerate()
            .map(|(i, r)| RankRow {
                rank: i + 1,
                name: name.to_owned(),
                time: crate::stats::fmt_ticks(r.ticks, rate),
                deaths: crate::card::grouped(u64::from(r.deaths)),
                date: crate::hud::leaderboard::date_short(r.finished_at),
                pb: i == 0,
            })
            .collect(),
    )
}

/// A tab header, the scope switch and the panels.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::shell) enum Part {
    TabButton(Tab),
    ScopeButton,
    RankingPanel,
    DetailsPanel,
}

/// Spawns the tab row and the Ranking panel; `details` is the Details panel's content.
pub(super) fn spawn(commands: &mut Commands, camera: Entity, details: impl Bundle) {
    let tab = |t: Tab, label: &'static str| {
        (
            Part::TabButton(t),
            bevy::ui_widgets::Button,
            bevy::picking::hover::Hovered::default(),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                padding: UiRect::vertical(Val::Px(4.0)),
                ..default()
            },
            children![
                (text(label, Type::barlow(17.0, 600), tokens::MUTED), TabLabel(t)),
                (
                    Node {
                        height: Val::Px(3.0),
                        width: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                    TabLine(t),
                ),
            ],
        )
    };
    let column = |part: Part| {
        (
            part,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(64.0),
                top: Val::Px(TOP + 48.0),
                right: Val::Px(super::list::FROM_RIGHT + 60.0),
                max_width: Val::Px(850.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(ROW_GAP),
                ..default()
            },
            UiTargetCamera(camera),
            DespawnOnExit(AppScreen::Select),
        )
    };
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(64.0),
            top: Val::Px(TOP),
            right: Val::Px(super::list::FROM_RIGHT + 60.0),
            max_width: Val::Px(850.0),
            align_items: AlignItems::FlexStart,
            column_gap: Val::Px(28.0),
            ..default()
        },
        UiTargetCamera(camera),
        DespawnOnExit(AppScreen::Select),
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            c.spawn(tab(Tab::Details, "Details"));
            c.spawn(tab(Tab::Ranking, "Ranking"));
            c.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
            c.spawn((
                button_with(
                    ButtonKind::Control,
                    34.0,
                    14.0,
                    (
                        Node {
                            column_gap: Val::Px(6.0),
                            align_items: AlignItems::Baseline,
                            ..default()
                        },
                        children![
                            text("Scope", Type::barlow(14.0, 400), tokens::MUTED),
                            (text("Local ▾", Type::barlow(14.0, 600), tokens::INK), ScopeLabel),
                        ],
                    ),
                ),
                Part::ScopeButton,
            ));
        })),
    ));
    commands.spawn(column(Part::RankingPanel));
    commands.spawn((column(Part::DetailsPanel), Visibility::Hidden, children![details]));
}

/// A tab's label and the accent line under the shown one.
#[derive(Component, Clone, Copy)]
pub(super) struct TabLabel(Tab);
#[derive(Component, Clone, Copy)]
pub(super) struct TabLine(Tab);
/// The scope switch's value.
#[derive(Component)]
pub(super) struct ScopeLabel;

/// A click on a tab shows it; on the scope switch flips Local and Global.
pub(super) fn on_activate(
    activate: On<Activate>,
    parts: Query<&Part>,
    tab: Option<ResMut<Tab>>,
    scope: Option<ResMut<Scope>>,
) {
    match parts.get(activate.entity) {
        Ok(Part::TabButton(t)) => {
            if let Some(mut tab) = tab
                && *tab != *t
            {
                *tab = *t;
            }
        }
        Ok(Part::ScopeButton) => {
            if let Some(mut s) = scope {
                *s = match *s {
                    Scope::Local => Scope::Global,
                    Scope::Global => Scope::Local,
                };
            }
        }
        _ => {}
    }
}

/// The tabs' looks, the panel shown and the Ranking rows for the pick.
#[allow(clippy::too_many_arguments, reason = "the tabs' sources and parts")]
pub(super) fn follow(
    tab: Res<Tab>,
    scope: Res<Scope>,
    sel: Res<Selection>,
    lib: Res<Library>,
    mods: Res<crate::mods::RunMods>,
    stats: Option<Res<crate::stats::Stats>>,
    mut labels: Query<(&TabLabel, &mut TextColor, &mut crate::ui::shape::Ink)>,
    mut lines: Query<(&TabLine, &mut BackgroundColor)>,
    mut scope_label: Query<&mut Text, With<ScopeLabel>>,
    mut panels: Query<(Entity, &Part, &mut Visibility)>,
    mut commands: Commands,
) {
    let stats_changed = stats.as_ref().is_some_and(|s| s.is_changed());
    if !tab.is_changed() && !scope.is_changed() && !sel.is_changed() && !lib.is_changed() && !mods.is_changed() && !stats_changed {
        return;
    }
    let accent = tokens::ACCENT;
    for (TabLabel(t), mut color, mut ink) in &mut labels {
        let want = if *t == *tab { tokens::INK } else { tokens::MUTED };
        color.0 = want;
        ink.0 = want;
    }
    for (TabLine(t), mut bg) in &mut lines {
        bg.0 = if *t == *tab { accent } else { Color::NONE };
    }
    for mut t in &mut scope_label {
        let want = Type::barlow(14.0, 600).set(match *scope {
            Scope::Local => "Local ▾",
            Scope::Global => "Global ▾",
        });
        if t.0 != want {
            t.0 = want;
        }
    }
    let board = match *scope {
        Scope::Global => Board::Offline,
        Scope::Local => {
            let empty = crate::stats::Book::default();
            let book = stats.as_ref().map_or(&empty, |s| &s.book);
            lib.games
                .get(sel.game)
                .and_then(|g| {
                    let map = g.maps.get(sel.map)?;
                    let key = crate::stats::map_key(&g.id, &map.id);
                    let rate = book.map(&key).map_or(50, |m| m.tick_rate);
                    let b = book.board(&key, true, &mods.0.codes())?;
                    Some(rows(&b.runs, rate, &crate::card::profile_name()))
                })
                .unwrap_or(Board::Empty)
        }
    };
    for (e, part, mut vis) in &mut panels {
        let shown = match part {
            Part::RankingPanel => *tab == Tab::Ranking,
            Part::DetailsPanel => *tab == Tab::Details,
            _ => continue,
        };
        vis.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
        if *part == Part::RankingPanel {
            crate::ui::live::queue_refresh(&mut commands, e, board.clone(), panel);
        }
    }
}

/// The Ranking panel: the rows, or why there are none.
fn panel(c: &mut ChildSpawner, board: &Board) {
    match board {
        Board::Offline => {
            c.spawn(text(
                "Global boards come with online play. Local shows your own runs.",
                Type::barlow(15.0, 500),
                tokens::MUTED,
            ));
        }
        Board::Empty => {
            c.spawn(text(
                "No clears yet — be the first",
                Type::barlow(17.0, 600),
                tokens::SOFT,
            ));
        }
        Board::Rows(rows) => {
            for r in rows {
                c.spawn(row(r));
            }
        }
    }
}

/// One ranking row: rank, avatar, name (and YOUR PB), time, deaths and date.
fn row(r: &RankRow) -> impl Bundle + use<> {
    let gold = r.rank == 1;
    let fill = if gold {
        tokens::over(tokens::GOLD, 0.16, tokens::GROUND)
    } else {
        tokens::white(0.05)
    };
    let rank_ink = if gold { tokens::GOLD } else { tokens::SOFT };
    let initial = r
        .name
        .chars()
        .next()
        .map_or_else(|| "?".to_owned(), |c| c.to_uppercase().collect());
    let r = r.clone();
    (
        crate::ui::widgets::sheared(ROW_H, 18.0, crate::ui::shape::Shape::fill(fill)),
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            let cell = |w: f32| Node {
                width: Val::Px(w),
                flex_shrink: 0.0,
                ..default()
            };
            c.spawn((cell(56.0), children![text(&format!("#{}", r.rank), Type::barlow(17.0, 700), rank_ink)]));
            c.spawn((
                Node {
                    width: Val::Px(30.0),
                    height: Val::Px(30.0),
                    border_radius: BorderRadius::all(Val::Px(6.0)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    flex_shrink: 0.0,
                    ..default()
                },
                BackgroundColor(tokens::ACCENT),
                children![text(&initial, Type::barlow(14.0, 700), tokens::INK)],
            ));
            c.spawn((
                Node {
                    flex_grow: 1.0,
                    min_width: Val::Px(0.0),
                    column_gap: Val::Px(10.0),
                    align_items: AlignItems::Baseline,
                    margin: UiRect::left(Val::Px(12.0)),
                    ..default()
                },
                Children::spawn(SpawnWith({
                    let name = r.name.clone();
                    let pb = r.pb;
                    move |c: &mut ChildSpawner| {
                        c.spawn(ellipsis_text(&name, Type::barlow(17.0, 600), tokens::ROW_NAME));
                        if pb {
                            c.spawn(text("YOUR PB", Type::LABEL, tokens::MUTED));
                        }
                    }
                })),
            ));
            c.spawn((cell(130.0), children![text(&r.time, Type::barlow(17.0, 700), tokens::INK)]));
            c.spawn((cell(90.0), children![text(&r.deaths, Type::barlow(15.0, 500), tokens::SOFT)]));
            c.spawn((
                Node {
                    width: Val::Px(70.0),
                    justify_content: JustifyContent::FlexEnd,
                    flex_shrink: 0.0,
                    ..default()
                },
                children![text(&r.date, Type::barlow(14.0, 500), tokens::MUTED)],
            ));
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(ticks: u64, deaths: u32) -> crate::stats::Run {
        crate::stats::Run {
            ticks,
            deaths,
            splits: vec![ticks],
            segment_deaths: vec![deaths],
            finished_at: 1_762_000_000,
            replay: None,
            death_log: Default::default(),
        }
    }

    #[test]
    fn the_board_lists_your_runs_fastest_first_with_the_pb_marked() {
        let Board::Rows(rows) = rows(&[run(5000, 12), run(6000, 40)], 50, "nugamus") else {
            panic!("rows");
        };
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].rank, 1);
        assert!(rows[0].pb && !rows[1].pb);
        assert_eq!(rows[0].time, crate::stats::fmt_ticks(5000, 50));
        assert_eq!(rows[1].deaths, "40");
    }

    #[test]
    fn no_runs_is_the_empty_line_and_the_list_is_capped() {
        assert_eq!(rows(&[], 50, "x"), Board::Empty);
        let many: Vec<_> = (0..10).map(|i| run(1000 + i, 0)).collect();
        let Board::Rows(r) = rows(&many, 50, "x") else { panic!() };
        assert_eq!(r.len(), MAX_ROWS);
    }
}
