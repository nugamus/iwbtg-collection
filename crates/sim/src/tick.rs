//! The tick pipeline (docs/plans/sim-core.md 2.1, 2.2).
//!
//! One function per row of the table in plan 2.1 (`events::run_phase_with`). Phase placement
//! that the spec marks Inferred or UNKNOWN is a `TickRules` field, never an `if` in built-ins.
//! WP0 declared; WP1 owns.
//!
//! CLONE_SPEC tick loop, Confirmed (recorder) order (traces/rely-the-needle-4-2/t-evo-1.csv):
//! `xprevious`/`yprevious` at tick start, Begin Step, timeline moments, Alarm, Keyboard, Step,
//! motion, End of Path, Outside Room, Intersect Boundary, Collision, End Step, Draw, then the
//! animation update with Animation End; then the destroy sweep and the room change.
//!
//! [`Hooks`] holds the calls into modules other WPs own, so the pipeline runs with test
//! stand-ins (the `Probe` built-in) before they merge; [`Hooks::ENGINE`] is the real set.

use crate::collide::{bbox, overlap, resolve};
use crate::def::{GameDef, IRect};
use crate::error::SimError;
use crate::events::{self, Phase};
use crate::input::InputFrame;
use crate::motion::{self, MotionEvents};
use crate::num::Real;
use crate::room::{self, RoomRequest};
use crate::rules::tick::{AnimSlot, BoundarySlot, DirectLatch, PrevCapture};
use crate::sound;
use crate::timing::{self, Split};
use crate::view;
use crate::world::{Ctx, Flow, InstId, World};

/// A collision event run inside [`Hooks::solid_resolve`].
pub type EventRun<'r> = &'r mut dyn FnMut(&mut Ctx) -> Flow;

/// The pipeline's calls into other WPs' modules, as plain function pointers.
#[derive(Clone, Copy, Debug)]
pub struct Hooks {
    /// `xprevious = x; yprevious = y` (WP2 `Motion::capture_previous`).
    pub capture_previous: fn(&mut Ctx, InstId) -> Result<(), SimError>,
    /// Automatic motion of one instance (WP2 `motion::apply_auto_motion`).
    pub auto_motion: fn(&mut Ctx, InstId) -> Result<MotionEvents, SimError>,
    /// The instance's box at its position, room space, inclusive (WP3 `bbox::instance_box`);
    /// `None` without a mask.
    pub bbox: fn(&Ctx, InstId) -> Result<Option<IRect>, SimError>,
    /// Whether two instances overlap at their positions (WP3 `overlap::overlaps`).
    pub overlaps: fn(&Ctx, InstId, InstId) -> Result<bool, SimError>,
    /// The solid put-back around one collision event (WP3 `resolve::solid_resolve`).
    pub solid_resolve: fn(&mut Ctx, InstId, InstId, EventRun) -> Result<Flow, SimError>,
    /// View follow, once per tick before the Draw slot (WP4b `view::follow_update`, R19).
    pub view_follow: fn(&mut Ctx) -> Result<(), SimError>,
    /// The room change sequence (WP4a `room::perform_change`).
    pub room_change: fn(&mut Ctx, RoomRequest) -> Result<(), SimError>,
    /// Sound expiry, once per tick at row 16 (WP10 `sound::tick`).
    pub sound_tick: fn(&mut Ctx),
    /// Timing triggers at row 16 (WP8b `timing::evaluate`).
    pub timing: fn(&mut World, &GameDef) -> Result<Vec<Split>, SimError>,
}

fn engine_capture_previous(ctx: &mut Ctx, id: InstId) -> Result<(), SimError> {
    if let Some(inst) = ctx.w.get_mut(id) {
        inst.m.capture_previous();
    }
    Ok(())
}

fn engine_auto_motion(ctx: &mut Ctx, id: InstId) -> Result<MotionEvents, SimError> {
    let def = ctx.def;
    let rules = &ctx.rules.motion;
    match ctx.w.get_mut(id) {
        Some(inst) => motion::apply_auto_motion(&mut inst.m, inst.path.as_mut(), def, rules),
        None => Ok(MotionEvents::default()),
    }
}

fn engine_bbox(ctx: &Ctx, id: InstId) -> Result<Option<IRect>, SimError> {
    let Some(inst) = ctx.w.get(id) else {
        return Ok(None);
    };
    let at = (inst.m.x(), inst.m.y());
    Ok(bbox::instance_box(ctx.def, inst, at, &ctx.rules.collide)?.map(|p| p.bbox))
}

fn engine_overlaps(ctx: &Ctx, a: InstId, b: InstId) -> Result<bool, SimError> {
    let (Some(ia), Some(ib)) = (ctx.w.get(a), ctx.w.get(b)) else {
        return Ok(false);
    };
    let rules = &ctx.rules.collide;
    let pa = bbox::instance_box(ctx.def, ia, (ia.m.x(), ia.m.y()), rules)?;
    let pb = bbox::instance_box(ctx.def, ib, (ib.m.x(), ib.m.y()), rules)?;
    match (pa, pb) {
        (Some(pa), Some(pb)) => overlap::overlaps(&pa, &pb),
        _ => Ok(false),
    }
}

