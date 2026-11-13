//! Events and dispatch (docs/plans/sim-core.md 1.3, 2.1, 2.3).
//!
//! WP0 declared the enums, `EventSet` and `ProbeState`; WP1 owns the file: dispatch, the phase
//! walker every row of the tick table uses, the collision phase, the Draw slot and the `Probe`
//! test built-in.
//!
//! **Phase walk** (CLONE_SPEC tick loop, Confirmed (recorder)). A phase takes its order once at
//! its start ([`dispatch_order`]: object index order, then creation order,
//! traces/rely-the-needle-4-2/t-ord-1.csv) and visits each instance that is still alive and
//! active when its turn comes. An instance created during the phase joins it per
//! `TickRules::new_instances_join_phase` (R9). After each handler, a room request applies
//! `TickRules::room_request` (R8); `Flow::Halt` stops everything.
//!
//! Calls into modules other WPs own (motion, collision geometry, views, rooms, sound, timing)
//! go through [`Hooks`], so the pipeline can be tested with the `Probe` built-in before those
//! WPs merge. [`Hooks::ENGINE`] is the real set.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::alarm;
use crate::anim;
use crate::def::{ClassMask, GameDef, KindId, ParamValue, Params, TimelineId, VarId};
use crate::error::SimError;
use crate::input::Action;
use crate::num::{Real, to_int_trunc};
use crate::room::RoomRequest;
use crate::rules::TickRules;
use crate::rules::collide::PairOrder;
use crate::rules::tick::{JoinPhase, PrevCapture, RoomRequestRule};
use crate::tick::Hooks;
use crate::timeline;
use crate::world::{Ctx, Flow, InstId, Instance, KindState, SimStatus, SpawnInit, Value, World};
use crate::{builtins, script};

pub use crate::rules::tick::DispatchOrder;

/// One GM event an instance can receive. Keys are actions: the GM "Key Q pressed" kid event is
/// `KeyPress(Suicide)`, "Key R pressed" on the world is `KeyPress(Restart)`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EventKind {
    /// Create.
    Create,
    /// Destroy.
    Destroy,
    /// Begin Step.
    BeginStep,
    /// Alarm `n` (0-11).
    Alarm(u8),
    /// Keyboard (held).
    KeyHeld(Action),
    /// Key press.
    KeyPress(Action),
    /// Key release.
    KeyRelease(Action),
    /// Step.
    Step,
    /// Outside Room.
    OutsideRoom,
    /// Intersect Boundary.
    IntersectBoundary,
    /// Collision with an instance of handler class `with`.
    Collision {
        /// The handler class that matched.
        with: ClassMask,
        /// The other instance.
        other: InstId,
    },
    /// End Step.
    EndStep,
    /// The Draw slot's gameplay handlers (R15).
    Late,
    /// Animation End.
    AnimationEnd,
    /// Room Start.
    RoomStart,
    /// Room End.
    RoomEnd,
    /// Game Start.
    GameStart,
    /// User event `n`.
    User(u8),
    /// A timeline moment at this step.
    Moment(u32),
    /// End of Path.
    EndOfPath,
}

/// What a kind handles; built once per kind at load and cached in `KindDef::events`.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct EventSet {
    /// Begin Step.
    pub begin_step: bool,
    /// Step.
    pub step: bool,
    /// End Step.
    pub end_step: bool,
    /// Draw-slot gameplay handler.
    pub late: bool,
    /// Bit `n`: alarm `n`. An alarm ticks only when the object handles it (Confirmed).
    pub alarms: u16,
    /// Bit per action (`Action::bit`): Keyboard (held) events.
    pub key_held: u32,
    /// Bit per action: Key Press events.
    pub key_press: u32,
    /// Bit per action: Key Release events.
    pub key_release: u32,
    /// Collision handler classes, in handler order. One pair runs only one of them
    /// ([`pair_handler`]).
    pub collisions: SmallVec<[ClassMask; 4]>,
    /// Outside Room.
    pub outside_room: bool,
    /// Intersect Boundary.
    pub intersect_boundary: bool,
    /// Animation End.
    pub animation_end: bool,
    /// Room Start.
    pub room_start: bool,
    /// Room End.
    pub room_end: bool,
    /// Bit `n`: user event `n`.
    pub user: u16,
}

/// The rows of the tick pipeline (plan 2.1), for `run_phase` and `--stages` probe records.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Phase {
    /// Row 0: latch input.
    LatchInput,
    /// Row 1: capture `xprevious`/`yprevious` if `prev_capture = TickStart`.
    CapturePrevious,
    /// Row 2.
    BeginStep,
    /// Row 3.
    Timelines,
    /// Row 4.
    Alarms,
    /// Row 5: held, then press, then release.
    Keyboard,
    /// Row 6: kept for order; no gameplay use.
    Mouse,
    /// Row 7.
    Step,
    /// Row 8 (and 8b when `anim_slot = AfterMotion`).
    Motion,
    /// Row 9: Outside Room, Intersect Boundary.
    Boundary,
    /// Row 10.
    Collision,
    /// Row 11.
    EndStep,
    /// Row 12: `Late` handlers in draw order, visible instances only.
    Draw,
    /// Row 13 when `anim_slot = AfterDraw`.
    Animation,
    /// Row 14.
    Sweep,
    /// Row 15.
    RoomChange,
    /// Row 16.
    TickEnd,
    /// Room change step 7.
    RoomStart,
    /// Room change step 1.
    RoomEnd,
    /// Room change step 5, first room only.
    GameStart,
}

/// Order of the three keyboard event kinds inside row 5 (R4, UNKNOWN U11).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyEdge {
    /// Keyboard (held).
    Held,
    /// Key Press.
    Press,
    /// Key Release.
    Release,
}

// ---- Order ----

/// Which order a walk uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Order {
    /// `TickRules::dispatch` (R2).
    Dispatch,
    /// Draw order: depth descending, then creation (Confirmed manual).
    Draw,
    /// Rank, then creation, whatever `TickRules::dispatch` says: the collision phase's outer
    /// instances (R12).
    Rank,
}

/// A sort key. `group` is everything but the creation order; R9's `LaterInOrder` compares it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) struct OrderKey {
    group: (u64, u16),
    id: InstId,
}

/// `depth` as a key that sorts higher depths first. -0 and +0 are one depth.
fn depth_desc(depth: Real) -> u64 {
    let d = depth.get();
    let bits = if d == 0.0 { 0.0f64.to_bits() } else { d.to_bits() };
    let ascending = if bits >> 63 == 1 { !bits } else { bits | 1 << 63 };
    !ascending
}

