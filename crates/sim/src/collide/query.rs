//! GML collision queries (docs/plans/sim-core.md 3.4, C12, C13, R21, R22).
//!
//! Each excludes `me`, skips inactive instances, skips dead ones unless
//! `TickRules::dead_in_queries` (R22), and returns `World::first_of` order (R21).
//! WP0 declares; WP3 owns.
//!
//! **Which box.** Place tests (`place_free`, `place_empty`, `place_meeting`, `instance_place`)
//! and the precise branch of the point, rectangle and line queries use the collision mask
//! (`bbox::instance_box`). A non-precise point, rectangle or line query (`prec` false, or a
//! target whose mask is a plain rectangle, "only done when the sprite ... has precise collision
//! checking set", manual 403_04) tests the reported box (`bbox::reported_box`): a non-precise
//! collision_point hits the mirrored tree's reported range, extra column included
//! (CLONE_SPEC collision, Confirmed (recorder), poronkusema t-col-09-e.csv rows 4-8).
//!
//! **Candidates.** The broadphase grid gives candidates near the query box plus every moved
//! instance; an unbuilt grid gives every instance. Every hit is collected, then the result is
//! picked by the R21 order, so the grid never changes an outcome.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::bbox::{PlacedMask, instance_box, place_origin, real_of, reported_box, reported_edges};
use super::overlap::{intersect, overlaps, point_in, rect_in};
use crate::def::{ClassMask, GameDef, IRect, KindId, MaskShape};
use crate::error::SimError;
use crate::events;
use crate::num::{Real, sqrt};
use crate::rules::RunnerRules;
use crate::rules::collide::{DistBoxes, DistCombine, DistOffset};
use crate::rules::tick::{DispatchOrder, FirstRule};
use crate::world::{Ctx, InstId, Instance, World};

/// What a query looks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Target {
    /// Any instance with one of these classes (GM parent emulation).
    Class(ClassMask),
    /// Instances of one kind.
    Kind(KindId),
    /// One instance.
    Inst(InstId),
    /// Any instance (`all`).
    Any,
}

impl Target {
    /// True when `inst` is one of the instances this target names.
    pub fn matches(self, inst: &Instance) -> bool {
        match self {
            Target::Class(c) => inst.classes.intersects(c),
            Target::Kind(k) => inst.kind == k,
            Target::Inst(id) => inst.id == id,
            Target::Any => true,
        }
    }
}

/// Most points a `collision_line` samples (Design): a longer segment is `Unsupported`.
pub const MAX_LINE_POINTS: i64 = 1 << 20;

/// Pixels a box is widened by before the cheap reported-box filter (the collision box lies
/// within the reported box widened by one pixel; one more covers the C2 rounding switch).
const FILTER_PAD: i64 = 2;

/// Read-only view of what a query needs.
#[derive(Clone, Copy)]
pub(crate) struct Q<'a> {
    pub def: &'a GameDef,
    pub rules: &'a RunnerRules,
    pub w: &'a World,
}

impl<'a> Q<'a> {
    pub(crate) fn of(ctx: &'a Ctx) -> Q<'a> {
        Q {
            def: ctx.def,
            rules: ctx.rules,
            w: ctx.w,
        }
    }

    /// The instance `id`, or `Invariant`.
    pub(crate) fn inst(&self, id: InstId) -> Result<&'a Instance, SimError> {
        self.w.get(id).ok_or_else(|| {
            SimError::invariant(format!(
                "collision query on instance {} that does not exist",
                id.0
            ))
        })
    }

    /// Active, alive (or dead with R22 on), not `notme`, and named by `t`.
    fn eligible(&self, inst: &Instance, t: Target, notme: Option<InstId>) -> bool {
        Some(inst.id) != notme
            && inst.active
            && (inst.alive || self.rules.tick.dead_in_queries)
            && t.matches(inst)
    }

