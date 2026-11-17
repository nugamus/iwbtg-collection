//! The script error screen (#101; docs/format/README.md, principle 3;
//! docs/architecture/client/script_error.md): a script error stops that game, never the client.
//!
//! When the sim halts (`SimStatus::Halted`: a script error, or a case the sim refuses to guess)
//! the run goes to `RunScreen::Error` (shell, #839): the sim does not tick, `OnEnter` hides the
//! game and the HUD and gives the backdrop the loading dim. The card says what stopped, where
//! (the script file and line) and why, from [`Report::new`]. Back (Enter, Escape or Start) goes to
//! game select, or quits when the client was started on one package (`shell::leave`); Retry (or
//! the quick-retry key) starts the run again.
//!
//! No artboard draws this screen: it is the loading card's look (`loading/page.rs`) built from
//! the #490 kit, a default to revisit (#101).

mod page;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;
use bevy::window::PrimaryWindow;
use iwbtg_format::manifest::Manifest;
use iwbtg_sim::error::SimError;
use iwbtg_sim::world::SimStatus;

use crate::art::ArtFiles;
use crate::hud::Suspended;
use crate::shell::{self, InRun, RUN_ONLY, RunScreen};
use crate::ui::backdrop::{Backdrop, dim};
use crate::view::{Game, GameCamera};

/// The page's design height: it is laid out at 1080 px and scaled to the window, as the loading
/// card is.
const DESIGN_H: f32 = 1080.0;
/// The narrowest the page gets before it scales by width instead (the card and some air).
const MIN_DESIGN_WIDTH: f32 = 1000.0;

/// The screen's command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug, Clone, Copy, Default)]
pub struct ScriptErrorArgs {
    /// Screenshot checks: open the script error screen at once with a sample error.
    #[arg(long, hide = true)]
    pub script_error_preview: bool,
}

/// What stopped the game, ready to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// `Script error`, or `The game stopped` for a halt that is not a script's.
    pub heading: &'static str,
    /// The script file in the package (`scripts/boss.rhai`).
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    /// What went wrong, without the file and position.
    pub message: String,
    /// Where in the game: `kind @boss · instance 100089 · Alarm(0) · tick 24`.
    pub context: Option<String>,
}

impl Report {
    /// The report for a halt. A script error's message is the script host's:
    /// `<file>: <what> (line L, position P) [kind K, instance I, <event>, tick T]`; the file, the
    /// position and the bracket come out of it when they are there.
    pub fn new(error: &SimError) -> Report {
        let SimError::Script { message } = error else {
            return Report {
                heading: "The game stopped",
                file: None,
                line: None,
                column: None,
                message: error.to_string(),
                context: None,
            };
        };
        let mut rest = message.trim();
        let mut context = None;
        if rest.ends_with(']')
            && let Some(open) = rest.rfind(" [")
        {
            context = Some(rest[open + 2..rest.len() - 1].replace(", ", " · "));
            rest = rest[..open].trim_end();
        }
        let mut file = None;
        if let Some((name, what)) = rest.split_once(": ")
            && name.starts_with("scripts/")
            && !name.contains(' ')
        {
            file = Some(name.to_owned());
            rest = what;
        }
        let (text, line, column) = match position(rest) {
            Some((start, end, line, column)) => {
                let text = format!("{}{}", rest[..start].trim_end(), &rest[end..]);
                (text, Some(line), Some(column))
            }
            None => (rest.to_owned(), None, None),
        };
        Report {
            heading: "Script error",
            file,
            line,
            column,
            message: text.trim().to_owned(),
            context,
        }
    }

    /// `scripts/boss.rhai · line 9, column 43`, or what of it is known.
    pub fn place(&self) -> Option<String> {
        let at = match (self.line, self.column) {
            (Some(l), Some(c)) => Some(format!("line {l}, column {c}")),
            (Some(l), None) => Some(format!("line {l}")),
            _ => None,
        };
        match (&self.file, at) {
            (Some(f), Some(at)) => Some(format!("{f} · {at}")),
            (Some(f), None) => Some(f.clone()),
            (None, Some(at)) => Some(at),
            (None, None) => None,
        }
    }