/// The sort key of `inst` in `order`.
pub(crate) fn order_key(inst: &Instance, mode: DispatchOrder, order: Order) -> OrderKey {
    let group = match (order, mode) {
        (Order::Draw, _) => (depth_desc(inst.depth), 0),
        (Order::Rank, _) => (0, inst.rank),
        (Order::Dispatch, DispatchOrder::RankThenCreation) => (0, inst.rank),
        (Order::Dispatch, DispatchOrder::Creation) => (0, 0),
        (Order::Dispatch, DispatchOrder::DepthThenRankThenCreation) => (depth_desc(inst.depth), inst.rank),
    };
    OrderKey { group, id: inst.id }
}

/// The instances a phase visits, in dispatch order (R2), among the alive, active instances
/// `filter` accepts.
pub fn dispatch_order(w: &World, rules: &TickRules, filter: impl Fn(&Instance) -> bool) -> Vec<InstId> {
    let mut keyed: Vec<OrderKey> = w
        .instances()
        .iter()
        .filter(|i| i.alive && i.active && filter(i))
        .map(|i| order_key(i, rules.dispatch, Order::Dispatch))
        .collect();
    keyed.sort_unstable();
    keyed.into_iter().map(|k| k.id).collect()
}

/// Instances in draw order (depth descending, then creation; Confirmed manual) among the alive,
/// active, visible instances `filter` accepts. The order the client draws in.
pub fn draw_order(w: &World, filter: impl Fn(&Instance) -> bool) -> Vec<InstId> {
    let mut keyed: Vec<OrderKey> = w
        .instances()
        .iter()
        .filter(|i| i.alive && i.active && i.visible && filter(i))
        .map(|i| order_key(i, DispatchOrder::Creation, Order::Draw))
        .collect();
    keyed.sort_unstable();
    keyed.into_iter().map(|k| k.id).collect()
}

/// Keyboard event order inside row 5: held, press, release; actions in bit order (R4).
pub fn key_event_order() -> [KeyEdge; 3] {
    [KeyEdge::Held, KeyEdge::Press, KeyEdge::Release]
}

/// The event set of a kind that does not exist (never for a loaded game).
static NO_EVENTS: LazyLock<EventSet> = LazyLock::new(EventSet::default);

/// What `kind` handles (cached in the `GameDef`). An unknown kind handles nothing.
pub fn event_set(def: &GameDef, kind: KindId) -> &EventSet {
    def.kind(kind).map_or(&*NO_EVENTS, |k| &k.events)
}

// ---- Dispatch ----