    /// Candidates that may touch `region`, sorted by id: grid hits plus moved instances, or
    /// every instance while the grid is unbuilt.
    fn candidates(&self, region: IRect) -> Result<Vec<&'a Instance>, SimError> {
        let grid = &self.w.grid;
        if !grid.is_built() {
            return Ok(self.w.instances().iter().collect());
        }
        let mut ids: BTreeSet<InstId> = grid
            .candidates(region.left, region.top, region.right, region.bottom)?
            .into_iter()
            .collect();
        ids.extend(grid.moved().iter().copied());
        Ok(ids.into_iter().filter_map(|id| self.w.get(id)).collect())
    }

    /// True when `inst`'s reported box, widened, may touch `region` (or it has no reported box,
    /// in which case it has no mask and touches nothing).
    fn near(&self, inst: &Instance, region: &IRect) -> Result<bool, SimError> {
        let at = (inst.m.x(), inst.m.y());
        let Some(b) = reported_box(self.def, inst, at, &self.rules.collide)? else {
            return Ok(false);
        };
        let wide = IRect {
            left: b.left.saturating_sub(FILTER_PAD),
            top: b.top.saturating_sub(FILTER_PAD),
            right: b.right.saturating_add(FILTER_PAD),
            bottom: b.bottom.saturating_add(FILTER_PAD),
        };
        Ok(intersect(&wide, region).is_some())
    }

    /// The collision mask of `inst` at its own position.
    fn placed(&self, inst: &Instance) -> Result<Option<PlacedMask<'a>>, SimError> {
        instance_box(self.def, inst, (inst.m.x(), inst.m.y()), &self.rules.collide)
    }

    /// Every eligible instance whose collision mask overlaps `me` placed at `(x, y)`; `solid`
    /// keeps only solid ones.
    pub(crate) fn place_hits(
        &self,
        me: InstId,
        x: Real,
        y: Real,
        t: Target,
        solid_only: bool,
    ) -> Result<Vec<&'a Instance>, SimError> {
        let me_inst = self.inst(me)?;
        let Some(pm) = instance_box(self.def, me_inst, (x, y), &self.rules.collide)? else {
            return Ok(Vec::new());
        };
        let mut hits = Vec::new();
        for c in self.candidates(pm.bbox)? {
            if !self.eligible(c, t, Some(me)) || (solid_only && !c.solid) || !self.near(c, &pm.bbox)? {
                continue;
            }
            if let Some(cb) = self.placed(c)?
                && overlaps(&pm, &cb)?
            {
                hits.push(c);
            }
        }
        Ok(hits)
    }

    /// Every eligible instance that `test` reports as touching the shape whose bounding box is
    /// `region`. `test` gets the precise placed mask (`Some`) when `prec` is on and the mask is
    /// not a plain rectangle, else the reported box only.
    fn shape_hits(
        &self,
        region: IRect,
        t: Target,
        prec: bool,
        notme: Option<InstId>,
        test: impl Fn(&IRect, Option<&PlacedMask>) -> Result<bool, SimError>,
    ) -> Result<Vec<&'a Instance>, SimError> {
        let mut hits = Vec::new();
        for c in self.candidates(region)? {
            if !self.eligible(c, t, notme) || !self.near(c, &region)? {
                continue;
            }
            let at = (c.m.x(), c.m.y());
            let precise = if prec {
                self.placed(c)?
                    .filter(|p| matches!(p.mask.shape, MaskShape::Bits(_)))
            } else {
                None
            };
            let hit = match precise {
                Some(p) => test(&p.bbox, Some(&p))?,
                None => match reported_box(self.def, c, at, &self.rules.collide)? {
                    Some(b) => test(&b, None)?,
                    None => false,
                },
            };
            if hit {
                hits.push(c);
            }
        }
        Ok(hits)
    }

    /// "The first" of `hits` by R21 (`World::first_of` order).
    pub(crate) fn first(&self, hits: &[&Instance]) -> Option<InstId> {
        let tick = &self.rules.tick;
        let by = |key: fn(&Instance) -> (u16, InstId)| hits.iter().map(|i| key(i)).min().map(|k| k.1);
        match (tick.first_rule, tick.dispatch) {
            (FirstRule::LowestId, _) | (FirstRule::DispatchOrder, DispatchOrder::Creation) => {
                by(|i| (0, i.id))
            }
            (FirstRule::DispatchOrder, DispatchOrder::RankThenCreation) => by(|i| (i.rank, i.id)),
            (FirstRule::DispatchOrder, DispatchOrder::DepthThenRankThenCreation) => {
                // Depth order is WP1's to define; ask its dispatch order.
                let set: BTreeSet<InstId> = hits.iter().map(|i| i.id).collect();
                events::dispatch_order(self.w, tick, |i| set.contains(&i.id))
                    .first()
                    .copied()
            }
        }
    }

    /// The room pixel a real point places at (C12: rounded like a position, C2).
    fn pixel(&self, x: Real, y: Real) -> Result<(i64, i64), SimError> {
        Ok((
            place_origin(x, &self.rules.collide)?,
            place_origin(y, &self.rules.collide)?,
        ))
    }
}

