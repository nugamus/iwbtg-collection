//! Sprite strips and their sidecars (docs/plans/sim-core.md 8.1, 8.2 step 2; docs/format/v0.md,
//! Assets).
//!
//! `sprites/<id>.png` is a horizontal strip of `frames` frames; `sprites/<id>.toml` is the
//! sidecar. Image pixels never reach the sim: only the masks built here do.
//!
//! Which frames share a mask (CLONE_SPEC sprites and collision, Confirmed manual 202_00): without
//! `separate_masks` one mask combines all frames; with it each frame has its own. A manual box is
//! one box for every frame, so only a precise mask can still differ per frame then (cherries).

use std::io::Cursor;
use std::ops::Range;

use iwbtg_sim::def::{IRect, Mask, MaskId, MaskSet, SpriteDef};
use serde::Deserialize;
use toml::Spanned;

use crate::diag::Diagnostic;
use crate::manifest::toml_error;
use crate::masks::{CollisionShape, build_union_mask, empty_mask};
use crate::number::Number;

/// Largest image side (docs/format/v0.md, Assets).
pub const MAX_IMAGE_SIDE: u32 = 4096;

/// Largest frame side the loader accepts at all, even in `LoadMode::Trace` (a mask of this size
/// still fits `MAX_MASK_WORDS`).
pub const MAX_FRAME_SIDE: u32 = 16_384;

/// Most bytes the PNG decoder may allocate for one strip.
const PNG_DECODE_LIMIT: usize = 256 * 1024 * 1024;

/// The `collision` value of a sidecar.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Collision {
    /// Per pixel.
    #[default]
    Precise,
    /// The box.
    Rect,
    /// A disk inside the box.
    Disk,
    /// A diamond inside the box.
    Diamond,
    /// No mask at all (GM's empty mask: an inverted box or a fully transparent sprite).
    None,
}

impl Collision {
    /// The mask shape, or `None` for no mask.
    pub const fn shape(self) -> Option<CollisionShape> {
        match self {
            Collision::Precise => Some(CollisionShape::Precise),
            Collision::Rect => Some(CollisionShape::Rect),
            Collision::Disk => Some(CollisionShape::Disk),
            Collision::Diamond => Some(CollisionShape::Diamond),
            Collision::None => None,
        }
    }
}

