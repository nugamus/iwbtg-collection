//! Placement and bounding boxes: the only place collision code rounds (docs/plans/sim-core.md
//! 3.4, C1, C2). WP0 declares; WP3 owns.
//!
//! Two boxes, kept apart because the spec shows they differ (CLONE_SPEC collision, Confirmed
//! (recorder)):
//!
//! - [`instance_box`]: the **collision** box and mask that place tests, queries and collision
//!   events use. Unit scale, angle 0: `round(x) + (mask.left - origin.x)` (C1), ties to even
//!   (C2); scale, mirror and right angles per `xform` (C3, C4). The kid covers `x-5..x+5`,
//!   `y-12..y+8` in both facings.
//! - [`reported_box`]: the box GML reads as `bbox_*`, which `distance_to_object` and the
//!   non-precise point, rectangle and line queries use. Facing left it reads `x-7..x+5`.
//!   Empty masks (no-frame sprite, maskNothing, inverted rectangles) have a reported box but no
//!   collision box (research#224, research#230; [`reported_edges`]).

use super::xform::{self, Xform, add, sub};
use crate::def::{GameDef, IRect, Mask, MaskId, MaskSet, MaskShape, SpriteDef, SpriteId};
use crate::error::SimError;
use crate::num::{NumError, Real, dcos, dsin, to_int_round};
use crate::rules::CollideRules;
use crate::rules::collide::{BoxRound, PlaceRound};
use crate::world::Instance;

/// A mask placed in the room.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PlacedMask<'a> {
    /// Bounding box in room space, inclusive.
    pub bbox: IRect,
    /// The mask.
    pub mask: &'a Mask,
    /// How sprite space maps to room space.
    pub xf: Xform,
}

/// The integer origin a coordinate places at: banker's rounding (Inferred, C2; Confirmed
/// (recorder) for the reported bbox, machinary t-col-08-a.csv: 566.5 places at 566).
pub fn place_origin(v: Real, _rules: &CollideRules) -> Result<i64, SimError> {
    to_int_round(v).map_err(|e| match e {
        NumError::NotFinite(_) => SimError::non_finite("collision position"),
        NumError::OutOfRange(_) | NumError::DivisionByZero => SimError::overflow("collision position"),
    })
}

/// The coordinate a place test at `base + delta` uses (C2 `PlaceRound`). GML code such as
/// `place_free(x, y + 1)` adds before the call, so callers that port such a test pass the two
/// parts here: `TestedCoord` (default, Inferred "Standing heights") gives `base + delta`, which
/// the query then rounds; `RoundThenAdd` gives `round(base) + delta`.
pub fn place_at(base: Real, delta: Real, rules: &CollideRules) -> Result<Real, SimError> {
    match rules.place_round {
        PlaceRound::TestedCoord => Ok(base + delta),
        PlaceRound::RoundThenAdd => Ok(real_of(place_origin(base, rules)?) + delta),
    }
}

/// An i64 coordinate as a real. Room coordinates are far below 2^53, so this is exact.
pub(crate) fn real_of(v: i64) -> Real {
    #[allow(clippy::cast_precision_loss, reason = "room coordinates are far below 2^53")]
    Real::new(v as f64)
}

/// A mask and the origin it is placed by.
pub type MaskAt<'a> = (&'a Mask, (i32, i32));

/// The sprite that owns `mask` (first in table order).
fn owner(def: &GameDef, mask: MaskId) -> Option<&SpriteDef> {
    mask_sprite(def, mask).and_then(|s| def.sprite(s))
}

/// GML `mask_index` for a mask: the sprite that owns it (first in table order), `None` when no
/// sprite does.
pub fn mask_sprite(def: &GameDef, mask: MaskId) -> Option<SpriteId> {
    def.sprites
        .iter()
        .position(|s| match &s.masks {
            MaskSet::Shared(m) => *m == mask,
            MaskSet::PerFrame(v) => v.contains(&mask),
        })
        .and_then(|i| u32::try_from(i).ok())
        .map(SpriteId)
}

/// The origin of the sprite that owns `mask`, or (0, 0).
fn owner_origin(def: &GameDef, mask: MaskId) -> (i32, i32) {
    owner(def, mask).map_or((0, 0), |s| s.origin)
}