/// Runs one event on one instance: the base built-in's handler (`builtins::on_event`, which
/// routes the kid to `kid::on_event`), then the script's (`script::on_event`, WP9) when the kind
/// has one. A handler's `Err` halts the sim through `Ctx::halt` and gives `Flow::Halt`.
///
/// Instances that are destroyed, deactivated or unknown get nothing (`Continue`). When the base
/// handler requests a room under `RoomRequestRule::AbortTick`, the script part does not run:
/// "not the later actions of the same event" (CLONE_SPEC tick loop, Confirmed (recorder),
/// traces/rely-the-needle-4-2/tr-room-02-c.csv row 13).
pub fn dispatch(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Flow {
    let Some(inst) = ctx.w.get(me).filter(|i| i.alive && i.active) else {
        return Flow::Continue;
    };
    let Some(kd) = ctx.def.kind(inst.kind) else {
        let e = SimError::invariant(format!("instance {} has unknown kind {}", me.0, inst.kind.0));
        return ctx.halt(e);
    };
    let has_script = kd.script.is_some();
    let before = ctx.w.room_requests;
    let flow = match builtins::on_event(ctx, me, ev) {
        Ok(f) => f,
        Err(e) => return ctx.halt(e),
    };
    let requested = flow == Flow::AbortTick || ctx.w.room_requests != before;
    if !has_script
        || flow.is_halt()
        || (requested && ctx.rules.tick.room_request == RoomRequestRule::AbortTick)
    {
        return flow;
    }
    match script::on_event(ctx, me, ev) {
        Ok(f) => flow.combine(f),
        Err(e) => ctx.halt(e),
    }
}

/// Phase bookkeeping for R8: whether a request under `FinishPhase` has been seen.
pub(crate) struct Ctl {
    rule: RoomRequestRule,
    aborted: bool,
}

impl Ctl {
    fn new(ctx: &Ctx) -> Ctl {
        Ctl {
            rule: ctx.rules.tick.room_request,
            aborted: false,
        }
    }

    /// After a handler that started with `room_requests == before` returned `flow`:
    /// `Some(flow)` when the phase must stop now.
    fn after(&mut self, ctx: &Ctx, flow: Flow, before: u32) -> Option<Flow> {
        if flow.is_halt() || matches!(ctx.w.status, SimStatus::Halted(_)) {
            return Some(Flow::Halt);
        }
        if flow == Flow::AbortTick || ctx.w.room_requests != before {
            match self.rule {
                RoomRequestRule::AbortTick => return Some(Flow::AbortTick),
                RoomRequestRule::FinishPhase => self.aborted = true,
                RoomRequestRule::FinishTick => {}
            }
        }
        None
    }

    /// The phase's flow once every instance had its turn.
    fn end(&self) -> Flow {
        if self.aborted {
            Flow::AbortTick
        } else {
            Flow::Continue
        }
    }
}

/// Dispatches `ev` to `me` and applies R8: `Some(flow)` when the phase must stop.
fn fire(ctx: &mut Ctx, ctl: &mut Ctl, me: InstId, ev: &EventKind) -> Option<Flow> {
    let before = ctx.w.room_requests;
    let flow = dispatch(ctx, me, ev);
    ctl.after(ctx, flow, before)
}

/// Halts with `e` and stops the phase.
fn stop(ctx: &mut Ctx, e: SimError) -> Option<Flow> {
    Some(ctx.halt(e))
}

/// Walks one phase: the order snapshot from its start, each instance that is still alive,
/// active and accepted by `filter` when its turn comes, and new instances per R9. `each`
/// returns `Some(flow)` to stop the phase.
fn walk(
    ctx: &mut Ctx,
    order: Order,
    filter: &dyn Fn(&Ctx, &Instance) -> bool,
    mut each: impl FnMut(&mut Ctx, InstId, &mut Ctl) -> Option<Flow>,
) -> Flow {
    let mode = ctx.rules.tick.dispatch;
    let join = ctx.rules.tick.new_instances_join_phase;
    let ready = |ctx: &Ctx, i: &Instance| i.alive && i.active && filter(ctx, i);
    let mut list: Vec<OrderKey> = ctx
        .w
        .instances()
        .iter()
        .filter(|i| ready(ctx, i))
        .map(|i| order_key(i, mode, order))
        .collect();
    list.sort_unstable();
    let mut ctl = Ctl::new(ctx);
    let mut seen = ctx.w.next_id;
    let mut pos = 0;
    while let Some(&current) = list.get(pos) {
        pos += 1;
        if ctx.w.get(current.id).is_some_and(|i| ready(ctx, i))
            && let Some(flow) = each(ctx, current.id, &mut ctl)
        {
            return flow;
        }
        if join != JoinPhase::Never && ctx.w.next_id != seen {
            let all = ctx.w.instances();
            let start = all.partition_point(|i| i.id.0 < seen);
            let joining: Vec<OrderKey> = all
                .get(start..)
                .unwrap_or_default()
                .iter()
                .filter(|i| ready(ctx, i))
                .map(|i| order_key(i, mode, order))
                .filter(|k| join == JoinPhase::Always || k.group > current.group)
                .collect();
            for k in joining {
                let rest = list.get(pos..).unwrap_or_default();
                let at = pos + rest.partition_point(|x| *x < k);
                list.insert(at, k);
            }
            seen = ctx.w.next_id;
        }
    }
    ctl.end()
}

/// Dispatches `ev` to each of `ids` in the given order, skipping instances that are destroyed
/// or deactivated by their turn, with R8 applied after each handler. For sequences whose order
/// is not the dispatch order: Room Start on room entry runs the placed instances in file order,
/// then the persistent ones (Confirmed (recorder), traces/rely-the-needle-4-2/t-room-1.csv row
/// 21); WP4a calls this with that list.
pub fn dispatch_each(ctx: &mut Ctx, ids: &[InstId], ev: &EventKind) -> Flow {
    let mut ctl = Ctl::new(ctx);
    for &id in ids {
        if ctx.w.exists(id)
            && let Some(flow) = fire(ctx, &mut ctl, id, ev)
        {
            return flow;
        }
    }
    ctl.end()
}

/// An event phase whose handlers are flagged in the event set.
fn simple_phase(ctx: &mut Ctx, handles: fn(&EventSet) -> bool, ev: EventKind) -> Flow {
    walk(
        ctx,
        Order::Dispatch,
        &|c: &Ctx, i: &Instance| handles(event_set(c.def, i.kind)),
        |c, id, ctl| fire(c, ctl, id, &ev),
    )
}

/// Runs an event phase: snapshot the dispatch order once, then dispatch to each instance that
/// is still alive and active. Returns `Halt` at once; handles `AbortTick` per
/// `TickRules::room_request` (R8). Uses [`Hooks::ENGINE`]; see [`run_phase_with`].
pub fn run_phase(ctx: &mut Ctx, phase: Phase) -> Flow {
    run_phase_with(ctx, phase, &Hooks::ENGINE)
}

/// [`run_phase`] with explicit hooks. Every row of plan 2.1 except row 0 (it needs the input
/// frame) and row 16 (`tick::run_tick_with` does both). `Draw` runs the Draw slot only; the view
/// follow before it is `tick`'s. `RoomStart` here uses the dispatch order; room entry uses
/// [`dispatch_each`] (see there).
pub fn run_phase_with(ctx: &mut Ctx, phase: Phase, hooks: &Hooks) -> Flow {
    match phase {
        Phase::LatchInput | Phase::Mouse | Phase::TickEnd => Flow::Continue,
        Phase::CapturePrevious => capture_previous_phase(ctx, hooks),
        Phase::BeginStep => simple_phase(ctx, |s| s.begin_step, EventKind::BeginStep),
        Phase::Timelines => timeline_phase(ctx),
        Phase::Alarms => alarm_phase(ctx),
        Phase::Keyboard => keyboard_phase(ctx),
        Phase::Step => simple_phase(ctx, |s| s.step, EventKind::Step),
        Phase::Motion => motion_phase(ctx, hooks),
        Phase::Boundary => boundary_phase(ctx, hooks),
        Phase::Collision => collision_phase_with(ctx, hooks),
        Phase::EndStep => simple_phase(ctx, |s| s.end_step, EventKind::EndStep),
        Phase::Draw => run_draw_phase(ctx),
        Phase::Animation => animation_phase(ctx),
        Phase::Sweep => {
            ctx.w.sweep();
            Flow::Continue
        }
        Phase::RoomChange => match ctx.w.pending.take() {
            Some(req) => match (hooks.room_change)(ctx, req) {
                Ok(()) => Flow::Continue,
                Err(e) => ctx.halt(e),
            },
            None => Flow::Continue,
        },
        Phase::RoomStart => simple_phase(ctx, |s| s.room_start, EventKind::RoomStart),
        Phase::RoomEnd => simple_phase(ctx, |s| s.room_end, EventKind::RoomEnd),
        Phase::GameStart => simple_phase(ctx, |_| true, EventKind::GameStart),
    }
}

/// Row 1: `xprevious = x`, `yprevious = y` for every alive, active instance (no events).
fn capture_previous_phase(ctx: &mut Ctx, hooks: &Hooks) -> Flow {
    let ids: Vec<InstId> = ctx
        .w
        .instances()
        .iter()
        .filter(|i| i.alive && i.active)
        .map(|i| i.id)
        .collect();
    for id in ids {
        if let Err(e) = (hooks.capture_previous)(ctx, id) {
            return ctx.halt(e);
        }
    }
    Flow::Continue
}

/// Row 3: running timelines advance and run their moments (R6, `timeline::advance`).
fn timeline_phase(ctx: &mut Ctx) -> Flow {
    walk(
        ctx,
        Order::Dispatch,
        &|_: &Ctx, i: &Instance| i.timeline.as_ref().is_some_and(|r| r.running),
        |c, id, ctl| {
            let before = c.w.room_requests;
            match timeline::advance(c, id) {
                Ok(flow) => ctl.after(c, flow, before),
                Err(e) => stop(c, e),
            }
        },
    )
}

/// Row 4: per instance, alarms 0 to 11 in index order, only those its kind handles (R3,
/// `alarm::tick`). An alarm set by an earlier alarm event of the same instance counts down in
/// the same phase (Confirmed (recorder), traces/go-the-jun-g-world/num-eps-alpha.csv row 45).
fn alarm_phase(ctx: &mut Ctx) -> Flow {
    walk(
        ctx,
        Order::Dispatch,
        &|c: &Ctx, i: &Instance| event_set(c.def, i.kind).alarms != 0,
        |c, id, ctl| {
            for n in 0u8..12 {
                let inst = c.w.get_mut_unmoved(id).filter(|i| i.alive && i.active)?;
                if c.def
                    .kind(inst.kind)
                    .is_none_or(|k| k.events.alarms & 1 << n == 0)
                {
                    continue;
                }
                let fired = inst.alarms.get_mut(usize::from(n)).is_some_and(alarm::tick);
                if fired && let Some(flow) = fire(c, ctl, id, &EventKind::Alarm(n)) {
                    return Some(flow);
                }
            }
            None
        },
    )
}

/// Row 5: per instance, held, then press, then release (`key_event_order`), actions in bit
/// order (R4).
fn keyboard_phase(ctx: &mut Ctx) -> Flow {
    walk(
        ctx,
        Order::Dispatch,
        &|c: &Ctx, i: &Instance| {
            let s = event_set(c.def, i.kind);
            let f = c.w.input.frame;
            (s.key_held & f.held) | (s.key_press & f.pressed) | (s.key_release & f.released) != 0
        },
        |c, id, ctl| {
            for edge in key_event_order() {
                for a in Action::ALL {
                    let inst = c.w.get(id).filter(|i| i.alive && i.active)?;
                    let s = event_set(c.def, inst.kind);
                    let f = c.w.input.frame;
                    let (handles, on, ev) = match edge {
                        KeyEdge::Held => (s.key_held, f.held, EventKind::KeyHeld(a)),
                        KeyEdge::Press => (s.key_press, f.pressed, EventKind::KeyPress(a)),
                        KeyEdge::Release => (s.key_release, f.released, EventKind::KeyRelease(a)),
                    };
                    if handles & on & a.bit() != 0
                        && let Some(flow) = fire(c, ctl, id, &ev)
                    {
                        return Some(flow);
                    }
                }
            }
            None
        },
    )
}

/// Row 8: per instance, `xprevious`/`yprevious` when `prev_capture = BeforeMotion`, then
/// automatic motion (WP2), then End of Path when the path ended (Confirmed (recorder): after
/// Step, before Outside Room and Collision, traces/rely-the-needle-4-2/t-evo-1.csv frame 13).
fn motion_phase(ctx: &mut Ctx, hooks: &Hooks) -> Flow {
    let capture = ctx.rules.tick.prev_capture == PrevCapture::BeforeMotion;
    let flow = walk(
        ctx,
        Order::Dispatch,
        &|_: &Ctx, _: &Instance| true,
        |c, id, ctl| {
            if capture && let Err(e) = (hooks.capture_previous)(c, id) {
                return stop(c, e);
            }
            match (hooks.auto_motion)(c, id) {
                Ok(ev) if ev.end_of_path => fire(c, ctl, id, &EventKind::EndOfPath),
                Ok(_) => None,
                Err(e) => stop(c, e),
            }
        },
    );
    if flow.is_halt() {
        return flow;
    }
    if let Err(e) = ctx.w.flush_grid(ctx.def, &ctx.rules.collide) {
        return ctx.halt(e);
    }
    flow
}

/// Row 9: Outside Room, then Intersect Boundary, per instance (R5).
///
/// Confirmed (recorder) at the right edge of an 800 px room, from the reported bbox: Intersect
/// Boundary from `bbox_right > 800` (none at 800), also at `bbox_left == 800`; Outside Room from
/// `bbox_left > 800` (none at 800), and then both every tick (traces/wanna/t-evo-2.csv rows
/// 16-29 with the pa.bbox columns; traces/rely-the-needle-4-2/t-evo-1.csv frames 14-35). So the
/// room is `[0, width] x [0, height]`: intersect means some part lies beyond an edge line, outside
/// means all of it does. The left, top and bottom edges follow by symmetry (Design). The box is
/// the collision box (`Hooks::bbox`); an instance without a mask gets neither event.
fn boundary_phase(ctx: &mut Ctx, hooks: &Hooks) -> Flow {
    let Some(room) = ctx.def.room(ctx.w.room) else {
        let e = SimError::invariant(format!("current room {} does not exist", ctx.w.room.0));
        return ctx.halt(e);
    };
    let (w, h) = (i64::from(room.width), i64::from(room.height));
    let outside = |b: &crate::def::IRect| b.right < 0 || b.left > w || b.bottom < 0 || b.top > h;
    let crosses = |b: &crate::def::IRect| b.left < 0 || b.right > w || b.top < 0 || b.bottom > h;
    walk(
        ctx,
        Order::Dispatch,
        &|c: &Ctx, i: &Instance| {
            let s = event_set(c.def, i.kind);
            s.outside_room || s.intersect_boundary
        },
        |c, id, ctl| {
            let bbox = match (hooks.bbox)(c, id) {
                Ok(Some(b)) => b,
                Ok(None) => return None,
                Err(e) => return stop(c, e),
            };
            let handles =
                |c: &Ctx, f: fn(&EventSet) -> bool| c.w.get(id).is_some_and(|i| f(event_set(c.def, i.kind)));
            if outside(&bbox)
                && handles(c, |s| s.outside_room)
                && let Some(flow) = fire(c, ctl, id, &EventKind::OutsideRoom)
            {
                return Some(flow);
            }
            if !c.w.exists(id) {
                return None;
            }
            let bbox = match (hooks.bbox)(c, id) {
                Ok(Some(b)) => b,
                Ok(None) => return None,
                Err(e) => return stop(c, e),
            };
            if crosses(&bbox) && handles(c, |s| s.intersect_boundary) {
                return fire(c, ctl, id, &EventKind::IntersectBoundary);
            }
            None
        },
    )
}

/// Row 12: `Late` handlers in draw order (depth descending, then creation), visible only
/// (Confirmed manual: Draw runs only for visible instances).
pub fn run_draw_phase(ctx: &mut Ctx) -> Flow {
    walk(
        ctx,
        Order::Draw,
        &|c: &Ctx, i: &Instance| i.visible && event_set(c.def, i.kind).late,
        |c, id, ctl| fire(c, ctl, id, &EventKind::Late),
    )
}

/// Rows 8b and 13: the animation update for every alive, active instance, visible or not, and
/// Animation End where it fires and the kind handles it (`anim::advance`, R7).
fn animation_phase(ctx: &mut Ctx) -> Flow {
    walk(
        ctx,
        Order::Dispatch,
        &|_: &Ctx, _: &Instance| true,
        |c, id, ctl| {
            let def = c.def;
            let rules = &c.rules.tick;
            let inst = c.w.get_mut(id)?;
            let frames = inst
                .img
                .sprite()
                .and_then(|s| def.sprite(s))
                .map_or(0, |s| s.frames);
            let handles = def.kind(inst.kind).is_some_and(|k| k.events.animation_end);
            match anim::advance(&mut inst.img, frames, rules) {
                Ok(true) if handles => fire(c, ctl, id, &EventKind::AnimationEnd),
                Ok(_) => None,
                Err(e) => stop(c, e),
            }
        },
    )
}

/// Row 10: collision events with the solid put-back rule (plan 2.3). Uses
/// [`Hooks::ENGINE`]; see [`collision_phase_with`].
pub fn collision_phase(ctx: &mut Ctx) -> Flow {
    collision_phase_with(ctx, &Hooks::ENGINE)
}

/// [`collision_phase`] with explicit hooks.
///
/// Under the default `CollideRules::pair_order`, `LowerRankFirst` (R12, Confirmed (recorder),
/// OpenGMK only: CLONE_SPEC collision "Order of collision events: pairs of instances, lower
/// object first", traces/rely-the-needle-4-2/t-ord-2.md), the phase walks pairs of instances:
/// 1. the outer instance `a` goes by its own rank, then id (new instances join per R9);
/// 2. when `a`'s turn comes, the inner instance `b` is every other alive, active instance whose
///    rank is greater than or equal to `a`'s, by rank then id (lower ranks are not met again);
/// 3. per pair that overlaps when it is reached, `a`'s handler for `b` runs, then `b`'s handler
///    for `a`, right there; two instances of one rank meet twice (`a1`-`a2` in `a1`'s turn,
///    `a2`-`a1` in `a2`'s), both directions each time.
///
/// The alternatives `OtherRankThenId` and `HandlerListOrder` go instance by instance in
/// dispatch order, each instance with its own pairs, sorted by the other's rank and id, or by
/// handler list.
///
/// Either way, one direction of a pair (`me` with handlers, `other`):
/// - runs one handler: the matching handler class most specific for `other` (lowest
///   [`ClassMask::generality`], then first in handler order), the nearest-ancestor rule (C8,
///   R12b; Confirmed (recorder): a platform runs only the kid's platform event, and his block or
///   solidblock event only when he has none, traces/showdown/t-mot-08-a/b,
///   traces/help-the-fairy-tale/t-col-06-e/f);
/// - is skipped when `me` or `other` is no longer alive and active (R22);
/// - with a solid `other`, is re-checked for overlap first (C7, Confirmed (recorder),
///   traces/showdown/t-mot-07-a.csv) and runs inside `collide::solid_resolve` (WP3); with a
///   non-solid `other`, is re-checked under `CollideRules::recheck_nonsolid` (C7b) and runs at
///   the current position. The first direction of a `LowerRankFirst` pair follows its overlap
///   test directly, so it is not tested twice.
pub fn collision_phase_with(ctx: &mut Ctx, hooks: &Hooks) -> Flow {
    if ctx.rules.collide.pair_order != PairOrder::LowerRankFirst {
        return walk(
            ctx,
            Order::Dispatch,
            &|c: &Ctx, i: &Instance| !event_set(c.def, i.kind).collisions.is_empty(),
            |c, a, ctl| collide_one(c, hooks, a, ctl),
        );
    }
    // Instances that neither handle a collision nor match anyone's handler take part in no
    // event, so the walk leaves them out.
    let targeted = ctx
        .def
        .kinds
        .iter()
        .fold(ClassMask::NONE, |m, k| m | handler_classes(&k.events));
    walk(
        ctx,
        Order::Rank,
        &|c: &Ctx, i: &Instance| {
            i.classes.intersects(targeted) || !event_set(c.def, i.kind).collisions.is_empty()
        },
        |c, a, ctl| meet_all(c, hooks, a, ctl),
    )
}

/// Every handler class of an event set, as one mask.
fn handler_classes(events: &EventSet) -> ClassMask {
    events.collisions.iter().fold(ClassMask::NONE, |m, c| m | *c)
}

/// The one handler of `handlers` that runs for an other with classes `classes`, with its index:
/// the matching handler class most specific for it (lowest [`ClassMask::generality`]), then the
/// first in handler order; `None` when none matches (C8, R12b).
pub fn pair_handler(handlers: &[ClassMask], classes: ClassMask) -> Option<(usize, ClassMask)> {
    handlers
        .iter()
        .enumerate()
        .filter(|(_, with)| classes.intersects(**with))
        .min_by_key(|(handler, with)| (with.generality(), *handler))
        .map(|(handler, with)| (handler, *with))
}

/// `a`'s turn as the outer instance under `PairOrder::LowerRankFirst` (see
/// [`collision_phase_with`]). The inner list is taken when the turn starts.
fn meet_all(ctx: &mut Ctx, hooks: &Hooks, a: InstId, ctl: &mut Ctl) -> Option<Flow> {
    let def = ctx.def;
    let me = ctx.w.get(a)?;
    let (rank, classes) = (me.rank, me.classes);
    let mine = handler_classes(event_set(def, me.kind));
    let mut inner: Vec<(u16, InstId)> = ctx
        .w
        .instances()
        .iter()
        .filter(|b| b.id != a && b.alive && b.active && b.rank >= rank)
        .filter(|b| b.classes.intersects(mine) || classes.intersects(handler_classes(event_set(def, b.kind))))
        .map(|b| (b.rank, b.id))
        .collect();
    inner.sort_unstable();
    for (_, b) in inner {
        if !ctx.w.exists(a) {
            return None;
        }
        if !ctx.w.exists(b) {
            continue;
        }
        match (hooks.overlaps)(ctx, a, b) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(e) => return stop(ctx, e),
        }
        if let Some(flow) = one_way(ctx, hooks, ctl, a, b, true) {
            return Some(flow);
        }
        if let Some(flow) = one_way(ctx, hooks, ctl, b, a, false) {
            return Some(flow);
        }
    }
    None
}

