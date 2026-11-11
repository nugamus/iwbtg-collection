//! Run info, the HUD's right column (`RunInfo.dc.html`; docs/product.md, In-game display):
//! paused line, mod row, timer, deaths, stage of total, deaths this stage and the input display.
//!
//! [`read`] turns the sim, the run's mods and the input collector into [`RunInfo`] every frame;
//! [`build`] makes the panel for a [`Look`] (what changes the panel's shape: ranking, mods,
//! toggles, extra actions) and [`show`] writes the numbers and lit keys into it.

use bevy::prelude::*;
use iwbtg_sim::input::Action;
use iwbtg_sim::mods::ModSet;
use iwbtg_sim::stripped::PlayMode;

use super::layout::Density;
use crate::input::Collector;
use crate::mods::RunMods;
use crate::settings::HudShow;
use crate::stats::fmt_ticks;
use crate::ui::fonts::Type;
use crate::ui::icons::{Icon, icon};
use crate::ui::shape::{Ink, Shape};
use crate::ui::tokens;
use crate::ui::widgets::{sheared, text};
use crate::view::Game;

/// The input display's arrows: 26 px each, 18 px apart when there is room.
const ARROW: f32 = 26.0;
const ARROW_GAP: f32 = 18.0;
/// The arrows in the artboard's order, with their turn from pointing up.
const ARROWS: [(Action, f32); 4] = [
    (Action::Left, -90.0),
    (Action::Up, 0.0),
    (Action::Down, 180.0),
    (Action::Right, 90.0),
];
/// The words under the arrows: the standard three, then the extra actions the player has bound.
const BUTTONS: [(Action, &str); 7] = [
    (Action::Jump, "Jump"),
    (Action::Shoot, "Shoot"),
    (Action::Restart, "Restart"),
    (Action::Extra1, "Extra 1"),
    (Action::Extra2, "Extra 2"),
    (Action::Extra3, "Extra 3"),
    (Action::Extra4, "Extra 4"),
];
const EXTRAS: u32 =
    Action::Extra1 as u32 | Action::Extra2 as u32 | Action::Extra3 as u32 | Action::Extra4 as u32;
/// `Paused · timer running`: `#ffb3bd`.
const PAUSED_INK: Color = Color::srgb_u8(0xff, 0xb3, 0xbd);
/// A held arrow or word: white (the artboard's glow is not drawn).
const HELD: Color = Color::WHITE;

/// Written by the pause menu (`pause.rs`, #479): whether it is open, and the ticks the run has
/// spent paused while its timer ran, which the timer adds to `Sim::ranked_ticks` (the timer keeps
/// running).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Paused {
    pub open: bool,
    pub ticks: u64,
}

/// What gives the panel its shape. The panel is rebuilt when it changes.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    /// Play original, or a mod that keeps the run off the boards: the amber Unranked pill.
    pub unranked: bool,
    pub mods: ModSet,
    pub show: HudShow,
    /// The extra actions the player has bound ([`Action`] bits).
    pub extras: u32,
}

impl Default for Look {
    fn default() -> Self {
        Look {
            unranked: false,
            mods: ModSet::NONE,
            show: HudShow::default(),
            extras: 0,
        }
    }
}

impl Look {
    /// The mod row shows only for an unranked run or when mods are on.
    fn mod_row(&self) -> bool {
        self.show.mods && (self.unranked || self.mods != ModSet::NONE)
    }
}

/// The run's numbers this frame.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct RunInfo {
    /// Ranked time (`Sim::ranked_ticks` plus the paused ticks) and the game's ticks per second.
    pub ticks: u64,
    pub rate: u32,
    /// The death counter.
    pub deaths: u32,
    /// The stage being played, from 1, and how many there are (stage splits + 1).
    pub stage: u32,
    pub stages: u32,
    pub stage_deaths: u32,
    /// The actions the last tick saw held or pressed.
    pub held: u32,
    pub paused: bool,
}

