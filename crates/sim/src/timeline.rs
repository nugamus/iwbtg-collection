//! Running timelines (docs/architecture/sim/tick.md row 3, R6, R6b).
//!
//! One system in the R30 tick scheduler: `TimingRules::timelines` declares a moment to be a tick
//! offset, and `TimingRules::timeline_slot` places the phase that advances the position.
//!
//! CLONE_SPEC sound section, Confirmed (recorder): a timeline set at position 0 during room
//! entry runs moment 0 on the room's first tick, and moment N runs N ticks later;
//! `timeline_position` reads 1 after that first tick (traces/eat-the-rori-girl/audio-rori-sync-b.csv
//! rows 2-9801, traces/clear-the-easy-miku-2/audio-miku-sync-mute-a.csv). So each tick runs the
//! moments in `[position, position + speed)` in order, then adds the speed. A timeline set during
//! a tick (moment 0 that tick or the next) is not traced; this rule gives the next tick.
//! Looping and negative speeds are not in the spec (Design, see [`advance`]).
//!
//! The order of a tick's due moments is [`TimelineModel`]: `Moments` (the `gm8` default) keeps
//! the recorded declaration order, reversed for a negative speed; `DatedCallbacks` (the
//! `standard` value) treats each moment as a dated callback and runs the due set by deadline,
//! then declaration index. [`due_moments`] is the one place that decides it.

use serde::{Deserialize, Serialize};

use crate::def::{TimelineDef, TimelineId};
use crate::error::SimError;
use crate::events::{self, EventKind};
use crate::num::Real;
use crate::rules::tick::{RoomRequestRule, TimelineModel};
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
    let (speed, looping) = run.as_ref().map_or((Real::ONE, false), |r| (r.speed, r.looping));
    *run = Some(TimelineRun {
        timeline,
        position,
        speed,
        running: true,
        looping,
    });
    Ok(())
}

/// [`start`], then sets `timeline_loop` when the def's timeline has `loop = true`. Starters that
/// have the game def (trigger actions, the script host) call this one.
pub fn start_with_def(
    run: &mut Option<TimelineRun>,
    def: &crate::def::GameDef,
    timeline: TimelineId,
    position: Real,
) -> Result<(), SimError> {
    start(run, timeline, position)?;
    let flagged = timeline
        .index()
        .and_then(|i| def.timelines.get(i))
        .is_some_and(|t| t.looping);
    if flagged && let Some(r) = run.as_mut() {
        r.looping = true;
    }
    Ok(())
}

/// The run's timeline id and whether it still runs, for checks between moments.
fn current(ctx: &Ctx, me: InstId) -> Option<(TimelineId, bool)> {
    if !ctx.w.visible(me, false) {
        return None;
    }
    let inst = ctx.w.get(me)?;
    inst.timeline.as_ref().map(|r| (r.timeline, r.running))
}

/// The moments due in this tick's advance `[position, position + speed)`, in the order they run
/// (R6). [`TimelineModel::Moments`] keeps the recorded declaration order, reversed for a
/// negative speed (Design); [`TimelineModel::DatedCallbacks`] sorts the same due set by
/// deadline, then declaration index, whatever the direction.
fn due_moments(def: &TimelineDef, position: Real, speed: Real, model: TimelineModel) -> Vec<u32> {
    let lo = position.get();
    let hi = (position + speed).get();
    let mut due: Vec<(usize, u32)> = if hi >= lo {
        def.moments
            .iter()
            .copied()
            .enumerate()
            .filter(|&(_, m)| lo <= f64::from(m) && f64::from(m) < hi)
            .collect()
    } else {
        let mut due: Vec<(usize, u32)> = def
            .moments
            .iter()
            .copied()
            .enumerate()
            .filter(|&(_, m)| hi < f64::from(m) && f64::from(m) <= lo)
            .collect();
        due.reverse();
        due
    };
    if model == TimelineModel::DatedCallbacks {
        due.sort_by_key(|&(index, moment)| (moment, index));
    }
    due.into_iter().map(|(_, moment)| moment).collect()
}

/// Tick row 3 for one instance: run the due moments (see [`due_moments`] for the order under
/// each [`TimelineModel`]), then add the speed to the position the moments left. A looping
/// timeline whose position has passed its last moment starts again at 0 (Design; the manual
/// only says it restarts at the end).
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
        .ok_or_else(|| {
            SimError::invariant(format!(
                "instance {} runs unknown timeline {}",
                me.0, run.timeline.0
            ))
        })?;
    let mut due = due_moments(def, run.position, run.speed, ctx.rules.tick.timing.timeline_model);
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
    Ok(if requested {
        Flow::AbortTick
    } else {
        Flow::Continue
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::TimelineDef;

    fn def(moments: Vec<u32>) -> TimelineDef {
        TimelineDef {
            name: "t".into(),
            moments,
            ..TimelineDef::default()
        }
    }

    #[test]
    fn moments_keep_the_recorded_order_and_reverse_for_a_negative_speed() {
        let d = def(vec![0, 1, 2, 3]);
        assert_eq!(
            due_moments(&d, Real::new(1.0), Real::new(2.0), TimelineModel::Moments),
            vec![1, 2]
        );
        assert_eq!(
            due_moments(&d, Real::new(2.0), Real::new(-2.0), TimelineModel::Moments),
            vec![2, 1]
        );
    }

    #[test]
    fn dated_callbacks_run_by_deadline_then_declaration_index() {
        // The loader requires ascending unique moments; the ordering function is defined for
        // any declaration order, which makes the rule visible here.
        let d = def(vec![3, 0, 2, 1]);
        assert_eq!(
            due_moments(&d, Real::ZERO, Real::new(4.0), TimelineModel::Moments),
            vec![3, 0, 2, 1],
            "gm8 keeps declaration order"
        );
        assert_eq!(
            due_moments(&d, Real::ZERO, Real::new(4.0), TimelineModel::DatedCallbacks),
            vec![0, 1, 2, 3]
        );
        // A negative speed does not reorder dated callbacks.
        assert_eq!(
            due_moments(&d, Real::new(3.0), Real::new(-3.0), TimelineModel::DatedCallbacks),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn a_partial_advance_runs_only_the_moments_it_reaches() {
        let d = def(vec![0, 1, 2, 3]);
        assert_eq!(
            due_moments(&d, Real::new(1.0), Real::new(2.0), TimelineModel::DatedCallbacks),
            vec![1, 2],
            "moment 3 is outside [1, 3)"
        );
        assert_eq!(
            due_moments(&d, Real::new(2.0), Real::ZERO, TimelineModel::DatedCallbacks),
            Vec::<u32>::new()
        );
    }
}