/// A sprite's sidecar TOML (frames, origin, `collision`, `bbox`, `alpha_tolerance`,
/// `separate_masks`).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SpriteSidecar {
    /// Number of frames in the strip; 0 for a sprite without images (no PNG needed).
    pub frames: u16,
    /// Frame size in pixels.
    pub frame_size: (u32, u32),
    /// Origin in sprite space; may lie outside the frame.
    pub origin: (i32, i32),
    /// Mask shape.
    pub collision: Collision,
    /// Manual bounding box, inclusive; `None` is GM's automatic box.
    pub bbox: Option<IRect>,
    /// Alpha tolerance of a precise mask (C5 rule in `masks`).
    pub alpha_tolerance: u8,
    /// GM "separate collision masks": one mask per frame instead of one combined mask.
    pub separate_masks: bool,
    /// Another sprite whose masks this sprite uses (v0 `mask`).
    pub mask: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSidecar {
    frames: Spanned<u16>,
    frame_size: Spanned<[u32; 2]>,
    origin: Spanned<[i32; 2]>,
    #[serde(default)]
    collision: Option<Spanned<RawCollision>>,
    #[serde(default)]
    bbox: Option<Spanned<[i64; 4]>>,
    #[serde(default)]
    alpha_tolerance: Option<Spanned<Number>>,
    #[serde(default)]
    separate_masks: Option<bool>,
    #[serde(default)]
    mask: Option<Spanned<String>>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum RawCollision {
    Precise,
    Rect,
    Disk,
    Diamond,
    None,
}

/// Where the sidecar's keys are, for later diagnostics.
#[derive(Clone, Debug, Default)]
pub struct SidecarSpans {
    /// `frames`.
    pub frames: Range<usize>,
    /// `bbox`, or `frames` when absent.
    pub bbox: Range<usize>,
    /// `mask`, when present.
    pub mask: Option<Range<usize>>,
}

/// Parses a sidecar. `path` and `text` are for diagnostics. Checks the ranges that do not need
/// the image: frame size at least 1, a manual box inside the frame (or inverted, which is empty),
/// tolerance 0..=255.
pub fn parse_sidecar(path: &str, text: &str) -> Result<(SpriteSidecar, SidecarSpans), Diagnostic> {
    let raw: RawSidecar = toml::from_str(text).map_err(|e| toml_error(path, text, &e))?;
    let err = |span: Range<usize>, msg: String| Diagnostic::error_at(path, text, span.start, msg);
    let [fw, fh] = *raw.frame_size.get_ref();
    if fw == 0 || fh == 0 || fw > MAX_FRAME_SIDE || fh > MAX_FRAME_SIDE {
        return Err(err(
            raw.frame_size.span(),
            format!("frame_size must be between 1 and {MAX_FRAME_SIDE} on each side"),
        ));
    }
    let tolerance = match &raw.alpha_tolerance {
        None => 0,
        Some(t) => {
            let v = t.get_ref().0;
            if !(0.0..=255.0).contains(&v) || v.fract() != 0.0 {
                return Err(err(
                    t.span(),
                    "alpha_tolerance must be an integer from 0 to 255".to_owned(),
                ));
            }
            // Checked just above: an integer in 0..=255.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let v = v as u8;
            v
        }
    };
    let collision = match raw.collision.as_ref().map(|c| *c.get_ref()) {
        None | Some(RawCollision::Precise) => Collision::Precise,
        Some(RawCollision::Rect) => Collision::Rect,
        Some(RawCollision::Disk) => Collision::Disk,
        Some(RawCollision::Diamond) => Collision::Diamond,
        Some(RawCollision::None) => Collision::None,
    };
    let bbox = match &raw.bbox {
        None => None,
        Some(b) => {
            let [left, top, right, bottom] = *b.get_ref();
            let inverted = left > right || top > bottom;
            let inside = left >= 0 && top >= 0 && right < i64::from(fw) && bottom < i64::from(fh);
            if !inverted && !inside {
                return Err(err(
                    b.span(),
                    format!("bbox [{left}, {top}, {right}, {bottom}] must lie inside the {fw} x {fh} frame"),
                ));
            }
            Some(IRect {
                left,
                top,
                right,
                bottom,
            })
        }
    };
    let spans = SidecarSpans {
        frames: raw.frames.span(),
        bbox: raw.bbox.as_ref().map_or(raw.frames.span(), Spanned::span),
        mask: raw.mask.as_ref().map(Spanned::span),
    };
    let [ox, oy] = *raw.origin.get_ref();
    Ok((
        SpriteSidecar {
            frames: *raw.frames.get_ref(),
            frame_size: (fw, fh),
            origin: (ox, oy),
            collision,
            bbox,
            alpha_tolerance: tolerance,
            separate_masks: raw.separate_masks.unwrap_or(false),
            mask: raw.mask.map(Spanned::into_inner),
        },
        spans,
    ))
}

/// Decodes a PNG strip, splits frames and builds the masks. Image pixels never reach the sim.
///
/// The returned [`SpriteDef`]'s `masks` index the returned `Vec<Mask>` from 0; the loader offsets
/// them when it appends the masks to `GameDef::masks`. A sprite with 0 frames needs no image
/// (`png` may be empty) and gets one empty mask. Errors name `sprites/<id>.png`.
pub fn load_sprite(
    id: &str,
    png: &[u8],
    sidecar: &SpriteSidecar,
) -> Result<(SpriteDef, Vec<Mask>), Diagnostic> {
    let png_path = format!("sprites/{id}.png");
    let def = |masks: MaskSet| SpriteDef {
        frames: sidecar.frames,
        size: sidecar.frame_size,
        origin: sidecar.origin,
        masks,
    };
    let (fw, fh) = sidecar.frame_size;
    if fw == 0 || fh == 0 || fw > MAX_FRAME_SIDE || fh > MAX_FRAME_SIDE {
        return Err(Diagnostic::file_error(
            &format!("sprites/{id}.toml"),
            format!("frame_size must be between 1 and {MAX_FRAME_SIDE} on each side"),
        ));
    }
    if sidecar.frames == 0 {
        return Ok((def(MaskSet::Shared(MaskId(0))), vec![empty_mask()]));
    }
    let Some(shape) = sidecar.collision.shape() else {
        return Ok((def(MaskSet::Shared(MaskId(0))), vec![empty_mask()]));
    };
    let (rgba, w, h) = decode_rgba(png).map_err(|e| Diagnostic::file_error(&png_path, e))?;
    let strip_w = u64::from(fw) * u64::from(sidecar.frames);
    if u64::from(w) != strip_w || h != fh {
        return Err(Diagnostic::file_error(
            &png_path,
            format!(
                "image is {w} x {h}, but the sidecar says {} frames of {fw} x {fh} ({strip_w} x {fh})",
                sidecar.frames
            ),
        ));
    }
    let frames = split_frames(&rgba, fw, fh, sidecar.frames);
    let all: Vec<&[u8]> = frames.iter().map(Vec::as_slice).collect();
    let tol = sidecar.alpha_tolerance;
    // One mask per frame only when frames can differ: separate masks, more than one frame, and
    // either a precise shape or an automatic box (each frame then has its own box).
    let per_frame = sidecar.separate_masks
        && sidecar.frames > 1
        && (shape == CollisionShape::Precise || sidecar.bbox.is_none());
    if per_frame {
        let masks: Vec<Mask> = all
            .iter()
            .map(|f| build_union_mask(&[f], fw, fh, shape, sidecar.bbox, tol))
            .collect();
        let ids = (0..masks.len())
            .map(|i| MaskId(u32::try_from(i).unwrap_or(u32::MAX)))
            .collect();
        Ok((def(MaskSet::PerFrame(ids)), masks))
    } else {
        let mask = build_union_mask(&all, fw, fh, shape, sidecar.bbox, tol);
        Ok((def(MaskSet::Shared(MaskId(0))), vec![mask]))
    }
}

/// Copies frame `i` of a strip out as its own `fw` x `fh` RGBA buffer.
fn split_frames(rgba: &[u8], fw: u32, fh: u32, frames: u16) -> Vec<Vec<u8>> {
    let row_bytes = usize::try_from(fw).unwrap_or(0) * 4;
    let strip_row = row_bytes * usize::from(frames);
    (0..usize::from(frames))
        .map(|f| {
            let mut out = Vec::with_capacity(row_bytes * usize::try_from(fh).unwrap_or(0));
            for y in 0..usize::try_from(fh).unwrap_or(0) {
                let start = y * strip_row + f * row_bytes;
                if let Some(row) = rgba.get(start..start + row_bytes) {
                    out.extend_from_slice(row);
                }
            }
            out
        })
        .collect()
}

/// Decodes any PNG to 8-bit RGBA, row-major. Returns `(pixels, width, height)`.
pub fn decode_rgba(png: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(png),
        png::Limits {
            bytes: PNG_DECODE_LIMIT,
        },
    );
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("not a readable PNG: {e}"))?;
    let (color, _) = reader.output_color_type();
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| "PNG is too large to decode".to_owned())?;
    let mut buf = vec![0; size];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("PNG data is damaged: {e}"))?;
    buf.truncate(info.buffer_size());
    let (w, h) = (info.width, info.height);
    let channels = match color {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        png::ColorType::Indexed => return Err("indexed PNG was not expanded".to_owned()),
    };
    let line = usize::try_from(w).unwrap_or(0) * channels;
    let mut out = Vec::with_capacity(usize::try_from(u64::from(w) * u64::from(h) * 4).unwrap_or(0));
    for row in buf
        .chunks(info.line_size.max(1))
        .take(usize::try_from(h).unwrap_or(0))
    {
        for px in row.get(..line).unwrap_or(row).chunks(channels) {
            match *px {
                [r, g, b, a] => out.extend_from_slice(&[r, g, b, a]),
                [r, g, b] => out.extend_from_slice(&[r, g, b, 255]),
                [l, a] => out.extend_from_slice(&[l, l, l, a]),
                [l] => out.extend_from_slice(&[l, l, l, 255]),
                _ => {}
            }
        }
    }
    Ok((out, w, h))
}

