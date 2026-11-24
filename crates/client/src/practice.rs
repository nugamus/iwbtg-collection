//! The practice assists that act on the clock (#551; docs/product.md, Mods): Slow motion (`SL`)
//! and Frame advance (`FA`), picked in the mod select sheet or with `--mods` (mods.rs) and read when
//! a run opens. Neither changes what a tick
//! computes, only when ticks run, so a run's replay plays back the same; a run with either is
//! never ranked (`ModSet::unranked_by`, said at start by `mods::describe` and in the HUD's mod
//! row).
//!
//! - Slow motion runs the fixed clock at [`SlowSpeed`] percent of the game's rate (75 or 50, a
//!   client setting, not in the replay) through `view::ClockSpeed`; sounds play at the same
//!   factor (audio.rs), so the music stays on the tick clock.
//! - Frame advance holds the clock after the run's first tick: the sim ticks only when the
//!   frame-advance key (a client key, F by default, bindings.rs) is pressed, one tick per press.
//!   Input collected between steps goes into the next step's frame with its edges, so a jump
//!   pressed before the step is a press on that tick. Music holds; one-shot sounds play.

use bevy::audio::AudioSink;
use bevy::prelude::*;
use iwbtg_sim::mods::{Mod, ModSet};

use crate::audio::MusicSync;
use crate::input::Collector;
use crate::mods::RunMods;
use crate::view::{ClockSpeed, SimTick};

/// The slow motion speeds the mod select offers, in percent.
pub const SLOW_SPEEDS: [u8; 2] = [75, 50];

/// The clock assists' command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug, Default)]
pub struct PracticeArgs {
    /// Slow motion's speed in percent of the game's rate, with `--mods SL`: 75 or 50. Overrides
    /// the settings file's `[practice] slow_speed` (default 75) for this run and is not saved.
    #[arg(long, value_name = "PERCENT", value_parser = parse_speed)]
    pub slow_speed: Option<u8>,
}

/// The practice assists' place in the module list (main.rs): its flags and plugin.
pub struct Registration;

impl crate::registration::ClientModule for Registration {
    type Args = PracticeArgs;

    fn add(
        app: &mut bevy::prelude::App,
        args: PracticeArgs,
        _startup: &crate::registration::Startup,
    ) {
        app.add_plugins(PracticePlugin {
            slow_speed: args.slow_speed,
        });
    }
}

fn parse_speed(text: &str) -> Result<u8, String> {
    text.trim()
        .trim_end_matches('%')
        .parse::<u8>()
        .ok()
        .filter(|p| SLOW_SPEEDS.contains(p))
        .ok_or_else(|| format!("`{text}` is not a slow motion speed (75 or 50)"))
}

/// Slow motion's speed in percent (`--slow-speed`, else `[practice] slow_speed`; the mod select
/// sheet's 75% / 50% change it and write the setting).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlowSpeed(pub u8);

/// The first [`SlowSpeed`]: the flag when given, else the setting.
fn first_speed(flag: Option<u8>, setting: u8) -> u8 {
    flag.unwrap_or(setting)
}

/// The clock speed a run with `mods` plays at.
pub fn clock_speed(mods: ModSet, slow_speed: u8) -> ClockSpeed {
    if mods.contains(Mod::SlowMotion) {
        ClockSpeed(f64::from(slow_speed) / 100.0)
    } else {
        ClockSpeed::default()
    }
}