/// GML `place_free`: no solid instance at `(x, y)` (solids only, Confirmed).
pub fn place_free(ctx: &Ctx, me: InstId, x: Real, y: Real) -> Result<bool, SimError> {
    Ok(Q::of(ctx).place_hits(me, x, y, Target::Any, true)?.is_empty())
}

/// GML `place_empty`: no instance at all at `(x, y)`.
pub fn place_empty(ctx: &Ctx, me: InstId, x: Real, y: Real) -> Result<bool, SimError> {
    Ok(Q::of(ctx).place_hits(me, x, y, Target::Any, false)?.is_empty())
}

/// GML `place_meeting`.
pub fn place_meeting(ctx: &Ctx, me: InstId, x: Real, y: Real, t: Target) -> Result<bool, SimError> {
    Ok(!Q::of(ctx).place_hits(me, x, y, t, false)?.is_empty())
}

/// GML `instance_place`.
pub fn instance_place(
    ctx: &Ctx,
    me: InstId,
    x: Real,
    y: Real,
    t: Target,
) -> Result<Option<InstId>, SimError> {
    let q = Q::of(ctx);
    let hits = q.place_hits(me, x, y, t, false)?;
    Ok(q.first(&hits))
}

/// GML `instance_position`: the point test with precise masks where the target has them.
pub fn instance_position(ctx: &Ctx, x: Real, y: Real, t: Target) -> Result<Option<InstId>, SimError> {
    collision_point(ctx, x, y, t, true, None)
}

/// GML `collision_point`. The point rounds like a position (C12).
pub fn collision_point(
    ctx: &Ctx,
    x: Real,
    y: Real,
    t: Target,
    prec: bool,
    notme: Option<InstId>,
) -> Result<Option<InstId>, SimError> {
    let q = Q::of(ctx);
    let (px, py) = q.pixel(x, y)?;
    let region = IRect {
        left: px,
        top: py,
        right: px,
        bottom: py,
    };
    let hits = q.shape_hits(region, t, prec, notme, |b, p| match p {
        Some(p) => point_in(p, px, py),
        None => Ok(intersect(b, &region).is_some()),
    })?;
    Ok(q.first(&hits))
}

/// GML `collision_rectangle`; corners per C12 (UNKNOWN; default: each corner rounds like a
/// position, ties to even, and the rectangle includes both corners, in any order).
#[allow(clippy::too_many_arguments, reason = "mirrors GML collision_rectangle")]
pub fn collision_rectangle(
    ctx: &Ctx,
    x1: Real,
    y1: Real,
    x2: Real,
    y2: Real,
    t: Target,
    prec: bool,
    notme: Option<InstId>,
) -> Result<Option<InstId>, SimError> {
    let q = Q::of(ctx);
    let (ax, ay) = q.pixel(x1, y1)?;
    let (bx, by) = q.pixel(x2, y2)?;
    let region = IRect {
        left: ax.min(bx),
        top: ay.min(by),
        right: ax.max(bx),
        bottom: ay.max(by),
    };
    let hits = q.shape_hits(region, t, prec, notme, |b, p| match p {
        Some(p) => rect_in(p, region.left, region.top, region.right, region.bottom),
        None => Ok(intersect(b, &region).is_some()),
    })?;
    Ok(q.first(&hits))
}