/// Counts the deaths since the current stage began. A new stage, or a retry (the tick or the
/// death counter going back), starts again from zero; a run from a save starts at zero too.
#[derive(Resource, Default, Debug)]
pub struct StageDeaths {
    /// Tick, stages done and the death counter when this stage began.
    last: Option<(u64, u16, u32)>,
}

impl StageDeaths {
    pub fn observe(&mut self, tick: u64, stages_done: u16, deaths: u32) -> u32 {
        match self.last {
            Some((t, s, base)) if tick >= t && s == stages_done && deaths >= base => {
                self.last = Some((tick, s, base));
                deaths - base
            }
            _ => {
                self.last = Some((tick, stages_done, deaths));
                0
            }
        }
    }
}

/// For screenshot checks (`--hud-preview-held`): keys drawn as held, as a run would show them.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct Preview {
    pub held: u32,
}

/// `1,284`.
pub fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `42:10` and `.34`: the timer's two parts.
pub fn timer_parts(ticks: u64, rate: u32) -> (String, String) {
    let t = fmt_ticks(ticks, rate);
    let dot = t.rfind('.').unwrap_or(t.len());
    let (main, cs) = t.split_at(dot);
    (main.to_owned(), cs.to_owned())
}

/// Reads the run into [`RunInfo`] and [`Look`].
#[allow(clippy::too_many_arguments, reason = "one read of everything the panel shows")]
pub fn read(
    game: NonSend<Game>,
    mods: Option<Res<RunMods>>,
    collector: Option<Res<Collector>>,
    paused: Res<Paused>,
    preview: Res<Preview>,
    shown: Res<super::Shown>,
    mut stage_deaths: ResMut<StageDeaths>,
    mut info: ResMut<RunInfo>,
    mut look: ResMut<Look>,
) {
    let sim = &game.sim;
    let world = sim.world();
    let mods = mods.map_or(ModSet::NONE, |m| m.0);
    let unranked = sim.options().mode == PlayMode::Original || mods.unranked_by().is_some();
    let (held, extras) = collector.map_or((0, 0), |c| {
        let f = c.last();
        let bound = BUTTONS
            .iter()
            .filter(|(a, _)| *a as u32 & EXTRAS != 0 && c.bindings().binds(*a))
            .fold(0, |m, (a, _)| m | a.bit());
        (f.held | f.pressed, bound)
    });
    let want = Look {
        unranked,
        mods,
        show: shown.0,
        extras,
    };
    if *look != want {
        *look = want;
    }
    let stages = sim.def().timing.stages.len() as u32 + 1;
    let deaths = world.engine.run.deaths;
    let now = RunInfo {
        ticks: sim.ranked_ticks() + paused.ticks,
        rate: sim.def().tick_rate,
        deaths,
        stage: (u32::from(world.timing.stages_done) + 1).min(stages),
        stages,
        stage_deaths: stage_deaths.observe(world.tick, world.timing.stages_done, deaths),
        held: held | preview.held,
        paused: paused.open,
    };
    if *info != now {
        *info = now;
    }
}

/// A number the panel shows.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Time,
    Hundredths,
    Deaths,
    Stage,
    /// ` / 7`.
    Stages,
    StageDeaths,
    /// Your row's time on the leaderboard.
    YourTime,
}

/// The paused line.
#[derive(Component)]
pub struct PausedLine;

/// An arrow or a word of the input display.
#[derive(Component)]
pub struct Key(pub Action);

/// The bar under a key: the accent while it is held.
#[derive(Component)]
pub struct KeyBar(pub Action);

