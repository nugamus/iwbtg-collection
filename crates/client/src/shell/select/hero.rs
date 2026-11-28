//! Game select's left side (#148; m5 `A3_SelectTimeAttack`, m4 `Main.dc.html`): the title wedge
//! cut from the picked game's key art, its stats row and its stage strip.
//!
//! The page is built once; each part is a `ui::live` slot keyed on the data it shows
//! ([`HeroData`]), so moving through the list rebuilds a part only when what it shows changed
//! (UI foundation, #1680). The wedge's art and the backdrop follow the pick from the library's
//! previews, built off the UI thread.
//!
//! What the client cannot know offline (the average clear, the clear rate, the world record:
//! phase 7) shows `—`; tags and the year wait for the format's metadata fields (#1631).

use bevy::prelude::*;

use super::library::{GameInfo, Library, Records};
use super::list::Selection;
use crate::stats::MapStats;
use crate::ui::fonts::Type;
use crate::ui::shape::{Band, Shape, Side, Veil};
use crate::ui::tokens;
use crate::ui::widgets::{ellipsis_text, ranked_pill, star_pill, stat, text, unranked_pill};

/// The wedge's place and size, design px (under the toolbar).
const WEDGE_TOP: f32 = 96.0;
const WEDGE_W: f32 = 960.0;
const WEDGE_H: f32 = 290.0;
/// The left edge of the side's content.
const LEFT: f32 = 64.0;
/// The stats row and the stage strip, design px from the top.
const STATS_TOP: f32 = 405.0;
const STRIP_TOP: f32 = 486.0;
/// A stage bar's height.
const BAR_H: f32 = 22.0;

/// What the left side shows for the pick.
#[derive(Clone, Debug, PartialEq)]
pub struct HeroData {
    pub title: String,
    pub by: String,
    /// The creator's initial and the hue of its avatar.
    pub initial: String,
    pub hue: f32,
    pub map: String,
    pub stars: Option<f32>,
    pub ranked: bool,
    pub stats: Vec<(&'static str, String)>,
    pub stages: Vec<StageBar>,
}

/// One stage of the strip: its label and your deaths there.
#[derive(Clone, Debug, PartialEq)]
pub struct StageBar {
    pub label: String,
    pub deaths: u64,
}

impl HeroData {
    /// The left side for map `map` of `game`, with your records and the map's stats.
    pub fn of(game: &GameInfo, map: usize, records: Option<&Records>, stats: Option<&MapStats>, ranked: bool) -> HeroData {
        let info = game.maps.get(map);
        let pb = records
            .and_then(|r| r.pbs.get(map).cloned().flatten())
            .unwrap_or_else(|| "No PB".to_owned());
        let (deaths, clears, seconds, segments) = stats.map_or((0, 0, 0.0, &[][..]), |s| {
            (s.deaths, s.clears, s.play_seconds, s.segment_deaths.as_slice())
        });
        let stages: Vec<StageBar> = info.map_or(&[][..], |m| m.stages.as_slice())
            .iter()
            .enumerate()
            .map(|(i, name)| StageBar {
                label: if name.to_lowercase().contains("boss") {
                    "Boss".to_owned()
                } else {
                    (i + 1).to_string()
                },
                deaths: segments.get(i).copied().unwrap_or(0),
            })
            .collect();
        let stage_count = if stages.is_empty() {
            "—".to_owned()
        } else {
            stages.len().to_string()
        };
        HeroData {
            title: game.title.clone(),
            by: if game.creator.is_empty() {
                "Creator not listed".to_owned()
            } else {
                format!("by {}", game.creator)
            },
            initial: game
                .creator
                .chars()
                .next()
                .map_or_else(|| "?".to_owned(), |c| c.to_uppercase().collect()),
            hue: (game
                .creator
                .bytes()
                .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)))
                % 360) as f32,
            map: info.map_or_else(String::new, |m| m.name.clone()),
            stars: info.and_then(|m| m.stars),
            ranked,
            stats: vec![
                ("Stages", stage_count),
                ("Your PB", pb),
                ("Your deaths", crate::card::grouped(deaths)),
                (
                    "Clears",
                    if clears == 0 {
                        "No clears".to_owned()
                    } else {
                        crate::card::grouped(u64::from(clears))
                    },
                ),
                ("Time played", super::super::shelf::clock(seconds)),
                ("World record", "—".to_owned()),
            ],
            stages,
        }
    }
}

/// The wedge (its art follows the pick).
#[derive(Component)]
pub(super) struct Wedge;

/// The slots the left side's parts are built in.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HeroSlot {
    Head,
    Stats,
    Strip,
}

