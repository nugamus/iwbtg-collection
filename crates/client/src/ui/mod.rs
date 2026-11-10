//! The client's UI kit (docs/decisions/0003-ui-toolkit.md, docs/architecture/client.md, UI): design
//! tokens, the bundled fonts, the icons, the sheared shape material, the shared components and motion.
//! Every screen builds from these parts; none brings its own colours, sizes or shapes.

pub mod fonts;
pub mod gallery;
pub mod icons;
pub mod motion;
pub mod shape;
pub mod tokens;
pub mod widgets;

use bevy::prelude::*;

/// The UI's command-line flags.
#[derive(clap::Args, Debug)]
pub struct UiArgs {
    /// Open the UI gallery (every UI component, for design review) instead of a game.
    #[arg(long)]
    pub ui_gallery: bool,
    /// Turn off slides, fades and pops: every UI transition lands at once.
    #[arg(long)]
    pub reduced_motion: bool,
}

/// Adds the fonts, the shape material, the motion systems and the component behaviour.
pub struct UiKitPlugin;

impl Plugin for UiKitPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((fonts::plugin, shape::plugin, motion::plugin, widgets::plugin, icons::plugin));
    }
}