/// `me`'s handler for `other`, if it has one, when both are still alive and active. `tested`:
/// the pair's overlap was tested right before, so it is not re-checked.
fn one_way(
    ctx: &mut Ctx,
    hooks: &Hooks,
    ctl: &mut Ctl,
    me: InstId,
    other: InstId,
    tested: bool,
) -> Option<Flow> {
    let def = ctx.def;
    let (Some(m), Some(o)) = (
        ctx.w.get(me).filter(|i| i.alive && i.active),
        ctx.w.get(other).filter(|i| i.alive && i.active),
    ) else {
        return None;
    };
    let (_, with) = pair_handler(&event_set(def, m.kind).collisions, o.classes)?;
    let solid = o.solid;
    if !tested && (solid || ctx.rules.collide.recheck_nonsolid) {
        match (hooks.overlaps)(ctx, me, other) {
            Ok(true) => {}
            Ok(false) => return None,
            Err(e) => return stop(ctx, e),
        }
    }
    run_pair(ctx, hooks, ctl, me, other, with, solid)
}

/// Runs `me`'s collision event with `other` (handler class `with`): inside
/// `collide::solid_resolve` when `other` is solid, else at the current position.
fn run_pair(
    ctx: &mut Ctx,
    hooks: &Hooks,
    ctl: &mut Ctl,
    me: InstId,
    other: InstId,
    with: ClassMask,
    solid: bool,
) -> Option<Flow> {
    let ev = EventKind::Collision { with, other };
    if !solid {
        return fire(ctx, ctl, me, &ev);
    }
    let before = ctx.w.room_requests;
    let mut run = |c: &mut Ctx| dispatch(c, me, &ev);
    match (hooks.solid_resolve)(ctx, me, other, &mut run) {
        Ok(flow) => ctl.after(ctx, flow, before),
        Err(e) => stop(ctx, e),
    }
}

