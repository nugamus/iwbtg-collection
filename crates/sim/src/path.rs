//! Path following (docs/plans/sim-core.md 3.1, M8; UNKNOWN U9, U-MECH-4). Tier 2.
//!
//! [`advance`] is the only path rule. What the spec has (CLONE_SPEC motion U9, Confirmed
//! (recorder) by `traces/rely-the-needle-4-2/t-evo-1.csv`): a probe on a 40 px path at path speed
//! 4 moves 4 px and +0.1 path_position per tick, its `speed` reads 0 throughout, the position
//! reads `0.9999999999999999` after ten ticks, and End of Path fires on the next tick, when the
//! position reads 1.0 and x stays at the end. Everything else here is the M8 default (Design,
//! OPEN until TR-MECH-02): straight segments between the points, the position advancing by
//! `path_speed / length` per tick, a closed path's last point joined to its first, the points'
//! speed factors ignored, smooth paths `SimError::Unsupported`. WP2 owns this file.

use serde::{Deserialize, Serialize};

use crate::def::{GameDef, PathDef, PathId};
use crate::error::SimError;
use crate::motion::Motion;
use crate::num::{self, Real};

/// A running path on one instance.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct PathRun {
    /// The path.
    pub path: PathId,
    /// GM `path_position`, 0 to 1.
    pub position: Real,
    /// GM `path_speed`.
    pub speed: Real,
    /// What happens at the end.
    pub end_action: PathEnd,
    /// Absolute (true) or relative to the start position.
    pub absolute: bool,
    /// GM `path_scale`.
    pub scale: Real,
    /// Start position for a relative path.
    pub xstart: Real,
    /// Start position for a relative path.
    pub ystart: Real,
}

/// GM `path_endaction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PathEnd {
    /// Stop.
    Stop,
    /// Restart from the beginning.
    Restart,
    /// Continue from the current position.
    Continue,
    /// Reverse.
    Reverse,
}

/// GML `path_start`: a run at position 0 with scale 1, anchored at the instance's current
/// position for a relative path. The instance does not move until the next [`advance`]
/// (M8, OPEN: whether an absolute path places the instance at its start at once).
///
/// Errors with `SimError::NonFinite` on a non-finite speed.
pub fn start(
    m: &Motion,
    path: PathId,
    speed: Real,
    end_action: PathEnd,
    absolute: bool,
) -> Result<PathRun, SimError> {
    if !speed.is_finite() {
        return Err(SimError::non_finite("path_speed"));
    }
    Ok(PathRun {
        path,
        position: Real::ZERO,
        speed,
        end_action,
        absolute,
        scale: Real::ONE,
        xstart: m.x(),
        ystart: m.y(),
    })
}

/// A point of a path.
type Point = (Real, Real);

/// One straight piece of a path: from, to, length.
type Segment = (Point, Point, Real);

/// The segments of a path as `(from, to, length)`, in order; a closed path adds the segment
/// from the last point back to the first.
fn segments(def: &PathDef) -> Vec<Segment> {
    let points: Vec<Point> = def.points.iter().map(|&(x, y, _)| (x, y)).collect();
    let mut out = Vec::with_capacity(points.len());
    for pair in points.windows(2) {
        out.push((pair[0], pair[1], seg_len(pair[0], pair[1])));
    }
    if def.closed
        && let (Some(&last), Some(&first)) = (points.last(), points.first())
        && points.len() > 1
    {
        out.push((last, first, seg_len(last, first)));
    }
    out
}

/// Length of one segment: `point_distance`.
fn seg_len(a: Point, b: Point) -> Real {
    num::point_distance(a.0, a.1, b.0, b.1)
}