/// Writes the numbers, the paused line and the lit keys.
#[allow(clippy::type_complexity, reason = "one pass over the panel's parts")]
pub fn show(
    info: Res<RunInfo>,
    mut texts: Query<(&Field, &mut Text)>,
    mut paused: Query<&mut Node, With<PausedLine>>,
    mut keys: Query<(&Key, Option<&mut ImageNode>, Option<&mut Ink>)>,
    mut bars: Query<(&KeyBar, &mut BackgroundColor)>,
    added: Query<(), Added<Field>>,
) {
    if !info.is_changed() && added.is_empty() {
        return;
    }
    let (time, cs) = timer_parts(info.ticks, info.rate);
    for (field, mut t) in &mut texts {
        let want = match field {
            Field::Time => time.clone(),
            Field::Hundredths => cs.clone(),
            Field::Deaths => thousands(info.deaths),
            Field::Stage => info.stage.to_string(),
            Field::Stages => format!(" / {}", info.stages),
            Field::StageDeaths => thousands(info.stage_deaths),
            Field::YourTime => format!("{time}{cs}"),
        };
        if t.0 != want {
            t.0 = want;
        }
    }
    let display = if info.paused { Display::Flex } else { Display::None };
    for mut node in &mut paused {
        if node.display != display {
            node.display = display;
        }
    }
    let on = |a: Action| info.held & a.bit() != 0;
    for (Key(a), image, ink) in &mut keys {
        if let Some(mut image) = image {
            let want = if on(*a) { HELD } else { tokens::KEY_IDLE };
            if image.color != want {
                image.color = want;
            }
        }
        if let Some(mut ink) = ink {
            let want = if on(*a) { HELD } else { tokens::BUTTON_IDLE };
            if ink.0 != want {
                ink.0 = want;
            }
        }
    }
    for (KeyBar(a), mut bg) in &mut bars {
        let want = if on(*a) { tokens::ACCENT } else { Color::NONE };
        if bg.0 != want {
            bg.0 = want;
        }
    }
}

/// Run info `width` UI px wide for `look`: the shown elements only.
pub fn build(width: f32, density: Density, look: Look) -> impl Bundle {
    let (timer, cs, value, gap) = match density {
        Density::Full => (58.0, 36.0, 24.0, 18.0),
        Density::Compact => (44.0, 28.0, 20.0, 18.0),
        Density::Small => (30.0, 19.0, 16.0, 10.0),
    };
    let stack = density == Density::Small;
    let show = look.show;
    (
        Node {
            width: Val::Px(width),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(gap),
            ..default()
        },
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            c.spawn(paused_line());
            if look.mod_row() {
                c.spawn(mod_row(look));
            }
            if show.timer {
                c.spawn((
                    Node {
                        align_items: AlignItems::Baseline,
                        ..default()
                    },
                    children![
                        (
                            text(
                                "0:00",
                                Type {
                                    tracking: -0.01,
                                    ..Type::condensed(timer, 600)
                                },
                                tokens::INK
                            ),
                            Field::Time,
                        ),
                        (
                            text(".00", Type::condensed(cs, 600), tokens::SOFT),
                            Field::Hundredths
                        ),
                    ],
                ));
            }
            if show.deaths || show.stage || show.stage_deaths {
                c.spawn(stats(look.show, stack, value));
            }
            if show.input && !stack {
                c.spawn(input_display(width, look.extras));
            }
        })),
    )
}

/// The red dot and `Paused · timer running`, hidden until a pause.
fn paused_line() -> impl Bundle {
    let style = Type {
        tracking: 0.12,
        caps: true,
        ..Type::barlow(12.0, 700)
    };
    (
        Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            display: Display::None,
            ..default()
        },
        PausedLine,
        children![
            (
                Node {
                    width: Val::Px(8.0),
                    height: Val::Px(8.0),
                    flex_shrink: 0.0,
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(tokens::ACCENT),
                BoxShadow::new(
                    tokens::ACCENT.with_alpha(0.25),
                    Val::ZERO,
                    Val::ZERO,
                    Val::Px(4.0),
                    Val::ZERO,
                ),
            ),
            text("Paused · timer running", style, PAUSED_INK),
        ],
    )
}

