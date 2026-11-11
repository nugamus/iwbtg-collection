//! Where the game and the two HUD columns go (docs/product.md, In-game display; artboards
//! `Hud169`, `Hud43`, `Hud219`, `HudSmall` in docs/design/mockups/m4/).
//!
//! The game is centred and never stretched. It fills the height unless that leaves the columns
//! less than their room (4:3), then it shrinks. The columns take what is left beside it, up to
//! 360 px each; past that the backdrop shows. Small windows (960x540 and below) give the columns
//! less room, so the game keeps priority. The UI scale scales the columns' contents only: the
//! game's size never depends on it. With the HUD hidden the game takes the whole area.
//!
//! Sizes here are logical px (the artboards' CSS px); `compute` works in physical px.

use bevy::math::{Rect, Vec2};

/// The room the left and right columns keep at 100% (`Hud43`: the 0.8x game at x 204 leaves
/// 180 px on the right of 1024).
pub const MIN_LEFT: f32 = 204.0;
pub const MIN_RIGHT: f32 = 180.0;
/// Each column in a small window (`HudSmall`: 124 px beside the 712 px game).
pub const SMALL_COLUMN: f32 = 124.0;
/// A column grows to this at 100% UI scale (`Hud219`); the rest is backdrop.
pub const MAX_COLUMN: f32 = 360.0;
/// From this column width (in UI px) the panels show everything (`Hud169`: 249).
pub const FULL_FROM: f32 = 240.0;
/// A window this size or smaller in either direction is small.
pub const SMALL_WINDOW: Vec2 = Vec2::new(960.0, 540.0);
/// The UI scale range, in percent.
pub const UI_SCALE_RANGE: std::ops::RangeInclusive<u32> = 75..=150;

/// How much the panels show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Density {
    /// Leaderboard with flags, run info with the input display (`Hud169`, `Hud219`).
    #[default]
    Full,
    /// No flags, a smaller timer (`Hud43`).
    Compact,
    /// The leaderboard shows rank and avatar plus your own time; run info stacks under the timer
    /// and the input display hides (`HudSmall`).
    Small,
}

/// The player's HUD choices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    /// Shift+Tab: the HUD is shown.
    pub shown: bool,
    /// UI scale, 0.75 to 1.5.
    pub ui_scale: f32,
    /// Use the largest whole scale that fits instead.
    pub integer: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            shown: true,
            ui_scale: 1.0,
            integer: false,
        }
    }
}

/// The result, in physical px of the window.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Layout {
    /// The game's rectangle, on whole pixels.
    pub game: Rect,
    /// Physical px per game pixel.
    pub scale: f32,
    /// The columns beside the game (empty when the HUD is hidden).
    pub left: Rect,
    pub right: Rect,
    pub density: Density,
    /// Logical px per UI px of the panels: the UI scale, made smaller when a column is too
    /// narrow for the panel at that scale.
    pub content: f32,
}

