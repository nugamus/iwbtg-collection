//! UI responsiveness, measured (UI foundation, #1683).
//!
//! Every frame in which a key, a mouse button or a gamepad button went down, the time the main
//! thread spent on that frame (`First` to `Last`: every system, any rebuild a screen did in
//! answer) is recorded. That is the cost of answering the input; a screen that respawns its tree
//! on a key press shows up here as a spike. The render runs pipelined after it and is not
//! counted.
//!
//! F3 shows the numbers: outside a run in a small readout in the window's bottom-left corner, in
//! a run as a line of the debug overlay (`overlay.rs`). The setting is the overlay's
//! (`hud.overlay`), so one key shows both.
//!
//! [`tests`] also has the guard the foundation's screens use in their own tests:
//! [`node_set`] lists every UI node, so a test can show a key press spawned or despawned none.

use std::collections::VecDeque;

use bevy::prelude::*;

use crate::time::{Duration, Instant};
use crate::ui::fonts::Type;
use crate::ui::{tokens, widgets};

/// The window the worst figure covers.
const WINDOW: Duration = Duration::from_secs(1);

/// The latest input frames' main-thread times.
#[derive(Resource, Debug, Default)]
pub struct UiLatency {
    started: Option<Instant>,
    input: bool,
    /// The last input frame's time, ms.
    pub last_ms: Option<f32>,
    /// Input frames within [`WINDOW`]: when, and how long, ms.
    recent: VecDeque<(Instant, f32)>,
}

impl UiLatency {
    /// The slowest input frame within the last second, ms.
    pub fn worst_ms(&self) -> Option<f32> {
        self.recent.iter().map(|(_, ms)| *ms).reduce(f32::max)
    }

    /// Records one input frame that took `ms`, at `now`.
    pub fn record(&mut self, now: Instant, ms: f32) {
        self.last_ms = Some(ms);
        self.recent.push_back((now, ms));
        while self
            .recent
            .front()
            .is_some_and(|(at, _)| now.duration_since(*at) > WINDOW)
        {
            self.recent.pop_front();
        }
    }

    /// The line the readouts show.
    pub fn line(&self) -> String {
        match (self.last_ms, self.worst_ms()) {
            (Some(last), Some(worst)) => format!("ui input {last:.1} ms  worst 1 s {worst:.1} ms"),
            (Some(last), None) => format!("ui input {last:.1} ms"),
            _ => "ui input -".to_owned(),
        }
    }
}

/// The readout outside a run.
#[derive(Component)]
struct Readout;

pub fn plugin(app: &mut App) {
    app.init_resource::<UiLatency>()
        .add_systems(First, start)
        .add_systems(Last, finish)
        .add_systems(Update, readout);
}

fn start(
    mut latency: ResMut<UiLatency>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    pads: Query<&Gamepad>,
) {
    let input = keys.is_some_and(|k| k.get_just_pressed().next().is_some())
        || mouse.is_some_and(|m| m.get_just_pressed().next().is_some())
        || pads.iter().any(|p| p.get_just_pressed().next().is_some());
    latency.started = Some(Instant::now());
    latency.input = input;
}

fn finish(mut latency: ResMut<UiLatency>) {
    let Some(started) = latency.started.take() else {
        return;
    };
    if std::mem::take(&mut latency.input) {
        let now = Instant::now();
        latency.record(now, now.duration_since(started).as_secs_f32() * 1000.0);
    }
}

/// The readout outside a run: shown while the overlay setting is on (F3 outside a run toggles
/// it too).
fn readout(
    mut commands: Commands,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    latency: Res<UiLatency>,
    screen: Option<Res<State<crate::shell::AppScreen>>>,
    shot: Option<Res<crate::view::ScreenshotTo>>,
    mut shown: Query<(Entity, &Children), With<Readout>>,
    mut texts: Query<&mut Text>,
) {
    let in_run = screen.is_some_and(|s| *s.get() == crate::shell::AppScreen::Run);
    if !in_run && keys.is_some_and(|k| k.just_pressed(KeyCode::F3)) {
        crate::settings::update(|s| s.overlay = !s.overlay);
    }
    // A `--screenshot` check never draws it: the numbers differ on every machine.
    let on = !in_run && shot.is_none() && crate::settings::get().overlay;
    match (on, shown.single_mut()) {
        (false, Ok((e, _))) => commands.entity(e).despawn(),
        (true, Err(_)) => {
            commands.spawn((
                Readout,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(8.0),
                    bottom: Val::Px(8.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(tokens::GLASS),
                GlobalZIndex(90),
                Pickable::IGNORE,
                children![widgets::text(&latency.line(), Type::barlow(12.0, 500), tokens::SOFT)],
            ));
        }
        (true, Ok((_, children))) if latency.is_changed() => {
            let line = latency.line();
            for c in children {
                if let Ok(mut t) = texts.get_mut(*c)
                    && t.0 != line
                {
                    t.0 = line.clone();
                }
            }
        }
        _ => {}
    }
}

/// Every UI node in the world: a screen's test takes it before and after a key press, and the
/// two are equal when the screen updated in place (UI foundation rule 1).
#[cfg(test)]
pub fn node_set(world: &mut World) -> std::collections::BTreeSet<Entity> {
    world.query_filtered::<Entity, With<Node>>().iter(world).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_last_and_the_worst_of_the_last_second() {
        let mut l = UiLatency::default();
        assert_eq!(l.line(), "ui input -");
        let t0 = Instant::now();
        l.record(t0, 9.0);
        l.record(t0 + Duration::from_millis(500), 2.0);
        assert_eq!(l.last_ms, Some(2.0));
        assert_eq!(l.worst_ms(), Some(9.0));
        l.record(t0 + Duration::from_millis(1600), 3.0);
        assert_eq!(l.worst_ms(), Some(3.0), "the 9 ms frame is older than a second");
        assert_eq!(l.line(), "ui input 3.0 ms  worst 1 s 3.0 ms");
    }

    #[test]
    fn only_input_frames_count() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>().add_plugins(plugin);
        app.update();
        assert_eq!(app.world().resource::<UiLatency>().last_ms, None, "no input yet");
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ArrowDown);
        app.update();
        assert!(app.world().resource::<UiLatency>().last_ms.is_some());
    }

    #[test]
    fn node_set_sees_a_respawn() {
        let mut world = World::new();
        let a = world.spawn(Node::default()).id();
        let before = node_set(&mut world);
        assert_eq!(node_set(&mut world), before);
        world.despawn(a);
        world.spawn(Node::default());
        assert_ne!(node_set(&mut world), before);
    }
}