/// One line for the terminal on how to play the clock assists, or `None` without them.
pub fn describe(mods: ModSet, slow_speed: u8) -> Option<String> {
    let mut parts = Vec::new();
    if mods.contains(Mod::SlowMotion) {
        parts.push(format!("Slow motion at {slow_speed}%"));
    }
    if mods.contains(Mod::FrameAdvance) {
        parts.push("Frame advance: the frame-advance key (F by default) runs one tick".to_string());
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// Frame advance's pending steps: presses not yet run as ticks. A run starts with one, so the
/// first room is on screen before the first press.
#[derive(Resource, Debug)]
struct Advance {
    steps: u32,
}

impl Default for Advance {
    fn default() -> Advance {
        Advance { steps: 1 }
    }
}

/// Adds the clock assists; each run plays with the ones in its [`RunMods`] and at the
/// [`SlowSpeed`] it opened with. `slow_speed` is `--slow-speed`; without it the first
/// [`SlowSpeed`] is the settings file's `[practice] slow_speed`.
pub struct PracticePlugin {
    pub slow_speed: Option<u8>,
}

impl Plugin for PracticePlugin {
    fn build(&self, app: &mut App) {
        let frame_advance = crate::mods::run_has(Mod::FrameAdvance);
        let first = first_speed(self.slow_speed, crate::settings::get().slow_speed);
        app.insert_resource(SlowSpeed(first))
            .init_resource::<ClockSpeed>()
            // Before the view's core: `view::enter` sets the run's first clock rate with it, and
            // `follow_room_speed` keeps it so after every tick.
            .add_systems(
                OnEnter(crate::shell::AppScreen::Run),
                start.before(crate::shell::RunSteps::Core),
            )
            .add_systems(
                OnEnter(crate::shell::AppScreen::Run),
                (|mut commands: Commands| commands.insert_resource(Advance::default()))
                    .run_if(frame_advance.clone())
                    .in_set(crate::shell::RunSteps::Resources),
            )
            .add_systems(
                OnExit(crate::shell::AppScreen::Run),
                (|world: &mut World| {
                    world.remove_resource::<Advance>();
                })
                .in_set(crate::shell::RunSteps::Resources),
            )
            // Without Frame advance there is no `Advance` and every step ticks.
            .configure_sets(
                FixedUpdate,
                SimTick.run_if(|a: Option<Res<Advance>>| a.is_none_or(|a| a.steps > 0)),
            )
            .add_systems(
                FixedUpdate,
                take.before(SimTick).run_if(resource_exists::<Advance>),
            )
            .add_systems(
                FixedUpdate,
                spend
                    .in_set(SimTick)
                    .after(crate::view::tick)
                    .run_if(resource_exists::<Advance>),
            )
            .add_systems(
                Update,
                hold_music
                    .run_if(resource_exists::<Advance>)
                    .in_set(crate::shell::InRun),
            );
    }
}

/// When a run opens: the clock speed of its mods, and the terminal line on how to play them.
fn start(world: &mut World) {
    let mods = RunMods::of(world);
    let slow = world.get_resource::<SlowSpeed>().map_or(SLOW_SPEEDS[0], |s| s.0);
    if let Some(line) = describe(mods, slow) {
        eprintln!("{line}");
    }
    world.insert_resource(clock_speed(mods, slow));
}

/// Each fixed step: the frame-advance presses since the last one become pending steps.
fn take(mut collector: ResMut<Collector>, mut advance: ResMut<Advance>) {
    advance.steps = advance.steps.saturating_add(collector.take_advance());
}

/// After a tick ran: one step less. Skipped with the tick when something else holds the sim (the
/// pause menu, a pending retry), so the press waits for the next tick that runs.
fn spend(mut advance: ResMut<Advance>) {
    advance.steps = advance.steps.saturating_sub(1);
}

/// Holds every looping tune: between steps the tick clock stands still, and the music follows it
/// (`audio::sync_music` puts it where the ticks say).
fn hold_music(mut tunes: Query<&mut AudioSink, With<MusicSync>>) {
    for sink in &mut tunes {
        if !sink.is_paused() {
            sink.pause();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Ticks(u32);

    fn count(mut ticks: ResMut<Ticks>) {
        ticks.0 += 1;
    }

    /// An app with only the plugin and a stand-in tick in `SimTick`.
    fn app(mods: &str) -> App {
        let mods = ModSet::parse(mods).unwrap();
        let mut app = App::new();
        app.insert_resource(Collector::new(crate::bindings::Bindings::default()))
            .insert_resource(RunMods(mods))
            .init_resource::<Ticks>()
            .add_plugins(PracticePlugin { slow_speed: Some(75) })
            .add_systems(FixedUpdate, count.in_set(SimTick));
        // What opening a run does (there is no `AppScreen` here).
        start(app.world_mut());
        if mods.contains(Mod::FrameAdvance) {
            app.init_resource::<Advance>();
        }
        app
    }

    fn steps(app: &mut App, n: u32) -> u32 {
        for _ in 0..n {
            app.world_mut().run_schedule(FixedUpdate);
        }
        app.world().resource::<Ticks>().0
    }

    #[test]
    fn frame_advance_runs_one_tick_per_press() {
        let mut app = app("FA");
        assert_eq!(steps(&mut app, 5), 1, "the first tick runs, then the clock holds");
        app.world_mut().resource_mut::<Collector>().press(b'F');
        assert_eq!(steps(&mut app, 5), 2, "one press, one tick");
        let mut c = app.world_mut().resource_mut::<Collector>();
        c.release(b'F');
        c.press(b'F');
        c.release(b'F');
        c.press(b'F');
        assert_eq!(steps(&mut app, 5), 4, "two presses before a step: two ticks");
    }

    #[test]
    fn without_frame_advance_every_step_ticks() {
        let mut app = app("SL");
        assert_eq!(steps(&mut app, 3), 3);
        assert_eq!(*app.world().resource::<ClockSpeed>(), ClockSpeed(0.75));
    }

    #[test]
    fn slow_motion_is_75_or_50_percent() {
        assert_eq!(parse_speed("75"), Ok(75));
        assert_eq!(parse_speed("50%"), Ok(50));
        assert!(parse_speed("25").is_err());
        assert!(parse_speed("fast").is_err());
        let sl = ModSet::parse("SL").unwrap();
        assert_eq!(clock_speed(sl, 50), ClockSpeed(0.5));
        assert_eq!(clock_speed(ModSet::parse("FA").unwrap(), 50), ClockSpeed(1.0));
    }

    #[test]
    fn the_flag_overrides_the_setting() {
        assert_eq!(first_speed(None, 50), 50);
        assert_eq!(first_speed(Some(75), 50), 75);
        assert_eq!(first_speed(Some(50), 75), 50);
    }

    #[test]
    fn the_start_line_says_how_to_play_the_assists() {
        assert_eq!(describe(ModSet::parse("SD").unwrap(), 75), None);
        assert_eq!(
            describe(ModSet::parse("SL").unwrap(), 50).as_deref(),
            Some("Slow motion at 50%")
        );
        assert!(
            describe(ModSet::parse("FA").unwrap(), 75)
                .unwrap()
                .starts_with("Frame advance: the frame-advance key (F by default)")
        );
    }
}