/// The mask an instance collides with and the origin it is placed by, or `None` when it has
/// none (no mask and no sprite, or a sprite with no frames).
///
/// `mask_index` (`Image::mask`) wins over the sprite (Confirmed, manual 403_05); its origin is
/// that of the sprite whose mask set holds it (a separate mask sprite such as maskPlayer keeps
/// its own origin (17, 23)), or (0, 0) when no sprite holds it. Otherwise the sprite's mask:
/// the shared one, or the current subimage's (`Image::subimage`, R7c, C6).
pub fn instance_mask<'a>(def: &'a GameDef, inst: &Instance) -> Result<Option<MaskAt<'a>>, SimError> {
    if let Some(m) = inst.img.mask() {
        let mask = def.mask(m).ok_or_else(|| {
            SimError::invariant(format!("mask {m:?} of instance {} does not exist", inst.id.0))
        })?;
        return Ok(Some((mask, owner_origin(def, m))));
    }
    let Some(s) = inst.img.sprite() else {
        return Ok(None);
    };
    let sprite = def.sprite(s).ok_or_else(|| {
        SimError::invariant(format!("sprite {s:?} of instance {} does not exist", inst.id.0))
    })?;
    let id = match &sprite.masks {
        MaskSet::Shared(m) => *m,
        MaskSet::PerFrame(v) => {
            let Some(sub) = inst.img.subimage(sprite.frames) else {
                return Ok(None);
            };
            *v.get(usize::from(sub))
                .ok_or_else(|| SimError::invariant(format!("sprite {s:?} has no mask for subimage {sub}")))?
        }
    };
    let mask = def
        .mask(id)
        .ok_or_else(|| SimError::invariant(format!("mask {id:?} of sprite {s:?} does not exist")))?;
    Ok(Some((mask, sprite.origin)))
}

/// The instance's mask placed as if it stood at `at`; `None` when it has no mask, an empty
/// one, or one that covers no pixel (scale 0). Unit scale, angle 0:
/// `bbox_left = round(x) + (mask.left - origin.x)` (Inferred, C1). `CollideRules::box_round`
/// picks how a .5 position rounds (C2).
pub fn instance_box<'a>(
    def: &'a GameDef,
    inst: &Instance,
    at: (Real, Real),
    rules: &CollideRules,
) -> Result<Option<PlacedMask<'a>>, SimError> {
    let Some((mask, origin)) = instance_mask(def, inst)? else {
        return Ok(None);
    };
    if matches!(mask.shape, MaskShape::Empty)
        || mask.bbox.left > mask.bbox.right
        || mask.bbox.top > mask.bbox.bottom
    {
        return Ok(None);
    }
    let mut xf = Xform {
        at: (0, 0),
        origin,
        xscale: inst.img.xscale(),
        yscale: inst.img.yscale(),
        angle: inst.img.angle(),
    };
    let Some(local) = xform::offset_box(mask.bbox, &xf)? else {
        return Ok(None);
    };
    let bbox = match rules.box_round {
        BoxRound::OriginFirst => {
            xf.at = (place_origin(at.0, rules)?, place_origin(at.1, rules)?);
            IRect {
                left: add(local.left, xf.at.0)?,
                top: add(local.top, xf.at.1)?,
                right: add(local.right, xf.at.0)?,
                bottom: add(local.bottom, xf.at.1)?,
            }
        }
        BoxRound::AfterOffset => {
            let edge = |v: Real, d: i64| place_origin(v + real_of(d), rules);
            let b = IRect {
                left: edge(at.0, local.left)?,
                top: edge(at.1, local.top)?,
                right: edge(at.0, local.right)?,
                bottom: edge(at.1, local.bottom)?,
            };
            // Pixels are sampled from the rounded left and top edges.
            xf.at = (sub(b.left, local.left)?, sub(b.top, local.top)?);
            b
        }
    };
    Ok(Some(PlacedMask { bbox, mask, xf }))
}

