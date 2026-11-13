//! Audio lock: a browser plays no sound until the page has had a key press, click or touch
//! (docs/architecture/client/audio.md, Web audio unlock). Until then the audio module holds every
//! looping tune that starts, with no sound voice yet, and drops one-shot sounds; when the lock
//! opens, each held tune starts where the tick clock says it is (`MusicSync`), so the first
//! room's music comes in on the beat. A note in the corner says how to turn the sound on.
//!
//! The browser side is `web/index.html`: it keeps every `AudioContext` the page makes, resumes
//! them on the first gesture and sets `window.iwbtgAudioUnlocked` once one runs. Natively there
//! is nothing to unlock; the hidden `--audio-locked` flag acts like a fresh browser page for
//! tests (any key or mouse button opens it).

use std::time::Duration;

use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings};
use bevy::prelude::*;
use bevy::text::LineBreak;

use super::MusicSync;
use crate::ui::fonts::Type;
use crate::ui::motion::{Ease, OffsetX, Opacity, Tween};
use crate::ui::notice;
use crate::ui::shape::{Band, Shape, Side, UiOpacity};
use crate::ui::tokens;
use crate::ui::widgets::text;
use crate::view::Game;

/// The global `web/index.html` sets to `true` once the browser runs the page's audio.
#[cfg_attr(
    not(all(target_arch = "wasm32", target_os = "unknown")),
    allow(dead_code, reason = "read by the browser build and a test")
)]
pub const BROWSER_FLAG: &str = "iwbtgAudioUnlocked";

/// The note's heading and text. A controller press is no user gesture to a browser, so the
/// note names only keys and clicks.
pub const HEADING: &str = "Sound off";
pub const BODY: &str = "Press a key or click to turn the sound on.";

/// Whether the audio is still locked.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lock {
    locked: bool,
}

impl Lock {
    pub fn new(locked: bool) -> Lock {
        Lock { locked }
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }
}

/// A looping tune that started while the audio was locked: its sound, waiting for the lock to
/// open. Its `PlaybackSettings` keep the volume, the game's pitch (the clock speed is applied on
/// release) and whether it is paused.
#[derive(Component)]
pub struct Held(pub Handle<AudioSource>);

/// What becomes of a voice that starts now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// Played at once.
    Play,
    /// Kept without sound until the lock opens (a looping tune).
    Hold,
    /// Not played at all: a one-shot sound has passed by the time the lock opens.
    Drop,
}

/// A voice starting under `lock` (`None`: no lock at all).
pub fn start(lock: Option<&Lock>, looping: bool) -> Start {
    match (lock.is_some_and(Lock::is_locked), looping) {
        (false, _) => Start::Play,
        (true, true) => Start::Hold,
        (true, false) => Start::Drop,
    }
}

/// Adds the lock (open unless `locked`) and, when locked, the systems that open it and the note.
pub fn plugin(app: &mut App, locked: bool) {
    app.insert_resource(Lock::new(locked));
    if !locked {
        return;
    }
    if !app.is_plugin_added::<crate::ui::UiKitPlugin>() {
        app.add_plugins(crate::ui::UiKitPlugin);
    }
    app.add_systems(Startup, spawn_note).add_systems(
        Update,
        (
            poll.before(super::play),
            release.after(poll).before(super::play),
            (place_note, hide_note).after(poll),
        ),
    );
}

/// Opens the lock once the browser runs the page's audio (natively, under `--audio-locked`, on
/// the first key or mouse button).
fn poll(
    mut lock: ResMut<Lock>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
) {
    if !lock.locked {
        return;
    }
    let opened = if cfg!(all(target_arch = "wasm32", target_os = "unknown")) {
        browser_unlocked()
    } else {
        keys.is_some_and(|k| k.get_just_pressed().next().is_some())
            || mouse.is_some_and(|m| m.get_just_pressed().next().is_some())
    };
    if opened {
        lock.locked = false;
    }
}

