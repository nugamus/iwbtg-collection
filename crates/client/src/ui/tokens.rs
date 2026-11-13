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

/// Dark glass under controls over art: `rgba(12,14,22,.72)`, as [`dark`]`(.., 0.72)` (#713).
pub const GLASS: Color = Color::srgba(12.0 / 255.0, 14.0 / 255.0, 22.0 / 255.0, 0.9392);
/// What white glass usually sits on: the blurred backdrop art under its dim (#692), measured from
/// the screenshots (the demo art at the gameplay dim is about `#18204a`, the gallery art at the
/// select dim `#110f21`). [`white`] picks its alphas against this.
pub const BACKDROP: Color = Color::srgb_u8(0x14, 0x17, 0x34);

/// Light glass of secondary buttons: `rgba(255,255,255,.12)`, as [`white`]`(0.12)`.
pub const SECONDARY: Color = Color::srgba(1.0, 1.0, 1.0, 0.0253);
/// Mod pills: `rgba(255,255,255,.16)`, as [`white`]`(0.16)`.
pub const CHIP: Color = Color::srgba(1.0, 1.0, 1.0, 0.0381);

/// A leaderboard row (`Leaderboard.dc.html`): `rgba(12,14,22,.62)`, as [`dark`]`(.., 0.62)` (#713).
#[allow(dead_code, reason = "other players' rows: the Global scope (#484)")]
pub const ROW: Color = Color::srgba(12.0 / 255.0, 14.0 / 255.0, 22.0 / 255.0, 0.8811);
/// Your own leaderboard row: `rgba(255,255,255,.22)`, as [`white`]`(0.22)`.
pub const ROW_YOU: Color = Color::srgba(1.0, 1.0, 1.0, 0.0618);
/// One of your past runs on the Local leaderboard: `rgba(255,255,255,.07)`, as [`white`]`(0.07)`.
pub const ROW_PAST: Color = Color::srgba(1.0, 1.0, 1.0, 0.0125);
/// The name on someone else's leaderboard row.
pub const ROW_NAME: Color = Color::srgb_u8(0xe3, 0xe5, 0xee);
/// The three dots between a pinned leaderboard row and the rest.
pub const GAP_DOT: Color = Color::srgb_u8(0x6a, 0x70, 0x88);
/// A player with no avatar: the square and the mark on it.
pub const AVATAR_EMPTY: Color = Color::srgb_u8(0x2a, 0x2e, 0x3d);
pub const AVATAR_MARK: Color = Color::srgb_u8(0x5d, 0x63, 0x78);
/// An input display arrow that is not held (`RunInfo.dc.html`): `rgba(255,255,255,.22)`, as
/// [`white`]`(0.22)`.
pub const KEY_IDLE: Color = Color::srgba(1.0, 1.0, 1.0, 0.0618);
/// An input display button label that is not held: `rgba(255,255,255,.28)`, as [`white`]`(0.28)`.
pub const BUTTON_IDLE: Color = Color::srgba(1.0, 1.0, 1.0, 0.0913);
/// The glow under a held arrow (`drop-shadow(0 0 6px rgba(255,255,255,.55))`) and a held word
/// (`text-shadow: 0 0 8px rgba(255,255,255,.5)`): peak alphas as [`white`], the blur in CSS px
/// (`ui::glow::Glow`).
pub const KEY_GLOW: Color = Color::srgba(1.0, 1.0, 1.0, 0.3038);
pub const KEY_GLOW_BLUR: f32 = 6.0;
pub const BUTTON_GLOW: Color = Color::srgba(1.0, 1.0, 1.0, 0.2539);
pub const BUTTON_GLOW_BLUR: f32 = 8.0;

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

/// Bevy's alpha for a black overlay at the artboard's CSS alpha `a` (#627). The browser blends
/// `rgba(...)` in sRGB, Bevy in linear light, where `a` would leave `(1 - a)^(1/2.2)` of the sRGB
/// value under it (0.5 keeps about 73 %, 0.94 about 28 %). Keeping `(1 - a)^2.2` of the light
/// leaves `1 - a` in sRGB, as the browser does, for black over anything and for stacked black
/// layers: a little darker than the browser, by up to about 8 of 255 steps where the result is near
/// black, as the sRGB curve is not quite a 2.2 power. Use it for shadows, dims and dark glass (dark colours are close to black).
pub fn artboard_alpha(a: f32) -> f32 {
    1.0 - (1.0 - a).powf(2.2)
}

/// Black at the artboard's alpha `a`: `rgba(0,0,0,a)` as the browser draws it ([`artboard_alpha`]).
pub fn black(a: f32) -> Color {
    dark(Color::BLACK, a)
}

/// A dark glass colour at the artboard's alpha `a` (`rgba(14,16,24,.82)`): the black rule, which
/// is within a few sRGB steps for colours this close to black. Light or saturated colours go
/// through [`over`] instead. The `CARD` constants above are this, computed by hand.
pub fn dark(color: Color, a: f32) -> Color {
    color.with_alpha(artboard_alpha(a))
}

