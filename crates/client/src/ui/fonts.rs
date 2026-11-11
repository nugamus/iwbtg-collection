//! The bundled fonts (SIL OFL 1.1; the licences sit next to the files in `assets/fonts/`) and the
//! type styles the mockups use. Fonts are compiled into the binary, so native and wasm32 load
//! them the same way, with no asset folder.

use bevy::prelude::*;
use bevy::text::{FontFeatureTag, FontFeatures, FontSize, FontSource, FontStyle, FontWeight, LetterSpacing};

/// Every face we ship. Bevy registers each under its embedded family name, so text asks for a
/// family and a weight and the font database picks the face.
const FACES: [&[u8]; 11] = [
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
    // Symbols Barlow lacks (★ ☆ ▾ ✓, arrows, geometric shapes): a subset of Noto Sans Symbols 2.
    include_bytes!("../../assets/fonts/NotoSansSymbols2-subset.ttf"),
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
    /// The family list text asks for: the family, then the symbol fallback.
    fn names(self) -> &'static str {
        match self {
            Family::Barlow => "Barlow, 'Noto Sans Symbols 2'",
            Family::Condensed => "'Barlow Condensed', 'Noto Sans Symbols 2'",
            Family::Pixel => "'Pixelify Sans', 'Noto Sans Symbols 2'",
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

    #[test]
    fn every_face_is_a_font_file() {
        for face in FACES {
            assert_eq!(&face[..4], &[0, 1, 0, 0], "TrueType header");
        }
    }
}
