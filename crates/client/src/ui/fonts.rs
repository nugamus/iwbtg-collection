//! The bundled fonts (SIL OFL 1.1; the licences sit next to the files in `assets/fonts/`) and the
//! type styles the mockups use. Fonts are compiled into the binary, so native and wasm32 load
//! them the same way, with no asset folder.

use bevy::prelude::*;
use bevy::text::{FontFeatureTag, FontFeatures, FontSize, FontSource, FontStyle, FontWeight, LetterSpacing};

/// Every face we ship. Bevy registers each under its embedded family name, so text asks for a
/// family and a weight and the font database picks the face.
const FACES: [&[u8]; 16] = [
    include_bytes!("../../assets/fonts/Barlow-Regular.ttf"),
    include_bytes!("../../assets/fonts/Barlow-Medium.ttf"),
    include_bytes!("../../assets/fonts/Barlow-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/Barlow-Bold.ttf"),
    include_bytes!("../../assets/fonts/BarlowCondensed-Medium.ttf"),
    include_bytes!("../../assets/fonts/BarlowCondensed-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/BarlowCondensed-Bold.ttf"),
    include_bytes!("../../assets/fonts/BarlowCondensed-SemiBoldItalic.ttf"),
    include_bytes!("../../assets/fonts/BarlowCondensed-BoldItalic.ttf"),
    include_bytes!("../../assets/fonts/PixelifySans.ttf"),
    // Symbols Barlow lacks (★ ☆ ▾ ▸ ✓, heavy arrows, geometric shapes): a subset of Noto Sans
    // Symbols 2.
    include_bytes!("../../assets/fonts/NotoSansSymbols2-subset.ttf"),
    // The basic arrows (U+2190..U+2199: ← ↑ → ↓ ↔ ↕ ↖ ↗ ↘ ↙), which Symbols 2 lacks: a subset of
    // Inter in the four weights the UI sets, so an arrow in bold text is as heavy as the digits
    // around it (#720); 3 KB each (`make-arrows-subset.py`).
    include_bytes!("../../assets/fonts/InterArrows-Regular.ttf"),
    include_bytes!("../../assets/fonts/InterArrows-Medium.ttf"),
    include_bytes!("../../assets/fonts/InterArrows-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/InterArrows-Bold.ttf"),
    // Japanese kana, punctuation, full-width forms and the 2965 JIS level 1 kanji, one weight
    // (Medium) for every weight: a subset of Noto Sans JP, about 1 MB.
    include_bytes!("../../assets/fonts/NotoSansJP-subset.ttf"),
];

/// Keeps the font assets alive.
#[derive(Resource)]
#[allow(dead_code, reason = "held only so the fonts stay loaded")]
struct BundledFonts(Vec<Handle<Font>>);

pub(super) fn plugin(app: &mut App) {
    let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
    let handles = FACES
        .iter()
        .map(|bytes| fonts.add(Font::from_bytes(bytes.to_vec())))
        .collect();
    app.insert_resource(BundledFonts(handles));
}

/// A font family we ship.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Barlow, 400 to 700: all UI text.
    Barlow,
    /// Barlow Condensed, 500 to 700 and italic 600/700: titles, ranks, stage names, mod codes.
    Condensed,
    /// Pixelify Sans: only inside the game-space pause box.
    Pixel,
}

impl Family {
    /// The family list text asks for: the family, then the symbol, arrow and Japanese fallbacks.
    fn names(self) -> &'static str {
        match self {
            Family::Barlow => "Barlow, 'Noto Sans Symbols 2', Inter, 'Noto Sans JP'",
            Family::Condensed => "'Barlow Condensed', 'Noto Sans Symbols 2', Inter, 'Noto Sans JP'",
            Family::Pixel => "'Pixelify Sans', 'Noto Sans Symbols 2', Inter, 'Noto Sans JP'",
        }
    }
}

/// One text style: family, size in logical px, weight, italic, letter spacing in em and
/// whether the text is set in capitals. Tabular numerals are always on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Type {
    pub family: Family,
    pub size: f32,
    pub weight: u16,
    pub italic: bool,
    pub tracking: f32,
    pub caps: bool,
}

impl Type {
    /// Barlow at `size` and `weight`.
    pub const fn barlow(size: f32, weight: u16) -> Type {
        Type {
            family: Family::Barlow,
            size,
            weight,
            italic: false,
            tracking: 0.0,
            caps: false,
        }
    }

    /// Barlow Condensed at `size` and `weight`.
    pub const fn condensed(size: f32, weight: u16) -> Type {
        Type {
            family: Family::Condensed,
            ..Type::barlow(size, weight)
        }
    }