fn engine_solid_resolve(ctx: &mut Ctx, a: InstId, b: InstId, run: EventRun) -> Result<Flow, SimError> {
    resolve::solid_resolve(ctx, a, b, |c| run(c))
}

impl Hooks {
    /// The real modules.
    pub const ENGINE: Hooks = Hooks {
        capture_previous: engine_capture_previous,
        auto_motion: engine_auto_motion,
        bbox: engine_bbox,
        overlaps: engine_overlaps,
        solid_resolve: engine_solid_resolve,
        view_follow: view::follow_update,
        room_change: room::perform_change,
        sound_tick: sound::tick,
        timing: timing::evaluate,
    };
}

/// Runs one tick (rows 0-16) and returns the splits it recorded. An error halts the sim. When a
/// phase returns `Flow::Halt` (the world is already `Halted`), the tick stops there: no sweep,
/// no room change, no `tick += 1`. Uses [`Hooks::ENGINE`]; see [`run_tick_with`].
pub fn run_tick(ctx: &mut Ctx, input: InputFrame) -> Result<Vec<Split>, SimError> {
    run_tick_with(ctx, input, &Hooks::ENGINE)
}

/// [`run_tick`] with explicit hooks.
///
/// A room request applies `TickRules::room_request` (R8): under `AbortTick` the rest of rows
/// 2-13 is skipped and the tick goes on at row 14. Row 15 resets `room_tick` to 0 after a room
/// change; otherwise row 16 counts it up.
pub fn run_tick_with(ctx: &mut Ctx, input: InputFrame, hooks: &Hooks) -> Result<Vec<Split>, SimError> {
    let rules = &ctx.rules.tick;
    // Row 0.
    ctx.w.room_requests = 0;
    ctx.scripts.begin_tick();
    ctx.w
        .input
        .latch_with(input, rules.direct_latch == DirectLatch::Latch);
    // Rows 1-13; `None` from `rows` means the sim halted.
    let Some(()) = rows(ctx, hooks) else {
        return Ok(Vec::new());
    };
    // Row 14.
    ctx.w.sweep();
    // Row 15.
    let changed = match ctx.w.pending.take() {
        Some(req) => {
            (hooks.room_change)(ctx, req)?;
            ctx.w.room_tick = 0;
            true
        }
        None => false,
    };
    ctx.w.room_requests = 0;
    ctx.scripts.begin_tick();
    // Row 16.
    (hooks.sound_tick)(ctx);
    ctx.w.tick = ctx
        .w
        .tick
        .checked_add(1)
        .ok_or_else(|| SimError::overflow("tick"))?;
    if !changed {
        ctx.w.room_tick = ctx
            .w
            .room_tick
            .checked_add(1)
            .ok_or_else(|| SimError::overflow("room_tick"))?;
    }
    ctx.w.flush_grid(ctx.def, &ctx.rules.collide)?;
    (hooks.timing)(ctx.w, ctx.def)
}

/// Rows 1-13. `Some(())` when the tick goes on to row 14 (normally or after a room request),
/// `None` when the sim halted.
fn rows(ctx: &mut Ctx, hooks: &Hooks) -> Option<()> {
    let rules = &ctx.rules.tick;
    let (prev, anim, boundary) = (rules.prev_capture, rules.anim_slot, rules.boundary_slot);
    let mut plan: Vec<Phase> = Vec::with_capacity(14);
    if prev == PrevCapture::TickStart {
        plan.push(Phase::CapturePrevious);
    }
    plan.extend([
        Phase::BeginStep,
        Phase::Timelines,
        Phase::Alarms,
        Phase::Keyboard,
        Phase::Mouse,
        Phase::Step,
        Phase::Motion,
    ]);
    if anim == AnimSlot::AfterMotion {
        plan.push(Phase::Animation);
    }
    if boundary == BoundarySlot::AfterMotion {
        plan.push(Phase::Boundary);
    }
    plan.push(Phase::Collision);
    if boundary == BoundarySlot::AfterCollision {
        plan.push(Phase::Boundary);
    }
    plan.extend([Phase::EndStep, Phase::Draw]);
    if anim == AnimSlot::AfterDraw {
        plan.push(Phase::Animation);
    }
    for phase in plan {
        if phase == Phase::Draw
            && let Err(e) = (hooks.view_follow)(ctx)
        {
            ctx.halt(e);
            return None;
        }
        match events::run_phase_with(ctx, phase, hooks) {
            Flow::Continue => {}
            Flow::AbortTick => return Some(()),
            Flow::Halt => return None,
        }
    }
    Some(())
}

/// GML `fps` (R28, Design): the game's tick rate, never a measured frame rate.
pub fn real_time_fps(def: &GameDef) -> Real {
    Real::new(f64::from(def.tick_rate))
}

/// GML `current_time` and other millisecond clocks (R28, Design): `tick * 1000 / tick_rate`,
/// rounded down, from the run's tick count, never a clock.
pub fn real_time_ms(w: &World, def: &GameDef) -> Result<Real, SimError> {
    let rate = u64::from(def.tick_rate);
    let ms = w
        .tick
        .checked_mul(1000)
        .and_then(|t| t.checked_div(rate))
        .ok_or_else(|| SimError::overflow("current_time"))?;
    // Exact below 2^53 ms (285,000 years of ticks).
    Ok(Real::new(ms as f64))
}
