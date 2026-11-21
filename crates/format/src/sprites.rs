//! Sprite strips and their sidecars (docs/format/assets.md).
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
use crate::masks::{Collider, ColliderSet, CollisionShape, build_union_mask, empty_mask, hex_rows_to_bits};
use crate::number::Number;

/// Largest image side (docs/format/assets.md).
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
    /// Frames per row of the image grid, read row by row; 0 is one row of all frames (the strip).
    pub columns: u16,
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
    /// A baked collider, when the sidecar has a `[collider]` table.
    pub collider: Option<BakedCollider>,
}

/// A sprite sidecar's baked collider and the alpha tolerance it was baked with.
///
/// The loader validates the table against the masks it builds from the PNG alpha data and keeps
/// using those masks; making the baked collider the runtime shape is #1182 (D2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BakedCollider {
    /// The alpha tolerance the bake used. The table declares it explicitly.
    pub alpha_tolerance: u8,
    /// One collider for all frames, or one per frame.
    pub set: ColliderSet,
}

impl SpriteSidecar {
    /// The image grid as `(columns, rows)`: `columns` frames per row (all frames when unset).
    pub fn grid(&self) -> (u32, u32) {
        let frames = u32::from(self.frames).max(1);
        let cols = if self.columns == 0 {
            frames
        } else {
            u32::from(self.columns)
        };
        (cols, frames.div_ceil(cols))
    }

    /// The image size in pixels the sidecar asks for: the grid times the frame size.
    pub fn image_size(&self) -> (u64, u64) {
        let (cols, rows) = self.grid();
        (
            u64::from(self.frame_size.0) * u64::from(cols),
            u64::from(self.frame_size.1) * u64::from(rows),
        )
    }
}

#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct RawSidecar {
    #[cfg_attr(feature = "schema", schemars(with = "u16"))]
    frames: Spanned<u16>,
    #[cfg_attr(feature = "schema", schemars(with = "[u32; 2]"))]
    frame_size: Spanned<[u32; 2]>,
    #[cfg_attr(feature = "schema", schemars(with = "[i32; 2]"))]
    origin: Spanned<[i32; 2]>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<u16>"))]
    columns: Option<Spanned<u16>>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<RawCollision>"))]
    collision: Option<Spanned<RawCollision>>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<[i64; 4]>"))]
    bbox: Option<Spanned<[i64; 4]>>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<Number>"))]
    alpha_tolerance: Option<Spanned<Number>>,
    #[serde(default)]
    separate_masks: Option<bool>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<String>"))]
    mask: Option<Spanned<String>>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<RawCollider>"))]
    collider: Option<Spanned<RawCollider>>,
}

#[derive(Deserialize, Clone, Copy)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
enum RawCollision {
    Precise,
    Rect,
    Disk,
    Diamond,
    None,
}

/// The `[collider]` table: a canonical collider baked from the art.
///
/// It has either the shared keys (`shape` with `bbox`/`bits`) or one `frames` entry per frame,
/// never both. `alpha_tolerance` is required so the bake cannot silently use a different rule
/// from the sidecar's.
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct RawCollider {
    #[cfg_attr(feature = "schema", schemars(with = "Number"))]
    alpha_tolerance: Spanned<Number>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<RawColliderShape>"))]
    shape: Option<Spanned<RawColliderShape>>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<[i64; 4]>"))]
    bbox: Option<Spanned<[i64; 4]>>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<Vec<String>>"))]
    bits: Option<Spanned<Vec<String>>>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<Vec<RawColliderFrame>>"))]
    frames: Option<Spanned<Vec<RawColliderFrame>>>,
}

/// One frame of a per-frame `[collider]` table.
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct RawColliderFrame {
    #[cfg_attr(feature = "schema", schemars(with = "RawColliderShape"))]
    shape: RawColliderShape,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<[i64; 4]>"))]
    bbox: Option<[i64; 4]>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<Vec<String>>"))]
    bits: Option<Vec<String>>,
}

