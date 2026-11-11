//! The live leaderboard, the HUD's left column (`Leaderboard.dc.html`, `LbStates.dc.html`,
//! `Hud169Local.dc.html`; docs/product.md, In-game display). Modelled on osu!lazer's gameplay
//! leaderboard: you start at #1 at 0:00 and fall as your time passes other runs' final times.
//!
//! This is the Local scope: your own top runs on this map, mode and mod combination, from the
//! [`Stats`] book. Global (#484) plugs other runs into the same [`Entry`] list.
//!
//! [`track`] reads the run each frame into [`Live`] (the runs, your time, your place, the pace
//! flash); [`build`] makes the panel for it, and [`animate`] moves it: rows slide to their new
//! slots (450 ms OutQuint), your row pops wider (OutElastic), the list scrolls to keep your row
//! centred and fades a row's height at an end only where more rows exist. The panel is built in
//! one of three modes: live, frozen (results, #41: #1 pinned on top) and mini (`Density::Small`:
//! rank and avatar, your time). Sizes are UI px, before `Layout::content`.

use std::sync::Arc;

use bevy::prelude::*;

use super::layout::Density;
use super::run_info::{Field, Preview, RunInfo};
use crate::mods::RunMods;
use crate::stats::{Stats, fmt_ticks, map_key};
use crate::ui::fonts::Type;
use crate::ui::icons::{Icon, icon};
use crate::ui::motion::{Ease, OffsetY, Opacity, Tween, Width};
use crate::ui::shape::{FadeMask, Ink, Shape, UiOpacity};
use crate::ui::tokens;
use crate::ui::widgets::{sheared, text};
use crate::view::Game;

/// Rows in view; yours is the middle one unless it is near an end.
pub const ROWS: usize = 7;
const ROW_HEIGHT: f32 = 44.0;
/// Rows are 3 px apart.
const PITCH: f32 = ROW_HEIGHT + 3.0;
/// How far a sheared row reaches past its box on each side.
const SHEAR_OVERHANG: f32 = tokens::SHEAR * ROW_HEIGHT / 2.0;
/// The small leaderboard's rows start this far in (`HudSmall` places the list at -4 with a 10 px
/// indent: 6 px), and its rank, gaps and padding are tighter than the artboard's so your row
/// (rank, avatar, time) fits inside the 124 px column.
const MINI_INDENT: f32 = 6.0;
/// Your row pops this much wider when it changes place (the artboard's 20 px).
const POP: f32 = 20.0;

/// One run you race: its final time, its time at the end of each segment and the line that
/// names it (the date, for your own runs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub ticks: u64,
    /// Empty when the run's segments do not match the game's.
    pub splits: Vec<u64>,
    pub label: String,
    /// The label on a compact panel, where the full one does not fit (`3 Nov`: no year).
    pub short: String,
}

/// Your place, from 1, at `elapsed` ticks among `entries` (fastest first): every run that took
/// `elapsed` ticks or fewer is ahead: you drop below a run only once your tick count exceeds
/// its, and on a tie it stays ahead (docs/product.md, Leaderboards).
pub fn place(entries: &[Entry], elapsed: u64) -> usize {
    1 + entries.partition_point(|e| e.ticks <= elapsed)
}

/// The slot (row from the top, from 0) of entry `k` while you are at `place`.
pub fn slot(k: usize, place: usize) -> usize {
    if k + 1 < place { k } else { k + 1 }
}

/// The first slot in view: your row centred, pushed to the edge at either end.
pub fn scroll(place: usize, total: usize, rows: usize) -> usize {
    (place - 1)
        .saturating_sub(rows / 2)
        .min(total.saturating_sub(rows))
}

/// Your split against another run's at one stage clear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pace {
    /// Your ticks minus theirs: negative is ahead.
    pub delta: i64,
    /// Their place.
    pub vs: usize,
}

/// The pace flash at the end of segment `segment` (stage `segment + 1` cleared) with `yours`
/// ticks on the clock: against the run directly above you, or at #1 the run directly below.
/// `None` when there is no such run or it has no split there.
pub fn pace(entries: &[Entry], place: usize, segment: usize, yours: u64) -> Option<Pace> {
    let (k, vs) = if place >= 2 {
        (place - 2, place - 1)
    } else {
        (0, 2)
    };
    let theirs = *entries.get(k)?.splits.get(segment)?;
    Some(Pace {
        delta: yours as i64 - theirs as i64,
        vs,
    })
}

