//! Design tokens: the colours, sizes and type of the M4 mockups (docs/design/mockups/m4/README.md,
//! Shared design language). Screens take every colour and size from here, never their own.

use bevy::prelude::*;

/// Horizontal shear of rows, panels, pills and buttons: CSS `skewX(-11.31deg)`, 0.2 px per px.
pub const SHEAR: f32 = 0.2;
/// Corner radius of every sheared shape, before the shear.
pub const RADIUS: f32 = 10.0;

/// Near-black ground behind everything.
pub const GROUND: Color = Color::srgb_u8(0x0b, 0x0d, 0x14);
/// Main text.
pub const INK: Color = Color::srgb_u8(0xf3, 0xf4, 0xf8);
/// Labels and secondary text.
pub const MUTED: Color = Color::srgb_u8(0xa2, 0xa8, 0xbc);
/// Text between ink and muted (deaths, small units).
pub const SOFT: Color = Color::srgb_u8(0xc9, 0xcd, 0xe0);
/// Cherry red: the primary action and selection marks.
pub const ACCENT: Color = Color::srgb_u8(0xff, 0x3d, 0x55);
/// Ahead of the comparison (shown with a `−` sign as well).
pub const AHEAD: Color = Color::srgb_u8(0x45, 0xd4, 0x83);
/// Behind the comparison (shown with a `+` sign as well).
pub const BEHIND: Color = Color::srgb_u8(0xff, 0x6b, 0x6b);
/// Best segment and first place (shown with a `★` as well).
pub const GOLD: Color = Color::srgb_u8(0xff, 0xc9, 0x3c);
/// Unranked and Offline.
pub const AMBER: Color = Color::srgb_u8(0xff, 0xc4, 0x6b);
/// The Ranked pill.
pub const RANKED: Color = Color::srgb_u8(0x8e, 0xcb, 0xff);
/// Dark text on a light pill (star pills, mod codes).
pub const ON_LIGHT: Color = Color::srgb_u8(0x10, 0x12, 0x1a);
/// Text on the Ranked pill.
pub const ON_RANKED: Color = Color::srgb_u8(0x0b, 0x1a, 0x2a);
/// Text on the Unranked pill.
pub const ON_AMBER: Color = Color::srgb_u8(0x2a, 0x1d, 0x05);

/// Dark glass under controls over art: `rgba(12,14,22,.72)`.
pub const GLASS: Color = Color::srgba(12.0 / 255.0, 14.0 / 255.0, 22.0 / 255.0, 0.72);
/// Light glass of secondary buttons: `rgba(255,255,255,.12)`.
pub const SECONDARY: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);
/// Tags and mod pills: `rgba(255,255,255,.16)`.
pub const CHIP: Color = Color::srgba(1.0, 1.0, 1.0, 0.16);

/// A leaderboard row (`Leaderboard.dc.html`): `rgba(12,14,22,.62)`.
#[allow(dead_code, reason = "other players' rows: the Global scope (#484)")]
pub const ROW: Color = Color::srgba(12.0 / 255.0, 14.0 / 255.0, 22.0 / 255.0, 0.62);
/// Your own leaderboard row: `rgba(255,255,255,.22)`.
pub const ROW_YOU: Color = Color::srgba(1.0, 1.0, 1.0, 0.22);
/// One of your past runs on the Local leaderboard: `rgba(255,255,255,.07)`.
pub const ROW_PAST: Color = Color::srgba(1.0, 1.0, 1.0, 0.07);
/// The name on someone else's leaderboard row.
pub const ROW_NAME: Color = Color::srgb_u8(0xe3, 0xe5, 0xee);
/// The three dots between a pinned leaderboard row and the rest.
pub const GAP_DOT: Color = Color::srgb_u8(0x6a, 0x70, 0x88);
/// A player with no avatar: the square and the mark on it.
pub const AVATAR_EMPTY: Color = Color::srgb_u8(0x2a, 0x2e, 0x3d);
pub const AVATAR_MARK: Color = Color::srgb_u8(0x5d, 0x63, 0x78);
/// An input display arrow that is not held (`RunInfo.dc.html`).
pub const KEY_IDLE: Color = Color::srgba(1.0, 1.0, 1.0, 0.22);
/// An input display button label that is not held.
pub const BUTTON_IDLE: Color = Color::srgba(1.0, 1.0, 1.0, 0.28);

