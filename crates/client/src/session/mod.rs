//! Classic's session summary (m5 `A8_Session`, `A9_SessionFinished`; `Session.dc.html`, #1624):
//! leaving a Classic game (Back to the shelf in the pause menu) or finishing it shows what the
//! session did instead of the Time Attack results: no placement, no splits. It is the run screen
//! `RunScreen::Session` (shell, #839): the sim does not tick, the game camera is off, the HUD is
//! suspended and the backdrop takes the results' dim, as on the results page.
//!
//! The numbers come from the open session (`stats::SessionLog`, #1623) and the book's Classic
//! numbers (`Book::game_mode`, #1619). Back to the shelf (Escape) leaves the run
//! (`shell::leave`); the primary button is Continue playing (back to the game, `Playing`) or,
//! after the ending, Play again (a new run, `retry::request`).

mod page;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;
use bevy::window::PrimaryWindow;
use iwbtg_sim::stripped::PlayMode;

use crate::hud::Suspended;
use crate::ui::backdrop::{Backdrop, dim};
use crate::shell::{self, InRun, RUN_ONLY, RunScreen};
use crate::stats::Stats;
use crate::view::{Game, GameCamera};

/// The summary's buttons.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Back,
    Primary,
}

/// `--session-preview`: open the page at once, as after leaving (`playing`) or the ending
/// (`finished`), for screenshot checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Preview {
    Playing,
    Finished,
}

/// The session summary's command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug, Clone, Copy, Default)]
pub struct SessionArgs {
    /// Screenshot checks: open the Classic session summary at sim tick 25.
    #[arg(long, hide = true, value_name = "STATE")]
    pub session_preview: Option<Preview>,
}

/// The module's place in the module list (main.rs).
pub struct Registration;

impl crate::registration::ClientModule for Registration {
    type Args = SessionArgs;

    fn add(
        app: &mut bevy::prelude::App,
        args: SessionArgs,
        _startup: &crate::registration::Startup,
    ) {
        app.add_plugins(SessionPlugin { args });
    }
}

/// The page's state.
#[derive(Resource, Debug, Default)]
pub struct Session {
    /// The game was finished in this session (A9).
    pub finished: bool,
    picked: Option<Action>,
    /// `--session-preview`, waiting for its tick.
    preview: Option<Preview>,
    /// The clear the page opened for, so one clear opens it once.
    seen: Option<u64>,
    /// When the page opened, seconds since 1970 (the header's end time).
    pub opened_at: u64,
}

/// The tick `--session-preview` opens at.
const PREVIEW_TICK: u64 = 25;

pub struct SessionPlugin {
    pub args: SessionArgs,
}

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        let preview = self.args.session_preview;
        shell::run_resource(app, move |_| Session {
            preview,
            ..default()
        });
        app.add_observer(on_activate)
            .add_systems(OnEnter(RunScreen::Session), cover)
            .add_systems(OnExit(RunScreen::Session), uncover)
            .add_systems(
                Update,
                (
                    open_on_finish.run_if(in_state(RunScreen::Playing)),
                    (keys, act, show)
                        .chain()
                        .run_if(in_state(RunScreen::Session)),
                )
                    .chain()
                    .after(crate::screen::apply)
                    .in_set(InRun),
            );
    }
}

/// Opens the summary from the pause menu (Back to the shelf in Classic).
pub fn open(world: &mut World, finished: bool) {
    classic_accent(world);
    if let Some(mut s) = world.get_resource_mut::<Session>() {
        s.finished = finished;
        s.picked = None;
        s.opened_at = crate::stats::now_seconds();
    }
    shell::go(world, RunScreen::Session);
}

/// The page's primary button takes the mode's accent: a Classic run is Classic's.
fn classic_accent(world: &mut World) {
    if let Some(mut mode) = world.get_resource_mut::<crate::ui::mode::Mode>()
        && *mode != crate::ui::mode::Mode::Classic
    {
        *mode = crate::ui::mode::Mode::Classic;
    }
}

/// A Classic run's ending (its clear) opens the finished summary instead of the results (A9);
/// `--session-preview` opens it at its tick.
fn open_on_finish(
    game: NonSend<Game>,
    stats: Option<Res<Stats>>,
    mut session: ResMut<Session>,
    mut next: ResMut<NextState<RunScreen>>,
) {
    if let Some(p) = session.preview
        && game.sim.world().tick >= PREVIEW_TICK
    {
        session.preview = None;
        session.finished = p == Preview::Finished;
        session.opened_at = crate::stats::now_seconds();
        next.set(RunScreen::Session);
        return;
    }
    if game.sim.options().mode != PlayMode::Original {
        return;
    }
    let Some(stats) = stats else {
        return;
    };
    let finished = stats.open_session().is_some_and(|s| s.finished);
    let tick = game.sim.world().tick;
    if finished && session.seen.is_none() {
        session.seen = Some(tick);
        session.finished = true;
        session.opened_at = crate::stats::now_seconds();
        next.set(RunScreen::Session);
    }
}

/// Escape goes back to the shelf; Enter is the primary button.
fn keys(keys: Res<ButtonInput<KeyCode>>, mut session: ResMut<Session>) {
    if keys.just_pressed(KeyCode::Escape) {
        session.picked = Some(Action::Back);
    } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
        session.picked = Some(Action::Primary);
    }
}

/// A click picks its button.
fn on_activate(activate: On<Activate>, actions: Query<&Action>, session: Option<ResMut<Session>>) {
    if let Some(mut session) = session
        && let Ok(action) = actions.get(activate.entity)
    {
        session.picked = Some(*action);
    }
}