/// `−1.20` or `+0:01:02.30`: the sign always shows (`+` on a tie), seconds and hundredths under a
/// minute.
pub fn fmt_delta(delta: i64, rate: u32) -> String {
    let sign = if delta < 0 { '−' } else { '+' };
    let abs = delta.unsigned_abs();
    let cs = abs * 100 / u64::from(rate.max(1));
    if cs < 6000 {
        format!("{sign}{}.{:02}", cs / 100, cs % 100)
    } else {
        format!("{sign}{}", fmt_ticks(abs, rate))
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `3 Nov 2026` for seconds since 1970 (UTC).
pub fn date(secs: u64) -> String {
    // Days to a civil date (proleptic Gregorian), H. Hinnant's `civil_from_days`.
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{day} {} {year}", MONTHS[(month - 1) as usize])
}

/// `3 Nov` from `3 Nov 2026`: a compact panel has no room for the year.
pub fn short_date(date: &str) -> &str {
    date.rsplit_once(' ').map_or(date, |(day_month, _)| day_month)
}

/// Sample boards for screenshot checks (`--hud-preview-leaderboard`), from the artboards'
/// sample data (`Leaderboard.dc.html`, scenario `local`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Sample {
    /// `Hud169Local`: nine of your runs, you at 40:02.10 (#3).
    Local,
    /// The same with the pace flash on.
    Pace,
    /// The same at 0:00: you at #1.
    Top,
    /// No clears yet.
    Empty,
    /// Frozen on results at 44:58.24 (#6), #1 pinned.
    Frozen,
}

/// The artboard's runs (seconds) and dates.
const SAMPLE_RUNS: [(f64, &str); 9] = [
    (2292.46, "3 Nov 2026"),
    (2380.12, "29 Oct 2026"),
    (2465.30, "28 Oct 2026"),
    (2571.88, "21 Oct 2026"),
    (2660.06, "20 Oct 2026"),
    (2731.40, "14 Oct 2026"),
    (2810.92, "9 Oct 2026"),
    (2902.14, "2 Oct 2026"),
    (2987.50, "30 Sep 2026"),
];

impl Sample {
    /// Your time in seconds.
    fn seconds(self) -> f64 {
        match self {
            Sample::Local | Sample::Pace => 2402.10,
            Sample::Top => 0.0,
            Sample::Empty => 761.36,
            Sample::Frozen => 2698.24,
        }
    }

    /// Your time in ticks at `rate`.
    pub fn ticks(self, rate: u32) -> u64 {
        (self.seconds() * f64::from(rate)).round() as u64
    }

    fn entries(self, rate: u32) -> Vec<Entry> {
        if self == Sample::Empty {
            return Vec::new();
        }
        let ticks = |s: f64| (s * f64::from(rate)).round() as u64;
        SAMPLE_RUNS
            .iter()
            .map(|&(s, label)| Entry {
                ticks: ticks(s),
                // One stage, cleared at a third of the run.
                splits: vec![ticks(s / 3.0), ticks(s)],
                label: label.to_owned(),
                short: short_date(label).to_owned(),
            })
            .collect()
    }
}

/// The pace flash on your row.
#[derive(Clone, Debug, PartialEq)]
pub struct Flash {
    pub pace: Pace,
    /// Seconds left before it fades back to your time.
    pub left: f32,
}

/// The leaderboard's state, read every frame by [`track`].
#[derive(Resource, Debug, Default)]
pub struct Live {
    /// The board the runs came from (`<map>|<board>`), so a new map or board reloads them.
    source: String,
    /// The runs you race, fastest first.
    pub entries: Arc<Vec<Entry>>,
    /// Bumped when `entries` is reloaded: the panel is rebuilt.
    pub generation: u32,
    /// Ticks per second.
    pub rate: u32,
    /// Your time and place.
    pub elapsed: u64,
    pub place: usize,
    /// The run info stage last seen, to spot a stage clear.
    stage: u32,
    pub flash: Option<Flash>,
    /// The panel is the results' frozen one (`--hud-preview-leaderboard frozen` until #41).
    pub frozen: bool,
}

#[cfg(test)]
impl Live {
    fn clone_state(&self) -> Live {
        Live {
            source: self.source.clone(),
            entries: Arc::clone(&self.entries),
            flash: self.flash.clone(),
            ..*self
        }
    }
}

/// Reads the run: loads the board's runs when the map or board changes or a retry starts a new
/// run (the clear just recorded joins the next run's board), then your place and the pace flash.
#[allow(clippy::too_many_arguments, reason = "one read of what the leaderboard shows")]
pub fn track(
    game: NonSend<Game>,
    mods: Option<Res<RunMods>>,
    stats: Option<Res<Stats>>,
    info: Res<RunInfo>,
    preview: Res<Preview>,
    time: Res<Time>,
    mut live: ResMut<Live>,
) {
    let sim = &game.sim;
    let ranked = sim.options().mode == iwbtg_sim::stripped::PlayMode::Stripped;
    let codes = mods.map_or(iwbtg_sim::mods::ModSet::NONE, |m| m.0).codes();
    let map = map_key(&sim.def().id, sim.map_id());
    let source = match preview.leaderboard {
        Some(sample) => format!("preview|{sample:?}"),
        None => format!("{map}|{}", crate::stats::board_key(ranked, &codes)),
    };
    let retry = info.ticks < live.elapsed && preview.leaderboard.is_none();
    if source != live.source || retry {
        let rate = info.rate;
        let entries = match preview.leaderboard {
            Some(sample) => sample.entries(rate),
            None => {
                let segments = sim.def().timing.stages.len() + 1;
                stats
                    .as_deref()
                    .and_then(|s| s.book.board(&map, ranked, &codes))
                    .map(|b| from_runs(&b.runs, segments))
                    .unwrap_or_default()
            }
        };
        live.source = source;
        live.entries = Arc::new(entries);
        live.generation = live.generation.wrapping_add(1);
        live.rate = rate;
        live.stage = info.stage;
        live.frozen = preview.leaderboard == Some(Sample::Frozen);
        live.flash = (preview.leaderboard == Some(Sample::Pace)).then(|| Flash {
            pace: Pace {
                delta: -(i64::from(rate) * 6 / 5),
                vs: 2,
            },
            left: f32::INFINITY,
        });
    }
    if live.elapsed != info.ticks {
        live.elapsed = info.ticks;
    }
    let now = place(&live.entries, info.ticks);
    if live.place != now {
        live.place = now;
    }
    if info.stage != live.stage {
        if info.stage > live.stage && live.stage > 0 {
            let segment = (info.stage - 2) as usize;
            let flash = pace(&live.entries, now, segment, info.ticks).map(|pace| Flash {
                pace,
                left: tokens::motion::PACE_HOLD,
            });
            if flash.is_some() {
                live.flash = flash;
            }
        }
        live.stage = info.stage;
    }
    if let Some(flash) = &mut live.flash {
        flash.left -= time.delta_secs();
        if flash.left <= 0.0 {
            live.flash = None;
        }
    }
}

/// Your top runs as entries; splits are kept only when they match the game's segments.
fn from_runs(runs: &[crate::stats::Run], segments: usize) -> Vec<Entry> {
    runs.iter()
        .map(|r| Entry {
            ticks: r.ticks,
            splits: if r.splits.len() == segments {
                r.splits.clone()
            } else {
                Vec::new()
            },
            label: date(r.finished_at),
            short: short_date(&date(r.finished_at)).to_owned(),
        })
        .collect()
}

/// The list of rows, with what it shows now.
#[derive(Component, Debug)]
pub struct List {
    frozen: bool,
    place: usize,
    total: usize,
    rows: usize,
    you_width: f32,
}

/// The node that scrolls the rows.
#[derive(Component)]
pub struct Track;

/// A row: one of the runs (by index), or yours.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row(Option<usize>);

/// The rank text of a row.
#[derive(Component)]
pub struct Rank(Option<usize>);

/// Your time line and the pace line over it.
#[derive(Component)]
pub struct TimeLine;
#[derive(Component)]
pub struct PaceLine;
#[derive(Component)]
pub struct PaceText;

impl Row {
    fn slot(self, place: usize) -> usize {
        self.0.map_or(place - 1, |k| slot(k, place))
    }
}

/// The text styles of a row.
struct Styles {
    rank: Type,
    name_you: Type,
    name: Type,
    time_you: Type,
    time: Type,
}

const STYLES: Styles = Styles {
    rank: Type::condensed(16.0, 700),
    name_you: Type::barlow(15.0, 700),
    name: Type::barlow(15.0, 600),
    time_you: Type::barlow(14.0, 700),
    time: Type::barlow(14.0, 500),
};
/// Your time in the small leaderboard.
const MINI_TIME: Type = Type::barlow(13.0, 600);

/// The leaderboard `width` UI px wide, for `live`.
pub fn build(width: f32, density: Density, live: &Live) -> impl Bundle {
    let mini = density == Density::Small;
    let entries = Arc::clone(&live.entries);
    let place = live.place.max(1);
    let total = entries.len() + 1;
    let rows = total.min(ROWS);
    let frozen = live.frozen;
    let first = scroll(place, total, rows);
    let indent = if mini { MINI_INDENT } else { 10.0 };
    let you_width = if mini {
        // `HudSmall` draws it the column's full width after the indent, which overhangs the game
        // by 6 px plus the shear; here it stops at the column's edge, shear included.
        width - MINI_INDENT - SHEAR_OVERHANG
    } else {
        width - 10.0
    };
    let header = Type {
        tracking: 0.14,
        caps: true,
        ..Type::barlow(12.0, 700)
    };
    let geometry = Geometry {
        mini,
        rank: match density {
            Density::Full => 34.0,
            Density::Compact => 28.0,
            Density::Small => 22.0,
        },
        short: density == Density::Compact,
        indent,
        you_width,
        their_width: if mini { 84.0 } else { width - 30.0 },
    };
    let empty = entries.is_empty();
    let rate = live.rate;
    (
        Node {
            width: Val::Px(width),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            ..default()
        },
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            c.spawn((
                Node {
                    height: Val::Px(18.0),
                    padding: UiRect::left(Val::Px(10.0)),
                    align_items: AlignItems::Center,
                    display: if mini { Display::None } else { Display::Flex },
                    ..default()
                },
                children![text("Local · your runs", header, tokens::SOFT)],
            ));
            // Frozen with #1 out of view: #1 pinned on top, three dots, then the list.
            if frozen && first > 0 {
                c.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|c| {
                    let node = Node {
                        margin: UiRect::left(Val::Px(indent)),
                        ..row_node(&geometry, false, geometry.their_width)
                    };
                    c.spawn(their_row(&geometry, &entries[0], (0, 0), rate, node));
                    c.spawn(dots());
                });
            }
            let height = rows as f32 * PITCH - 3.0;
            c.spawn((
                Node {
                    width: Val::Px(width),
                    height: Val::Px(height),
                    ..default()
                },
                FadeMask {
                    top: fade(first as f32 * PITCH),
                    bottom: fade((total - rows - first) as f32 * PITCH),
                },
                List {
                    frozen,
                    place,
                    total,
                    rows,
                    you_width,
                },
            ))
            .with_children(|c| {
                let y = -(first as f32) * PITCH;
                c.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Px(width),
                        height: Val::Px(total as f32 * PITCH),
                        ..default()
                    },
                    Track,
                    UiTransform::from_translation(Val2::px(0.0, y)),
                    Tween::<OffsetY>::at_rest(y, tokens::motion::REORDER, Ease::OutQuint),
                ))
                .with_children(|c| {
                    for (k, entry) in entries.iter().enumerate() {
                        let row = Row(Some(k));
                        let s = row.slot(place);
                        let node = row_node(&geometry, true, geometry.their_width);
                        c.spawn((
                            their_row(&geometry, entry, (k, s), rate, node),
                            row,
                            slid(s),
                            shown(s, first, rows),
                        ));
                    }
                    let row = Row(None);
                    let s = row.slot(place);
                    let node = row_node(&geometry, true, geometry.you_width);
                    c.spawn((your_row(&geometry, place, frozen, node), row, slid(s)));
                });
            });
            if empty && !mini {
                c.spawn((
                    Node {
                        padding: UiRect::left(Val::Px(14.0)),
                        ..default()
                    },
                    children![text(
                        "No clears yet — be the first",
                        Type::barlow(14.0, 500),
                        tokens::MUTED
                    )],
                ));
            }
        })),
    )
}