/// The Unranked pill and the mods' codes: 24 px sheared pills, 6 px apart.
fn mod_row(look: Look) -> impl Bundle {
    let unranked = Type {
        tracking: 0.1,
        ..Type::PILL_CAPS
    };
    let code = Type {
        tracking: 0.04,
        ..Type::MOD_CODE
    };
    (
        Node {
            align_items: AlignItems::Center,
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(6.0),
            row_gap: Val::Px(6.0),
            // The shear leans 2.4 px past each side of a 24 px pill.
            padding: UiRect::horizontal(Val::Px(3.0)),
            ..default()
        },
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            if look.unranked {
                c.spawn((
                    sheared(24.0, 12.0, Shape::fill(tokens::AMBER)),
                    children![text("Unranked", unranked, tokens::ON_AMBER)],
                ));
            }
            for m in look.mods.iter() {
                let mut pill = sheared(24.0, 8.0, Shape::fill(tokens::CHIP));
                pill.0.min_width = Val::Px(38.0);
                c.spawn((pill, children![text(m.code, code, tokens::INK)]));
            }
        })),
    )
}

/// Deaths, stage of total and deaths this stage: two columns, or one stacked under the timer.
fn stats(show: HudShow, stack: bool, value: f32) -> impl Bundle {
    let label = Type::LABEL;
    let number = Type::barlow(value, 600);
    let stat = move |name: &str, values: Vec<(Field, &'static str, Color)>| {
        (
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                min_width: Val::Px(0.0),
                ..default()
            },
            Children::spawn((
                Spawn(text(name, label, tokens::MUTED)),
                Spawn((
                    Node {
                        align_items: AlignItems::Baseline,
                        ..default()
                    },
                    Children::spawn(SpawnIter(
                        values
                            .into_iter()
                            .map(move |(f, v, color)| (text(v, number, color), f)),
                    )),
                )),
            )),
        )
    };
    (
        Node {
            display: Display::Grid,
            grid_template_columns: if stack {
                vec![GridTrack::minmax(
                    MinTrackSizingFunction::Px(0.0),
                    MaxTrackSizingFunction::Fraction(1.0),
                )]
            } else {
                RepeatedGridTrack::minmax(
                    2,
                    MinTrackSizingFunction::Px(0.0),
                    MaxTrackSizingFunction::Fraction(1.0),
                )
            },
            row_gap: Val::Px(if stack { 6.0 } else { 14.0 }),
            column_gap: Val::Px(if stack { 6.0 } else { 16.0 }),
            ..default()
        },
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            if show.deaths {
                c.spawn(stat("Deaths", vec![(Field::Deaths, "0", tokens::INK)]));
            }
            if show.stage {
                c.spawn(stat(
                    "Stage",
                    vec![
                        (Field::Stage, "1", tokens::INK),
                        (Field::Stages, " / 1", tokens::MUTED),
                    ],
                ));
            }
            if show.stage_deaths {
                c.spawn(stat("This stage", vec![(Field::StageDeaths, "0", tokens::INK)]));
            }
        })),
    )
}

/// The input display: four arrows, then Jump, Shoot, Restart and the bound extra actions.
fn input_display(width: f32, extras: u32) -> impl Bundle {
    // `Hud43`'s 152 px panel is narrower than four arrows 18 px apart (158 px): close the gaps.
    let gap = ((width - 4.0 * ARROW) / 3.0).clamp(4.0, ARROW_GAP);
    let arrows = ARROWS.map(|(action, deg)| {
        (
            Node {
                width: Val::Px(ARROW),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(6.0),
                ..default()
            },
            children![
                (
                    icon(Icon::Arrow, ARROW, tokens::KEY_IDLE),
                    UiTransform {
                        rotation: Rot2::degrees(deg),
                        ..default()
                    },
                    Key(action),
                ),
                bar(action),
            ],
        )
    });
    let style = Type {
        tracking: 0.08,
        caps: true,
        ..Type::barlow(13.0, 700)
    };
    let buttons: Vec<_> = BUTTONS
        .into_iter()
        .filter(|(a, _)| *a as u32 & EXTRAS == 0 || extras & a.bit() != 0)
        .map(|(action, label)| {
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                children![
                    (text(label, style, tokens::BUTTON_IDLE), Key(action)),
                    bar(action)
                ],
            )
        })
        .collect();
    (
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(14.0),
            padding: UiRect::top(Val::Px(10.0)),
            ..default()
        },
        children![
            (
                Node {
                    column_gap: Val::Px(gap),
                    ..default()
                },
                Children::spawn(SpawnIter(arrows.into_iter())),
            ),
            (
                Node {
                    flex_wrap: FlexWrap::Wrap,
                    row_gap: Val::Px(10.0),
                    column_gap: Val::Px(14.0),
                    ..default()
                },
                Children::spawn(SpawnIter(buttons.into_iter())),
            ),
        ],
    )
}

