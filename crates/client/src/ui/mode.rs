//! The play mode and its accent (m5 A1, A2, B8; docs/product.md, Two ways to play each game).
//!
//! The player picks Time Attack or Classic once on the main menu and switches it in the toolbar on
//! every screen outside a run. [`Mode`] is that pick; every screen's accent follows it: the
//! primary buttons, the toolbar's underline, the main menu's glow. Time Attack keeps the cherry
//! accent ([`tokens::ACCENT`]), Classic is violet ([`tokens::CLASSIC`]).

use bevy::prelude::*;

use super::icons::Icon;
use super::tokens;

/// The play mode the player picked (Time Attack at first start).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mode {
    /// The game stripped to the run, timed, with leaderboards.
    #[default]
    TimeAttack,
    /// The game as it was made: its title, saves and difficulty select.
    Classic,
}

impl Mode {
    /// Both modes, left to right as the toolbar and the main menu show them.
    pub const ALL: [Mode; 2] = [Mode::TimeAttack, Mode::Classic];

    /// The mode's accent: cherry or violet.
    pub fn accent(self) -> Color {
        match self {
            Mode::TimeAttack => tokens::ACCENT,
            Mode::Classic => tokens::CLASSIC,
        }
    }

    /// The mode's name.
    pub fn label(self) -> &'static str {
        match self {
            Mode::TimeAttack => "Time Attack",
            Mode::Classic => "Classic",
        }
    }

    /// The toolbar's 20 px icon: a stopwatch or a floppy disk.
    pub fn small_icon(self) -> Icon {
        match self {
            Mode::TimeAttack => Icon::TimeAttack,
            Mode::Classic => Icon::Classic,
        }
    }
}

/// The accent of the mode in `mode`, Time Attack's when there is none (a test without the kit).
pub fn accent(mode: Option<&Mode>) -> Color {
    mode.copied().unwrap_or_default().accent()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_mode_has_its_own_accent() {
        assert_eq!(Mode::TimeAttack.accent(), tokens::ACCENT);
        assert_eq!(Mode::Classic.accent(), tokens::CLASSIC);
        assert_eq!(accent(None), tokens::ACCENT, "Time Attack until one is picked");
    }
}