    /// The small capital labels above values (`MUSIC PREVIEW`, `STAGES`): 11 px, 700, 0.14 em.
    pub const LABEL: Type = Type {
        tracking: 0.14,
        caps: true,
        ..Type::barlow(11.0, 700)
    };
    /// A stat value: 22 px, 600.
    pub const STAT: Type = Type::barlow(22.0, 600);
    /// Body text in lists: 17 px, 500.
    pub const BODY: Type = Type::barlow(17.0, 500);
    /// Button text: 17 px, 600.
    pub const BUTTON: Type = Type::barlow(17.0, 600);
    /// The primary button: 19 px, 700.
    pub const PRIMARY: Type = Type::barlow(19.0, 700);
    /// Star pill text: 14 px, 700.
    pub const STARS: Type = Type::barlow(14.0, 700);
    /// Pill words in capitals (`RANKED`, `UNRANKED`): 12 px, 700, 0.12 em.
    pub const PILL_CAPS: Type = Type {
        tracking: 0.12,
        caps: true,
        ..Type::barlow(12.0, 700)
    };
    /// Mod codes: Barlow Condensed 15 px, 700.
    pub const MOD_CODE: Type = Type::condensed(15.0, 700);
    /// The game title on the wedge: Barlow Condensed 76 px, 700 italic.
    pub const TITLE: Type = Type {
        italic: true,
        tracking: -0.01,
        ..Type::condensed(76.0, 700)
    };

    /// Italic.
    pub const fn italic(self) -> Type {
        Type { italic: true, ..self }
    }

    /// The Bevy font settings for this style.
    pub fn font(self) -> TextFont {
        TextFont {
            font: FontSource::families(self.family.names()),
            font_size: FontSize::Px(self.size),
            weight: FontWeight(self.weight),
            style: if self.italic {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            },
            font_features: FontFeatures::from([FontFeatureTag::TABULAR_FIGURES]),
            ..default()
        }
    }

    /// The letter spacing for this style.
    pub fn spacing(self) -> LetterSpacing {
        LetterSpacing::Px(self.tracking * self.size)
    }