/// The 18x2 bar under a key.
fn bar(action: Action) -> impl Bundle {
    (
        Node {
            width: Val::Px(18.0),
            height: Val::Px(2.0),
            border_radius: BorderRadius::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::NONE),
        KeyBar(action),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_timer_splits_off_its_hundredths() {
        assert_eq!(timer_parts(0, 50), ("0:00".into(), ".00".into()));
        assert_eq!(
            timer_parts(50 * (42 * 60 + 10) + 17, 50),
            ("42:10".into(), ".34".into())
        );
        assert_eq!(timer_parts(50 * 3723, 50), ("1:02:03".into(), ".00".into()));
    }

    #[test]
    fn deaths_read_with_thousands_separators() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(212), "212");
        assert_eq!(thousands(1284), "1,284");
        assert_eq!(thousands(1_000_000), "1,000,000");
    }

    #[test]
    fn deaths_this_stage_start_again_on_a_new_stage_or_a_retry() {
        let mut s = StageDeaths::default();
        assert_eq!(s.observe(0, 0, 7), 0, "a run from a save starts at zero");
        assert_eq!(s.observe(10, 0, 9), 2);
        assert_eq!(s.observe(10, 0, 9), 2, "a frame without a tick");
        assert_eq!(s.observe(20, 1, 9), 0, "the next stage");
        assert_eq!(s.observe(30, 1, 10), 1);
        assert_eq!(s.observe(0, 0, 10), 0, "a retry");
        assert_eq!(s.observe(5, 0, 11), 1);
    }

    /// The demo kid presses suicide: deaths and deaths this stage go up, the input display sees
    /// the key, and Play original shows the Unranked pill.
    #[test]
    fn a_death_counts_in_the_panel_and_the_held_key_lights() {
        use bevy::ecs::system::RunSystemOnce;
        use iwbtg_sim::input::InputFrame;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo-clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut world = crate::view::test_world(&pkg);
        world.insert_resource(RunMods(ModSet::NONE));
        world.insert_resource(super::super::Shown::default());
        world.init_resource::<Paused>();
        world.init_resource::<Preview>();
        world.init_resource::<StageDeaths>();
        world.init_resource::<RunInfo>();
        world.init_resource::<Look>();
        world.run_system_once(read).unwrap();
        assert_eq!(world.resource::<RunInfo>().stages, 2, "one stage split");
        assert!(world.resource::<Look>().unranked);
        for _ in 0..100 {
            if world.resource::<RunInfo>().deaths > 0 {
                break;
            }
            let mut c = world.resource_mut::<Collector>();
            c.press(iwbtg_sim::input::vk::Q);
            let frame: InputFrame = c.take_frame();
            world.non_send_mut::<Game>().sim.step(frame);
            world.run_system_once(read).unwrap();
        }
        let info = world.resource::<RunInfo>().clone();
        assert_eq!((info.deaths, info.stage_deaths, info.stage), (1, 1, 1));
        assert_ne!(info.held & Action::Suicide.bit(), 0);
    }

    #[test]
    fn the_mod_row_shows_for_unranked_runs_and_mods_only() {
        let ranked = Look::default();
        assert!(!ranked.mod_row(), "a plain ranked run shows nothing there");
        let sd = Look {
            mods: ModSet::parse("SD").unwrap(),
            ..ranked
        };
        assert!(sd.mod_row());
        assert!(
            Look {
                unranked: true,
                ..ranked
            }
            .mod_row()
        );
        let off = Look {
            show: HudShow {
                mods: false,
                ..HudShow::default()
            },
            ..sd
        };
        assert!(!off.mod_row());
    }
}