/// One collision pair: the other instance, the handler class and its index in the handler list.
struct Pair {
    other: InstId,
    with: ClassMask,
    handler: usize,
}

/// Collision events of one instance `a` under `OtherRankThenId` or `HandlerListOrder` (see
/// [`collision_phase_with`]).
fn collide_one(ctx: &mut Ctx, hooks: &Hooks, a: InstId, ctl: &mut Ctl) -> Option<Flow> {
    let me = ctx.w.get(a)?;
    let handlers = event_set(ctx.def, me.kind).collisions.clone();
    let any = handlers.iter().fold(ClassMask::NONE, |m, c| m | *c);
    let candidates: Vec<InstId> = ctx
        .w
        .instances()
        .iter()
        .filter(|b| b.id != a && b.alive && b.active && b.classes.intersects(any))
        .map(|b| b.id)
        .collect();
    let mut pairs = Vec::new();
    for b in candidates {
        match (hooks.overlaps)(ctx, a, b) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(e) => return stop(ctx, e),
        }
        let classes = ctx.w.get(b).map_or(ClassMask::NONE, |i| i.classes);
        if let Some((handler, with)) = pair_handler(&handlers, classes) {
            pairs.push(Pair {
                other: b,
                with,
                handler,
            });
        }
    }
    let rules = &ctx.rules;
    let w = &*ctx.w;
    let rank_id = |id: InstId| w.get(id).map_or((u16::MAX, id), |i| (i.rank, i.id));
    let dispatch_key = |id: InstId| {
        w.get(id)
            .map(|i| order_key(i, rules.tick.dispatch, Order::Dispatch))
    };
    match rules.collide.pair_order {
        PairOrder::HandlerListOrder => pairs.sort_by_key(|p| (p.handler, dispatch_key(p.other))),
        PairOrder::OtherRankThenId | PairOrder::LowerRankFirst => {
            pairs.sort_by_key(|p| (rank_id(p.other), p.handler))
        }
    }
    for p in pairs {
        if !ctx.w.exists(a) {
            return None;
        }
        let Some(solid) = ctx
            .w
            .get(p.other)
            .filter(|b| b.alive && b.active)
            .map(|b| b.solid)
        else {
            continue;
        };
        if solid || ctx.rules.collide.recheck_nonsolid {
            match (hooks.overlaps)(ctx, a, p.other) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(e) => return stop(ctx, e),
            }
        }
        if let Some(flow) = run_pair(ctx, hooks, ctl, a, p.other, p.with, solid) {
            return Some(flow);
        }
    }
    None
}