/// The reported box edges before rounding (left, top, right, bottom), or `None` when the
/// instance has no sprite and no mask, or an empty mask the spec has no rule for.
///
/// General bbox rule (CLONE_SPEC collision, Confirmed (recorder): poronkusema
/// t-col-09-a..g.csv, wanna mask-rotate.csv): per axis `L = (mask_left - origin_x) * xscale`
/// and `R = (mask_right + 1 - origin_x) * xscale - 1`; the four corners rotate by image_angle
/// (`dx = u cos a + v sin a`, `dy = -u sin a + v cos a`), x and y are added, then min and max.
///
/// Masks with no collision shape still report a box (CLONE_SPEC collision, "bbox_* of an
/// instance that has a sprite but no usable box", research#224, Confirmed (recorder):
/// traces/help-the-fairy-tale/bbox-empty-mask.csv and -b.csv):
///
/// - A sprite with no frames, as `sprite_index` without a mask, or as `mask_index`: the general
///   rule on a 1x1 mask at the origin (mask 0..0, origin 0), so `round(x)`, `round(y)` at unit
///   scale. A `mask_index` with frames wins over a no-frame sprite.
/// - maskNothing (2x2, bounds 1,0,1,0): the general rule on its inverted box, whose min and max
///   over the corners normalize it.
/// - Any other inverted manual rectangle (sprite520, 32x32, bounds 31,0,31,0; temp, 7x8,
///   bounds 6,0,7,0): the raw per-axis edges with no swap, so the box stays inverted
///   ([`inverted_edges`]). The spec has no rule that tells these from maskNothing (UNKNOWN);
///   the sim follows the measured rows.
/// - Any other empty mask (a fully transparent precise mask, a sprite with no collision shape):
///   `None`, UNKNOWN.
///
/// These masks have a reported box but no collision mask: [`instance_box`] is `None` for them,
/// so place tests and collision events never see them, while `distance_to_object` and
/// non-precise point, rectangle and line queries use this box (research#230, `query`).
pub fn reported_edges(
    def: &GameDef,
    inst: &Instance,
    at: (Real, Real),
) -> Result<Option<[Real; 4]>, SimError> {
    let source = match (inst.img.mask(), inst.img.sprite()) {
        (None, None) => return Ok(None),
        (Some(m), _) => owner(def, m),
        (None, Some(s)) => def.sprite(s),
    };
    if source.is_some_and(|s| s.frames == 0) {
        return general_edges(POINT_MASK, (0, 0), inst, at).map(Some);
    }
    let Some((mask, origin)) = instance_mask(def, inst)? else {
        return Ok(None);
    };
    if !matches!(mask.shape, MaskShape::Empty)
        || (mask.bbox == MASK_NOTHING && source.is_some_and(|s| s.size == (2, 2)))
    {
        return general_edges(mask.bbox, origin, inst, at).map(Some);
    }
    inverted_edges(mask.bbox, origin, inst, at)
}

/// An inverted manual rectangle other than maskNothing (left > right or top > bottom, every
/// bound >= 0): `x + L`, `y + T`, `x + R`, `y + B` of the general rule with no rotation and no
/// min and max, as sprite520 and temp read at unit scale and sprite520 at scale 2
/// (bbox-empty-mask.csv k4, k5; bbox-empty-mask-b.csv k4). Only angle 0 and positive scales
/// were matched (temp at angle 90, bbox-empty-mask-b.csv k5, fits no rule), so anything else
/// gives `None`, as does the converter's empty mask (0, 0, -1, -1) of a fully transparent
/// automatic box.
fn inverted_edges(
    b: IRect,
    origin: (i32, i32),
    inst: &Instance,
    at: (Real, Real),
) -> Result<Option<[Real; 4]>, SimError> {
    let img = &inst.img;
    let inverted = b.left > b.right || b.top > b.bottom;
    let manual = b.left >= 0 && b.top >= 0 && b.right >= 0 && b.bottom >= 0;
    let plain = img.angle().get() == 0.0 && img.xscale().get() > 0.0 && img.yscale().get() > 0.0;
    if !inverted || !manual || !plain {
        return Ok(None);
    }
    let (l, r, t, bt) = axis_edges(b, origin, inst, at)?;
    Ok(Some([at.0 + l, at.1 + t, at.0 + r, at.1 + bt]))
}

/// The general bbox rule for a mask box `b` placed by `origin`: per axis
/// `L = (b.left - origin_x) * xscale`, `R = (b.right + 1 - origin_x) * xscale - 1`, the four
/// corners rotated by image_angle and moved to `at`, then min and max (left, top, right, bottom).
fn general_edges(
    b: IRect,
    origin: (i32, i32),
    inst: &Instance,
    at: (Real, Real),
) -> Result<[Real; 4], SimError> {
    let img = &inst.img;
    let (l, r, t, bt) = axis_edges(b, origin, inst, at)?;
    let (c, s) = (dcos(img.angle()), dsin(img.angle()));
    let mut out: Option<[Real; 4]> = None;
    for (u, v) in [(l, t), (r, t), (l, bt), (r, bt)] {
        let x = at.0 + (u * c + v * s);
        let y = at.1 + (v * c - u * s);
        out = Some(match out {
            None => [x, y, x, y],
            Some([x1, y1, x2, y2]) => [
                if x.get() < x1.get() { x } else { x1 },
                if y.get() < y1.get() { y } else { y1 },
                if x.get() > x2.get() { x } else { x2 },
                if y.get() > y2.get() { y } else { y2 },
            ],
        });
    }
    out.ok_or_else(|| SimError::invariant("a box has four corners"))
}

