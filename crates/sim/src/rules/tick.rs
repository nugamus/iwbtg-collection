//! Tick pipeline switches (docs/plans/sim-core.md 2, register R1-R10, R21, R22).
//!
//! WP0 declares every field with its engine_rev 0 default; WP1 owns the file.

/// Rules for the tick pipeline and event dispatch.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TickRules {
    /// R1 (UNKNOWN U1): when `xprevious`/`yprevious` are captured.
    pub prev_capture: PrevCapture,
    /// R2 (UNKNOWN U2): instance order within a phase.
    pub dispatch: DispatchOrder,
    /// R21 (UNKNOWN): which instance is "the first" in queries, the kid, view follow, saveGame.
    pub first_rule: FirstRule,
    /// R6 (UNKNOWN U6): where timelines advance.
    pub timeline_slot: TimelineSlot,
    /// R7 (Conflict): where the animation update runs.
    pub anim_slot: AnimSlot,
    /// R5 (Inferred / UNKNOWN U4): where Outside Room and Intersect Boundary run.
    pub boundary_slot: BoundarySlot,
    /// R8 (Inferred / UNKNOWN U8): what still runs after a room request.
    pub room_request: RoomRequestRule,
    /// R9 (UNKNOWN U10): an instance created during a phase runs that phase too. Default false,
    /// an engine_rev 0 placeholder: CLONE_SPEC tick loop now records (recorder) that it does
    /// when its object comes later in the order than the creator's, and not for the creator's
    /// own object. A bool cannot say that; WP1 widens this field and WP11 changes the default.
    pub new_instances_join_phase: bool,
    /// R22: an instance destroyed this tick is still seen by collisions and queries. Default
    /// false (Design).
    pub dead_in_queries: bool,
}

/// R1: when `xprevious`/`yprevious` are captured.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PrevCapture {
    /// At the start of the tick (tick row 1).
    TickStart,
    /// Right before automatic motion (tick row 8). Default.
    #[default]
    BeforeMotion,
}

/// R2: instance order within one phase (`events::dispatch_order`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DispatchOrder {
    /// Object index order (rank), then creation order (Inferred). Default.
    #[default]
    RankThenCreation,
    /// Creation order only.
    Creation,
    /// Depth, then rank, then creation (the spec lists depth as UNKNOWN; T-ORD-1).
    DepthThenRankThenCreation,
}

/// R21: which instance is "the first" (`World::first_of`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FirstRule {
    /// The first in [`TickRules::dispatch`] order. Default.
    #[default]
    DispatchOrder,
    /// The lowest instance id.
    LowestId,
}

/// R6: where timelines advance.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TimelineSlot {
    /// After Begin Step, before alarms (Design). Default.
    #[default]
    AfterBeginStep,
}

/// R7: where the animation update runs.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AnimSlot {
    /// After the Draw slot (tick Inferred 13). Default.
    #[default]
    AfterDraw,
    /// After motion, before collisions (sprites Inferred).
    AfterMotion,
}

/// R5: where Outside Room and Intersect Boundary run.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BoundarySlot {
    /// Right after automatic motion. Default.
    #[default]
    AfterMotion,
    /// After the collision phase.
    AfterCollision,
}

/// R8: what still runs in the tick after a handler requests a room change.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RoomRequestRule {
    /// No further events this tick; jump to the sweep and the room change (Inferred). Default.
    #[default]
    AbortTick,
    /// Finish the current phase, then jump.
    FinishPhase,
    /// Finish the whole tick.
    FinishTick,
}