/// The sizes rows are built at.
struct Geometry {
    mini: bool,
    /// The rank column: the artboard's 34 px; 28 when compact, so a time still fits at 4:3;
    /// 22 in the small leaderboard.
    rank: f32,
    /// Compact: rows name runs by [`Entry::short`].
    short: bool,
    indent: f32,
    you_width: f32,
    their_width: f32,
}

/// How strong the fade at an end is when `px` of rows lie past it: a row's height once a whole
/// row is there.
fn fade(px: f32) -> f32 {
    ROW_HEIGHT * (px / PITCH).clamp(0.0, 1.0)
}

/// A row's place on the track, ready to slide.
fn slid(slot: usize) -> (UiTransform, Tween<OffsetY>) {
    let y = slot as f32 * PITCH;
    (
        UiTransform::from_translation(Val2::px(0.0, y)),
        Tween::<OffsetY>::at_rest(y, tokens::motion::REORDER, Ease::OutQuint),
    )
}

/// Rows well out of view are not drawn.
fn shown(slot: usize, first: usize, rows: usize) -> Visibility {
    if slot + 2 < first || slot > first + rows + 1 {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    }
}

/// A row's box, `width` wide: absolute on the track (`on_track`) or in flow (the pinned row).
fn row_node(g: &Geometry, on_track: bool, width: f32) -> Node {
    let mut node = sheared(ROW_HEIGHT, 0.0, Shape::fill(Color::NONE)).0;
    node.width = Val::Px(width);
    if on_track {
        node.position_type = PositionType::Absolute;
        node.left = Val::Px(g.indent);
        node.top = Val::Px(0.0);
    }
    node.justify_content = JustifyContent::FlexStart;
    node.padding = if g.mini {
        UiRect::new(Val::Px(8.0), Val::Px(4.0), Val::ZERO, Val::ZERO)
    } else {
        UiRect::new(Val::Px(10.0), Val::Px(12.0), Val::ZERO, Val::ZERO)
    };
    node.column_gap = Val::Px(if g.mini { 4.0 } else { 10.0 });
    node
}