/// `color` at the artboard's alpha `a` over a ground of `under`, for coloured overlays such as
/// the accent glow (#627). No single alpha matches the browser for a colour over every ground: a
/// light colour over a dark ground needs a smaller alpha than the artboard's, a dark one over a
/// light ground a larger one. This picks the alpha that gives the browser's luminance over
/// `under` (pass what the overlay usually sits on, [`GROUND`] for the dark screens); over other
/// grounds it is an approximation. For black it equals [`black`] up to the sRGB curve's toe.
pub fn over(color: Color, a: f32, under: Color) -> Color {
    let luminance = |c: LinearRgba| 0.2126 * c.red + 0.7152 * c.green + 0.0722 * c.blue;
    let (top, ground) = (color.to_srgba(), under.to_srgba());
    let mixed = Srgba::new(
        top.red * a + ground.red * (1.0 - a),
        top.green * a + ground.green * (1.0 - a),
        top.blue * a + ground.blue * (1.0 - a),
        1.0,
    );
    let (yt, yg, ym) = (
        luminance(top.into()),
        luminance(ground.into()),
        luminance(mixed.into()),
    );
    let alpha = if (yt - yg).abs() < 1e-6 {
        a
    } else {
        ((ym - yg) / (yt - yg)).clamp(0.0, 1.0)
    };
    color.with_alpha(alpha)
}

/// White at the artboard's alpha `a` over the backdrop: `rgba(255,255,255,a)` as the browser draws
/// it over [`BACKDROP`] (#692). The copied alpha would come out about twice as bright: white at
/// 0.22 over the dimmed art needs about 0.06 in linear light. The backdrop's brightness varies
/// with the art, so this is exact only over `BACKDROP`; over the measured backdrops it stays
/// within about 10 of 255 sRGB steps of the browser (the copied alpha was off by up to 60).
/// White glass over a known opaque ground is an sRGB mix by hand instead (`TRACK`, `RULE`).
/// The white constants above are this, precomputed (a test keeps them equal).
pub fn white(a: f32) -> Color {
    over(Color::WHITE, a, BACKDROP)
}

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

    /// Blends `top` over `under` in linear light, as Bevy does, and returns the sRGB result.
    fn bevy_blend(top: Color, under: Color) -> Srgba {
        let (t, u) = (top.to_linear(), under.to_linear());
        let mix = |a: f32, b: f32| a * t.alpha + b * (1.0 - t.alpha);
        LinearRgba::rgb(mix(t.red, u.red), mix(t.green, u.green), mix(t.blue, u.blue)).into()
    }

    #[test]
    fn black_keeps_one_minus_a_of_the_srgb_value() {
        // The pause dim (#479): black at .5 over a mid tile halves its sRGB value, up to about
        // 8 of 255 steps darker, as the sRGB curve is not quite a 2.2 power.
        let tile = Color::srgb(0.9, 0.55, 0.12);
        for a in [0.35, 0.5, 0.6, 0.94] {
            let got = bevy_blend(black(a), tile);
            assert!((got.red - 0.9 * (1.0 - a)).abs() < 0.035, "a {a}: {got:?}");
            assert!((got.green - 0.55 * (1.0 - a)).abs() < 0.035, "a {a}: {got:?}");
        }
        assert!((artboard_alpha(0.5) - 0.782).abs() < 0.001);
    }

    #[test]
    fn white_glass_matches_the_browser_over_the_backdrop() {
        for (token, a) in [
            (SECONDARY, 0.12),
            (CHIP, 0.16),
            (ROW_YOU, 0.22),
            (ROW_PAST, 0.07),
            (KEY_IDLE, 0.22),
            (BUTTON_IDLE, 0.28),
            (KEY_GLOW, 0.55),
            (BUTTON_GLOW, 0.5),
        ] {
            assert!(
                (token.alpha() - white(a).alpha()).abs() < 0.0005,
                "{a}: {} vs {}",
                token.alpha(),
                white(a).alpha()
            );
        }
        // Over the backdrops measured in the screenshots (gameplay dim, select dim) and the
        // ground, the row comes out within 10 sRGB steps of the browser in every channel's mean.
        for ground in [
            Color::srgb_u8(24, 32, 74),
            Color::srgb_u8(17, 15, 33),
            BACKDROP,
            GROUND,
        ] {
            let got = bevy_blend(ROW_YOU, ground);
            let g = ground.to_srgba();
            let want = (0.22 * 3.0 + 0.78 * (g.red + g.green + g.blue)) / 3.0;
            let mean = (got.red + got.green + got.blue) / 3.0;
            assert!((mean - want).abs() < 10.0 / 255.0, "{ground:?}: {mean} vs {want}");
        }
    }

    #[test]
    fn dark_glass_holds_the_dark_alpha() {
        for (token, a) in [(GLASS, 0.72), (ROW, 0.62)] {
            let want = dark(token, a).alpha();
            assert!(
                (token.alpha() - want).abs() < 0.0005,
                "{a}: {} vs {want}",
                token.alpha()
            );
        }
        // Over the backdrop the glass keeps about 1 - a of the sRGB value, as the browser does.
        let got = bevy_blend(GLASS, BACKDROP);
        let b = BACKDROP.to_srgba();
        let want = 0.28 * b.blue + 0.72 * 22.0 / 255.0;
        assert!((got.blue - want).abs() < 0.035, "{got:?} vs {want}");
    }

    #[test]
    fn a_coloured_overlay_matches_the_browser_over_its_ground() {
        // The primary button's glow, `rgba(255,61,85,.35)` over the ground.
        let glow = over(ACCENT, 0.35, GROUND);
        assert!(
            glow.alpha() < 0.35,
            "light over dark takes less alpha: {}",
            glow.alpha()
        );
        let got = bevy_blend(glow, GROUND);
        let (top, ground) = (ACCENT.to_srgba(), GROUND.to_srgba());
        let want = top.red * 0.35 + ground.red * 0.65;
        assert!((got.red - want).abs() < 0.03, "{got:?} vs {want}");
        // Black comes close to the black rule (which is not exact over every ground either).
        let dark = over(Color::BLACK, 0.6, Color::srgb(0.5, 0.5, 0.5));
        assert!((dark.alpha() - artboard_alpha(0.6)).abs() < 0.03);
    }
}
