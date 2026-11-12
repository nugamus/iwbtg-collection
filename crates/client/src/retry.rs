//! Quick-retry and auto-retry (docs/product.md, Restart and Mods).
//!
//! `R` stays the game's own restart from the last save (the sim's `Restart` action). The
//! quick-retry key starts the whole run again from where it started: a new sim with the same
//! options. It is a client key, not a sim action, so it never reaches the game or a replay; a
//! retry is a new run. Auto-retry does the same after every death, for Sudden Death and
//! deathless attempts. The key is bound with the other bindings (`bindings.rs`, `quick-retry`,
//! Backquote by default); auto-retry is the settings file's `[retry] auto`, which `--auto-retry`
//! overrides for one run.

use bevy::prelude::*;

use crate::view::{Game, SimTick};

/// The retry command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug, Clone, Copy, Default)]
pub struct RetryArgs {
    /// Start the run again from the start after every death (meant for Sudden Death and
    /// deathless attempts), for this run only; `--auto-retry=false` turns it off. Without the
    /// flag the settings file decides (`[retry] auto`). The quick-retry key works either way.
    #[arg(
        long,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        value_name = "BOOL"
    )]
    pub auto_retry: Option<bool>,
}

/// Retry state: whether auto-retry is on, the death counter after the last tick, and a retry
/// waiting for the next frame.
#[derive(Resource)]
struct Retry {
    auto: bool,
    deaths: Option<u32>,
    pending: Option<Why>,
}

/// What asked for the retry, for the terminal line.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Why {
    Key,
    Death,
}

pub struct RetryPlugin(pub RetryArgs);

impl Plugin for RetryPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Retry {
            auto: auto_retry(self.0.auto_retry, crate::settings::get().auto_retry),
            deaths: None,
            pending: None,
        })
        // No tick runs between a death that auto-retries and the retry itself.
        .configure_sets(
            FixedUpdate,
            SimTick.run_if(|retry: Res<Retry>| retry.pending.is_none()),
        )
        .add_systems(FixedUpdate, watch.after(SimTick))
        .add_systems(
            Update,
            (key, apply.run_if(|retry: Res<Retry>| retry.pending.is_some())).chain(),
        );
    }
}

/// Auto-retry for this run: the flag when given, else the settings value.
fn auto_retry(flag: Option<bool>, setting: bool) -> bool {
    flag.unwrap_or(setting)
}

/// Whether a tick that moved the death counter from `before` to `now` auto-retries.
fn retries_on(auto: bool, before: Option<u32>, now: u32) -> bool {
    auto && before.is_some_and(|b| now > b)
}

/// After each tick: a death with auto-retry on asks for a retry.
fn watch(game: NonSend<Game>, mut retry: ResMut<Retry>) {
    if retry.pending.is_some() {
        return;
    }
    let now = game.sim.world().engine.run.deaths;
    if retries_on(retry.auto, retry.deaths, now) {
        retry.pending = Some(Why::Death);
    }
    retry.deaths = Some(now);
}

/// The quick-retry key or controller input (the collector's client-key flag, never an input
/// frame) asks for a retry.
fn key(collector: Option<ResMut<crate::input::Collector>>, mut retry: ResMut<Retry>) {
    if let Some(mut c) = collector
        && c.take_quick_retry()
    {
        retry.pending = Some(Why::Key);
    }
}

/// Asks for a quick-retry on the next frame (the pause menu's Restart run).
pub(crate) fn request(world: &mut World) {
    if let Some(mut retry) = world.get_resource_mut::<Retry>() {
        retry.pending = Some(Why::Key);
    }
}

/// Starts the run again: the view's new sim, then silence, then the mods' run state.
fn apply(world: &mut World) {
    let why = {
        let mut retry = world.resource_mut::<Retry>();
        retry.deaths = None;
        retry.pending.take()
    };
    // The ended run's replay is written while its sim is still there.
    crate::replays::reset(world);
    match crate::view::restart_run(world) {
        Ok(()) => {
            // The new sim's room start plays from its first step's output (#466).
            crate::audio::reset(world);
            crate::mods::reset(world);
            crate::stats::reset(world);
            crate::pause::reset(world);
            let why = match why {
                Some(Why::Death) => "auto-retry",
                _ => "quick-retry",
            };
            eprintln!("retry: run restarted ({why})");
        }
        Err(e) => eprintln!("retry: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_overrides_the_setting_for_one_run() {
        assert!(auto_retry(None, true));
        assert!(!auto_retry(None, false));
        assert!(auto_retry(Some(true), false));
        assert!(!auto_retry(Some(false), true));
    }

    #[test]
    fn only_a_new_death_with_auto_retry_on_retries() {
        assert!(retries_on(true, Some(0), 1));
        assert!(!retries_on(false, Some(0), 1));
        assert!(!retries_on(true, Some(1), 1));
        // The first tick has nothing to compare with: a run started from a save with deaths
        // does not retry at once.
        assert!(!retries_on(true, None, 5));
    }

    /// The demo kid presses suicide until the death counter moves; auto-retry then starts the
    /// run over at tick 0 with no deaths, and the next ticks run again.
    #[test]
    fn a_death_with_auto_retry_starts_the_run_over() {
        use bevy::ecs::system::RunSystemOnce;
        use iwbtg_sim::input::{Action, InputFrame};

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut world = crate::view::test_world(&pkg);
        world.insert_resource(Retry {
            auto: true,
            deaths: None,
            pending: None,
        });
        let suicide = InputFrame {
            held: Action::Suicide.bit(),
            pressed: Action::Suicide.bit(),
            released: 0,
        };
        world.run_system_once(watch).unwrap();
        let mut ticks = 0;
        while world.resource::<Retry>().pending.is_none() {
            assert!(ticks < 100, "no death in 100 ticks");
            world.non_send_mut::<Game>().sim.step(suicide);
            world.run_system_once(watch).unwrap();
            ticks += 1;
        }
        assert_eq!(world.resource::<Retry>().pending, Some(Why::Death));
        assert_eq!(world.non_send::<Game>().sim.world().engine.run.deaths, 1);
        world.run_system_once(apply).unwrap();
        let sim = &world.non_send::<Game>().sim;
        assert_eq!(sim.world().tick, 0);
        assert_eq!(sim.world().engine.run.deaths, 0);
        assert!(world.resource::<Retry>().pending.is_none());
    }
}
