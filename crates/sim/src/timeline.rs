//! Running timelines (docs/plans/sim-core.md 2.1 row 3, R6, R6b). WP0 declared; WP1 owns.
//!
//! CLONE_SPEC sound section, Confirmed (recorder): a timeline set at position 0 during room
//! entry runs moment 0 on the room's first tick, and moment N runs N ticks later;
//! `timeline_position` reads 1 after that first tick (traces/eat-the-rori-girl/audio-rori-sync-b.csv
//! rows 2-9801, traces/clear-the-easy-miku-2/audio-miku-sync-mute-a.csv). So each tick runs the
//! moments in `[position, position + speed)` in order, then adds the speed. A timeline set during
//! a tick (moment 0 that tick or the next) is not traced; this rule gives the next tick.
//! Looping and negative speeds are not in the spec (Design, see [`advance`]).

use serde::{Deserialize, Serialize};

use crate::def::TimelineId;
use crate::error::SimError;
use crate::events::{self, EventKind};
use crate::num::Real;
use crate::rules::tick::RoomRequestRule;
use crate::world::{Ctx, Flow, InstId};

/// A timeline running on one instance.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct TimelineRun {
    /// The timeline.
    pub timeline: TimelineId,
    /// GM `timeline_position`.
    pub position: Real,
    /// GM `timeline_speed`.
    pub speed: Real,
    /// GM `timeline_running`.
    pub running: bool,
    /// GM `timeline_loop`.
    pub looping: bool,
}

/// GML Set Time Line at `position`, running. `timeline_speed` and `timeline_loop` carry over from
/// the instance's previous timeline, else 1 and false. Moment N then runs on the Nth timeline
/// phase after `position` (R6b, see the module docs). A non-finite position halts (N15).
pub fn start(run: &mut Option<TimelineRun>, timeline: TimelineId, position: Real) -> Result<(), SimError> {
    if !position.is_finite() {
        return Err(SimError::non_finite("timeline_position"));
    }
    let (speed, looping) = run
        .as_ref()
        .map_or((Real::ONE, false), |r| (r.speed, r.looping));
    *run = Some(TimelineRun {
        timeline,
        position,
        speed,
        running: true,
        looping,
    });
    Ok(())
}

/// The run's timeline id and whether it still runs, for checks between moments.
fn current(ctx: &Ctx, me: InstId) -> Option<(TimelineId, bool)> {
    let inst = ctx.w.get(me).filter(|i| i.alive && i.active)?;
    inst.timeline.as_ref().map(|r| (r.timeline, r.running))
}

/// Tick row 3 for one instance: run the moments in `[position, position + speed)` (ascending;
/// for a negative speed, `(position + speed, position]` descending, Design), then add the speed
/// to the position the moments left. A looping timeline whose position has passed its last
/// moment starts again at 0 (Design; the manual only says it restarts at the end).
///
/// Stops early when a moment destroys the instance, stops or replaces its timeline, halts, or
/// requests a room under `RoomRequestRule::AbortTick`; returns `AbortTick` for a room request so
/// the phase applies R8.
pub fn advance(ctx: &mut Ctx, me: InstId) -> Result<Flow, SimError> {
    let Some(run) = ctx.w.get(me).and_then(|i| i.timeline.clone()) else {
        return Ok(Flow::Continue);
    };
    if !run.running {
        return Ok(Flow::Continue);
    }
    let def = ctx
        .def
        .timelines
        .get(run.timeline.index().unwrap_or(usize::MAX))
        .ok_or_else(|| SimError::invariant(format!("instance {} runs unknown timeline {}", me.0, run.timeline.0)))?;
    let lo = run.position.get();
    let hi = (run.position + run.speed).get();
    let mut due: Vec<u32> = if hi >= lo {
        def.moments
            .iter()
            .copied()
            .filter(|&m| lo <= f64::from(m) && f64::from(m) < hi)
            .collect()
    } else {
        def.moments
            .iter()
            .rev()
            .copied()
            .filter(|&m| hi < f64::from(m) && f64::from(m) <= lo)
            .collect()
    };
    let last = def.moments.last().copied();
    let mut requested = false;
    for m in due.drain(..) {
        let before = ctx.w.room_requests;
        let flow = events::dispatch(ctx, me, &EventKind::Moment(m));
        if flow.is_halt() {
            return Ok(Flow::Halt);
        }
        if flow == Flow::AbortTick || ctx.w.room_requests != before {
            requested = true;
            if ctx.rules.tick.room_request == RoomRequestRule::AbortTick {
                break;
            }
        }
        if current(ctx, me) != Some((run.timeline, true)) {
            break;
        }
    }
    if current(ctx, me) == Some((run.timeline, true))
        && let Some(inst) = ctx.w.get_mut(me)
        && let Some(r) = inst.timeline.as_mut()
    {
        let next = r.position + r.speed;
        if !next.is_finite() {
            return Err(SimError::non_finite("timeline_position"));
        }
        r.position = next;
        if r.looping
            && let Some(last) = last
            && r.position.get() > f64::from(last)
        {
            r.position = Real::ZERO;
        }
    }
    Ok(if requested { Flow::AbortTick } else { Flow::Continue })
}