/// The player card (`ProfileCard.dc.html`): its glass from the left edge to 60 %
/// (`rgba(14,16,24,.9)`, `.78`) and the warm right end (`rgba(40,14,22,.78)`). The alphas are
/// the artboard's turned into Bevy's (`1 - (1 - a)^2.2`): Bevy blends in linear light, which
/// would let the art through about twice as much (#627).
pub const CARD: Color = Color::srgba(14.0 / 255.0, 16.0 / 255.0, 24.0 / 255.0, 0.994);
pub const CARD_MID: Color = Color::srgba(14.0 / 255.0, 16.0 / 255.0, 24.0 / 255.0, 0.964);
pub const CARD_WARM: Color = Color::srgba(40.0 / 255.0, 14.0 / 255.0, 22.0 / 255.0, 0.964);
/// The card's colour with no glass: for a shape that floats over other UI (a notice), where even
/// `CARD`'s 0.6 % lets white text under it show (#664). Glass is for shapes over art.
pub const CARD_SOLID: Color = Color::srgb_u8(14, 16, 24);
/// The card's avatar gradient, top left to bottom right (`hsl(352 62% 52%)`, `hsl(330 55% 30%)`).
pub const AVATAR_FROM: Color = Color::hsl(352.0, 0.62, 0.52);
pub const AVATAR_TO: Color = Color::hsl(330.0, 0.55, 0.30);
/// A progress bar's empty track on the card: the artboard's `rgba(255,255,255,.12)` over the card
/// glass, mixed in sRGB as the browser does (white over dark has no single Bevy alpha, #627).
pub const TRACK: Color = Color::srgb_u8(0x2b, 0x2d, 0x34);
/// The light end of the accent gradient on a progress bar (`#ffb3bd`).
pub const ACCENT_LIGHT: Color = Color::srgb_u8(0xff, 0xb3, 0xbd);
/// The notches that cut a progress bar into steps: `rgba(8,9,14,.55)`, alpha as for [`CARD`].
pub const NOTCH: Color = Color::srgba(8.0 / 255.0, 9.0 / 255.0, 14.0 / 255.0, 0.828);

/// Results (`Results169.dc.html`): the line between split rows (`rgba(255,255,255,.07)`), the one
/// over the sum of best (`.22`), a chart's base line (`.18`) and the deaths-per-stage bars (`.38`,
/// the stage with the most deaths `.85`). White over dark has no single Bevy alpha (#627), so
/// these are the artboard's whites mixed over the ground in sRGB, as the browser blends them.
pub const RULE: Color = Color::srgb_u8(0x1c, 0x1e, 0x24);
pub const RULE_STRONG: Color = Color::srgb_u8(0x41, 0x42, 0x48);
pub const AXIS: Color = Color::srgb_u8(0x37, 0x39, 0x3e);
pub const BAR: Color = Color::srgb_u8(0x68, 0x69, 0x6d);
pub const BAR_PEAK: Color = Color::srgb_u8(0xda, 0xdb, 0xdc);

/// The star colour ramp (`Main.dc.html`, `ramp`): from each star count up, its colour.
pub const STAR_RAMP: [(f32, Color); 8] = [
    (0.0, Color::srgb_u8(0x7f, 0xc8, 0xff)),
    (2.0, Color::srgb_u8(0x6f, 0xe0, 0xc0)),
    (3.0, Color::srgb_u8(0xa9, 0xe2, 0x66)),
    (4.0, Color::srgb_u8(0xf2, 0xd5, 0x5a)),
    (5.0, Color::srgb_u8(0xff, 0x9f, 0x55)),
    (6.0, Color::srgb_u8(0xff, 0x60, 0x70)),
    (7.0, Color::srgb_u8(0xd4, 0x57, 0xad)),
    (8.0, Color::srgb_u8(0x9a, 0x6c, 0xf5)),
];

/// The colour of a star rating: the last ramp step at or below it (steps, not a blend).
pub fn star_color(stars: f32) -> Color {
    STAR_RAMP
        .iter()
        .take_while(|(from, _)| stars >= *from)
        .last()
        .map_or(STAR_RAMP[0].1, |(_, c)| *c)
}

/// Motion durations in seconds (README, Motion).
#[allow(
    dead_code,
    reason = "the full scale for the screens; the gallery uses part of it"
)]
pub mod motion {
    /// Most transitions (OutQuint).
    pub const STANDARD: f32 = 0.4;
    /// The shortest OutQuint transition.
    pub const QUICK: f32 = 0.3;
    /// The longest OutQuint transition.
    pub const SLOW: f32 = 0.5;
    /// Small pops (OutElastic).
    pub const POP: f32 = 0.5;
    /// Hover and press feedback on buttons.
    pub const HOVER: f32 = 0.15;
    /// Leaderboard rows changing place (OutQuint).
    pub const REORDER: f32 = 0.45;
    /// How long the pace flash holds before it fades back to your time.
    pub const PACE_HOLD: f32 = 3.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stars_take_the_step_at_or_below() {
        assert_eq!(star_color(0.5), STAR_RAMP[0].1);
        assert_eq!(star_color(1.99), STAR_RAMP[0].1);
        assert_eq!(star_color(2.0), STAR_RAMP[1].1);
        assert_eq!(star_color(6.84), STAR_RAMP[5].1);
        assert_eq!(star_color(8.21), STAR_RAMP[7].1);
        assert_eq!(star_color(-1.0), STAR_RAMP[0].1);
    }

    #[test]
    fn tokens_match_the_mockup_hex() {
        assert_eq!(GROUND.to_srgba().to_hex(), "#0B0D14");
        assert_eq!(ACCENT.to_srgba().to_hex(), "#FF3D55");
        assert_eq!(STAR_RAMP[5].1.to_srgba().to_hex(), "#FF6070");
    }
}