// ---- Probe built-in ----

/// Which event a probe step reacts to.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ProbeOn {
    /// Create.
    Create,
    /// Destroy.
    Destroy,
    /// Begin Step.
    BeginStep,
    /// Alarm `n`.
    Alarm(u8),
    /// Any Keyboard (held) event.
    KeyHeld,
    /// Any Key press event.
    KeyPress,
    /// Any Key release event.
    KeyRelease,
    /// Step.
    Step,
    /// Outside Room.
    OutsideRoom,
    /// Intersect Boundary.
    IntersectBoundary,
    /// Any collision event.
    Collision,
    /// End Step.
    EndStep,
    /// The Draw slot.
    Late,
    /// Animation End.
    AnimationEnd,
    /// Room Start.
    RoomStart,
    /// Room End.
    RoomEnd,
    /// Game Start.
    GameStart,
    /// User event `n`.
    User(u8),
    /// Timeline moment `m`.
    Moment(u32),
    /// End of Path.
    EndOfPath,
}

impl ProbeOn {
    /// The probe trigger an event matches.
    pub fn of(ev: &EventKind) -> ProbeOn {
        match ev {
            EventKind::Create => ProbeOn::Create,
            EventKind::Destroy => ProbeOn::Destroy,
            EventKind::BeginStep => ProbeOn::BeginStep,
            EventKind::Alarm(n) => ProbeOn::Alarm(*n),
            EventKind::KeyHeld(_) => ProbeOn::KeyHeld,
            EventKind::KeyPress(_) => ProbeOn::KeyPress,
            EventKind::KeyRelease(_) => ProbeOn::KeyRelease,
            EventKind::Step => ProbeOn::Step,
            EventKind::OutsideRoom => ProbeOn::OutsideRoom,
            EventKind::IntersectBoundary => ProbeOn::IntersectBoundary,
            EventKind::Collision { .. } => ProbeOn::Collision,
            EventKind::EndStep => ProbeOn::EndStep,
            EventKind::Late => ProbeOn::Late,
            EventKind::AnimationEnd => ProbeOn::AnimationEnd,
            EventKind::RoomStart => ProbeOn::RoomStart,
            EventKind::RoomEnd => ProbeOn::RoomEnd,
            EventKind::GameStart => ProbeOn::GameStart,
            EventKind::User(n) => ProbeOn::User(*n),
            EventKind::Moment(m) => ProbeOn::Moment(*m),
            EventKind::EndOfPath => ProbeOn::EndOfPath,
        }
    }

    /// The research probes' one-letter code of an event, where they have one.
    pub fn letter(ev: &EventKind) -> Option<char> {
        Some(match ev {
            EventKind::Create => 'C',
            EventKind::BeginStep => 'b',
            EventKind::Moment(_) => 't',
            EventKind::Alarm(_) => 'a',
            EventKind::Step => 's',
            EventKind::EndOfPath => 'p',
            EventKind::OutsideRoom => 'o',
            EventKind::IntersectBoundary => 'i',
            EventKind::Collision { .. } => 'k',
            EventKind::EndStep => 'e',
            EventKind::Late => 'd',
            EventKind::AnimationEnd => 'n',
            _ => return None,
        })
    }