    /// The sample `--script-error-preview` shows: the `script_error.pkg` fixture's error.
    pub fn sample() -> Report {
        Report::new(&SimError::Script {
            message: "scripts/boss.rhai: Array index 3 out of bounds: only 2 elements in array \
                      (line 9, position 43) [kind @boss, instance 100089, Alarm(0), tick 24]"
                .to_owned(),
        })
    }
}

/// The first `(line L, position P)` in `text`: its byte range, the line and the column.
fn position(text: &str) -> Option<(usize, usize, u32, u32)> {
    let start = text.find("(line ")?;
    let end = start + text[start..].find(')')? + 1;
    let inner = &text[start + "(line ".len()..end - 1];
    let (line, column) = inner.split_once(", position ")?;
    Some((start, end, line.trim().parse().ok()?, column.trim().parse().ok()?))
}

/// The screen's state: the report shows in `RunScreen::Error`.
#[derive(Resource, Default)]
pub struct Stopped {
    report: Option<Report>,
    /// `--script-error-preview`: open with the sample on the first frame of play.
    preview: bool,
    picked: Option<Action>,
}

impl Stopped {
    /// What the screen shows, while it shows.
    #[cfg(test)]
    pub fn report(&self) -> Option<&Report> {
        self.report.as_ref()
    }
}

/// A button on the card.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Game select, or quit without one.
    Back,
    Retry,
}

/// A retry (`retry.rs`) starts the run again: the screen closes.
pub(crate) fn reset(world: &mut World) {
    if let Some(mut s) = world.get_resource_mut::<Stopped>() {
        s.report = None;
        s.picked = None;
    }
    if shell::run_screen(world) == Some(RunScreen::Error) {
        shell::go(world, RunScreen::Playing);
    }
}

pub struct ScriptErrorPlugin(pub ScriptErrorArgs);

impl Plugin for ScriptErrorPlugin {
    fn build(&self, app: &mut App) {
        let preview = self.0.script_error_preview;
        shell::run_resource(app, move |_| Stopped {
            preview,
            ..default()
        });
        let on_screen = in_state(RunScreen::Error);
        app.add_systems(OnEnter(RunScreen::Error), cover)
            .add_systems(OnExit(RunScreen::Error), uncover)
            .add_observer(on_activate)
            .add_systems(
                Update,
                (
                    open.run_if(in_state(RunScreen::Playing)),
                    keys.run_if(on_screen.clone()),
                    act.run_if(on_screen.clone()),
                    show.run_if(on_screen),
                )
                    .chain()
                    .after(crate::screen::apply)
                    .in_set(InRun),
            );
    }
}

/// A halted sim (or the preview) opens the screen.
fn open(game: NonSend<Game>, mut stopped: ResMut<Stopped>, mut next: ResMut<NextState<RunScreen>>) {
    if stopped.report.is_some() {
        return;
    }
    let report = if stopped.preview {
        stopped.preview = false;
        Report::sample()
    } else if let SimStatus::Halted(e) = &game.sim.world().status {
        eprintln!("the game stopped: {e}");
        Report::new(e)
    } else {
        return;
    };
    stopped.report = Some(report);
    next.set(RunScreen::Error);
}

/// A click (or Enter on a focused button) picks its action.
fn on_activate(activate: On<Activate>, actions: Query<&Action>, mut stopped: ResMut<Stopped>) {
    if let Ok(action) = actions.get(activate.entity)
        && stopped.report.is_some()
    {
        stopped.picked = Some(*action);
    }
}

/// Escape, Enter or Start goes back. Enter on a focused button is that button's (`Activate`).
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    collector: Option<Res<crate::input::Collector>>,
    focus: Option<Res<bevy::input_focus::InputFocus>>,
    mut stopped: ResMut<Stopped>,
    mut pad_before: Local<bool>,
) {
    let pad = collector.is_some_and(|c| c.pad_down(crate::pause::PAUSE_PAD));
    let pad_pressed = pad && !*pad_before;
    *pad_before = pad;
    let focused = focus.is_some_and(|f| f.get().is_some());
    let enter = keys.just_pressed(KeyCode::Enter) && !focused;
    if keys.just_pressed(KeyCode::Escape) || enter || pad_pressed {
        stopped.picked = Some(Action::Back);
    }
}