/// The baked shape: `none` (nothing collides), `rect` (the whole box) or `bits` (per pixel).
#[derive(Deserialize, Clone, Copy)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
enum RawColliderShape {
    None,
    Rect,
    Bits,
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
    let columns = match &raw.columns {
        None => 0,
        Some(c) => {
            let v = *c.get_ref();
            if v == 0 || v > *raw.frames.get_ref().max(&1) {
                return Err(err(
                    c.span(),
                    format!("columns must be between 1 and frames ({})", raw.frames.get_ref()),
                ));
            }
            v
        }
    };
    let tolerance = match &raw.alpha_tolerance {
        None => 0,
        Some(t) => parse_alpha_tolerance(path, text, t.span(), t.get_ref())?,
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
    let collider = match &raw.collider {
        None => None,
        Some(c) => Some(parse_collider(
            path,
            text,
            c.get_ref(),
            c.span(),
            tolerance,
            *raw.frames.get_ref(),
            (fw, fh),
        )?),
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
            columns,
            origin: (ox, oy),
            collision,
            bbox,
            alpha_tolerance: tolerance,
            separate_masks: raw.separate_masks.unwrap_or(false),
            mask: raw.mask.map(Spanned::into_inner),
            collider,
        },
        spans,
    ))
}

/// Parses an `alpha_tolerance` value (the sidecar's and the collider's share the rule).
fn parse_alpha_tolerance(
    path: &str,
    text: &str,
    span: Range<usize>,
    value: &Number,
) -> Result<u8, Diagnostic> {
    let v = value.0;
    if !(0.0..=255.0).contains(&v) || v.fract() != 0.0 {
        return Err(Diagnostic::error_at(
            path,
            text,
            span.start,
            "alpha_tolerance must be an integer from 0 to 255",
        ));
    }
    // Checked just above: an integer in 0..=255.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(v as u8)
}

/// The fields of one baked collider, shared or from a `frames` entry, with spans for diagnostics.
struct ColliderFields {
    shape: RawColliderShape,
    shape_span: Range<usize>,
    bbox: Option<[i64; 4]>,
    bbox_span: Range<usize>,
    bits: Option<Vec<String>>,
    bits_span: Range<usize>,
}

/// Parses and structurally validates a `[collider]` table. The table declares its own
/// `alpha_tolerance`, which must equal the sidecar's; it holds either the shared keys or one
/// `frames` entry per frame.
fn parse_collider(
    path: &str,
    text: &str,
    raw: &RawCollider,
    table: Range<usize>,
    sidecar_tolerance: u8,
    frames: u16,
    frame_size: (u32, u32),
) -> Result<BakedCollider, Diagnostic> {
    let tolerance = parse_alpha_tolerance(
        path,
        text,
        raw.alpha_tolerance.span(),
        raw.alpha_tolerance.get_ref(),
    )?;
    if tolerance != sidecar_tolerance {
        return Err(Diagnostic::error_at(
            path,
            text,
            raw.alpha_tolerance.span().start,
            format!(
                "collider alpha_tolerance {tolerance} differs from the sidecar's {sidecar_tolerance}; a baked collider uses the same alpha data"
            ),
        ));
    }
    let shared_span = raw.shape.as_ref().map_or(table.clone(), Spanned::span);
    let set = match &raw.frames {
        Some(raw_frames) => {
            if raw.shape.is_some() || raw.bbox.is_some() || raw.bits.is_some() {
                return Err(Diagnostic::error_at(
                    path,
                    text,
                    shared_span.start,
                    "a collider table has either `frames` or the shared `shape`, `bbox` and `bits`, not both",
                ));
            }
            let entries = raw_frames.get_ref();
            if entries.len() != usize::from(frames) {
                return Err(Diagnostic::error_at(
                    path,
                    text,
                    raw_frames.span().start,
                    format!(
                        "the collider has {} frame(s), but the sprite has {frames}",
                        entries.len()
                    ),
                ));
            }
            let span = raw_frames.span();
            let mut colliders = Vec::with_capacity(entries.len());
            for (i, entry) in entries.iter().enumerate() {
                let fields = ColliderFields {
                    shape: entry.shape,
                    shape_span: span.clone(),
                    bbox: entry.bbox,
                    bbox_span: span.clone(),
                    bits: entry.bits.clone(),
                    bits_span: span.clone(),
                };
                let collider = parse_collider_entry(path, text, &fields, frame_size).map_err(|mut d| {
                    d.message = format!("collider frame {i}: {}", d.message);
                    d
                })?;
                colliders.push(collider);
            }
            ColliderSet::PerFrame(colliders)
        }
        None => {
            let Some(shape) = raw.shape.as_ref() else {
                return Err(Diagnostic::error_at(
                    path,
                    text,
                    table.start,
                    "a collider table needs `shape`, or a `frames` array",
                ));
            };
            let fields = ColliderFields {
                shape: *shape.get_ref(),
                shape_span: shape.span(),
                bbox: raw.bbox.as_ref().map(|b| *b.get_ref()),
                bbox_span: raw.bbox.as_ref().map_or(shape.span(), Spanned::span),
                bits: raw.bits.as_ref().map(|b| b.get_ref().clone()),
                bits_span: raw.bits.as_ref().map_or(shape.span(), Spanned::span),
            };
            ColliderSet::Shared(parse_collider_entry(path, text, &fields, frame_size)?)
        }
    };
    set.validate(frame_size, frames)
        .map_err(|e| Diagnostic::error_at(path, text, table.start, format!("collider: {e}")))?;
    Ok(BakedCollider {
        alpha_tolerance: tolerance,
        set,
    })
}