    /// `text` as this style sets it (capitals when `caps`).
    pub fn set(self, text: &str) -> String {
        if self.caps {
            text.to_uppercase()
        } else {
            text.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_styles_set_capitals_and_track_in_em() {
        assert_eq!(Type::LABEL.set("Music preview"), "MUSIC PREVIEW");
        assert_eq!(Type::BODY.set("Hard · Any%"), "Hard · Any%");
        assert!(matches!(Type::LABEL.spacing(), LetterSpacing::Px(px) if (px - 1.54).abs() < 1e-4));
    }

    /// The family `ch` is drawn with when text asks for `family`: as the font database falls
    /// back, the first family in the list that has a face mapping `ch`.
    fn family_for(family: Family, ch: char) -> Option<String> {
        use skrifa::{FontRef, MetadataProvider, string::StringId};
        let faces: Vec<(String, FontRef)> = FACES
            .iter()
            .map(|bytes| {
                let font = FontRef::new(bytes).expect("font file");
                let name = [StringId::TYPOGRAPHIC_FAMILY_NAME, StringId::FAMILY_NAME]
                    .into_iter()
                    .find_map(|id| font.localized_strings(id).english_or_first())
                    .expect("family name")
                    .to_string();
                (name, font)
            })
            .collect();
        family
            .names()
            .split(',')
            .map(|name| name.trim().trim_matches('\''))
            .find_map(|name| {
                faces
                    .iter()
                    .any(|(face, font)| face == name && font.charmap().map(ch).is_some())
                    .then(|| name.to_string())
            })
    }

    #[test]
    fn arrows_and_symbols_fall_back_to_a_face_that_has_them() {
        for family in [Family::Barlow, Family::Condensed, Family::Pixel] {
            // The results death timeline label (#41) sets an arrow in running text.
            for ch in "0:00 → 38:12.46".chars() {
                assert!(
                    family_for(family, ch).is_some(),
                    "{family:?} has no face for {ch:?}"
                );
            }
            for ch in ['←', '↑', '→', '↓'] {
                assert_eq!(family_for(family, ch).as_deref(), Some("Inter"), "{ch:?}");
            }
            for ch in ['★', '☆', '▾', '▸', '✓'] {
                assert_eq!(
                    family_for(family, ch).as_deref(),
                    Some("Noto Sans Symbols 2"),
                    "{ch:?}"
                );
            }
            assert_eq!(family_for(family, 'ア').as_deref(), Some("Noto Sans JP"));
            // The ellipsis that ends cut text (widgets::ellipsis_text, #636).
            assert!(
                family_for(family, '…').is_some(),
                "{family:?} has no face for '…'"
            );
        }
        assert_eq!(family_for(Family::Barlow, '3').as_deref(), Some("Barlow"));
        assert_eq!(family_for(Family::Barlow, '…').as_deref(), Some("Barlow"));
        assert_eq!(
            family_for(Family::Condensed, '…').as_deref(),
            Some("Barlow Condensed")
        );
    }

    /// Arrows come in every weight the UI sets, so `→` in 700 text is drawn bold (#720).
    #[test]
    fn arrows_come_in_each_ui_weight() {
        use skrifa::{FontRef, MetadataProvider, string::StringId};
        let mut weights: Vec<u16> = FACES
            .iter()
            .map(|bytes| FontRef::new(bytes).expect("font file"))
            .filter(|font| {
                font.localized_strings(StringId::FAMILY_NAME)
                    .english_or_first()
                    .is_some_and(|name| name.to_string().starts_with("Inter"))
            })
            .inspect(|font| {
                for ch in ['←', '↑', '→', '↓', '↔', '↕', '↖', '↗', '↘', '↙'] {
                    assert!(font.charmap().map(ch).is_some(), "{ch:?}");
                }
            })
            .map(|font| font.attributes().weight.value() as u16)
            .collect();
        weights.sort_unstable();
        assert_eq!(weights, [400, 500, 600, 700]);
    }

    /// The fallback faces have Barlow's ascent and descent per em, so a `▾`, `★`, arrow or kana
    /// in a Barlow line leaves its baseline where Barlow puts it (#546; the layout takes a line's
    /// tallest ascent and descent). `assets/fonts/fallback_metrics.py` sets them.
    #[test]
    fn fallback_faces_keep_barlow_s_baseline() {
        use skrifa::{
            FontRef, MetadataProvider,
            instance::{LocationRef, Size},
            string::StringId,
        };
        let per_em = |font: &FontRef| {
            let m = font.metrics(Size::unscaled(), LocationRef::default());
            let em = f32::from(m.units_per_em);
            (m.ascent / em, m.descent / em, m.leading / em)
        };
        let faces: Vec<(String, (f32, f32, f32))> = FACES
            .iter()
            .map(|bytes| {
                let font = FontRef::new(bytes).expect("font file");
                let name = font
                    .localized_strings(StringId::FAMILY_NAME)
                    .english_or_first()
                    .expect("family name")
                    .to_string();
                (name, per_em(&font))
            })
            .collect();
        let barlow = faces.iter().find(|(name, _)| name == "Barlow").expect("Barlow").1;
        let mut fallbacks = 0;
        for (name, metrics) in &faces {
            if ["Noto Sans Symbols 2", "Inter", "Noto Sans JP"]
                .iter()
                .any(|family| name.starts_with(family))
            {
                fallbacks += 1;
                let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
                assert!(
                    close(metrics.0, barlow.0) && close(metrics.1, barlow.1) && close(metrics.2, 0.0),
                    "{name}: {metrics:?}, Barlow {barlow:?}"
                );
            }
        }
        assert_eq!(fallbacks, 6, "Symbols 2, four Inter weights, JP");
    }

    /// The baseline of one line of Barlow 14/600 text, laid out with every bundled face as Bevy
    /// lays out UI text (font size relative line height 1.15).
    fn barlow_baseline(text: &str) -> f32 {
        use parley::{FontContext, FontFamily, FontWeight, LayoutContext, LineHeight, StyleProperty};
        let mut fonts = FontContext::new();
        for bytes in FACES {
            fonts.collection.register_fonts(
                parley::fontique::Blob::new(std::sync::Arc::new(bytes.to_vec())),
                None,
            );
        }
        let mut layouts = LayoutContext::<[u8; 4]>::new();
        let mut builder = layouts.ranged_builder(&mut fonts, text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(FontFamily::Source(
            Family::Barlow.names().into(),
        )));
        builder.push_default(StyleProperty::FontSize(14.0));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(600.0)));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(1.15)));
        let mut layout = builder.build(text);
        layout.break_all_lines(None);
        layout.lines().next().expect("one line").metrics().baseline
    }

    /// `Stars ▾`, `★ 6.84`, `→` and kana sit on the same baseline as plain Barlow (#546).
    #[test]
    fn a_fallback_glyph_does_not_move_the_baseline() {
        let plain = barlow_baseline("Pop");
        for text in ["Stars ▾", "★ 6.84", "39:40.12 → 38:12.46", "れおちゅー", "▸"] {
            let baseline = barlow_baseline(text);
            assert!(
                (baseline - plain).abs() < 0.01,
                "{text:?}: baseline {baseline}, Barlow {plain}"
            );
        }
    }

    #[test]
    fn every_face_is_a_font_file() {
        for face in FACES {
            assert_eq!(&face[..4], &[0, 1, 0, 0], "TrueType header");
        }
    }
}
