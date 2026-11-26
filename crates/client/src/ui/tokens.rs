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
/// Secondary text over art: the creator on a game panel (`#dfe2ec`, `Main.dc.html`).
pub const ON_ART: Color = Color::srgb_u8(0xdf, 0xe2, 0xec);
/// Text between ink and muted (deaths, small units).
pub const SOFT: Color = Color::srgb_u8(0xc9, 0xcd, 0xe0);
/// Cherry red: the primary action and selection marks.
pub const ACCENT: Color = Color::srgb_u8(0xff, 0x3d, 0x55);
/// Violet: Classic's accent, where cherry is Time Attack's (m5 A2, `ui::mode`).
pub const CLASSIC: Color = Color::srgb_u8(0x8f, 0x7b, 0xff);
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

/// Light glass of secondary buttons. The artboard's `rgba(255,255,255,.12)` renders at
/// `white(0.0253)` with Bevy's linear blending, which is 1.47:1 against the ground; raised so the
/// button's surface clears WCAG 1.4.11's 3:1 (3.05:1 over the lightest measured gameplay dim,
/// 3.58:1 over the ground). A deliberate departure from the artboard for #1476, for the user's
/// batch review.
pub const SECONDARY: Color = Color::srgba(1.0, 1.0, 1.0, 0.14);
/// Mod pills: the artboard's `rgba(255,255,255,.16)`, raised like [`SECONDARY`] so the pill
/// clears 3:1 (3.34:1 over the gameplay dim, 3.95:1 over the ground; #1476).
pub const CHIP: Color = Color::srgba(1.0, 1.0, 1.0, 0.16);
/// The on/off switch button's surface (`ButtonKind::Switch`, [`switch`] in `widgets.rs`). The
/// artboard's `rgba(255,255,255,.06)` was `white(0.06)` — about 0.010 alpha in linear light,
/// 1.15 to 1.19:1 against the grounds — far below WCAG 1.4.11's 3:1 (the nominal 0.06 alone is
/// only 2.1:1 over the ground). Raised to [`SECONDARY`]'s alpha so the control's own boundary
/// clears it (3.05:1 over the lightest gameplay dim, 3.58:1 over the ground; #1492, #1476). A
/// deliberate departure from the artboard for the user's batch review.
pub const SWITCH: Color = Color::srgba(1.0, 1.0, 1.0, 0.14);

/// A leaderboard row (`Leaderboard.dc.html`): `rgba(12,14,22,.62)`, as [`dark`]`(.., 0.62)` (#713).
#[allow(dead_code, reason = "other players' rows: the Global scope (#484)")]
pub const ROW: Color = Color::srgba(12.0 / 255.0, 14.0 / 255.0, 22.0 / 255.0, 0.8811);
/// Your own leaderboard row: `rgba(255,255,255,.22)`, as [`white`]`(0.22)`. Kept at the
/// artboard's value: it is a supplemental highlight, not the only cue (your row is wider, pops
/// and carries your name and rank), and raising it to 3:1 would drop the `BEHIND` pace text on
/// the row to 2.16:1, below the 3:1 large-text floor. WCAG 1.4.11's non-text requirement is met
/// by those non-colour cues; see #1476.
pub const ROW_YOU: Color = Color::srgba(1.0, 1.0, 1.0, 0.0618);
/// One of your past runs on the Local leaderboard: `rgba(255,255,255,.07)`, as [`white`]`(0.07)`.
pub const ROW_PAST: Color = Color::srgba(1.0, 1.0, 1.0, 0.0125);
/// The name on someone else's leaderboard row.
pub const ROW_NAME: Color = Color::srgb_u8(0xe3, 0xe5, 0xee);
/// The three dots between a pinned leaderboard row and the rest.
pub const GAP_DOT: Color = Color::srgb_u8(0x6a, 0x70, 0x88);
/// A player with no avatar: the square and the mark on it. The square clears 3:1 against the
/// row and the measured grounds (3.19:1 at the lightest), and the mark clears 3:1 against the
/// square (3.10:1); both were far below before #1476.
pub const AVATAR_EMPTY: Color = Color::srgb_u8(0x6a, 0x70, 0x88);
pub const AVATAR_MARK: Color = Color::srgb_u8(0xc9, 0xcd, 0xe0);
/// An input display arrow that is not held (`RunInfo.dc.html`): the artboard's
/// `rgba(255,255,255,.22)`, raised like [`SECONDARY`] so the idle state clears 3:1 (3.63:1 over
/// the gameplay dim, 4.31:1 over the ground; #1476).
pub const KEY_IDLE: Color = Color::srgba(1.0, 1.0, 1.0, 0.18);
/// An input display button label that is not held: the artboard's `rgba(255,255,255,.28)`,
/// raised like [`KEY_IDLE`] so the idle label clears 3:1 and reads as normal text (4.68:1 over
/// the ground; #1476).
pub const BUTTON_IDLE: Color = Color::srgba(1.0, 1.0, 1.0, 0.2);
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