/// Parses one `none`/`rect`/`bits` entry and checks its canonical form.
fn parse_collider_entry(
    path: &str,
    text: &str,
    fields: &ColliderFields,
    frame_size: (u32, u32),
) -> Result<Collider, Diagnostic> {
    let err = |span: &Range<usize>, msg: String| Diagnostic::error_at(path, text, span.start, msg);
    match fields.shape {
        RawColliderShape::None => {
            if fields.bbox.is_some() || fields.bits.is_some() {
                return Err(err(
                    &fields.shape_span,
                    "shape = \"none\" takes no `bbox` or `bits`".to_owned(),
                ));
            }
            Ok(Collider::None)
        }
        RawColliderShape::Rect => {
            if fields.bits.is_some() {
                return Err(err(
                    &fields.bits_span,
                    "shape = \"rect\" takes `bbox`, not `bits`".to_owned(),
                ));
            }
            let Some([left, top, right, bottom]) = fields.bbox else {
                return Err(err(
                    &fields.bbox_span,
                    "shape = \"rect\" needs a `bbox`".to_owned(),
                ));
            };
            let collider = Collider::Rect {
                bbox: IRect {
                    left,
                    top,
                    right,
                    bottom,
                },
            };
            collider
                .validate(frame_size)
                .map_err(|e| err(&fields.bbox_span, e.to_string()))?;
            Ok(collider)
        }
        RawColliderShape::Bits => {
            let Some([left, top, right, bottom]) = fields.bbox else {
                return Err(err(
                    &fields.bbox_span,
                    "shape = \"bits\" needs a `bbox`".to_owned(),
                ));
            };
            let Some(rows) = &fields.bits else {
                return Err(err(
                    &fields.bits_span,
                    "shape = \"bits\" needs `bits` rows".to_owned(),
                ));
            };
            let bbox = IRect {
                left,
                top,
                right,
                bottom,
            };
            // Check the box before its size drives the grid.
            Collider::Rect { bbox }
                .validate(frame_size)
                .map_err(|e| err(&fields.bbox_span, e.to_string()))?;
            let too_large = || err(&fields.bbox_span, "the collider bbox is too large".to_owned());
            let width = u32::try_from(right - left + 1).map_err(|_| too_large())?;
            let height = u32::try_from(bottom - top + 1).map_err(|_| too_large())?;
            let bits = hex_rows_to_bits(rows, width, height)
                .map_err(|e| err(&fields.bits_span, format!("bits: {e}")))?;
            let collider = Collider::Bits { bbox, bits };
            collider
                .validate(frame_size)
                .map_err(|e| err(&fields.bits_span, e.to_string()))?;
            Ok(collider)
        }
    }
}

