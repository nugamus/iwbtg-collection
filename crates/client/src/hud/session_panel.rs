//! The session panel beside a Classic run (m5 `A7_ClassicPanel`, `ClassicRun.dc.html`, #1624):
//! a Classic run has no leaderboard and no timer, only the game and its art. The panel is off by
//! default (`[hud.show] session`, `--hud-show session`); on, it sits in the left column: the
//! CLASSIC pill, this session's time and the game's Classic total, the session's deaths and the
//! game's, and the stage with the furthest one. It dims to 40% while the kid moves.

use bevy::prelude::*;

use crate::locale;
use crate::stats::Stats;
use crate::ui::fonts::Type;
use crate::ui::tokens;
use crate::ui::widgets::{classic_pill, text};

/// The panel's live texts.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Time,
    TimeNote,
    Deaths,
    DeathsNote,
    Stage,
    StageNote,
}

/// The panel's root, for its opacity.
#[derive(Component)]
pub struct Panel;

/// The panel `width` px wide.
pub fn build(width: f32) -> impl Bundle {
    let caption = |s: &str| text(s, Type::LABEL, tokens::MUTED);
    let note = |part: Part| (text("", Type::barlow(13.0, 400), tokens::MUTED), TextLayout::no_wrap(), part);
    (
        Panel,
        crate::ui::shape::UiOpacity(1.0),
        Node {
            width: Val::Px(width),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(16.0),
            ..default()
        },
        children![
            (
                Node {
                    align_self: AlignSelf::FlexStart,
                    ..default()
                },
                children![classic_pill()],
            ),
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    ..default()
                },
                children![
                    caption(locale::text("pause_this_session")),
                    (text("", Type::barlow(30.0, 600), tokens::INK), TextLayout::no_wrap(), Part::Time),
                    note(Part::TimeNote),
                ],
            ),
            (
                Node {
                    column_gap: Val::Px(22.0),
                    ..default()
                },
                children![
                    (
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(2.0),
                            ..default()
                        },
                        children![
                            caption(locale::text("pause_deaths")),
                            (text("", Type::barlow(22.0, 600), tokens::INK), Part::Deaths),
                            note(Part::DeathsNote),
                        ],
                    ),
                    (
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(2.0),
                            ..default()
                        },
                        children![
                            caption(locale::text("session_panel_stage")),
                            (text("", Type::barlow(22.0, 600), tokens::INK), Part::Stage),
                            note(Part::StageNote),
                        ],
                    ),
                ],
            ),
        ],
    )
}

/// What each part says now.
pub fn words(stats: Option<&Stats>, info: &super::run_info::RunInfo, game: &str) -> [(Part, String); 6] {
    let session = stats.and_then(Stats::open_session);
    let mode = stats.map(|s| s.book.game_mode(game, false)).unwrap_or_default();
    let (seconds, deaths) = session.map_or((0.0, 0), |s| (s.seconds, s.deaths));
    let clock = crate::shell::shelf::clock;
    [
        (Part::Time, clock(seconds)),
        (
            Part::TimeNote,
            locale::format("pause_in_this_game", &[("n", &clock(mode.play_seconds))]),
        ),
        (Part::Deaths, crate::card::grouped(deaths)),
        (
            Part::DeathsNote,
            locale::format("session_panel_total", &[("n", &crate::card::grouped(mode.deaths))]),
        ),
        (Part::Stage, info.stage.to_string()),
        (
            Part::StageNote,
            locale::format("session_panel_of", &[("n", &info.stages)]),
        ),
    ]
}

/// Keeps the panel's numbers live and dims it while the kid moves (`held` left or right).
pub fn update(
    game: NonSend<crate::view::Game>,
    stats: Option<Res<Stats>>,
    info: Res<super::run_info::RunInfo>,
    mut texts: Query<(&Part, &mut Text)>,
    mut panels: Query<&mut crate::ui::shape::UiOpacity, With<Panel>>,
) {
    if texts.is_empty() {
        return;
    }
    let now = words(stats.as_deref(), &info, &game.sim.def().id);
    for (part, mut text) in &mut texts {
        if let Some((_, w)) = now.iter().find(|(p, _)| p == part)
            && text.0 != *w
        {
            text.0.clone_from(w);
        }
    }
    let moving = info.held
        & (iwbtg_sim::input::Action::Left.bit() | iwbtg_sim::input::Action::Right.bit())
        != 0;
    let want = if moving { 0.4 } else { 1.0 };
    for mut o in &mut panels {
        if (o.0 - want).abs() > 1e-3 {
            o.0 = want;
        }
    }
}
