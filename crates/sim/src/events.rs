//! Events and dispatch (docs/plans/sim-core.md 1.3, 2.3).
//!
//! WP0 declares the enums, `EventSet` and `ProbeState`; WP1 owns dispatch.
#![allow(unused_variables, reason = "WP0 stubs: WP1 fills the bodies")]

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::def::{ClassMask, GameDef, KindId};
use crate::error::SimError;
use crate::input::Action;
use crate::rules::TickRules;
use crate::world::{Ctx, Flow, InstId, Instance, World};

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
    pub key_held: u16,
    /// Bit per action: Key Press events.
    pub key_press: u16,
    /// Bit per action: Key Release events.
    pub key_release: u16,
    /// Collision handler classes, in handler order.
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

/// The test probe's per-instance state (logs every event; can move, spawn, destroy, request a
/// room). WP1 adds the fields.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ProbeState {}

/// The `Probe` built-in's event set (`builtins::event_set` routes `Builtin::Probe` here).
pub fn probe_event_set() -> EventSet {
    unimplemented!("WP1: events::probe_event_set")
}

/// The `Probe` built-in's handler (WP1 tests, WP11 golden probe tests); `builtins::on_event`
/// routes `Builtin::Probe` here.
pub fn probe_on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    unimplemented!("WP1: events::probe_on_event")
}

/// The instances a phase visits, in dispatch order (R2), among those `filter` accepts.
pub fn dispatch_order(w: &World, rules: &TickRules, filter: impl Fn(&Instance) -> bool) -> Vec<InstId> {
    unimplemented!("WP1: events::dispatch_order")
}

/// Keyboard event order inside row 5: held, press, release; actions in bit order (R4).
pub fn key_event_order() -> [KeyEdge; 3] {
    unimplemented!("WP1: events::key_event_order")
}

/// What `kind` handles (cached in the `GameDef`).
pub fn event_set(def: &GameDef, kind: KindId) -> &EventSet {
    unimplemented!("WP1: events::event_set")
}

/// Runs one event on one instance: the base built-in's handler (`kid::on_event` for the kid,
/// `builtins::on_event` for the rest), then the script's (`script::on_event`, WP9) when the kind
/// has one. A handler's `Err` halts the sim through `Ctx::halt` and gives `Flow::Halt`.
pub fn dispatch(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Flow {
    unimplemented!("WP1: events::dispatch")
}

/// Runs an event phase: snapshot the dispatch order once, then dispatch to each instance that
/// is still alive and active. Returns `Halt` at once; handles `AbortTick` per
/// `TickRules::room_request` (R8).
pub fn run_phase(ctx: &mut Ctx, phase: Phase) -> Flow {
    unimplemented!("WP1: events::run_phase")
}

/// Row 12: `Late` handlers in draw order (depth descending, then creation), visible only.
pub fn run_draw_phase(ctx: &mut Ctx) -> Flow {
    unimplemented!("WP1: events::run_draw_phase")
}

/// Row 10: collision events with the solid put-back rule (plan 2.3).
pub fn collision_phase(ctx: &mut Ctx) -> Flow {
    unimplemented!("WP1: events::collision_phase")
}