/// Checks a baked collider's solidity against the masks the loader sampled from the art.
fn check_baked(stem: &str, baked: &BakedCollider, sampled: &ColliderSet) -> Result<(), Diagnostic> {
    let file = format!("{stem}.toml");
    match (&baked.set, sampled) {
        (ColliderSet::Shared(a), ColliderSet::Shared(b)) => {
            if a == b {
                Ok(())
            } else {
                Err(Diagnostic::file_error(
                    &file,
                    format!(
                        "the baked collider is `{a}`, but the PNG alpha data gives `{b}`; re-bake the collider"
                    ),
                ))
            }
        }
        (ColliderSet::PerFrame(a), ColliderSet::PerFrame(b)) => {
            if a.len() != b.len() {
                return Err(Diagnostic::file_error(
                    &file,
                    format!(
                        "the baked collider has {} frame(s), but the sprite has {}",
                        a.len(),
                        b.len()
                    ),
                ));
            }
            match a.iter().zip(b).position(|(x, y)| x != y) {
                None => Ok(()),
                Some(i) => Err(Diagnostic::file_error(
                    &file,
                    format!(
                        "the baked collider for frame {i} is `{}`, but the PNG alpha data gives `{}`; re-bake the collider",
                        a[i], b[i]
                    ),
                )),
            }
        }
        (ColliderSet::Shared(_), ColliderSet::PerFrame(_)) => Err(Diagnostic::file_error(
            &file,
            "the baked collider is shared, but `separate_masks` builds one mask per frame; use a `frames` array",
        )),
        (ColliderSet::PerFrame(_), ColliderSet::Shared(_)) => Err(Diagnostic::file_error(
            &file,
            "the baked collider has one entry per frame, but the sprite builds one shared mask; use the shared keys",
        )),
    }
}

/// The sprites of a package in the order of `GameDef::sprites` (by id, bundled engine pack
/// sprites keyed `engine:<pack>/<id>` among them): `(key, path of its PNG strip)`. A viewer uses
/// it to find the image of a `SpriteId`; `tests/load_objects.rs` pins the order against the loader.
pub fn sprite_pngs(files: &crate::files::PackageFiles) -> Vec<(String, String)> {
    let mut out: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for (path, _) in files.iter() {
        if let Some(stem) = path
            .strip_prefix("sprites/")
            .and_then(|s| s.strip_suffix(".toml"))
        {
            if !stem.contains('/') {
                out.insert(stem.to_owned(), format!("sprites/{stem}.png"));
            }
        } else if let Some((pack, id)) = crate::engine_pack::sprite_sidecar(path) {
            out.insert(
                format!("{}{pack}/{id}", crate::ids::ENGINE_PREFIX),
                format!("{}sprites/{id}.png", crate::engine_pack::pack_dir(pack)),
            );
        }
    }
    out.into_iter().collect()
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
    load_sprite_at(&format!("sprites/{id}"), png, sidecar)
}