/// The point at `distance` along the segments: linear inside the segment that contains it.
fn point_at(segs: &[Segment], distance: Real) -> Option<Point> {
    let mut walked = Real::ZERO;
    for (i, &(a, b, len)) in segs.iter().enumerate() {
        let last = i + 1 == segs.len();
        if distance.get() <= (walked + len).get() || last {
            if len.get() == 0.0 {
                return Some(a);
            }
            let t = (distance - walked) / len;
            return Some((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
        walked += len;
    }
    None
}

/// One tick of path following inside automatic motion (M8). Returns true when the path ended
/// this tick (End of Path).
///
/// The position moves by `speed / length`. Reaching 1 (or 0 with a negative speed, exact
/// compare) ends the path:
/// - `Stop`: the position stays at the end. The run keeps its speed, but a run already at its
///   end raises no further End of Path (Design: the caller may also drop it with [`end`]).
/// - `Restart`: the position wraps by one and the instance jumps back to the start.
/// - `Continue`: the position wraps, and a relative path is re-anchored so the next lap starts
///   where this one ended.
/// - `Reverse`: the speed changes sign and the position reflects back into the path.
///
/// Then the instance is placed at the point for the position: `xstart + (px - p0x) * scale` for
/// a relative path, the point itself for an absolute one (scale on absolute paths: OPEN).
///
/// Errors: `SimError::Invariant` for a path id that names nothing, `SimError::Unsupported` for a
/// smooth path or a path of zero length, `SimError::NonFinite` for a non-finite result.
pub fn advance(run: &mut PathRun, m: &mut Motion, def: &GameDef) -> Result<bool, SimError> {
    let pdef = run
        .path
        .index()
        .and_then(|i| def.paths.get(i))
        .ok_or_else(|| SimError::invariant(format!("path {:?} is not in the game", run.path)))?;
    if pdef.smooth {
        return Err(SimError::unsupported(format!(
            "smooth path {} (M8: only straight segments are implemented)",
            pdef.name
        )));
    }
    let segs = segments(pdef);
    let length = segs.iter().fold(Real::ZERO, |acc, s| acc + s.2);
    if length.get() <= 0.0 || !length.is_finite() {
        return Err(SimError::unsupported(format!(
            "path {} has no length (M8)",
            pdef.name
        )));
    }

    let before = run.position;
    let next = before + run.speed / length;
    if !next.is_finite() {
        return Err(SimError::non_finite("path_position"));
    }
    let one = Real::ONE;
    let forward = run.speed.get() > 0.0;
    let ended = (forward && next.get() >= 1.0) || (run.speed.get() < 0.0 && next.get() <= 0.0);
    let mut fired = false;
    if ended {
        match run.end_action {
            PathEnd::Stop => {
                let end = if forward { one } else { Real::ZERO };
                fired = if forward {
                    before.get() < 1.0
                } else {
                    before.get() > 0.0
                };
                run.position = end;
            }
            PathEnd::Restart | PathEnd::Continue => {
                run.position = if forward { next - one } else { next + one };
                fired = true;
                if run.end_action == PathEnd::Continue
                    && !run.absolute
                    && let (Some(first), Some(last)) = (point_at(&segs, Real::ZERO), point_at(&segs, length))
                {
                    let (dx, dy) = ((last.0 - first.0) * run.scale, (last.1 - first.1) * run.scale);
                    if forward {
                        run.xstart += dx;
                        run.ystart += dy;
                    } else {
                        run.xstart -= dx;
                        run.ystart -= dy;
                    }
                }
            }
            PathEnd::Reverse => {
                run.position = if forward { Real::new(2.0) - next } else { -next };
                run.speed = -run.speed;
                fired = true;
            }
        }
        // Keep the position inside the path even after a wrap by more than one lap.
        if run.position.get() < 0.0 || run.position.get() > 1.0 {
            run.position = num::clamp(run.position, Real::ZERO, one);
        }
    } else {
        run.position = next;
    }

    let (px, py) = point_at(&segs, run.position * length)
        .ok_or_else(|| SimError::invariant(format!("path {} has no points", pdef.name)))?;
    let (x, y) = if run.absolute {
        (px, py)
    } else {
        let (p0x, p0y) = point_at(&segs, Real::ZERO)
            .ok_or_else(|| SimError::invariant(format!("path {} has no points", pdef.name)))?;
        (
            run.xstart + (px - p0x) * run.scale,
            run.ystart + (py - p0y) * run.scale,
        )
    };
    m.place(x, y)?;
    Ok(fired)
}

/// GML `path_end`: the instance stops following its path.
pub fn end(run: &mut Option<PathRun>) {
    *run = None;
}