/// Spawns the left side's frame; [`follow`] fills it.
pub(super) fn spawn(commands: &mut Commands, camera: Entity) {
    let slot = |kind: HeroSlot, top: f32| {
        (
            kind,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(LEFT),
                top: Val::Px(top),
                width: Val::Px(WEDGE_W - LEFT - 70.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            UiTargetCamera(camera),
            DespawnOnExit(super::super::AppScreen::Select),
        )
    };
    commands.spawn((
        Wedge,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(-40.0),
            top: Val::Px(WEDGE_TOP),
            width: Val::Px(WEDGE_W + 40.0),
            height: Val::Px(WEDGE_H),
            ..default()
        },
        wedge_shape(None),
        Pickable::IGNORE,
        UiTargetCamera(camera),
        DespawnOnExit(super::super::AppScreen::Select),
    ));
    commands.spawn(slot(HeroSlot::Head, WEDGE_TOP + 32.0));
    commands.spawn(slot(HeroSlot::Stats, STATS_TOP));
    commands.spawn(slot(HeroSlot::Strip, STRIP_TOP));
}

/// The wedge: the art upright on the right, faded into the ground on the left, the accent line
/// along its bottom fading out.
fn wedge_shape(art: Option<Handle<Image>>) -> Shape {
    let mut shape = Shape::fill(tokens::black(0.25))
        .with_veil(Veil::across(
            tokens::GROUND.with_alpha(0.92),
            tokens::GROUND.with_alpha(0.7),
            tokens::GROUND.with_alpha(0.0),
            0.5,
        ))
        .with_band(Band {
            side: Side::Bottom,
            width: 3.0,
            color: tokens::ACCENT,
            fade_to: 0.85,
        });
    shape.radius = 0.0;
    if let Some(art) = art {
        shape = shape.with_image(art);
    }
    shape
}

/// Fills the left side for the pick: the slots rebuild only when their data changed; the
/// wedge's art and the backdrop follow the library's previews.
#[allow(clippy::too_many_arguments, reason = "the side's sources")]
pub(super) fn follow(
    sel: Res<Selection>,
    lib: Res<Library>,
    mods: Res<crate::mods::RunMods>,
    speed: Res<crate::practice::SlowSpeed>,
    stats: Option<Res<crate::stats::Stats>>,
    slots: Query<(Entity, &HeroSlot)>,
    mut wedge: Query<&mut Shape, With<Wedge>>,
    mut backdrop: ResMut<crate::ui::backdrop::Backdrop>,
    mut commands: Commands,
    mut seen: Local<u64>,
) {
    let previews = lib.revision() != *seen;
    *seen = lib.revision();
    let stats_changed = stats.as_ref().is_some_and(|s| s.is_changed());
    if !sel.is_changed() && !lib.is_changed() && !mods.is_changed() && !speed.is_changed() && !previews && !stats_changed {
        return;
    }
    let art = lib.backdrop.get(sel.game).cloned().flatten();
    for mut shape in &mut wedge {
        if shape.image != art {
            shape.image = art.clone();
        }
    }
    if backdrop.art != art && art.is_some() {
        backdrop.art = art;
    }
    let Some(game) = lib.games.get(sel.game) else {
        return;
    };
    let map_stats = stats.as_ref().and_then(|s| {
        let map = game.maps.get(sel.map)?;
        s.book.map(&crate::stats::map_key(&game.id, &map.id))
    });
    let map_name = game.maps.get(sel.map).map_or_else(String::new, |m| m.name.clone());
    let (ranked, _) = super::mod_sheet::header(mods.0, speed.0, &map_name);
    let data = HeroData::of(game, sel.map, lib.records.get(sel.game), map_stats, ranked);
    for (e, slot) in &slots {
        let d = data.clone();
        match slot {
            HeroSlot::Head => crate::ui::live::queue_refresh(&mut commands, e, head_key(&d), move |c, _| head(c, &d)),
            HeroSlot::Stats => crate::ui::live::queue_refresh(&mut commands, e, d.stats.clone(), |c, s| stats_row(c, s)),
            HeroSlot::Strip => crate::ui::live::queue_refresh(&mut commands, e, d.stages.clone(), |c, s| strip(c, s)),
        }
    }
}

/// What the head shows, as its key.
fn head_key(d: &HeroData) -> (String, String, String, Option<u32>, bool) {
    (d.title.clone(), d.by.clone(), d.map.clone(), d.stars.map(f32::to_bits), d.ranked)
}