/// Entry `k`, one of your past runs, at `slot`: rank, avatar, its date over its time.
fn their_row(
    g: &Geometry,
    entry: &Entry,
    (k, slot): (usize, usize),
    rate: u32,
    node: Node,
) -> impl Bundle + use<> {
    let mini = g.mini;
    let rank = g.rank;
    let label = if g.short {
        entry.short.clone()
    } else {
        entry.label.clone()
    };
    let time = fmt_ticks(entry.ticks, rate);
    (
        node,
        Shape::fill(tokens::ROW_PAST),
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            c.spawn(rank_cell(rank, &format!("#{}", slot + 1), tokens::MUTED))
                .insert(Rank(Some(k)));
            c.spawn(avatar());
            if mini {
                return;
            }
            c.spawn(text_column()).with_children(|c| {
                c.spawn((text(&label, STYLES.name, tokens::ROW_NAME), TextLayout::no_wrap()));
                c.spawn((text(&time, STYLES.time, tokens::MUTED), TextLayout::no_wrap()));
            });
        })),
    )
}

/// Your row: rank, avatar, `Now` (`This run` frozen) over your time, the pace flash over that.
fn your_row(g: &Geometry, place: usize, frozen: bool, node: Node) -> impl Bundle + use<> {
    let mini = g.mini;
    let rank = g.rank;
    let width = g.you_width;
    (
        node,
        Shape::fill(tokens::ROW_YOU).with_shadow(6.0, 18.0, Color::srgba(0.0, 0.0, 0.0, 0.35)),
        Tween::<Width>::at_rest(width, tokens::motion::POP, Ease::OutElastic),
        // Over the rows it slides past.
        ZIndex(1),
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            c.spawn(rank_cell(rank, &format!("#{place}"), tokens::INK))
                .insert(Rank(None));
            c.spawn(avatar());
            let time = if mini { MINI_TIME } else { STYLES.time_you };
            let lines = move |c: &mut ChildSpawner| {
                c.spawn(Node::default()).with_children(|c| {
                    c.spawn((
                        Node::default(),
                        TimeLine,
                        UiOpacity(1.0),
                        Tween::<Opacity>::at_rest(1.0, tokens::motion::QUICK, Ease::OutQuint),
                        children![(
                            text("0:00.00", time, tokens::INK),
                            TextLayout::no_wrap(),
                            Field::YourTime
                        )],
                    ));
                    c.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(0.0),
                            ..default()
                        },
                        PaceLine,
                        UiOpacity(0.0),
                        Tween::<Opacity>::at_rest(0.0, tokens::motion::QUICK, Ease::OutQuint),
                        children![(text("", time, tokens::AHEAD), TextLayout::no_wrap(), PaceText,)],
                    ));
                });
            };
            if mini {
                lines(c);
                return;
            }
            c.spawn(text_column()).with_children(|c| {
                let name = if frozen { "This run" } else { "Now" };
                c.spawn((text(name, STYLES.name_you, tokens::INK), TextLayout::no_wrap()));
                lines(c);
            });
        })),
    )
}