/// Does what was picked.
fn act(world: &mut World) {
    let Some(action) = world.resource_mut::<Stopped>().picked.take() else {
        return;
    };
    match action {
        Action::Back => shell::leave(world),
        Action::Retry => crate::retry::request(world),
    }
}

/// The page's root.
#[derive(Component)]
struct Root {
    art: Option<Handle<Image>>,
}

/// The camera the page draws on: UI only, over everything.
#[derive(Component)]
struct ErrorCamera;

/// Entering: the game camera off, the HUD suspended, the backdrop at the loading dim.
fn cover(
    mut game_cameras: Query<&mut Camera, With<GameCamera>>,
    suspended: Option<ResMut<Suspended>>,
    backdrop: Option<ResMut<Backdrop>>,
) {
    shell::cover_game(true, Some(dim::LOADING), &mut game_cameras, suspended, backdrop);
}

/// Leaving (a retry): the page and its camera go; the game and the HUD come back.
fn uncover(
    mut commands: Commands,
    roots: Query<Entity, With<Root>>,
    cameras: Query<Entity, With<ErrorCamera>>,
    mut game_cameras: Query<&mut Camera, With<GameCamera>>,
    suspended: Option<ResMut<Suspended>>,
    backdrop: Option<ResMut<Backdrop>>,
) {
    for entity in roots.iter().chain(&cameras) {
        commands.entity(entity).despawn();
    }
    shell::cover_game(false, None, &mut game_cameras, suspended, backdrop);
}

/// Draws the page (again when the key art arrives) at the window's UI scale.
#[allow(clippy::too_many_arguments, reason = "one pass over what the page covers")]
fn show(
    mut commands: Commands,
    stopped: Res<Stopped>,
    files: Option<Res<ArtFiles>>,
    game: NonSend<Game>,
    shell: Option<Res<shell::Shell>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut ui_scale: ResMut<UiScale>,
    mut roots: Query<(Entity, &Root, &mut Node)>,
    cameras: Query<Entity, With<ErrorCamera>>,
    backdrop: Res<Backdrop>,
) {
    let Some(report) = stopped.report.as_ref() else {
        return;
    };
    let win = window.size();
    if win.x < 1.0 || win.y < 1.0 {
        return;
    }
    let scale = (win.y / DESIGN_H).min(win.x / MIN_DESIGN_WIDTH);
    if (ui_scale.0 - scale).abs() > 1e-4 {
        ui_scale.0 = scale;
    }
    let (w, h) = (Val::Px(win.x / scale), Val::Px(win.y / scale));
    let mut current = false;
    for (entity, root, mut node) in &mut roots {
        if root.art == backdrop.art {
            current = true;
            if (node.width, node.height) != (w, h) {
                (node.width, node.height) = (w, h);
            }
        } else {
            commands.entity(entity).despawn();
        }
    }
    if current {
        return;
    }
    let camera = match cameras.iter().next() {
        Some(c) => c,
        None => commands
            .spawn((
                Camera2d,
                Camera {
                    order: 20,
                    clear_color: ClearColorConfig::None,
                    ..default()
                },
                RenderLayers::none(),
                ErrorCamera,
                RUN_ONLY,
            ))
            .id(),
    };
    let manifest = files
        .as_ref()
        .and_then(|f| f.0.get("game.toml"))
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|t| Manifest::parse(t).ok());
    let id = &game.sim.def().id;
    let look = page::Look {
        title: manifest
            .as_ref()
            .map(|m| m.title.get_ref().clone())
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| id.clone()),
        creator: manifest.as_ref().and_then(crate::loading::authors),
        art: backdrop.art.clone(),
        back: shell.is_some_and(|s| s.goes_back()),
    };
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: w,
            height: h,
            ..default()
        },
        Root {
            art: backdrop.art.clone(),
        },
        UiTargetCamera(camera),
        RUN_ONLY,
        children![page::page(report, &look)],
    ));
}

#[cfg(test)]
mod tests;