/// Whether the page's audio runs: `window.iwbtgAudioUnlocked`. A page without the shim
/// (the global unset) is taken as unlocked, as before the lock existed.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn browser_unlocked() -> bool {
    let Some(window) = web_sys::window() else {
        return true;
    };
    match js_sys::Reflect::get(&window, &js_sys::JsString::from(BROWSER_FLAG)) {
        Ok(v) if v.is_undefined() => true,
        Ok(v) => v.as_bool().unwrap_or(true),
        Err(_) => true,
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn browser_unlocked() -> bool {
    true
}

/// A held tune as `start_held` releases it.
pub type HeldParts = (
    Entity,
    &'static Held,
    &'static mut PlaybackSettings,
    Option<&'static mut MusicSync>,
);

/// Starts the held tunes when the lock has opened.
fn release(
    mut commands: Commands,
    lock: Res<Lock>,
    game: NonSend<Game>,
    fixed: Res<Time<Fixed>>,
    clock: Option<Res<crate::view::ClockSpeed>>,
    mut held: Query<HeldParts>,
) {
    if lock.locked || held.is_empty() {
        return;
    }
    let tick_rate = f64::from(game.sim.def().tick_rate.max(1));
    let now_tick = game.sim.world().tick as f64 + f64::from(fixed.overstep_fraction());
    let clock = clock.map_or(1.0, |c| c.0 as f32);
    let started = start_held(&mut commands, &mut held, now_tick, tick_rate, clock);
    info!("audio unlocked: {started} held tune(s) start at the tick clock's position");
}

/// Gives each held tune its sound voice, from where the tick clock has it (the start for a tune
/// without a known length) and at the clock speed. Returns how many started.
pub fn start_held(
    commands: &mut Commands,
    held: &mut Query<HeldParts>,
    now_tick: f64,
    tick_rate: f64,
    clock: f32,
) -> usize {
    let mut n = 0;
    for (entity, sound, mut settings, sync) in held.iter_mut() {
        if let Some(mut sync) = sync {
            let at = sync.expected(now_tick, tick_rate);
            settings.start_position = Some(Duration::from_secs_f64(at));
            sync.settle(now_tick);
        }
        settings.speed *= clock;
        commands
            .entity(entity)
            .remove::<Held>()
            .insert(AudioPlayer::new(sound.0.clone()));
        n += 1;
    }
    n
}

/// The note on screen while the audio is locked.
#[derive(Component)]
struct Note;

/// Its widest, UI px (the notices' width, less their shear room).
const NOTE_WIDTH: f32 = 316.0;

/// The note: the notice card (`ui/notice.rs`) without its timer, with the heading in the
/// accent colour and the text in soft ink. It slides and fades in like a notice.
fn note() -> impl Bundle + use<> {
    let heading = Type {
        tracking: 0.12,
        ..Type::PILL_CAPS
    };
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(notice::MARGIN),
            top: Val::Px(notice::MARGIN),
            width: Val::Px(NOTE_WIDTH),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            padding: UiRect {
                left: Val::Px(22.0),
                right: Val::Px(14.0),
                top: Val::Px(10.0),
                bottom: Val::Px(11.0),
            },
            ..default()
        },
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
        // Under the notices (30), over the HUD (10) and the pause overlay (20).
        GlobalZIndex(29),
        Note,
        children![
            text(HEADING, heading, tokens::ACCENT),
            (
                text(BODY, Type::barlow(14.0, 500), tokens::SOFT),
                TextLayout::linebreak(LineBreak::WordOrCharacter),
            ),
        ],
    )
}

fn spawn_note(mut commands: Commands) {
    commands.spawn(note());
}

/// Keeps the note in the top left of the notices' area (the leaderboard column in a game), clear
/// of the notices that stack from the bottom; without a HUD, in the top right of the window.
fn place_note(
    area: Res<notice::Area>,
    ui_scale: Res<UiScale>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut notes: Query<&mut Node, With<Note>>,
) {
    let Some(window) = window.iter().next() else {
        return;
    };
    let size = window.size() / ui_scale.0.max(0.01);
    let want = match area.0 {
        Some(r) => (
            Val::Px(r.min.x + notice::MARGIN),
            Val::Px(r.min.y + notice::MARGIN),
            Val::Px((r.width() - 2.0 * notice::MARGIN).clamp(notice::MIN_WIDTH, NOTE_WIDTH)),
        ),
        // No HUD: the top right of the window, clear of the F3 overlay in the top left.
        None => (
            Val::Px((size.x - NOTE_WIDTH - notice::MARGIN).max(0.0)),
            Val::Px(notice::MARGIN),
            Val::Px(NOTE_WIDTH),
        ),
    };
    for mut node in &mut notes {
        if (node.left, node.top, node.width) != want {
            (node.left, node.top, node.width) = want;
        }
    }
}