/// The pills, the title and the creator line.
fn head(c: &mut ChildSpawner, d: &HeroData) {
    c.spawn(Node {
        align_items: AlignItems::Center,
        column_gap: Val::Px(10.0),
        ..default()
    })
    .with_children(|row| {
        if d.ranked {
            row.spawn(ranked_pill());
        } else {
            row.spawn(unranked_pill());
        }
        if let Some(stars) = d.stars {
            row.spawn(star_pill(stars));
        }
        row.spawn(text(&d.map, Type::barlow(17.0, 600), tokens::INK));
    });
    c.spawn((
        Node {
            margin: UiRect::top(Val::Px(10.0)),
            width: Val::Percent(100.0),
            // A column stretches the cut line to the whole width.
            flex_direction: FlexDirection::Column,
            ..default()
        },
        children![ellipsis_text(&d.title, Type::condensed(76.0, 700).italic(), tokens::INK)],
    ));
    c.spawn(Node {
        align_items: AlignItems::Center,
        column_gap: Val::Px(10.0),
        margin: UiRect::top(Val::Px(4.0)),
        ..default()
    })
    .with_children(|row| {
        row.spawn((
            Node {
                width: Val::Px(26.0),
                height: Val::Px(26.0),
                border_radius: BorderRadius::all(Val::Px(13.0)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::hsl(d.hue, 0.7, 0.52)),
            children![text(&d.initial, Type::barlow(12.0, 700), tokens::INK)],
        ));
        row.spawn(text(&d.by, Type::barlow(18.0, 500), tokens::SOFT));
    });
}

/// Six stats in a row.
fn stats_row(c: &mut ChildSpawner, stats: &[(&'static str, String)]) {
    c.spawn(Node {
        justify_content: JustifyContent::SpaceBetween,
        column_gap: Val::Px(18.0),
        ..default()
    })
    .with_children(|row| {
        for (label, value) in stats {
            let color = if value == "—" || value == "No PB" || value == "No clears" {
                tokens::MUTED
            } else {
                tokens::INK
            };
            row.spawn(stat(None, label, value, color));
        }
    });
}

/// One sheared bar per stage, coloured by your deaths there (the worst in the accent), with
/// its label and count under it.
fn strip(c: &mut ChildSpawner, stages: &[StageBar]) {
    if stages.is_empty() {
        c.spawn(text(
            "This map lists no stages.",
            Type::barlow(14.0, 400),
            tokens::MUTED,
        ));
        return;
    }
    let max = stages.iter().map(|s| s.deaths).max().unwrap_or(0);
    c.spawn(Node {
        column_gap: Val::Px(8.0),
        ..default()
    })
    .with_children(|row| {
        for s in stages {
            let share = if max == 0 { 0.0 } else { s.deaths as f32 / max as f32 };
            let worst = max > 0 && s.deaths == max;
            let fill = if worst {
                tokens::ACCENT
            } else {
                tokens::over(tokens::ACCENT, 0.18 + 0.5 * share, tokens::GROUND)
            };
            row.spawn(Node {
                flex_grow: 1.0,
                flex_basis: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|cell| {
                cell.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(BAR_H),
                        ..default()
                    },
                    Shape {
                        radius: 4.0,
                        ..Shape::fill(fill)
                    },
                ));
                cell.spawn(text(&s.label, Type::barlow(14.0, 700), tokens::INK));
                cell.spawn(text(
                    &crate::card::grouped(s.deaths),
                    Type::barlow(12.0, 500),
                    tokens::MUTED,
                ));
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::select::library::MapInfo;

    fn game() -> GameInfo {
        GameInfo {
            path: "kaenbyou".into(),
            id: "kaenbyou".into(),
            title: "I wanna be the Kaenbyou".into(),
            creator: "Kira".into(),
            maps: vec![MapInfo {
                id: "vh".into(),
                name: "Very Hard".into(),
                stars: Some(6.84),
                stages: vec!["Stage 1".into(), "Stage 2".into(), "Boss".into()],
            }],
        }
    }

    #[test]
    fn the_side_reads_the_map_and_your_stats() {
        let stats = MapStats {
            deaths: 9412,
            clears: 0,
            segment_deaths: vec![380, 1520, 130],
            ..MapStats::default()
        };
        let d = HeroData::of(&game(), 0, None, Some(&stats), true);
        assert_eq!(d.map, "Very Hard");
        assert_eq!(d.by, "by Kira");
        assert_eq!(d.stars, Some(6.84));
        assert_eq!(d.stats[0], ("Stages", "3".to_owned()));
        assert_eq!(d.stats[1], ("Your PB", "No PB".to_owned()));
        assert_eq!(d.stats[3], ("Clears", "No clears".to_owned()));
        assert_eq!(d.stats[5], ("World record", "—".to_owned()), "offline");
        let labels: Vec<&str> = d.stages.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["1", "2", "Boss"]);
        assert_eq!(d.stages[1].deaths, 1520);
    }

    #[test]
    fn a_new_player_sees_zeros_not_gaps() {
        let d = HeroData::of(&game(), 0, None, None, true);
        assert!(d.stages.iter().all(|s| s.deaths == 0));
        assert_eq!(d.stats[2], ("Your deaths", "0".to_owned()));
    }
}
