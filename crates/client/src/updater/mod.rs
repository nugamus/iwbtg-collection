//! The client updates itself from the rolling `latest` CI build (#1606;
//! docs/architecture/client/updates.md).
//!
//! On start, and every 30 minutes while on a menu, a worker thread reads the release with no
//! token, compares its `build.json` commit with the commit the running exe was compiled from
//! ([`BUILD_COMMIT`], set by CI), and downloads the zip in the background. The download is
//! checked against `sha256.txt` before the one `.exe` inside it is staged; a mismatch is deleted
//! and shown as an update notice. Nothing is installed until the player picks **Restart and
//! update** in the m5 B10 dialog, and nothing ever happens during a run.
//!
//! The swap itself ([`install`]) moves the running exe aside, copies the new one in, restarts,
//! and deletes the old exe on the next start. Saves, settings, stats and packages are never in
//! the plan. The browser build has no updater: it cannot replace its own page.

#[cfg(not(target_arch = "wasm32"))]
mod install;
#[cfg(not(target_arch = "wasm32"))]
mod net;
#[cfg(not(target_arch = "wasm32"))]
mod ui;

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use bevy::prelude::*;
use serde::Deserialize;

use crate::locale;
use crate::shell::AppScreen;
use crate::ui::notice;

/// How often a menu checks for a newer build.
pub const CHECK_INTERVAL: f32 = 30.0 * 60.0;
/// The commit CI compiled this build from (`option_env!("IWBTG_GIT_SHA")`, set by the latest
/// and release workflows). A build made on a developer's machine has none and never updates.
pub const BUILD_COMMIT: Option<&str> = option_env!("IWBTG_GIT_SHA");
/// When CI built it, ISO 8601 (`option_env!("IWBTG_BUILD_DATE")`).
pub const BUILD_DATE: Option<&str> = option_env!("IWBTG_BUILD_DATE");

/// One published build, as `build.json` names it.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct BuildInfo {
    /// The full git SHA the build was made from.
    pub commit: String,
    /// When it was built.
    #[serde(default)]
    pub date: String,
    /// The client's version at that commit.
    #[serde(default)]
    pub version: String,
    /// The titles of the PRs merged since the previous build, for "What's new".
    #[serde(default)]
    pub prs: Vec<String>,
}

impl BuildInfo {
    /// The short SHA the Settings row shows.
    pub fn short_commit(&self) -> &str {
        short(&self.commit)
    }
}

/// A verified, staged download waiting for the player's OK.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadyUpdate {
    pub build: BuildInfo,
    pub exe: PathBuf,
}

/// What the updater is doing, for the dialog and the Settings row.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Phase {
    #[default]
    Idle,
    /// A worker is asking GitHub.
    Checking,
    /// The build is newer; its zip is downloading.
    Downloading,
    /// A verified exe waits for **Restart and update**.
    Ready(ReadyUpdate),
    /// The last check failed with this message.
    Failed(String),
}

/// The updater's state.
#[derive(Resource, Debug, Default)]
pub struct Updater {
    pub phase: Phase,
    /// The last check's outcome, also when it was quiet (a network error): the Settings row shows
    /// it, a notice would be noise every half hour.
    pub last: Option<String>,
    /// A `--screenshot` check: no network, no install.
    pub preview: bool,
}

/// The dialog's state: dismissed this session, and whether "What's new" shows every PR title.
#[derive(Resource, Debug, Default)]
pub struct Dialog {
    pub dismissed: bool,
    pub expanded: bool,
}

/// What a worker thread sends back.
#[derive(Debug)]
enum Event {
    Downloading { build: BuildInfo },
    Ready { build: BuildInfo, exe: PathBuf },
    UpToDate,
    Error { message: String, notify: bool },
}

/// The running check and the timer for the next one. The receiver sits behind a `Mutex` because a
/// Bevy resource must be `Sync` and an `mpsc::Receiver` is not; only the main thread locks it.
#[derive(Resource, Default)]
struct Worker {
    rx: Mutex<Option<Receiver<Event>>>,
    busy: bool,
    elapsed: f32,
    checked: bool,
    force: bool,
}

