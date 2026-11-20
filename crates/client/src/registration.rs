//! One list of client modules: each feature module exposes a single `Registration` holding its
//! clap `Args` group and its Bevy plugin, and `main.rs` names each module once
//! (docs/architecture/client/main.md).

use std::path::PathBuf;

use bevy::prelude::App;

/// What the command line says before the window opens. Modules read flags that belong to other
/// modules through it (`--screenshot` keeps stats, replays and imports off, for example), so
/// `main.rs` does not have to hand each one over.
#[derive(Debug, Default)]
pub struct Startup {
    /// A `--screenshot` check: no settings file, no stats, no replays, no saved imports.
    pub isolated: bool,
    /// `--screenshot`: the PNG to save after a few frames.
    pub screenshot: Option<PathBuf>,
    /// The package on the command line, if any.
    pub package: Option<PathBuf>,
    /// `--select`: the folder the game select lists.
    pub library: Option<PathBuf>,
    /// `--ranked`: always a new game, never the save file.
    pub ranked: bool,
    /// `--trace-mode`, for every package opened.
    pub trace_mode: bool,
    /// `--window`.
    pub window: Option<(u32, u32)>,
    /// `--ratio`, parsed (after `screen::configure` checked it).
    pub ratio: Option<(u32, u32)>,
}

impl Startup {
    /// Whether runs keep stats, replays and imported packages: a `--screenshot` check does not.
    pub fn keep(&self) -> bool {
        !self.isolated
    }
}

/// One client feature module: its command-line group and its plugin, from one registration item.
///
/// `main.rs` lists the modules once ([`client_modules!`]); the macro flattens each module's
/// `Args` into the command line, runs `prepare` and `standalone` in list order before the window
/// opens, and calls `add` for every module in the same order when a run or the select screen
/// starts. A module with nothing for one of those phases uses the default.
pub trait ClientModule {
    /// The module's clap group.
    type Args: clap::Args;

    /// Checks the module's flags and stores them where its plugin reads them, before the window
    /// opens. `Err` ends startup with that message.
    fn prepare(_args: &mut Self::Args, _startup: &Startup) -> Result<(), String> {
        Ok(())
    }

    /// Runs a mode that needs no game window (`--ui-gallery`, `--card-preview`,
    /// `--show-bindings`). `Some` ends startup with that result.
    fn standalone(_args: &Self::Args, _startup: &Startup) -> Option<Result<(), String>> {
        None
    }

    /// Adds the module's plugin to the app. Only called when a run or the select screen starts.
    fn add(_app: &mut App, _args: Self::Args, _startup: &Startup) {}
}

/// No command-line flags: a module with only a plugin takes the same list line as the rest.
/// `#[group(skip)]` keeps every flattened `NoArgs` from claiming the same clap group name.
#[derive(clap::Args, Debug, Default)]
#[group(skip)]
pub struct NoArgs {}

/// The client's feature modules, one line each.
///
/// The macro declares the module, flattens its `Args` group into the command line, and generates
/// the startup pass and the plugin list in list order. Adding a feature module is one line here
/// (plus its own `Registration`); the list's order is the order startup checks and plugins run.
#[macro_export]
macro_rules! client_modules {
    ($($module:ident),* $(,)?) => {
        $(mod $module;)*

        /// Plays a game package.
        #[derive(::clap::Parser)]
        #[command(name = "iwbtg-client", version, about = "IWBTG Collection client")]
        struct Args {
            $(
                #[command(flatten)]
                $module: <$module::Registration as $crate::registration::ClientModule>::Args,
            )*
        }

        impl Args {
            /// Runs every module's startup checks in list order. `Some` ends startup: a module's
            /// `standalone` mode, or a flag the modules refused.
            fn prepare(
                &mut self,
                startup: &$crate::registration::Startup,
            ) -> Option<::core::result::Result<(), ::std::string::String>> {
                $(
                    if let Err(e) =
                        <$module::Registration as $crate::registration::ClientModule>::prepare(
                            &mut self.$module,
                            startup,
                        )
                    {
                        return Some(Err(e));
                    }
                    if let Some(result) =
                        <$module::Registration as $crate::registration::ClientModule>::standalone(
                            &self.$module,
                            startup,
                        )
                    {
                        return Some(result);
                    }
                )*
                None
            }

            /// Adds each module's plugin, in list order.
            fn add_plugins(
                self,
                app: &mut ::bevy::prelude::App,
                startup: &$crate::registration::Startup,
            ) {
                $(
                    <$module::Registration as $crate::registration::ClientModule>::add(
                        app,
                        self.$module,
                        startup,
                    );
                )*
            }
        }
    };
}