#[cfg(test)]
mod tests {
    use iwbtg_sim::def::MaskShape;

    use super::*;

    /// Encodes RGBA pixels as a PNG.
    pub(crate) fn png_rgba(w: u32, h: u32, px: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut wr = enc.write_header().unwrap();
            wr.write_image_data(px).unwrap();
        }
        out
    }

    /// Two 2 x 1 frames: frame 0 has its left pixel opaque, frame 1 its right pixel.
    fn two_frames() -> Vec<u8> {
        png_rgba(4, 1, &[0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255])
    }

    fn sidecar(separate: bool, bbox: Option<IRect>) -> SpriteSidecar {
        SpriteSidecar {
            frames: 2,
            frame_size: (2, 1),
            origin: (0, 0),
            collision: Collision::Precise,
            bbox,
            alpha_tolerance: 0,
            separate_masks: separate,
            mask: None,
        }
    }

    #[test]
    fn combined_and_separate_masks() {
        let png = two_frames();
        let (def, masks) = load_sprite("s", &png, &sidecar(false, None)).unwrap();
        assert_eq!(def.masks, MaskSet::Shared(MaskId(0)));
        assert_eq!(masks.len(), 1);
        assert_eq!(masks[0].bbox.right, 1);
        let (def, masks) = load_sprite("s", &png, &sidecar(true, None)).unwrap();
        assert_eq!(def.masks, MaskSet::PerFrame(vec![MaskId(0), MaskId(1)]));
        assert_eq!((masks[0].bbox.left, masks[1].bbox.left), (0, 1));
        // A manual box is shared, but a precise mask still differs per frame (cherries).
        let b = IRect {
            left: 0,
            top: 0,
            right: 1,
            bottom: 0,
        };
        let (def, masks) = load_sprite("s", &png, &sidecar(true, Some(b))).unwrap();
        assert!(matches!(def.masks, MaskSet::PerFrame(_)));
        assert_eq!(masks[0].bbox, masks[1].bbox);
        assert_ne!(masks[0], masks[1]);
    }

    #[test]
    fn zero_frames_and_no_collision_give_an_empty_mask() {
        let mut s = sidecar(false, None);
        s.frames = 0;
        let (def, masks) = load_sprite("s", &[], &s).unwrap();
        assert_eq!(def.frames, 0);
        assert_eq!(masks[0].shape, MaskShape::Empty);
        let mut s = sidecar(false, None);
        s.collision = Collision::None;
        let (_, masks) = load_sprite("s", &two_frames(), &s).unwrap();
        assert_eq!(masks[0].shape, MaskShape::Empty);
    }

    #[test]
    fn strip_size_must_match_the_sidecar() {
        let mut s = sidecar(false, None);
        s.frames = 3;
        let d = load_sprite("s", &two_frames(), &s).unwrap_err();
        assert_eq!(d.file, "sprites/s.png");
        assert!(d.message.contains("3 frames"), "{}", d.message);
        assert!(load_sprite("s", b"not a png", &sidecar(false, None)).is_err());
    }

    #[test]
    fn rgb_and_gray_pngs_are_opaque() {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, 1, 1);
            enc.set_color(png::ColorType::Grayscale);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&[7]).unwrap();
        }
        assert_eq!(decode_rgba(&out).unwrap(), (vec![7, 7, 7, 255], 1, 1));
    }

    #[test]
    fn sidecar_checks() {
        let ok =
            "frames = 1\nframe_size = [4, 4]\norigin = [0, 0]\ncollision = \"disk\"\nbbox = [0, 0, 3, 3]\n";
        let (s, _) = parse_sidecar("sprites/a.toml", ok).unwrap();
        assert_eq!(s.collision, Collision::Disk);
        let outside = "frames = 1\nframe_size = [4, 4]\norigin = [0, 0]\nbbox = [0, 0, 4, 3]\n";
        let d = parse_sidecar("sprites/a.toml", outside).unwrap_err();
        assert_eq!(d.line, 4);
        let inverted = "frames = 1\nframe_size = [4, 4]\norigin = [0, 0]\nbbox = [31, 31, 0, 0]\n";
        assert!(parse_sidecar("sprites/a.toml", inverted).is_ok());
        let bad_tol = "frames = 1\nframe_size = [4, 4]\norigin = [0, 0]\nalpha_tolerance = 256\n";
        assert!(parse_sidecar("sprites/a.toml", bad_tol).is_err());
        let unknown = "frames = 1\nframe_size = [4, 4]\norigin = [0, 0]\nsmooth = true\n";
        assert!(parse_sidecar("sprites/a.toml", unknown).is_err());
    }
}