/// The updater's command-line flags.
#[derive(clap::Args, Debug, Default)]
pub struct UpdaterArgs {
    /// Screenshot checks: draw the update dialog with sample data (no network, no install).
    #[arg(long, hide = true, value_enum, value_name = "SCREEN")]
    pub update_preview: Option<Preview>,
    /// Screenshot checks: draw the Settings → Updates row with sample data (no network).
    #[arg(long, hide = true)]
    pub updates_preview: bool,
}

/// What `--update-preview` draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Preview {
    /// The m5 B10 dialog over the main menu.
    Dialog,
}

/// `Version 0.1.0 is ready (493e28).`
fn ready_line(build: &BuildInfo) -> String {
    locale::format(
        "update_ready",
        &[("version", &build.version), ("commit", &build.short_commit())],
    )
}

/// What the Settings → Updates row says the updater last did.
pub fn status_text(updater: &Updater) -> String {
    match &updater.phase {
        Phase::Idle => updater
            .last
            .clone()
            .unwrap_or_else(|| locale::text("update_not_checked").to_owned()),
        Phase::Checking => locale::text("update_checking").to_owned(),
        Phase::Downloading => updater
            .last
            .clone()
            .unwrap_or_else(|| locale::text("update_checking").to_owned()),
        Phase::Ready(ready) => ready_line(&ready.build),
        Phase::Failed(message) => message.clone(),
    }
}

/// The running build, for the Settings row: `Running build 493e28 · 2026-11-26`, or that it was
/// compiled locally and never updates itself.
pub fn running_line() -> String {
    match BUILD_COMMIT {
        Some(commit) => {
            let date = BUILD_DATE.map_or("?", |d| d.get(..10).unwrap_or(d));
            locale::format("set_updates_build", &[("commit", &short(commit)), ("date", &date)])
        }
        None => locale::text("set_updates_local").to_owned(),
    }
}

/// The Settings → Updates row's button: with a verified build waiting it shows the dialog again,
/// otherwise it checks now (on a menu; never in a run).
#[cfg(not(target_arch = "wasm32"))]
pub fn press(world: &mut World) {
    if world
        .get_resource::<Updater>()
        .is_some_and(|u| matches!(u.phase, Phase::Ready(_)))
    {
        world.resource_mut::<Dialog>().dismissed = false;
    } else {
        check_now(world);
    }
}

/// Starts a check on the next menu frame (Check now, or a changed channel). A screenshot check
/// contacts nothing.
#[cfg(not(target_arch = "wasm32"))]
pub fn check_now(world: &mut World) {
    if world.get_resource::<Updater>().is_some_and(|u| !u.preview)
        && let Some(mut worker) = world.get_resource_mut::<Worker>()
    {
        worker.force = true;
    }
}

/// The browser has no updater: the rows are not shown there.
#[cfg(target_arch = "wasm32")]
pub fn press(_world: &mut World) {}
#[cfg(target_arch = "wasm32")]
pub fn check_now(_world: &mut World) {}

