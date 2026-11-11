//! `--mods`: the run's mods (docs/product.md, Mods; the table is `iwbtg_sim::mods::MODS`).
//!
//! Sudden Death: the tick that raises the death counter is the run's last. The sim stops
//! ticking there and a "Run over" banner shows; the window stays open on that frame until the
//! quick-retry key, or at once with `--auto-retry` (`retry.rs`). The other
//! mods are rejected until they are built (#550, #551, #39).

use bevy::prelude::*;
use bevy::sprite::Anchor;
use iwbtg_sim::mods::{Mod, ModSet};

use crate::view::{Game, GameCamera, SimTick};

/// The mods the client can play so far.
const BUILT: [Mod; 1] = [Mod::SuddenDeath];

/// The mods' command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug)]
pub struct ModsArgs {
    /// Mods for this run, by code: `SD` (Sudden Death). Several join with `,` or `+`.
    #[arg(long, value_name = "CODES", value_parser = parse_built, default_value = "")]
    pub mods: ModSet,
}

/// [`ModSet::parse`], then refuses a mod the client does not play yet.
fn parse_built(text: &str) -> Result<ModSet, String> {
    let set = ModSet::parse(text)?;
    match set.iter().find(|d| !BUILT.contains(&d.id)) {
        Some(d) => Err(format!("{} ({}) is not built yet", d.code, d.name)),
        None => Ok(set),
    }
}

/// One line for the terminal: the play mode, the set, and whether the run can be ranked.
/// Play original (no `--ranked`) is never ranked, whatever the mods.
pub fn describe(set: ModSet, ranked: bool) -> String {
    if !ranked {
        return format!("Play original (unranked), mods: {}", set.codes());
    }
    match set.unranked_by() {
        None => format!("Play ranked, mods: {} (ranked)", set.codes()),
        Some(d) => format!("Play ranked, mods: {} (unranked: {})", set.codes(), d.name),
    }
}

/// The run's mods and where the run stands.
#[derive(Resource)]
struct Run {
    mods: ModSet,
    /// The death counter after the last tick, `None` before the first.
    deaths: Option<u32>,
    /// The tick the run ended on.
    over: Option<u64>,
}

/// The "Run over" banner.
#[derive(Component)]
struct Banner;

/// Adds the run's mods: the sim ticks only while the run is not over.
pub struct ModsPlugin(pub ModSet);

impl Plugin for ModsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Run {
            mods: self.0,
            deaths: None,
            over: None,
        })
        .configure_sets(FixedUpdate, SimTick.run_if(|run: Res<Run>| run.over.is_none()))
        .add_systems(FixedUpdate, watch.after(SimTick))
        .add_systems(Update, banner);
    }
}

/// A retry (`retry.rs`) starts a fresh run: not over, no banner.
pub(crate) fn reset(world: &mut World) {
    if let Some(mut run) = world.get_resource_mut::<Run>() {
        run.deaths = None;
        run.over = None;
    }
    let banners: Vec<Entity> = world
        .query_filtered::<Entity, With<Banner>>()
        .iter(world)
        .collect();
    for banner in banners {
        world.despawn(banner);
    }
}

/// After each tick: a death under Sudden Death ends the run.
fn watch(game: NonSend<Game>, mut run: ResMut<Run>) {
    if run.over.is_some() {
        return;
    }
    let world = game.sim.world();
    let now = world.engine.run.deaths;
    if let Some(before) = run.deaths
        && run.mods.ends_run(before, now)
    {
        run.over = Some(world.tick);
    }
    run.deaths = Some(now);
}

/// Shows the banner once the run is over, fixed to the camera so it stays on screen.
fn banner(
    mut commands: Commands,
    run: Res<Run>,
    camera: Single<Entity, With<GameCamera>>,
    shown: Query<(), With<Banner>>,
) {
    let Some(tick) = run.over else { return };
    if !shown.is_empty() {
        return;
    }
    commands.entity(*camera).with_child((
        Text2d::new(format!("SUDDEN DEATH\nRun over at tick {tick}")),
        TextFont {
            font_size: bevy::text::FontSize::Px(32.0),
            ..default()
        },
        TextLayout::new(Justify::Center, bevy::text::LineBreak::WordBoundary),
        TextColor(Color::srgb(1.0, 0.38, 0.44)),
        Anchor::CENTER,
        Transform::from_xyz(0.0, 0.0, 0.0),
        Banner,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_line_names_the_play_mode() {
        let sd = ModSet::parse("SD").unwrap();
        assert_eq!(
            describe(ModSet::NONE, false),
            "Play original (unranked), mods: NM"
        );
        assert_eq!(describe(sd, false), "Play original (unranked), mods: SD");
        assert_eq!(describe(ModSet::NONE, true), "Play ranked, mods: NM (ranked)");
        assert!(describe(sd, true).starts_with("Play ranked, mods: SD ("));
    }

    #[test]
    fn only_built_mods_are_accepted() {
        assert_eq!(parse_built("sd").unwrap().codes(), "SD");
        assert_eq!(parse_built("").unwrap(), ModSet::NONE);
        assert!(
            parse_built("SD,SL")
                .unwrap_err()
                .contains("SL (Slow motion) is not built yet")
        );
        assert!(parse_built("ZZ").unwrap_err().contains("unknown mod"));
    }

    #[test]
    fn the_line_says_ranked_or_why_not() {
        assert_eq!(
            describe(ModSet::parse("SD").unwrap(), true),
            "Play ranked, mods: SD (ranked)"
        );
        assert_eq!(
            describe(ModSet::parse("SD,SL").unwrap(), true),
            "Play ranked, mods: SD+SL (unranked: Slow motion)"
        );
    }
}