fn rank_cell(width: f32, rank: &str, color: Color) -> impl Bundle + use<> {
    (
        Node {
            width: Val::Px(width),
            flex_shrink: 0.0,
            ..default()
        },
        children![text(rank, STYLES.rank, color)],
    )
}

fn text_column() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        flex_grow: 1.0,
        min_width: Val::Px(0.0),
        row_gap: Val::Px(1.0),
        // The artboard's `white-space: nowrap; overflow: hidden`: a long line is cut at the row.
        overflow: Overflow::clip_x(),
        ..default()
    }
}

/// The 30 px avatar square; until profiles exist, the no-avatar mark. A shape, so the list's
/// fade reaches it.
fn avatar() -> impl Bundle {
    (
        Node {
            width: Val::Px(30.0),
            height: Val::Px(30.0),
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        Shape {
            shear: 0.0,
            radius: 7.0,
            ..Shape::fill(tokens::AVATAR_EMPTY)
        },
        children![(
            icon(Icon::Person, 16.0, tokens::AVATAR_MARK),
            Ink(tokens::AVATAR_MARK)
        )],
    )
}

/// The three dots under a pinned row.
fn dots() -> impl Bundle {
    let dot = || {
        (
            Node {
                width: Val::Px(3.0),
                height: Val::Px(3.0),
                border_radius: BorderRadius::all(Val::Px(1.5)),
                ..default()
            },
            BackgroundColor(tokens::GAP_DOT),
        )
    };
    (
        Node {
            margin: UiRect::left(Val::Px(22.0)),
            column_gap: Val::Px(5.0),
            ..default()
        },
        children![dot(), dot(), dot()],
    )
}

type RowParts = (
    &'static Row,
    &'static mut Tween<OffsetY>,
    &'static mut Visibility,
    Option<&'static mut Tween<Width>>,
);

/// Slides the rows, scrolls the list, pops your row, sets the ranks, fades the ends and shows
/// or hides the pace flash.
#[allow(clippy::too_many_arguments, reason = "one pass over the leaderboard's parts")]
#[allow(clippy::type_complexity, reason = "one pass over the leaderboard's parts")]
pub fn animate(
    live: Res<Live>,
    mut lists: Query<(&mut List, &mut FadeMask)>,
    mut track: Query<&mut Tween<OffsetY>, (With<Track>, Without<Row>)>,
    mut rows: Query<RowParts, Without<Track>>,
    ranks: Query<(&Rank, &Children)>,
    mut texts: Query<&mut Text, Without<PaceText>>,
    mut lines: Query<(&mut Tween<Opacity>, Has<PaceLine>), Or<(With<TimeLine>, With<PaceLine>)>>,
    mut pace: Query<(&mut Text, &mut Ink), With<PaceText>>,
) {
    let Ok((mut list, mut mask)) = lists.single_mut() else {
        return;
    };
    let Ok(mut track) = track.single_mut() else {
        return;
    };
    let place = live.place.max(1);
    if !list.frozen && list.place != place {
        list.place = place;
        let first = scroll(place, list.total, list.rows);
        track.retarget(-(first as f32) * PITCH);
        for (row, mut tween, _, width) in &mut rows {
            tween.retarget(row.slot(place) as f32 * PITCH);
            if let Some(mut width) = width {
                *width = Tween::new(
                    list.you_width - POP,
                    list.you_width,
                    tokens::motion::POP,
                    Ease::OutElastic,
                );
            }
        }
        for (rank, children) in &ranks {
            let r = rank.0.map_or(place, |k| slot(k, place) + 1);
            let want = format!("#{r}");
            for child in children.iter() {
                if let Ok(mut text) = texts.get_mut(child)
                    && text.0 != want
                {
                    text.0.clone_from(&want);
                }
            }
        }
    }
    // The fades and which rows are drawn follow the scroll as it moves.
    let y = -track.value();
    let below = (list.total - list.rows) as f32 * PITCH - y;
    let want = FadeMask {
        top: fade(y),
        bottom: fade(below),
    };
    if *mask != want {
        *mask = want;
    }
    let view = list.rows as f32 * PITCH;
    for (_, tween, mut visibility, _) in &mut rows {
        let at = tween.value();
        let want = if at + 2.0 * PITCH < y || at > y + view + PITCH {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *visibility != want {
            *visibility = want;
        }
    }
    // The pace flash: in over the time at once, out again after it holds.
    let on = live.flash.is_some();
    for (mut tween, is_pace) in &mut lines {
        let to = if on == is_pace { 1.0 } else { 0.0 };
        tween.retarget(to);
    }
    if let Some(flash) = &live.flash {
        let mini = list.you_width < 200.0;
        let delta = fmt_delta(flash.pace.delta, live.rate);
        let want = if mini {
            delta
        } else {
            format!("{delta} vs #{}", flash.pace.vs)
        };
        let color = if flash.pace.delta < 0 {
            tokens::AHEAD
        } else {
            tokens::BEHIND
        };
        for (mut text, mut ink) in &mut pace {
            if text.0 != want {
                text.0.clone_from(&want);
            }
            if ink.0 != color {
                ink.0 = color;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(ticks: &[u64]) -> Vec<Entry> {
        ticks
            .iter()
            .map(|&t| Entry {
                ticks: t,
                splits: vec![t / 2, t],
                label: String::new(),
                short: String::new(),
            })
            .collect()
    }

    /// The issue's user check: two PBs, you start at #1 and drop below each as your time
    /// passes it; a tie keeps the other run ahead.
    #[test]
    fn you_start_first_and_fall_as_your_time_passes_each_run() {
        let e = runs(&[100, 200]);
        assert_eq!(place(&e, 0), 1);
        assert_eq!(place(&e, 99), 1);
        assert_eq!(place(&e, 100), 2, "a tie: the other run stays ahead");
        assert_eq!(place(&e, 150), 2);
        assert_eq!(place(&e, 201), 3);
        assert_eq!(place(&[], 5000), 1);
    }

    #[test]
    fn passing_a_run_swaps_your_slots() {
        // At #2 the first run is above you, the second below.
        assert_eq!((slot(0, 2), slot(1, 2)), (0, 2));
        // Passing the second moves it up into your old slot.
        assert_eq!((slot(0, 3), slot(1, 3)), (0, 1));
    }

    #[test]
    fn your_row_stays_centred_and_pushes_to_the_edge_at_the_ends() {
        // 10 runs and you: 11 rows, 7 in view.
        assert_eq!(scroll(1, 11, 7), 0, "at the top your row is the first");
        assert_eq!(scroll(4, 11, 7), 0);
        assert_eq!(scroll(5, 11, 7), 1, "centred: three rows above, three below");
        assert_eq!(scroll(11, 11, 7), 4, "at the bottom your row is the last");
        assert_eq!(scroll(2, 3, 3), 0, "fewer rows than the view: no scroll");
    }

    #[test]
    fn the_pace_flash_compares_with_the_run_above_or_at_the_top_the_one_below() {
        let e = runs(&[100, 200, 300]);
        // #3, stage 1 cleared at 90: the run at #2 cleared it at 100.
        assert_eq!(pace(&e, 3, 0, 90), Some(Pace { delta: -10, vs: 2 }));
        // #1: against #2, the fastest run.
        assert_eq!(pace(&e, 1, 0, 60), Some(Pace { delta: 10, vs: 2 }));
        assert_eq!(pace(&[], 1, 0, 60), None, "nobody to compare with");
        let mut odd = runs(&[100]);
        odd[0].splits.clear();
        assert_eq!(pace(&odd, 2, 0, 60), None, "a run without matching splits");
    }

    #[test]
    fn deltas_always_carry_a_sign() {
        assert_eq!(fmt_delta(-60, 50), "−1.20");
        assert_eq!(fmt_delta(40, 50), "+0.80");
        assert_eq!(fmt_delta(0, 50), "+0.00");
        assert_eq!(fmt_delta(-50 * 62, 50), "−1:02.00");
    }

    #[test]
    fn dates_read_day_month_year() {
        assert_eq!(date(0), "1 Jan 1970");
        assert_eq!(date(951_782_400), "29 Feb 2000");
        assert_eq!(date(1_793_750_400 + 86_399), "4 Nov 2026");
        assert_eq!(short_date("29 Oct 2026"), "29 Oct");
    }

    /// Passing a run slides it above you and you below it, sets both ranks and pops your row;
    /// a stage clear's flash shows over your time.
    #[test]
    fn passing_a_run_slides_the_rows_and_the_flash_covers_your_time() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let mut live = Live {
            entries: Arc::new(runs(&[100, 200])),
            rate: 50,
            place: 1,
            ..Live::default()
        };
        world.spawn(build(240.0, Density::Full, &live));
        world.insert_resource(live.clone_state());
        world.run_system_once(animate).unwrap();
        live.place = 2;
        world.insert_resource(live.clone_state());
        world.run_system_once(animate).unwrap();
        let mut rows = world.query::<(&Row, &Tween<OffsetY>, Option<&Tween<Width>>)>();
        for (row, tween, width) in rows.iter(&world) {
            let want = match row.0 {
                Some(0) => 0.0,
                None => PITCH,
                _ => 2.0 * PITCH,
            };
            assert_eq!(tween.to, want, "{row:?}");
            if let Some(width) = width {
                assert_eq!(
                    (width.from, width.to),
                    (210.0, 230.0),
                    "your row pops 20 px wider"
                );
            }
        }
        let mut ranks = world.query::<(&Rank, &Children)>();
        let mut seen: Vec<(Option<usize>, String)> = ranks
            .iter(&world)
            .map(|(r, c)| (r.0, world.get::<Text>(c[0]).unwrap().0.clone()))
            .collect();
        seen.sort();
        assert_eq!(
            seen,
            [
                (None, "#2".to_owned()),
                (Some(0), "#1".to_owned()),
                (Some(1), "#3".to_owned())
            ]
        );
        live.flash = Some(Flash {
            pace: Pace { delta: -60, vs: 1 },
            left: 3.0,
        });
        world.insert_resource(live.clone_state());
        world.run_system_once(animate).unwrap();
        let mut lines = world.query::<(&Tween<Opacity>, Has<PaceLine>)>();
        for (tween, is_pace) in lines.iter(&world) {
            assert_eq!(tween.to, if is_pace { 1.0 } else { 0.0 });
        }
        let mut pace = world.query_filtered::<(&Text, &Ink), With<PaceText>>();
        let (text, ink) = pace.single(&world).unwrap();
        assert_eq!(text.0, "−1.20 vs #1");
        assert_eq!(ink.0, tokens::AHEAD);
    }

    /// `track` on the clear demo with the Local sample: your place follows the time, a stage
    /// clear flashes your split against the run above for 3 s, and a retry reloads the runs.
    #[test]
    fn track_follows_the_time_and_flashes_a_stage_clear() {
        use bevy::ecs::system::RunSystemOnce;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo-clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut world = crate::view::test_world(&pkg);
        world.insert_resource(Time::<()>::default());
        world.insert_resource(Preview {
            held: 0,
            leaderboard: Some(Sample::Local),
        });
        world.init_resource::<Live>();
        let info = |ticks: u64, stage: u32| RunInfo {
            ticks,
            rate: 50,
            stage,
            stages: 2,
            ..RunInfo::default()
        };
        // 2402.10 s: #3, as the artboard draws it.
        world.insert_resource(info(120_105, 1));
        world.run_system_once(track).unwrap();
        let live = world.resource::<Live>();
        assert_eq!((live.place, live.entries.len(), live.flash.clone()), (3, 9, None));
        // A retry (the time goes back), then stage 1 cleared at 790 s: you are #1 again.
        world.insert_resource(info(39_500, 2));
        world.run_system_once(track).unwrap();
        let flash = world.resource::<Live>().flash.clone().expect("a pace flash");
        assert_eq!(world.resource::<Live>().place, 1);
        // At #1 it compares with #2, the fastest run, whose stage split is 2292.46 / 3 s.
        assert_eq!(
            flash.pace,
            Pace {
                delta: 39_500 - 38_208,
                vs: 2
            }
        );
        assert_eq!(flash.left, tokens::motion::PACE_HOLD);
    }

    #[test]
    fn the_local_sample_puts_you_third_as_the_artboard_does() {
        let e = Sample::Local.entries(50);
        let p = place(&e, Sample::Local.ticks(50));
        assert_eq!(p, 3);
        assert_eq!(scroll(p, e.len() + 1, ROWS), 0);
        let f = Sample::Frozen;
        assert!(
            scroll(place(&f.entries(50), f.ticks(50)), 10, ROWS) > 0,
            "#1 is pinned"
        );
    }
}