    /// The log code of an event, in the style of the research probes (t-evo-1: `b` Begin Step,
    /// `s` Step, `e` End Step, `d` Draw, ...).
    pub fn code(ev: &EventKind) -> String {
        match ev {
            EventKind::Create => "C".into(),
            EventKind::Destroy => "D".into(),
            EventKind::BeginStep => "b".into(),
            EventKind::Alarm(n) => format!("a{n}"),
            EventKind::KeyHeld(a) => format!("kh{}", a.name()),
            EventKind::KeyPress(a) => format!("kp{}", a.name()),
            EventKind::KeyRelease(a) => format!("kr{}", a.name()),
            EventKind::Step => "s".into(),
            EventKind::OutsideRoom => "o".into(),
            EventKind::IntersectBoundary => "i".into(),
            EventKind::Collision { other, .. } => format!("k{}", other.0),
            EventKind::EndStep => "e".into(),
            EventKind::Late => "d".into(),
            EventKind::AnimationEnd => "n".into(),
            EventKind::RoomStart => "R".into(),
            EventKind::RoomEnd => "E".into(),
            EventKind::GameStart => "G".into(),
            EventKind::User(n) => format!("u{n}"),
            EventKind::Moment(m) => format!("t{m}"),
            EventKind::EndOfPath => "p".into(),
        }
    }
}

/// One thing a probe does in an event.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ProbeAct {
    /// Append this text to the log global.
    Log(String),
    /// `alarm[n] = v` (`alarm::set`).
    SetAlarm(u8, Real),
    /// `global.<var> = value`.
    SetGlobal(VarId, Value),
    /// `instance_create`. With `child`, the new instance's probe state is set before its Create
    /// runs (still inside this action).
    Spawn {
        /// The kind.
        kind: KindId,
        /// Position.
        x: Real,
        /// Position.
        y: Real,
        /// Probe state for a new probe instance.
        child: Option<Box<ProbeState>>,
    },
    /// `instance_destroy()` on itself; the rest of the event still runs.
    DestroySelf,
    /// Destroy the other instance of a collision event.
    DestroyOther,
    /// Destroy this instance.
    Destroy(InstId),
    /// A room request.
    RequestRoom(RoomRequest),
    /// `keyboard_check_direct` on the action with this bit; logs `D1` or `D0`.
    DirectRead(u32),
    /// `instance_change`.
    ChangeKind(KindId, bool),
    /// Halt with a script error carrying this message.
    Fail(String),
    /// Set `visible`.
    SetVisible(bool),
    /// Set `depth`.
    SetDepth(Real),
    /// Set `solid`.
    SetSolid(bool),
    /// Set `image_speed`.
    SetImageSpeed(Real),
    /// Move by `(dx, dy)` through the motion setters (WP2).
    Move(Real, Real),
    /// Set Time Line at a position.
    StartTimeline(TimelineId, Real),
}

/// One probe reaction: an event, an optional tick filter, an action.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ProbeStep {
    /// The event.
    pub on: ProbeOn,
    /// Only on this world tick.
    pub at_tick: Option<u64>,
    /// Only on ticks that are a multiple of this (0 is never).
    pub every: Option<u64>,
    /// Only once; then `done`.
    pub once: bool,
    /// A `once` step that ran.
    pub done: bool,
    /// What it does.
    pub act: ProbeAct,
}

impl ProbeStep {
    /// A step that runs `act` on every `on` event.
    pub fn on(on: ProbeOn, act: ProbeAct) -> ProbeStep {
        ProbeStep {
            on,
            at_tick: None,
            every: None,
            once: false,
            done: false,
            act,
        }
    }

    /// Only on world tick `t`.
    pub fn at(mut self, t: u64) -> ProbeStep {
        self.at_tick = Some(t);
        self
    }

    /// Only on ticks that are a multiple of `n`.
    pub fn every(mut self, n: u64) -> ProbeStep {
        self.every = Some(n);
        self
    }

    /// Only once.
    pub fn once(mut self) -> ProbeStep {
        self.once = true;
        self
    }

    fn matches(&self, on: ProbeOn, tick: u64) -> bool {
        self.on == on
            && !self.done
            && self.at_tick.is_none_or(|t| t == tick)
            && self.every.is_none_or(|n| n != 0 && tick.is_multiple_of(n))
    }
}

/// How a probe writes each event it gets to its log.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum ProbeLog {
    /// `<code><tag> ` with [`ProbeOn::code`]. Default.
    #[default]
    Words,
    /// `<letter><tag>`, the research probes' letters (traces/rely-the-needle-4-2/t-evo-1.md):
    /// C Create, b Begin Step, t moment, a alarm, s Step, p End of Path, o Outside Room,
    /// i Intersect Boundary, k collision, e End Step, d Draw, n Animation End; other events as
    /// in `Words`.
    Letters,
    /// Nothing; only `ProbeAct::Log` writes.
    Silent,
}

/// The test probe's per-instance state: logs every event it gets to a global string and runs
/// its steps (it can move, spawn, destroy, request a room, read keys and fail). WP1 tests and
/// WP11 golden probe tests build worlds from it.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ProbeState {
    /// Appended after each event code in the log (`sA` is A's Step).
    pub tag: String,
    /// The global that collects the log; `None` logs nothing.
    pub log: Option<VarId>,
    /// How events are written to the log.
    pub style: ProbeLog,
    /// Events received.
    pub events: u64,
    /// Reactions, in order.
    pub steps: Vec<ProbeStep>,
}

impl ProbeState {
    /// A probe tagged `tag` that logs to `log`.
    pub fn new(tag: impl Into<String>, log: Option<VarId>) -> ProbeState {
        ProbeState {
            tag: tag.into(),
            log,
            style: ProbeLog::Words,
            events: 0,
            steps: Vec::new(),
        }
    }

    /// Sets the log style.
    pub fn style(mut self, style: ProbeLog) -> ProbeState {
        self.style = style;
        self
    }

    /// Adds a step.
    pub fn with(mut self, step: ProbeStep) -> ProbeState {
        self.steps.push(step);
        self
    }

    /// From params, for probes placed in rooms: `tag` (text) and `log` (the global's `VarId`
    /// as a real). Other params are ignored.
    pub fn from_params(params: &Params, _def: &GameDef) -> ProbeState {
        let tag = match params.0.get("tag") {
            Some(ParamValue::Str(s)) => s.clone(),
            _ => String::new(),
        };
        let log = match params.0.get("log") {
            Some(ParamValue::Real(r)) => to_int_trunc(*r)
                .ok()
                .and_then(|v| u32::try_from(v).ok())
                .map(VarId),
            _ => None,
        };
        ProbeState::new(tag, log)
    }
}

/// The `Probe` built-in's event set (`builtins::event_set` routes `Builtin::Probe` here): every
/// event except collisions, whose handler classes a test sets on the kind.
pub fn probe_event_set() -> EventSet {
    EventSet {
        begin_step: true,
        step: true,
        end_step: true,
        late: true,
        alarms: 0x0fff,
        key_held: Action::MASK,
        key_press: Action::MASK,
        key_release: Action::MASK,
        collisions: SmallVec::new(),
        outside_room: true,
        intersect_boundary: true,
        animation_end: true,
        room_start: true,
        room_end: true,
        user: 0xffff,
    }
}