/// [`load_sprite`] for a sprite whose files are `<stem>.png` and `<stem>.toml` (a bundled engine
/// pack sprite lives under `engine/<pack>/sprites/`).
pub fn load_sprite_at(
    stem: &str,
    png: &[u8],
    sidecar: &SpriteSidecar,
) -> Result<(SpriteDef, Vec<Mask>), Diagnostic> {
    let png_path = format!("{stem}.png");
    let def = |masks: MaskSet| SpriteDef {
        frames: sidecar.frames,
        size: sidecar.frame_size,
        origin: sidecar.origin,
        masks,
    };
    let (fw, fh) = sidecar.frame_size;
    if fw == 0 || fh == 0 || fw > MAX_FRAME_SIDE || fh > MAX_FRAME_SIDE {
        return Err(Diagnostic::file_error(
            &format!("{stem}.toml"),
            format!("frame_size must be between 1 and {MAX_FRAME_SIDE} on each side"),
        ));
    }
    if sidecar.frames == 0 || sidecar.collision.shape().is_none() {
        if let Some(baked) = &sidecar.collider {
            check_baked(stem, baked, &ColliderSet::Shared(Collider::None))?;
        }
        return Ok((def(MaskSet::Shared(MaskId(0))), vec![empty_mask()]));
    }
    let Some(shape) = sidecar.collision.shape() else {
        return Ok((def(MaskSet::Shared(MaskId(0))), vec![empty_mask()]));
    };
    let (rgba, w, h) = decode_rgba(png).map_err(|e| Diagnostic::file_error(&png_path, e))?;
    let (img_w, img_h) = sidecar.image_size();
    if u64::from(w) != img_w || u64::from(h) != img_h {
        let (cols, rows) = sidecar.grid();
        return Err(Diagnostic::file_error(
            &png_path,
            format!(
                "image is {w} x {h}, but the sidecar says {} frames of {fw} x {fh} in {cols} column(s) and {rows} row(s) ({img_w} x {img_h})",
                sidecar.frames
            ),
        ));
    }
    let frames = split_frames(&rgba, fw, fh, sidecar.frames, sidecar.grid().0);
    let all: Vec<&[u8]> = frames.iter().map(Vec::as_slice).collect();
    let tol = sidecar.alpha_tolerance;
    // One mask per frame only when frames can differ: separate masks, more than one frame, and
    // either a precise shape or an automatic box (each frame then has its own box).
    let per_frame = sidecar.separate_masks
        && sidecar.frames > 1
        && (shape == CollisionShape::Precise || sidecar.bbox.is_none());
    let (def, masks) = if per_frame {
        let masks: Vec<Mask> = all
            .iter()
            .map(|f| build_union_mask(&[f], fw, fh, shape, sidecar.bbox, tol))
            .collect();
        let ids = (0..masks.len())
            .map(|i| MaskId(u32::try_from(i).unwrap_or(u32::MAX)))
            .collect();
        (def(MaskSet::PerFrame(ids)), masks)
    } else {
        let mask = build_union_mask(&all, fw, fh, shape, sidecar.bbox, tol);
        (def(MaskSet::Shared(MaskId(0))), vec![mask])
    };
    if let Some(baked) = &sidecar.collider {
        let sampled = ColliderSet::from_masks(per_frame, &masks);
        check_baked(stem, baked, &sampled)?;
    }
    Ok((def, masks))
}