/// `num / den` rounded to nearest, ties to even (`den > 0`).
fn div_round_even(num: i128, den: i128) -> i128 {
    let q = num.div_euclid(den);
    let r = num.rem_euclid(den);
    match (2 * r).cmp(&den) {
        std::cmp::Ordering::Less => q,
        std::cmp::Ordering::Greater => q + 1,
        std::cmp::Ordering::Equal => q + (q & 1),
    }
}

/// The pixels a segment between two rounded endpoints covers (C12, Design): `n + 1` evenly
/// spaced samples, `n` the larger of `|dx|` and `|dy|`, each rounded to nearest with ties to
/// even, so the samples form an 8-connected line with both endpoints included.
pub fn line_cells(a: (i64, i64), b: (i64, i64)) -> Result<Vec<(i64, i64)>, SimError> {
    let dx = i128::from(b.0) - i128::from(a.0);
    let dy = i128::from(b.1) - i128::from(a.1);
    let n = dx.abs().max(dy.abs());
    if n > i128::from(MAX_LINE_POINTS) {
        return Err(SimError::unsupported(format!(
            "collision_line over {n} pixels (limit {MAX_LINE_POINTS}; C12)"
        )));
    }
    if n == 0 {
        return Ok(vec![a]);
    }
    let mut out = Vec::new();
    for i in 0..=n {
        let x = i128::from(a.0) + div_round_even(dx * i, n);
        let y = i128::from(a.1) + div_round_even(dy * i, n);
        let x = i64::try_from(x).map_err(|_| SimError::overflow("collision_line"))?;
        let y = i64::try_from(y).map_err(|_| SimError::overflow("collision_line"))?;
        out.push((x, y));
    }
    Ok(out)
}

/// GML `collision_line`; cells per C12 ([`line_cells`]).
#[allow(clippy::too_many_arguments, reason = "mirrors GML collision_line")]
pub fn collision_line(
    ctx: &Ctx,
    x1: Real,
    y1: Real,
    x2: Real,
    y2: Real,
    t: Target,
    prec: bool,
    notme: Option<InstId>,
) -> Result<Option<InstId>, SimError> {
    let q = Q::of(ctx);
    let a = q.pixel(x1, y1)?;
    let b = q.pixel(x2, y2)?;
    let cells = line_cells(a, b)?;
    let region = IRect {
        left: a.0.min(b.0),
        top: a.1.min(b.1),
        right: a.0.max(b.0),
        bottom: a.1.max(b.1),
    };
    let hits = q.shape_hits(region, t, prec, notme, |bx, p| {
        for &(x, y) in &cells {
            if x < bx.left || x > bx.right || y < bx.top || y > bx.bottom {
                continue;
            }
            let hit = match p {
                Some(p) => point_in(p, x, y)?,
                None => true,
            };
            if hit {
                return Ok(true);
            }
        }
        Ok(false)
    })?;
    Ok(q.first(&hits))
}

/// Box edges `distance_to_object` measures for one instance (C13 `DistBoxes`): left, top,
/// right, bottom. An instance without a mask measures from its position as a point (Design).
fn dist_edges(q: &Q, inst: &Instance) -> Result<[Real; 4], SimError> {
    let at = (inst.m.x(), inst.m.y());
    let rules = &q.rules.collide;
    let from_rect = |b: IRect| {
        [
            real_of(b.left),
            real_of(b.top),
            real_of(b.right),
            real_of(b.bottom),
        ]
    };
    let edges = match rules.dist_boxes {
        DistBoxes::Reported => reported_box(q.def, inst, at, rules)?.map(from_rect),
        DistBoxes::RoundedInclusive => instance_box(q.def, inst, at, rules)?.map(|p| from_rect(p.bbox)),
        DistBoxes::Unrounded => reported_edges(q.def, inst, at)?,
    };
    match (edges, rules.dist_boxes) {
        (Some(e), _) => Ok(e),
        (None, DistBoxes::Unrounded) => Ok([at.0, at.1, at.0, at.1]),
        (None, _) => {
            let (x, y) = (
                real_of(place_origin(at.0, rules)?),
                real_of(place_origin(at.1, rules)?),
            );
            Ok([x, y, x, y])
        }
    }
}