/// Appends `text` to the string global `var` (a missing or non-string global starts empty).
fn append_log(w: &mut World, var: VarId, text: &str) {
    match w.globals.get_mut(&var) {
        Some(Value::Str(s)) => s.push_str(text),
        _ => {
            w.globals.insert(var, Value::Str(text.to_owned()));
        }
    }
}

/// The `Probe` built-in's handler (WP1 tests, WP11 golden probe tests); `builtins::on_event`
/// routes `Builtin::Probe` here. Logs `<code><tag> `, then runs the matching steps in order.
/// All of them run even after a destroy or a room request (one handler is one unit, R25);
/// `Halt` stops at once.
pub fn probe_on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    let tick = ctx.w.tick;
    let on = ProbeOn::of(ev);
    let Some(inst) = ctx.w.get_mut_unmoved(me) else {
        return Ok(Flow::Continue);
    };
    let KindState::Probe(p) = &mut inst.state else {
        return Err(SimError::invariant(format!(
            "probe instance {} has no probe state",
            me.0
        )));
    };
    p.events = p
        .events
        .checked_add(1)
        .ok_or_else(|| SimError::overflow("probe event count"))?;
    let mut acts = Vec::new();
    for step in p.steps.iter_mut().filter(|s| s.matches(on, tick)) {
        step.done = step.once;
        acts.push(step.act.clone());
    }
    let entry = match p.style {
        ProbeLog::Words => Some(format!("{}{} ", ProbeOn::code(ev), p.tag)),
        ProbeLog::Letters => Some(match ProbeOn::letter(ev) {
            Some(l) => format!("{l}{}", p.tag),
            None => format!("{}{} ", ProbeOn::code(ev), p.tag),
        }),
        ProbeLog::Silent => None,
    };
    let log = p.log;
    if let (Some(var), Some(entry)) = (log, entry) {
        append_log(ctx.w, var, &entry);
    }
    let mut flow = Flow::Continue;
    for act in acts {
        flow = flow.combine(probe_act(ctx, me, ev, act, log)?);
        if flow.is_halt() {
            return Ok(flow);
        }
    }
    Ok(flow)
}

/// Runs `f` on instance `me` if it is stored.
fn with_inst(
    ctx: &mut Ctx,
    me: InstId,
    f: &mut dyn FnMut(&mut Instance) -> Result<(), SimError>,
) -> Result<(), SimError> {
    match ctx.w.get_mut(me) {
        Some(inst) => f(inst),
        None => Ok(()),
    }
}

/// One probe action.
fn probe_act(
    ctx: &mut Ctx,
    me: InstId,
    ev: &EventKind,
    act: ProbeAct,
    log: Option<VarId>,
) -> Result<Flow, SimError> {
    match act {
        ProbeAct::Log(text) => {
            if let Some(var) = log {
                append_log(ctx.w, var, &text);
            }
        }
        ProbeAct::SetAlarm(n, v) => {
            let rules = &ctx.rules.num;
            if let Some(inst) = ctx.w.get_mut_unmoved(me) {
                alarm::set(inst, n, v, rules)?;
            }
        }
        ProbeAct::SetGlobal(var, value) => {
            ctx.w.globals.insert(var, value);
        }
        ProbeAct::Spawn { kind, x, y, child } => match child {
            None => {
                ctx.spawn(kind, x, y, SpawnInit::default())?;
            }
            Some(state) => {
                let init = SpawnInit {
                    run_create: false,
                    ..SpawnInit::default()
                };
                let id = ctx.spawn(kind, x, y, init)?;
                if let Some(inst) = ctx.w.get_mut_unmoved(id) {
                    inst.state = KindState::Probe(*state);
                }
                let flow = dispatch(ctx, id, &EventKind::Create);
                if flow.is_halt() {
                    return Ok(flow);
                }
            }
        },
        ProbeAct::DestroySelf => return Ok(ctx.destroy(me)),
        ProbeAct::DestroyOther => {
            if let EventKind::Collision { other, .. } = ev {
                return Ok(ctx.destroy(*other));
            }
        }
        ProbeAct::Destroy(id) => return Ok(ctx.destroy(id)),
        ProbeAct::RequestRoom(req) => {
            ctx.request_room(req);
            return Ok(Flow::AbortTick);
        }
        ProbeAct::DirectRead(bit) => {
            let Some(a) = Action::ALL.into_iter().find(|a| a.bit() == bit) else {
                return Err(SimError::invariant(format!(
                    "probe reads unknown action bit {bit}"
                )));
            };
            let down = ctx.w.input.direct(a);
            if let Some(var) = log {
                append_log(ctx.w, var, if down { "D1 " } else { "D0 " });
            }
        }
        ProbeAct::ChangeKind(kind, run_events) => return Ok(ctx.change_kind(me, kind, run_events)),
        ProbeAct::Fail(message) => return Err(SimError::Script { message }),
        ProbeAct::SetVisible(v) => with_inst(ctx, me, &mut |i| {
            i.visible = v;
            Ok(())
        })?,
        ProbeAct::SetDepth(d) => with_inst(ctx, me, &mut |i| {
            if !d.is_finite() {
                return Err(SimError::non_finite("depth"));
            }
            i.depth = d;
            Ok(())
        })?,
        ProbeAct::SetSolid(s) => with_inst(ctx, me, &mut |i| {
            i.solid = s;
            Ok(())
        })?,
        ProbeAct::SetImageSpeed(v) => with_inst(ctx, me, &mut |i| i.img.set_speed(v))?,
        ProbeAct::Move(dx, dy) => with_inst(ctx, me, &mut |i| {
            let (x, y) = (i.m.x() + dx, i.m.y() + dy);
            i.m.set_x(x)?;
            i.m.set_y(y)
        })?,
        ProbeAct::StartTimeline(tl, pos) => {
            let def = ctx.def;
            if let Some(inst) = ctx.w.get_mut_unmoved(me) {
                timeline::start_with_def(&mut inst.timeline, def, tl, pos)?;
            }
        }
    }
    Ok(Flow::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_key_sorts_higher_depth_first_and_merges_zeros() {
        let keys: Vec<u64> = [100.0, 50.0, 0.0, -0.0, -10.0, f64::MAX]
            .iter()
            .map(|d| depth_desc(Real::new(*d)))
            .collect();
        assert!(keys[5] < keys[0] && keys[0] < keys[1] && keys[1] < keys[2]);
        assert_eq!(keys[2], keys[3]);
        assert!(keys[3] < keys[4]);
    }
}