/// Copies each frame of the image grid out as its own `fw` x `fh` RGBA buffer; frame `f` is at
/// column `f % cols`, row `f / cols`.
fn split_frames(rgba: &[u8], fw: u32, fh: u32, frames: u16, cols: u32) -> Vec<Vec<u8>> {
    let (fw, fh, cols) = (
        usize::try_from(fw).unwrap_or(0),
        usize::try_from(fh).unwrap_or(0),
        usize::try_from(cols).unwrap_or(1).max(1),
    );
    let row_bytes = fw * 4;
    let image_row = row_bytes * cols;
    (0..usize::from(frames))
        .map(|f| {
            let (col, row) = (f % cols, f / cols);
            let mut out = Vec::with_capacity(row_bytes * fh);
            for y in 0..fh {
                let start = (row * fh + y) * image_row + col * row_bytes;
                if let Some(r) = rgba.get(start..start + row_bytes) {
                    out.extend_from_slice(r);
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
            columns: 0,
            origin: (0, 0),
            collision: Collision::Precise,
            bbox,
            alpha_tolerance: 0,
            separate_masks: separate,
            mask: None,
            collider: None,
        }
    }

    #[test]
    fn grid_of_rows_splits_like_a_strip() {
        // Three 2 x 1 frames in two columns: frame 2 is alone in row 1. Frame 0 opaque at its
        // left pixel, frame 1 at its right pixel, frame 2 at its right pixel.
        let o = [0, 0, 0, 255];
        let t = [0, 0, 0, 0];
        let px: Vec<u8> = [o, t, t, o, t, o, t, t].concat();
        let png = png_rgba(4, 2, &px);
        let mut sc = sidecar(true, None);
        sc.frames = 3;
        sc.columns = 2;
        assert_eq!(sc.image_size(), (4, 2));
        let (def, masks) = load_sprite("s", &png, &sc).unwrap();
        assert_eq!(
            def.masks,
            MaskSet::PerFrame(vec![MaskId(0), MaskId(1), MaskId(2)])
        );
        assert_eq!(
            (masks[0].bbox.left, masks[1].bbox.left, masks[2].bbox.left),
            (0, 1, 1)
        );
        // A strip-shaped image is wrong for a grid sidecar.
        assert!(load_sprite("s", &two_frames(), &sc).is_err());
    }

    #[test]
    fn columns_must_fit_the_frames() {
        let ok = "frames = 3
frame_size = [2, 1]
origin = [0, 0]
columns = 2
";
        assert_eq!(parse_sidecar("s.toml", ok).unwrap().0.columns, 2);
        for bad in ["columns = 0", "columns = 4"] {
            let t = format!(
                "frames = 3
frame_size = [2, 1]
origin = [0, 0]
{bad}
"
            );
            assert!(parse_sidecar("s.toml", &t).is_err(), "{bad}");
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

    #[test]
    fn baked_collider_matches_the_sampled_masks() {
        let plain = "frames = 2
frame_size = [2, 1]
origin = [0, 0]
";
        let (without, _) = parse_sidecar("sprites/s.toml", plain).unwrap();
        let (_, sampled) = load_sprite("s", &two_frames(), &without).unwrap();

        // The union of both frames is the box [0, 0, 1, 0] with both pixels solid: row `c`.
        let baked = "frames = 2
frame_size = [2, 1]
origin = [0, 0]

[collider]
alpha_tolerance = 0
shape = \"bits\"
bbox = [0, 0, 1, 0]
bits = [\"c\"]
";
        let (with, _) = parse_sidecar("sprites/s.toml", baked).unwrap();
        let (_, masks) = load_sprite("s", &two_frames(), &with).unwrap();
        assert_eq!(masks, sampled);
    }

    #[test]
    fn baked_collider_per_frame_matches_the_sampled_masks() {
        let plain = "frames = 2
frame_size = [2, 1]
origin = [0, 0]
separate_masks = true
";
        let (without, _) = parse_sidecar("sprites/s.toml", plain).unwrap();
        let (_, sampled) = load_sprite("s", &two_frames(), &without).unwrap();

        let baked = "frames = 2
frame_size = [2, 1]
origin = [0, 0]
separate_masks = true

[collider]
alpha_tolerance = 0
frames = [
  { shape = \"bits\", bbox = [0, 0, 0, 0], bits = [\"8\"] },
  { shape = \"bits\", bbox = [1, 0, 1, 0], bits = [\"8\"] },
]
";
        let (with, _) = parse_sidecar("sprites/s.toml", baked).unwrap();
        let (def, masks) = load_sprite("s", &two_frames(), &with).unwrap();
        assert_eq!(masks, sampled);
        assert!(matches!(def.masks, MaskSet::PerFrame(_)));
    }

    #[test]
    fn baked_collider_drift_is_rejected() {
        // One pixel where the art has two.
        let baked = "frames = 2
frame_size = [2, 1]
origin = [0, 0]

[collider]
alpha_tolerance = 0
shape = \"bits\"
bbox = [0, 0, 1, 0]
bits = [\"8\"]
";
        let (with, _) = parse_sidecar("sprites/s.toml", baked).unwrap();
        let d = load_sprite("s", &two_frames(), &with).unwrap_err();
        assert_eq!(d.file, "sprites/s.toml");
        assert!(d.message.contains("re-bake"), "{}", d.message);
        assert!(d.message.contains("2 solid pixel"), "{}", d.message);
    }

    #[test]
    fn baked_collider_is_explicit_none_for_degenerate_masks() {
        let transparent = png_rgba(4, 1, &[0; 16]);
        let baked = "frames = 2
frame_size = [2, 1]
origin = [0, 0]

[collider]
alpha_tolerance = 0
shape = \"none\"
";
        let (with, _) = parse_sidecar("sprites/s.toml", baked).unwrap();
        let (_, masks) = load_sprite("s", &transparent, &with).unwrap();
        assert_eq!(masks[0].shape, MaskShape::Empty);

        let not_none = "frames = 2
frame_size = [2, 1]
origin = [0, 0]

[collider]
alpha_tolerance = 0
shape = \"rect\"
bbox = [0, 0, 0, 0]
";
        let (with, _) = parse_sidecar("sprites/s.toml", not_none).unwrap();
        assert!(load_sprite("s", &transparent, &with).is_err());

        // A zero-frame sprite may declare the explicit none too.
        let zero = "frames = 0
frame_size = [2, 1]
origin = [0, 0]

[collider]
alpha_tolerance = 0
shape = \"none\"
";
        let (with, _) = parse_sidecar("sprites/s.toml", zero).unwrap();
        let (_, masks) = load_sprite("s", &[], &with).unwrap();
        assert_eq!(masks[0].shape, MaskShape::Empty);
    }

    #[test]
    fn baked_collider_parse_checks() {
        let base = "frames = 2\nframe_size = [2, 1]\norigin = [0, 0]\n";
        let with = |collider: &str| format!("{base}\n[collider]\n{collider}\n");
        let ok = with("alpha_tolerance = 0\nshape = \"none\"");
        assert!(parse_sidecar("sprites/a.toml", &ok).is_ok());

        let missing_tol = with("shape = \"none\"");
        assert!(parse_sidecar("sprites/a.toml", &missing_tol).is_err());

        let diff_tol = "frames = 2\nframe_size = [2, 1]\norigin = [0, 0]\nalpha_tolerance = 10\n\n[collider]\nalpha_tolerance = 0\nshape = \"none\"\n";
        let d = parse_sidecar("sprites/a.toml", diff_tol).unwrap_err();
        assert!(d.message.contains("differs"), "{}", d.message);

        let both = "frames = 2\nframe_size = [2, 1]\norigin = [0, 0]\n\n[collider]\nalpha_tolerance = 0\nshape = \"none\"\nframes = [{ shape = \"none\" }, { shape = \"none\" }]\n";
        let d = parse_sidecar("sprites/a.toml", both).unwrap_err();
        assert!(d.message.contains("not both"), "{}", d.message);

        let count = "frames = 2\nframe_size = [2, 1]\norigin = [0, 0]\n\n[collider]\nalpha_tolerance = 0\nframes = [{ shape = \"none\" }]\n";
        let d = parse_sidecar("sprites/a.toml", count).unwrap_err();
        assert!(d.message.contains("1 frame"), "{}", d.message);

        let none_body = with("alpha_tolerance = 0\nshape = \"none\"\nbbox = [0, 0, 0, 0]");
        assert!(parse_sidecar("sprites/a.toml", &none_body).is_err());

        let rect_no_bbox = with("alpha_tolerance = 0\nshape = \"rect\"");
        assert!(parse_sidecar("sprites/a.toml", &rect_no_bbox).is_err());

        let empty_bits = with("alpha_tolerance = 0\nshape = \"bits\"\nbbox = [0, 0, 1, 0]\nbits = [\"0\"]");
        let d = parse_sidecar("sprites/a.toml", &empty_bits).unwrap_err();
        assert!(d.message.contains("no solid pixel"), "{}", d.message);

        let padding = with("alpha_tolerance = 0\nshape = \"bits\"\nbbox = [0, 0, 1, 0]\nbits = [\"f\"]");
        let d = parse_sidecar("sprites/a.toml", &padding).unwrap_err();
        assert!(d.message.contains("padding"), "{}", d.message);

        let outside = with("alpha_tolerance = 0\nshape = \"rect\"\nbbox = [0, 0, 2, 0]");
        assert!(parse_sidecar("sprites/a.toml", &outside).is_err());
    }
}
