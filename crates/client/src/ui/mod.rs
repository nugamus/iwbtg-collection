//! The client's UI kit (docs/decisions/0003-ui-toolkit.md, docs/architecture/client/ui.md): design
//! tokens, the bundled fonts, the icons, glows, the backdrop, the sheared shape material, the
//! picture filter, the shared components and motion.
//! Every screen builds from these parts; none brings its own colours, sizes or shapes.

pub mod backdrop;
pub mod filter;
pub mod focus;
pub mod fonts;
pub mod gallery;
pub mod glow;
pub mod hints;
pub mod icons;
pub mod latency;
pub mod live;
pub mod mode;
pub mod motion;
pub mod notice;
pub mod notifications;
pub mod shape;
pub mod tokens;
pub mod toolbar;
pub mod widgets;

use bevy::prelude::*;

/// The UI's command-line flags.
#[derive(clap::Args, Debug, Default)]
pub struct UiArgs {
    /// Open the UI gallery (every UI component, for design review) instead of a game.
    #[arg(long)]
    pub ui_gallery: bool,
    /// Turn off slides, fades and pops: every UI transition lands at once.
    #[arg(long)]
    pub reduced_motion: bool,
    /// Screenshot checks: post a sample notice at start (`storage` when no kind is given).
    #[arg(long, hide = true, num_args = 0..=1, default_missing_value = "storage")]
    pub notice_preview: Option<NoticePreview>,
}

/// The UI's place in the module list (main.rs): its flags and the `--ui-gallery` mode. The UI kit
/// plugin itself is added by the screens that use it (the HUD, the gallery).
pub struct Registration;

impl crate::registration::ClientModule for Registration {
    type Args = UiArgs;

    fn prepare(args: &mut UiArgs, _startup: &crate::registration::Startup) -> Result<(), String> {
        configure(args);
        Ok(())
    }

    fn standalone(
        args: &UiArgs,
        startup: &crate::registration::Startup,
    ) -> Option<Result<(), String>> {
        args.ui_gallery.then(|| {
            gallery::run(
                startup.screenshot.clone(),
                args.reduced_motion,
                startup.window,
                startup.ratio,
            )
        })
    }
}

/// The sample notices of `--notice-preview`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum NoticePreview {
    /// Saves cannot be written.
    Storage,
    /// A dropped package with errors (the longest drop message, cut).
    Drop,
    /// One of each kind, and the bell's list open (m5 D2).
    List,
}

/// Called by `main`: posts the preview notice.
pub fn configure(args: &UiArgs) {
    match args.notice_preview {
        Some(NoticePreview::Storage) => notice::post(
            notice::Kind::Storage,
            "could not update the save file: C:/Users/player/AppData/Local/iwbtg-collection/saves/demo_art.save: Access is denied. (os error 5)",
        ),
        Some(NoticePreview::Drop) => notice::post(
            notice::Kind::Drop,
            "my-game.iwbz has 2 errors. The first: rooms/start.toml:3:5: unknown object kind \"spikee\"",
        ),
        Some(NoticePreview::List) => {
            notice::post(notice::Kind::Storage, "stats.toml: disk full");
            notice::post(notice::Kind::Drop, "level.zip is not a game package.");
            notice::post(
                notice::Kind::Controls,
                "Extra actions: Bomb (C), Teleport (unbound).",
            );
            notifications::open_at_start();
        }
        None => {}
    }
}

/// Adds the fonts, the shape material, the motion systems and the component behaviour.
pub struct UiKitPlugin;

impl Plugin for UiKitPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            fonts::plugin,
            shape::plugin,
            filter::plugin,
            focus::plugin,
            motion::plugin,
            widgets::plugin,
            icons::plugin,
            latency::plugin,
            glow::plugin,
            notice::plugin,
            notifications::plugin,
            toolbar::plugin,
        ));
        // The game screen may have added it already.
        if !app.is_plugin_added::<backdrop::BackdropPlugin>() {
            app.add_plugins(backdrop::BackdropPlugin);
        }
    }
}