/// The toolbar's update button shows while a verified build waits.
#[cfg(not(target_arch = "wasm32"))]
fn toolbar_button(
    updater: Res<Updater>,
    mut slots: Query<&mut Node, With<crate::ui::toolbar::UpdateSlot>>,
) {
    let want = if matches!(updater.phase, Phase::Ready(_)) {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut slots {
        if node.display != want {
            node.display = want;
        }
    }
}

/// The open Settings overlay redraws when the updater's state changes, so its status is current.
#[cfg(not(target_arch = "wasm32"))]
fn follow(updater: Res<Updater>, overlay: Option<ResMut<crate::settings_screen::Overlay>>) {
    if updater.is_changed()
        && !updater.is_added()
        && let Some(mut o) = overlay
        && o.open
    {
        o.refresh();
    }
}

/// The ready notification's row in the bell's list shows the dialog again (also after Later).
#[cfg(not(target_arch = "wasm32"))]
fn on_opened(opened: On<notice::Opened>, mut dialog: ResMut<Dialog>) {
    if opened.kind == notice::Kind::UpdateReady {
        dialog.dismissed = false;
    }
}

/// The updater's place in the module list (`main.rs`).
#[cfg(not(target_arch = "wasm32"))]
pub struct Registration;

#[cfg(not(target_arch = "wasm32"))]
impl crate::registration::ClientModule for Registration {
    type Args = UpdaterArgs;

    fn prepare(_args: &mut UpdaterArgs, startup: &crate::registration::Startup) -> Result<(), String> {
        // A `--screenshot` check leaves the machine as it found it, including the old exe.
        if !startup.isolated {
            install::remove_old_on_start();
            net::cleanup();
        }
        Ok(())
    }

    fn add(app: &mut App, args: UpdaterArgs, _startup: &crate::registration::Startup) {
        app.add_plugins(UpdaterPlugin { args });
    }
}

/// The browser has no updater (it cannot replace its own page); its module line still resolves.
#[cfg(target_arch = "wasm32")]
pub struct Registration;

#[cfg(target_arch = "wasm32")]
impl crate::registration::ClientModule for Registration {
    type Args = crate::registration::NoArgs;
}

#[cfg(not(target_arch = "wasm32"))]
pub struct UpdaterPlugin {
    args: UpdaterArgs,
}

#[cfg(not(target_arch = "wasm32"))]
impl Plugin for UpdaterPlugin {
    fn build(&self, app: &mut App) {
        let preview = self.args.update_preview.is_some() || self.args.updates_preview;
        app.init_resource::<Updater>()
            .init_resource::<Dialog>()
            .init_resource::<Worker>();
        if self.args.updates_preview {
            // The Settings overlay opens on its General section, where the Updates rows are.
            app.add_systems(Startup, |overlay: Option<ResMut<crate::settings_screen::Overlay>>| {
                if let Some(mut o) = overlay {
                    o.show();
                }
            });
        }
        if self.args.update_preview.is_some() {
            let sample = sample_update();
            notice::post(
                notice::Kind::UpdateReady,
                &locale::format(
                    "update_ready_notice",
                    &[("version", &sample.build.version), ("commit", &sample.build.short_commit())],
                ),
            );
            app.insert_resource(Updater {
                phase: Phase::Ready(sample_update()),
                preview: true,
                ..default()
            });
        } else if preview {
            app.insert_resource(Updater {
                phase: Phase::Idle,
                last: Some("Up to date".to_string()),
                preview: true,
            });
        }
        app.add_systems(Update, (tick, prompt, hide_in_run, toolbar_button, follow).chain())
            .add_observer(ui::on_activate)
            .add_observer(on_opened);
    }
}

/// A sample build for the screenshot checks, as the dialog draws it.
#[cfg(not(target_arch = "wasm32"))]
fn sample_update() -> ReadyUpdate {
    ReadyUpdate {
        build: BuildInfo {
            commit: "493e284f0a1b2c3d4e5f60718293a4b5c6d7e8f9".to_string(),
            date: "2026-11-26T15:00:00Z".to_string(),
            version: "0.1.0".to_string(),
            prs: vec![
                "Add the Classic shelf".to_string(),
                "Fix replays of skipped cutscenes".to_string(),
                "Show the mode accent on the toolbar".to_string(),
                "Draw every game's icon in game select".to_string(),
            ],
        },
        exe: PathBuf::new(),
    }
}

/// One frame of the updater: collect a finished worker, then start a check when the menu's
/// timer says so.
#[cfg(not(target_arch = "wasm32"))]
fn tick(
    time: Res<Time<Real>>,
    mut worker: ResMut<Worker>,
    mut updater: ResMut<Updater>,
    mut dialog: ResMut<Dialog>,
    screen: Option<Res<State<AppScreen>>>,
) {
    let pending = worker
        .rx
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    if let Some(rx) = pending {
        let mut finished = false;
        loop {
            match rx.try_recv() {
                Ok(event) => finished |= apply(event, &mut updater, &mut dialog),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            worker.busy = false;
        } else {
            *worker
                .rx
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(rx);
        }
    }
    if updater.preview
        || worker.busy
        // A verified download waits for the player: a new check would replace the staged exe.
        || matches!(updater.phase, Phase::Ready(_))
        || !on_menu(screen.as_deref())
    {
        return;
    }
    worker.elapsed += time.delta_secs();
    if worker.checked && !worker.force && worker.elapsed < CHECK_INTERVAL {
        return;
    }
    worker.checked = true;
    worker.force = false;
    worker.elapsed = 0.0;
    worker.busy = true;
    updater.phase = Phase::Checking;
    let (tx, rx) = mpsc::channel();
    *worker
        .rx
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(rx);
    let channel = crate::settings::get().update_channel;
    let running = BUILD_COMMIT.map(str::to_string);
    std::thread::spawn(move || net::run(channel, running, tx));
}

/// Applies one worker event; `true` means the worker is done.
#[cfg(not(target_arch = "wasm32"))]
fn apply(event: Event, updater: &mut Updater, dialog: &mut Dialog) -> bool {
    match event {
        Event::Downloading { build } => {
            updater.last = Some(locale::format(
                "update_downloading",
                &[("commit", &short(&build.commit))],
            ));
            updater.phase = Phase::Downloading;
            false
        }
        Event::Ready { build, exe } => {
            updater.last = Some(ready_line(&build));
            // The bell keeps it, so a Later or a run in between does not lose the update.
            notice::post(
                notice::Kind::UpdateReady,
                &locale::format(
                    "update_ready_notice",
                    &[("version", &build.version), ("commit", &build.short_commit())],
                ),
            );
            updater.phase = Phase::Ready(ReadyUpdate { build, exe });
            // A newer build prompts again even after an earlier Later.
            dialog.dismissed = false;
            true
        }
        Event::UpToDate => {
            net::cleanup();
            updater.last = Some(locale::text("update_up_to_date").to_owned());
            updater.phase = Phase::Idle;
            true
        }
        Event::Error { message, notify } => {
            if notify {
                notice::post(notice::Kind::Update, &message);
                updater.phase = Phase::Failed(message.clone());
            } else {
                updater.phase = Phase::Idle;
            }
            updater.last = Some(message);
            true
        }
    }
}

/// A check only runs on a menu (`AppScreen::Menu` or the select list), never in a run.
#[cfg(not(target_arch = "wasm32"))]
fn on_menu(screen: Option<&State<AppScreen>>) -> bool {
    screen.is_some_and(|s| *s.get() != AppScreen::Run)
}

/// Shows the m5 B10 dialog when a verified build waits and the player is on a menu.
#[cfg(not(target_arch = "wasm32"))]
fn prompt(
    mut commands: Commands,
    updater: Res<Updater>,
    dialog: Res<Dialog>,
    screen: Option<Res<State<AppScreen>>>,
    existing: Query<(), With<ui::UpdateDialog>>,
) {
    if dialog.dismissed || existing.iter().next().is_some() || !on_menu(screen.as_deref()) {
        return;
    }
    if let Phase::Ready(ready) = &updater.phase {
        ui::spawn_dialog(&mut commands, ready, dialog.expanded, updater.preview);
    }
}

/// The prompt never covers a run: leaving the menu takes it away (it comes back on the menu).
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::type_complexity)]
fn hide_in_run(
    mut commands: Commands,
    screen: Option<Res<State<AppScreen>>>,
    shown: Query<Entity, With<ui::UpdateDialog>>,
) {
    if screen.as_deref().is_some_and(|s| *s.get() == AppScreen::Run) {
        for entity in &shown {
            commands.entity(entity).despawn();
        }
    }
}

/// The first six characters of a SHA, for a compact label; the whole string if it is short.
pub fn short(commit: &str) -> &str {
    // A SHA is ASCII, so byte slicing is safe; take at most six characters.
    commit.get(..6).filter(|s| s.chars().all(|c| c.is_ascii_hexdigit())).unwrap_or(commit)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn a_build_manifest_reads_every_field() {
        let build: BuildInfo = serde_json::from_str(
            r#"{"commit":"493e284f","date":"2026-11-26T15:00:00Z","version":"0.1.0","prs":["A","B"]}"#,
        )
        .unwrap();
        assert_eq!(build.short_commit(), "493e28");
        assert_eq!(build.date, "2026-11-26T15:00:00Z");
        assert_eq!(build.prs, ["A", "B"]);
        // An older manifest without the optional fields still reads.
        let plain: BuildInfo = serde_json::from_str(r#"{"commit":"abcdef"}"#).unwrap();
        assert!(plain.prs.is_empty() && plain.date.is_empty());
    }

    #[test]
    fn a_short_commit_is_six_characters() {
        assert_eq!(short("493e284f0a"), "493e28");
        assert_eq!(short("abc"), "abc");
        assert_eq!(short(""), "");
    }
}