/// Fades the note out once the lock has opened, then removes it.
fn hide_note(
    mut commands: Commands,
    lock: Res<Lock>,
    mut notes: Query<(Entity, &mut Tween<Opacity>), With<Note>>,
    mut fading: Local<bool>,
) {
    if lock.locked {
        return;
    }
    for (entity, mut fade) in &mut notes {
        if !*fading {
            fade.retarget(0.0);
        } else if fade.finished() {
            commands.entity(entity).despawn();
        }
    }
    *fading = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::audio::PlaybackMode;

    /// The page sets the global the browser build reads, and resumes on keys and clicks.
    #[test]
    fn the_web_page_sets_the_unlock_flag() {
        let page = include_str!("../../web/index.html");
        assert!(page.contains(BROWSER_FLAG));
        assert!(page.contains("keydown") && page.contains("pointerdown"));
    }

    #[test]
    fn a_locked_start_holds_tunes_and_drops_one_shot_sounds() {
        let locked = Lock::new(true);
        let open = Lock::new(false);
        assert_eq!(start(Some(&locked), true), Start::Hold);
        assert_eq!(start(Some(&locked), false), Start::Drop);
        assert_eq!(start(Some(&open), false), Start::Play);
        assert_eq!(start(None, true), Start::Play);
    }

    #[test]
    fn natively_the_first_key_opens_the_lock() {
        let mut app = App::new();
        app.insert_resource(Lock::new(true))
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, poll);
        app.update();
        assert!(app.world().resource::<Lock>().is_locked());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        app.update();
        assert!(!app.world().resource::<Lock>().is_locked());
    }

    /// A 10 s tune held from tick 100, paused for nothing: unlocked at tick 400 (6 s of ticks
    /// later at 50 Hz), it starts 6 s in, at the clock speed, and is left to settle.
    #[test]
    fn a_held_tune_starts_where_the_tick_clock_has_it() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_systems(
            Update,
            |mut commands: Commands, mut held: Query<HeldParts>| {
                start_held(&mut commands, &mut held, 400.0, 50.0, 0.5);
            },
        );
        let tune = app
            .world_mut()
            .spawn((
                Held(Handle::default()),
                PlaybackSettings {
                    mode: PlaybackMode::Loop,
                    speed: 1.0,
                    ..default()
                },
                crate::audio::Voice(
                    iwbtg_sim::sound::SoundTarget::Builtin(iwbtg_sim::def::SoundId(0)),
                    1.0,
                ),
                MusicSync::new(100.0, 1.0, 10.0),
            ))
            .id();
        app.update();
        let e = app.world().entity(tune);
        assert!(e.get::<Held>().is_none());
        assert!(e.get::<AudioPlayer>().is_some());
        let s = e.get::<PlaybackSettings>().unwrap();
        let at = s.start_position.unwrap().as_secs_f64();
        assert!((at - 6.0).abs() < 1e-9, "starts at {at}");
        assert_eq!(s.speed, 0.5);
        // Settling: no seek right after the start.
        let mut sync = e.get::<MusicSync>().unwrap().clone();
        assert_eq!(sync.correction(401.0, 50.0, 0.0), None);
    }

    #[test]
    fn a_tune_paused_while_held_starts_paused_at_its_held_place() {
        let mut sync = MusicSync::new(0.0, 1.0, 100.0);
        sync.set_paused(100.0, 50.0, true);
        // Paused at 2 s; the lock opens much later and the tune is still at 2 s.
        assert!((sync.expected(1000.0, 50.0) - 2.0).abs() < 1e-9);
    }
}