/// Does what was picked; keys pressed on the page never reach the game.
fn act(world: &mut World) {
    let Some((action, finished)) = world
        .get_resource_mut::<Session>()
        .and_then(|mut s| s.picked.take().map(|a| (a, s.finished)))
    else {
        return;
    };
    if let Some(mut c) = world.get_resource_mut::<crate::input::Collector>() {
        c.take_frame();
    }
    match (action, finished) {
        (Action::Back, _) => {
            eprintln!("session: back to the shelf");
            shell::leave(world);
        }
        (Action::Primary, false) => shell::go(world, RunScreen::Playing),
        (Action::Primary, true) => crate::retry::request(world),
    }
}

/// The page's root.
#[derive(Component)]
struct Root {
    finished: bool,
}

/// The camera the page draws on: UI only, over everything.
#[derive(Component)]
struct SessionCamera;

/// Entering the page: the game camera off, the HUD suspended, the backdrop at the results dim.
fn cover(
    mode: Option<ResMut<crate::ui::mode::Mode>>,
    mut game_cameras: Query<&mut Camera, With<GameCamera>>,
    suspended: Option<ResMut<Suspended>>,
    backdrop: Option<ResMut<Backdrop>>,
) {
    if let Some(mut mode) = mode
        && *mode != crate::ui::mode::Mode::Classic
    {
        *mode = crate::ui::mode::Mode::Classic;
    }
    shell::cover_game(true, Some(dim::RESULTS), &mut game_cameras, suspended, backdrop);
}

/// Leaving the page: it and its camera go, the game and the HUD come back.
fn uncover(
    mut commands: Commands,
    roots: Query<Entity, With<Root>>,
    cameras: Query<Entity, With<SessionCamera>>,
    mut game_cameras: Query<&mut Camera, With<GameCamera>>,
    suspended: Option<ResMut<Suspended>>,
    backdrop: Option<ResMut<Backdrop>>,
) {
    for entity in roots.iter().chain(&cameras) {
        commands.entity(entity).despawn();
    }
    shell::cover_game(false, None, &mut game_cameras, suspended, backdrop);
}

/// Draws the page at the window's UI scale (laid out at 1920 x 1080, scaled to fit).
fn show(world: &mut World) {
    let Some(win) = world
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .iter(world)
        .next()
        .map(Window::size)
        .filter(|w| w.x >= 1.0 && w.y >= 1.0)
    else {
        return;
    };
    let scale = (win.x / page::DESIGN.x).min(win.y / page::DESIGN.y);
    if (world.resource::<UiScale>().0 - scale).abs() > 1e-4 {
        world.resource_mut::<UiScale>().0 = scale;
    }
    let finished = world.resource::<Session>().finished;
    let roots: Vec<(Entity, bool)> = world
        .query::<(Entity, &Root)>()
        .iter(world)
        .map(|(e, r)| (e, r.finished))
        .collect();
    let (w, h) = (win.x / scale, win.y / scale);
    let mut current = false;
    for (e, f) in roots {
        if f == finished {
            current = true;
            if let Some(mut node) = world.get_mut::<Node>(e)
                && (node.width, node.height) != (Val::Px(w), Val::Px(h))
            {
                (node.width, node.height) = (Val::Px(w), Val::Px(h));
            }
        } else {
            world.entity_mut(e).despawn();
        }
    }
    if current {
        return;
    }
    if world
        .query_filtered::<(), With<SessionCamera>>()
        .iter(world)
        .next()
        .is_none()
    {
        world.spawn((
            Camera2d,
            Camera {
                order: 20,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            RenderLayers::none(),
            SessionCamera,
            RUN_ONLY,
        ));
    }
    let summary = page::Summary::read(world, finished);
    let fruit = world
        .get_resource::<crate::pause::Fruit>()
        .map(|f| f.0.clone());
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Px(w),
            height: Val::Px(h),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        Root { finished },
        RUN_ONLY,
        children![page::page(&summary, fruit)],
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The summary reads the open session: its time, deaths and rooms, the deaths by room most
    /// first, and the game's Classic totals.
    #[test]
    fn the_summary_reads_the_session() {
        let log = crate::stats::SessionLog {
            game: "g".into(),
            map: "g/main".into(),
            ranked: false,
            tick_rate: 50,
            ticks: 500,
            seconds: 10.0,
            deaths: 3,
            death_log: vec![
                crate::stats::SessionDeath {
                    tick: 100,
                    room: Some("a".into()),
                },
                crate::stats::SessionDeath {
                    tick: 200,
                    room: Some("b".into()),
                },
                crate::stats::SessionDeath {
                    tick: 300,
                    room: Some("b".into()),
                },
            ],
            rooms: vec![
                crate::stats::RoomVisit {
                    tick: 0,
                    room: "a".into(),
                    new: false,
                },
                crate::stats::RoomVisit {
                    tick: 150,
                    room: "b".into(),
                    new: true,
                },
            ],
            saves: vec![crate::stats::SaveTouch {
                tick: 250,
                room: "b".into(),
            }],
            finished: false,
            started_at: 1_000,
        };
        let s = page::Summary::from_log(&log, &crate::stats::GameMode::default(), "Game".into(), false, 2_000);
        assert_eq!(s.hero, "0:00:10");
        assert_eq!(s.rooms[0], ("b".to_owned(), 2));
        assert_eq!(s.death_marks, vec![0.2, 0.4, 0.6]);
        assert_eq!(s.saves, vec![(0.5, "b".to_owned())]);
        assert!(s.facts.iter().any(|f| f.1 == "2" && f.2.contains("1 new")), "{:?}", s.facts);
    }
}