/// The mod select sheet (`ModSelect.dc.html`): the dim over game select (`rgba(5,6,10,.55)`), the
/// sheet's glass from top (`rgba(14,16,24,.94)`) to bottom (`rgba(8,9,14,.99)`), as [`dark`].
pub const SHEET_DIM: Color = Color::srgba(5.0 / 255.0, 6.0 / 255.0, 10.0 / 255.0, 0.8274);
pub const SHEET_TOP: Color = Color::srgba(14.0 / 255.0, 16.0 / 255.0, 24.0 / 255.0, 0.9979);
pub const SHEET_BOTTOM: Color = Color::srgba(8.0 / 255.0, 9.0 / 255.0, 14.0 / 255.0, 1.0);
/// The mod select columns' colours: Challenge red, Visual blue (the Ranked pill's), Practice amber.
pub const MOD_CHALLENGE: Color = Color::srgb_u8(0xff, 0x60, 0x70);
pub const MOD_VISUAL: Color = RANKED;
pub const MOD_PRACTICE: Color = AMBER;

/// Results (`Results169.dc.html`): the line between split rows (`rgba(255,255,255,.07)`), the one
/// over the sum of best (`.22`), a chart's base line (`.18`) and the deaths-per-stage bars (`.38`,
/// the stage with the most deaths `.85`). White over dark has no single Bevy alpha (#627), so
/// these are the artboard's whites mixed over the ground in sRGB, as the browser blends them.
///
/// `RULE`, `RULE_STRONG` and `AXIS` are separators and chart axes only: the split values and
/// bar lengths beside them carry the information, so WCAG 2.2 1.4.11's 3:1 non-text requirement
/// does not apply (documented, not forced bright, in #1476). `BAR` is the value and passes 3:1
/// (3.54:1 over the ground); its `TRACK` on the card is a redundant remainder, documented the
/// same way.
pub const RULE: Color = Color::srgb_u8(0x1c, 0x1e, 0x24);
pub const RULE_STRONG: Color = Color::srgb_u8(0x41, 0x42, 0x48);
pub const AXIS: Color = Color::srgb_u8(0x37, 0x39, 0x3e);
pub const BAR: Color = Color::srgb_u8(0x68, 0x69, 0x6d);
pub const BAR_PEAK: Color = Color::srgb_u8(0xda, 0xdb, 0xdc);
/// The results heatmap's dots: `rgba(255,94,77,a)`, a warmer red than [`BEHIND`] (#705).
pub const HEAT: Color = Color::srgb_u8(0xff, 0x5e, 0x4d);
/// The heatmap's room picture: `filter: saturate(.4) brightness(.7)` (`ui::filter`).
pub const HEAT_ROOM_SATURATE: f32 = 0.4;
pub const HEAT_ROOM_BRIGHTNESS: f32 = 0.7;

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
        assert_eq!(HEAT.to_srgba().to_hex(), "#FF5E4D");
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

    /// The artboard alphas that are still exact: the decorative row shades and the held glows.
    /// `SECONDARY`, `CHIP`, `KEY_IDLE` and `BUTTON_IDLE` were raised for #1476 and are checked by
    /// [`meaningful_surfaces_clear_non_text_contrast`] instead.
    #[test]
    fn white_glass_matches_the_browser_over_the_backdrop() {
        for (token, a) in [
            (ROW_YOU, 0.22),
            (ROW_PAST, 0.07),
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
        for (token, a) in [(GLASS, 0.72), (ROW, 0.62), (SHEET_DIM, 0.55), (SHEET_TOP, 0.94)] {
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

    /// Relative luminance of a linear colour (WCAG 2.2 definition; Bevy's linear channels are
    /// already linearised sRGB).
    fn luminance(c: Color) -> f32 {
        let l = c.to_linear();
        0.2126 * l.red + 0.7152 * l.green + 0.0722 * l.blue
    }

    /// What a translucent `top` renders to over `under`, with Bevy's linear blending.
    fn rendered_over(top: Color, under: Color) -> Color {
        let (t, u) = (top.to_linear(), under.to_linear());
        let a = t.alpha;
        let mix = |x: f32, y: f32| a * x + (1.0 - a) * y;
        Color::from(LinearRgba::rgb(
            mix(t.red, u.red),
            mix(t.green, u.green),
            mix(t.blue, u.blue),
        ))
    }

    /// WCAG contrast ratio of two rendered colours.
    fn contrast(a: Color, b: Color) -> f32 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    /// The grounds the screens actually put these surfaces on: the m4 palette's ground, the
    /// gameplay and select dims measured in the screenshots (`BACKDROP`'s doc), the card and the
    /// select list's panel.
    const SURFACE_GROUNDS: [Color; 5] = [
        GROUND,
        BACKDROP,
        Color::srgb_u8(0x11, 0x0f, 0x21),
        Color::srgb_u8(0x18, 0x20, 0x4a),
        CARD_SOLID,
    ];

    /// WCAG 2.2 1.4.11: the interactive and meaningful surfaces clear 3:1 against every ground
    /// they are drawn on (#1476). `ROW_YOU` is exempted in its doc: it is a supplemental
    /// highlight with non-colour cues, and raising it would break its pace text.
    #[test]
    fn meaningful_surfaces_clear_non_text_contrast() {
        for (name, token) in [
            ("SECONDARY", SECONDARY),
            ("CHIP", CHIP),
            ("SWITCH", SWITCH),
            ("KEY_IDLE", KEY_IDLE),
            ("BUTTON_IDLE", BUTTON_IDLE),
        ] {
            for ground in SURFACE_GROUNDS {
                let ratio = contrast(rendered_over(token, ground), ground);
                assert!(ratio >= 3.0, "{name} on {ground:?}: {ratio:.2}");
            }
        }
        // The avatar's square and the mark on it each carry information.
        for ground in SURFACE_GROUNDS {
            let ratio = contrast(AVATAR_EMPTY, ground);
            assert!(ratio >= 3.0, "AVATAR_EMPTY on {ground:?}: {ratio:.2}");
        }
        assert!(
            contrast(AVATAR_MARK, AVATAR_EMPTY) >= 3.0,
            "the mark on the avatar: {:.2}",
            contrast(AVATAR_MARK, AVATAR_EMPTY)
        );
    }

    /// #1492: the switch's knob is the state indicator, so it must clear 3:1 against its track in
    /// both states, not only the surface against the ground. The track is drawn over [`SWITCH`],
    /// which is drawn over each ground; the knob is ink in both states.
    #[test]
    fn the_switch_knob_clears_non_text_contrast_on_its_track() {
        use crate::ui::widgets::{knob_fill, track_fill};
        for ground in SURFACE_GROUNDS {
            let surface = rendered_over(SWITCH, ground);
            for on in [false, true] {
                let track = rendered_over(track_fill(on), surface);
                let ratio = contrast(knob_fill(), track);
                assert!(
                    ratio >= 3.0,
                    "switch knob on the {} track over {ground:?}: {ratio:.2}",
                    if on { "on" } else { "off" }
                );
            }
        }
    }

    /// The pairs documented as decorative stay below 3:1 by design; this pins why they are not
    /// raised (they carry no information) and that the value graphics do pass.
    #[test]
    fn decorative_lines_and_the_bar_are_pinned() {
        for (name, token) in [("RULE", RULE), ("RULE_STRONG", RULE_STRONG), ("AXIS", AXIS), ("TRACK", TRACK)] {
            assert!(
                contrast(token, GROUND) < 3.0,
                "{name} is documented as decorative; raise the doc if it changes"
            );
        }
        assert!(contrast(BAR, GROUND) >= 3.0, "the bar's value clears 3:1");
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