/// Lays out `view` (the game's size in game pixels) in `area` (physical px; the window, or the
/// part a fullscreen ratio allows) for a window with `dpi` physical px per logical px.
pub fn compute(area: Rect, dpi: f32, view: Vec2, o: Options) -> Layout {
    let dpi = dpi.max(0.01);
    let view = view.max(Vec2::ONE);
    let size = area.size() / dpi;
    let small = size.x <= SMALL_WINDOW.x + 0.5 || size.y <= SMALL_WINDOW.y + 0.5;
    let (min_l, min_r) = match (o.shown, small) {
        (false, _) => (0.0, 0.0),
        (true, true) => (SMALL_COLUMN, SMALL_COLUMN),
        (true, false) => (MIN_LEFT, MIN_RIGHT),
    };
    // Physical px per game pixel.
    let fit = (area.height() / view.y).min((area.width() - (min_l + min_r) * dpi).max(1.0) / view.x);
    let scale = if o.integer && fit >= 1.0 { fit.floor() } else { fit };
    let game_size = (view * scale).round().max(Vec2::ONE);
    let spare = area.width() - game_size.x;
    let (min_l, min_r) = (min_l * dpi, min_r * dpi);
    // Centred when both columns keep their room, else the left one gets exactly its room.
    let left = if spare >= min_l + min_r {
        (spare / 2.0).clamp(min_l, spare - min_r)
    } else if min_l + min_r > 0.0 {
        spare.max(0.0) * min_l / (min_l + min_r)
    } else {
        spare / 2.0
    }
    .floor();
    let right = spare - left;
    let min = Vec2::new(
        area.min.x.round() + left,
        (area.min.y + (area.height() - game_size.y) / 2.0).round(),
    );
    let game = Rect::from_corners(min, min + game_size);
    if !o.shown {
        return Layout {
            game,
            scale,
            content: o.ui_scale,
            ..Layout::default()
        };
    }
    let max_col = MAX_COLUMN * o.ui_scale * dpi;
    let col_l = left.min(max_col).max(0.0);
    let col_r = right.min(max_col).max(0.0);
    let left = Rect::new(game.min.x - col_l, area.min.y, game.min.x, area.max.y);
    let right = Rect::new(game.max.x, area.min.y, game.max.x + col_r, area.max.y);
    // In logical px.
    let (col_l, col_r) = (col_l / dpi, col_r / dpi);
    let density = if small {
        Density::Small
    } else if col_l.min(col_r) / o.ui_scale >= FULL_FROM {
        Density::Full
    } else {
        Density::Compact
    };
    let (need_l, need_r) = match density {
        Density::Full => (FULL_FROM, FULL_FROM),
        Density::Compact => (MIN_LEFT, MIN_RIGHT),
        Density::Small => (SMALL_COLUMN, SMALL_COLUMN),
    };
    let content = o.ui_scale.min(col_l / need_l).min(col_r / need_r).max(0.25);
    Layout {
        game,
        scale,
        left,
        right,
        density,
        content,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAME: Vec2 = Vec2::new(800.0, 608.0);

    fn at(w: f32, h: f32, o: Options) -> Layout {
        compute(Rect::new(0.0, 0.0, w, h), 1.0, GAME, o)
    }

    fn shown() -> Options {
        Options::default()
    }

    #[test]
    fn at_16_9_the_game_fills_the_height_and_the_panels_split_the_rest() {
        let l = at(1920.0, 1080.0, shown());
        assert!((l.scale - 1080.0 / 608.0).abs() < 1e-4, "1.78x");
        assert_eq!(l.game.size(), Vec2::new(1421.0, 1080.0));
        assert_eq!(l.left.width(), 249.0);
        assert_eq!(l.right.width(), 250.0);
        assert_eq!(l.density, Density::Full);
        assert_eq!(l.content, 1.0);
    }

    #[test]
    fn at_4_3_the_game_shrinks_so_both_panels_keep_their_room() {
        let l = at(1024.0, 768.0, shown());
        assert!((l.scale - 0.8).abs() < 1e-4, "0.8x");
        assert_eq!(l.game, Rect::new(204.0, 141.0, 844.0, 627.0));
        assert_eq!((l.left.width(), l.right.width()), (204.0, 180.0));
        assert_eq!(l.density, Density::Compact);
    }

    #[test]
    fn at_16_10_the_panels_are_compact() {
        let l = at(1920.0, 1200.0, shown());
        assert!((l.scale - 1.92).abs() < 1e-4);
        assert_eq!(l.density, Density::Compact);
    }

    #[test]
    fn ultrawide_panels_stop_at_360_and_the_game_stays_centred() {
        for w in [2560.0, 3840.0] {
            let l = at(w, 1080.0, shown());
            assert_eq!(l.game.size(), Vec2::new(1421.0, 1080.0));
            assert_eq!(l.game.center().x.round(), (w / 2.0).round());
            assert_eq!((l.left.width(), l.right.width()), (360.0, 360.0));
            assert_eq!(l.density, Density::Full);
        }
        // Hud219: the game at x 569.
        assert_eq!(at(2560.0, 1080.0, shown()).game.min.x, 569.0);
    }

    #[test]
    fn a_small_window_keeps_the_game_big_and_collapses_the_panels() {
        let l = at(960.0, 540.0, shown());
        assert!((l.scale - 540.0 / 608.0).abs() < 1e-4, "0.89x");
        assert_eq!(l.game.min.x, 124.0);
        assert_eq!(l.left.width(), 124.0);
        assert!(l.right.width() >= 124.0);
        assert_eq!(l.density, Density::Small);
    }

    #[test]
    fn hiding_the_hud_gives_the_game_the_whole_area() {
        let o = Options {
            shown: false,
            ..shown()
        };
        let l = at(1024.0, 768.0, o);
        assert!((l.scale - 768.0 / 608.0).abs() < 1e-4);
        assert_eq!(l.left.width(), 0.0);
        assert_eq!(at(800.0, 608.0, o).game, Rect::new(0.0, 0.0, 800.0, 608.0));
    }

    #[test]
    fn integer_scale_takes_the_largest_whole_step_that_fits() {
        let o = Options {
            integer: true,
            ..shown()
        };
        assert_eq!(at(1920.0, 1080.0, o).scale, 1.0);
        assert_eq!(at(2560.0, 1440.0, o).scale, 2.0);
        assert_eq!(at(3840.0, 2160.0, o).scale, 3.0);
        // Smaller than one game pixel per pixel: shrinks rather than cutting off.
        assert!(at(960.0, 540.0, o).scale < 1.0);
    }

    #[test]
    fn ui_scale_never_changes_the_game_and_never_overflows_a_column() {
        let big = Options {
            ui_scale: 1.5,
            ..shown()
        };
        for (w, h) in [
            (1920.0, 1080.0),
            (1024.0, 768.0),
            (2560.0, 1080.0),
            (960.0, 540.0),
        ] {
            let a = at(w, h, shown());
            let b = at(w, h, big);
            assert_eq!(a.game, b.game, "{w}x{h}");
            assert!(b.content >= a.content);
        }
        // Ultrawide has room: the full 150% and wider columns.
        let wide = at(3840.0, 1080.0, big);
        assert_eq!(wide.content, 1.5);
        assert_eq!(wide.left.width(), 540.0);
        // 1080p does not: the content grows only as far as the column allows.
        let tight = at(1920.0, 1080.0, big);
        assert!(tight.content < 1.5 && tight.content > 1.0);
        let small = Options {
            ui_scale: 0.75,
            ..shown()
        };
        assert_eq!(at(1920.0, 1080.0, small).content, 0.75);
    }

    #[test]
    fn a_hidpi_window_lays_out_in_logical_px() {
        let l = compute(Rect::new(0.0, 0.0, 3840.0, 2160.0), 2.0, GAME, shown());
        assert_eq!(l.left.width(), 499.0);
        assert_eq!(l.density, Density::Full);
        assert_eq!(l.content, 1.0);
    }

    #[test]
    fn a_ratio_area_offsets_everything() {
        let l = compute(Rect::new(448.0, 0.0, 1472.0, 768.0), 1.0, GAME, shown());
        assert_eq!(l.game.min, Vec2::new(652.0, 141.0));
        assert_eq!(l.left.min.x, 448.0);
    }
}