/// The gap on one axis between `[a1, a2]` and `[b1, b2]` (C13 `DistOffset`).
fn axis_gap(a1: Real, a2: Real, b1: Real, b2: Real, offset: DistOffset) -> Real {
    let d1 = b1 - a2;
    let d2 = a1 - b2;
    let diff = if d1.get() > d2.get() { d1 } else { d2 };
    if diff.get() <= 0.0 {
        return Real::ZERO;
    }
    match offset {
        DistOffset::PlusOne => diff,
        DistOffset::None => {
            let g = diff - Real::ONE;
            if g.get() > 0.0 { g } else { Real::ZERO }
        }
    }
}

/// GML `distance_to_object`: the gap between bounding boxes per C13 (default: reported boxes,
/// difference of facing edge coordinates, so touching boxes give 1 and only overlap gives 0,
/// Confirmed (recorder); the x and y gaps combine by `sqrt(dx*dx + dy*dy)`, Design). `None`
/// when no instance exists (A3 U4; callers treat it as a large value).
///
/// Every target instance is measured (no broadphase: the nearest may be anywhere).
pub fn distance_to_object(ctx: &Ctx, me: InstId, t: Target) -> Result<Option<Real>, SimError> {
    let q = Q::of(ctx);
    let me_inst = q.inst(me)?;
    let a = dist_edges(&q, me_inst)?;
    let rules = &q.rules.collide;
    let mut best: Option<Real> = None;
    for c in q.w.instances() {
        if !q.eligible(c, t, Some(me)) {
            continue;
        }
        let b = dist_edges(&q, c)?;
        let dx = axis_gap(a[0], a[2], b[0], b[2], rules.dist_offset);
        let dy = axis_gap(a[1], a[3], b[1], b[3], rules.dist_offset);
        let d = match rules.dist_combine {
            DistCombine::Euclid => sqrt(dx * dx + dy * dy),
            DistCombine::Max => {
                if dy.get() > dx.get() {
                    dy
                } else {
                    dx
                }
            }
        };
        if best.is_none_or(|b| d.get() < b.get()) {
            best = Some(d);
        }
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_division_ties_to_even() {
        assert_eq!(div_round_even(1, 2), 0);
        assert_eq!(div_round_even(3, 2), 2);
        assert_eq!(div_round_even(-1, 2), 0);
        assert_eq!(div_round_even(-3, 2), -2);
        assert_eq!(div_round_even(2, 3), 1);
    }

    #[test]
    fn line_cells_are_connected_and_include_ends() {
        let cells = line_cells((0, 0), (5, 2)).unwrap();
        assert_eq!(cells.first(), Some(&(0, 0)));
        assert_eq!(cells.last(), Some(&(5, 2)));
        assert_eq!(cells.len(), 6);
        for w in cells.windows(2) {
            assert!((w[1].0 - w[0].0).abs() <= 1 && (w[1].1 - w[0].1).abs() <= 1);
        }
        assert_eq!(line_cells((3, 3), (3, 3)).unwrap(), vec![(3, 3)]);
        assert!(matches!(
            line_cells((0, 0), (1 << 40, 0)),
            Err(SimError::Unsupported { .. })
        ));
    }

    #[test]
    fn axis_gap_offsets() {
        let r = Real::new;
        // Touching: 767 | 768.
        assert_eq!(
            axis_gap(r(757.0), r(767.0), r(768.0), r(799.0), DistOffset::PlusOne),
            r(1.0)
        );
        assert_eq!(
            axis_gap(r(757.0), r(767.0), r(768.0), r(799.0), DistOffset::None),
            r(0.0)
        );
        // One free pixel, from either side.
        assert_eq!(
            axis_gap(r(769.0), r(790.0), r(700.0), r(767.0), DistOffset::PlusOne),
            r(2.0)
        );
        // Overlap.
        assert_eq!(
            axis_gap(r(0.0), r(10.0), r(10.0), r(20.0), DistOffset::PlusOne),
            r(0.0)
        );
    }
}