/// The per-axis edges of the general rule before rotation, `(L, R, T, B)`, after checking that
/// scale, angle and position are finite.
fn axis_edges(
    b: IRect,
    origin: (i32, i32),
    inst: &Instance,
    at: (Real, Real),
) -> Result<(Real, Real, Real, Real), SimError> {
    let img = &inst.img;
    for (v, what) in [
        (img.xscale(), "image_xscale"),
        (img.yscale(), "image_yscale"),
        (img.angle(), "image_angle"),
        (at.0, "x"),
        (at.1, "y"),
    ] {
        if !v.is_finite() {
            return Err(SimError::non_finite(what));
        }
    }
    let ox = i64::from(origin.0);
    let oy = i64::from(origin.1);
    let l = real_of(sub(b.left, ox)?) * img.xscale();
    let r = real_of(sub(add(b.right, 1)?, ox)?) * img.xscale() - Real::ONE;
    let t = real_of(sub(b.top, oy)?) * img.yscale();
    let bt = real_of(sub(add(b.bottom, 1)?, oy)?) * img.yscale() - Real::ONE;
    Ok((l, r, t, bt))
}

/// The reported box (GML `bbox_left`, `bbox_top`, `bbox_right`, `bbox_bottom`): the
/// [`reported_edges`], each rounded to nearest with ties to even (Confirmed (recorder):
/// machinary t-col-08-a.csv, x 200.5 reads 196..206; poronkusema t-col-09-f.csv). An inverted
/// rectangle's box stays inverted (left > right): it contains no point.
pub fn reported_box(
    def: &GameDef,
    inst: &Instance,
    at: (Real, Real),
    rules: &CollideRules,
) -> Result<Option<IRect>, SimError> {
    let Some([l, t, r, b]) = reported_edges(def, inst, at)? else {
        return Ok(None);
    };
    Ok(Some(IRect {
        left: place_origin(l, rules)?,
        top: place_origin(t, rules)?,
        right: place_origin(r, rules)?,
        bottom: place_origin(b, rules)?,
    }))
}

/// What GML `bbox_*` reads for an instance (the trace probe's `bbox_*` columns).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BboxRead {
    /// A box: left, top, right, bottom.
    Box(IRect),
    /// No sprite and no mask: all four read -100000 (CLONE_SPEC collision, Confirmed (recorder),
    /// traces/animus/bbox-nomask.csv).
    NoMask,
    /// Not known: an empty mask the spec has no rule for (a rotated or mirrored inverted
    /// rectangle, a fully transparent precise mask, a sprite with no collision shape).
    Unknown,
}

/// The 1x1 mask a sprite with no frames reads as (mask 0..0, origin 0).
const POINT_MASK: IRect = IRect {
    left: 0,
    top: 0,
    right: 0,
    bottom: 0,
};

/// maskNothing, the GameMaker template empty mask: a 2x2 sprite whose manual rectangle has
/// left 1, right 0, top 1, bottom 0.
const MASK_NOTHING: IRect = IRect {
    left: 1,
    top: 1,
    right: 0,
    bottom: 0,
};

/// What `bbox_*` reads for `inst` standing at `at`: [`BboxRead::NoMask`] with no sprite and no
/// mask, else the [`reported_box`] (empty masks included, see [`reported_edges`]), or
/// [`BboxRead::Unknown`] where the spec has no rule.
pub fn bbox_read(
    def: &GameDef,
    inst: &Instance,
    at: (Real, Real),
    rules: &CollideRules,
) -> Result<BboxRead, SimError> {
    if inst.img.mask().is_none() && inst.img.sprite().is_none() {
        return Ok(BboxRead::NoMask);
    }
    Ok(reported_box(def, inst, at, rules)?.map_or(BboxRead::Unknown, BboxRead::Box))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_origin_rounds_half_to_even() {
        let r = CollideRules::default();
        for (v, want) in [
            (566.5, 566),
            (567.5, 568),
            (567.4, 567),
            (-0.5, 0),
            (-1.5, -2),
            (58.5, 58),
            (200.5, 200),
        ] {
            assert_eq!(place_origin(Real::new(v), &r).unwrap(), want, "{v}");
        }
        assert!(matches!(
            place_origin(Real::new(f64::INFINITY), &r),
            Err(SimError::NonFinite { .. })
        ));
        assert!(matches!(
            place_origin(Real::new(1e300), &r),
            Err(SimError::Overflow { .. })
        ));
    }

    #[test]
    fn place_at_switch() {
        let mut r = CollideRules::default();
        let y = Real::new(566.5);
        assert_eq!(place_at(y, Real::ONE, &r).unwrap(), Real::new(567.5));
        r.place_round = PlaceRound::RoundThenAdd;
        assert_eq!(place_at(y, Real::ONE, &r).unwrap(), Real::new(567.0));
    }
}
