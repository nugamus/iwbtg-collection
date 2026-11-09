//! Tick pipeline switches (docs/plans/sim-core.md 2, register R1-R10, R21, R22, I1).
//!
//! WP0 declared every field with its engine_rev 0 default; WP1 owns the file. Where the
//! CLONE_SPEC now records a recorder result (Confirmed (recorder), research a822655), the default
//! follows it and the field doc cites the trace; where the item is still open, the field keeps
//! the plan's default and says so.

/// Rules for the tick pipeline and event dispatch.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TickRules {
    /// R1 (U1, answered by the recorder): when `xprevious`/`yprevious` are captured.
    pub prev_capture: PrevCapture,
    /// R2 (U2, answered by the recorder): instance order within a phase.
    pub dispatch: DispatchOrder,
    /// R21 (answered by the recorder: the first in instance order, which for one object is the
    /// lowest id; famB-multikid-restart-b rows 25-26): which instance is "the first" in queries,
    /// the kid, view follow, saveGame.
    pub first_rule: FirstRule,
    /// R6 (U6, answered by the recorder): where timelines advance.
    pub timeline_slot: TimelineSlot,
    /// R7 (U7, answered by the recorder): where the animation update runs.
    pub anim_slot: AnimSlot,
    /// R7b (answered by the recorder): whether an instance without a sprite (or with a sprite of
    /// 0 frames) advances `image_index`.
    pub anim_nosprite: NoSpriteAnim,
    /// R7 (sprites Inferred step 3, not traced): a negative `image_speed` that takes
    /// `image_index` below 0.
    pub anim_below_zero: BelowZeroWrap,
    /// R5 (U4, answered by the recorder): where Outside Room and Intersect Boundary run.
    pub boundary_slot: BoundarySlot,
    /// R8 (U8, answered by the recorder for room_goto and game_restart): what still runs after a
    /// room request.
    pub room_request: RoomRequestRule,
    /// R9 (U10, answered by the recorder): whether an instance created during a phase runs that
    /// phase too.
    pub new_instances_join_phase: JoinPhase,
    /// R22: an instance destroyed this tick is still seen by collisions and queries. Default
    /// false (Design; spec item requested).
    pub dead_in_queries: bool,
    /// I1 (Inferred for the real runner; the recorder has no latch): the
    /// `keyboard_check_direct` "pressed since the last direct read" latch.
    pub direct_latch: DirectLatch,
    /// `instance_change` (not in the spec): which kind properties the instance takes.
    pub change_kind: ChangeKindProps,
}

/// R1: when `xprevious`/`yprevious` are captured.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PrevCapture {
    /// At the start of the tick, before Begin Step (tick row 1). Default: Confirmed (recorder),
    /// traces/wanna/t-prev-1.csv rows 11-41, traces/machinary/t-mot-02-a.csv rows 85-87.
    #[default]
    TickStart,
    /// Right before automatic motion (tick row 8). The plan's earlier guess.
    BeforeMotion,
}

/// R2: instance order within one phase (`events::dispatch_order`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DispatchOrder {
    /// Object index order (rank), then creation order. Default: Confirmed (recorder),
    /// traces/rely-the-needle-4-2/t-ord-1.csv ("depth does not matter").
    #[default]
    RankThenCreation,
    /// Creation order only.
    Creation,
    /// Depth (higher first, as drawing), then rank, then creation.
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
    /// After Begin Step, before alarms. Default: Confirmed (recorder),
    /// traces/rely-the-needle-4-2/t-evo-1.csv frames 5, 8, 11.
    #[default]
    AfterBeginStep,
}

/// R7: where the animation update runs.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AnimSlot {
    /// After the Draw slot (tick row 13). Default: Confirmed (recorder),
    /// traces/rely-the-needle-4-2/anim-save-end.csv row 22, anim-switch-g.csv rows 19-21.
    #[default]
    AfterDraw,
    /// After motion, before collisions (the sprites section's older Inferred text).
    AfterMotion,
}

/// R7b: animation of an instance with no sprite.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NoSpriteAnim {
    /// `image_index += image_speed` and never wraps. Default: Confirmed (recorder),
    /// traces/little-trap/anim-nosprite.csv rows 2-61.
    #[default]
    Advance,
    /// No advance (the plan's earlier default).
    Frozen,
}

/// R7: what an update that takes `image_index` below 0 does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BelowZeroWrap {
    /// Add `image_number` once and fire Animation End. Default (sprites Inferred step 3; not
    /// traced).
    #[default]
    AddFramesAndFire,
    /// Leave the negative index, no event.
    None,
}

/// R5: where Outside Room and Intersect Boundary run.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BoundarySlot {
    /// Right after automatic motion and End of Path. Default: Confirmed (recorder),
    /// traces/rely-the-needle-4-2/t-evo-1.csv, traces/wanna/t-evo-2.csv.
    #[default]
    AfterMotion,
    /// After the collision phase.
    AfterCollision,
}

/// R8: what still runs in the tick after a handler requests a room change.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RoomRequestRule {
    /// The requesting handler finishes; no further events this tick; jump to the sweep and the
    /// room change. Default: Confirmed (recorder), traces/rely-the-needle-4-2/t-room-1.csv rows
    /// 19-21, tr-room-02-c.csv row 13.
    #[default]
    AbortTick,
    /// Finish the current phase, then jump.
    FinishPhase,
    /// Finish the whole tick.
    FinishTick,
}

/// R9: whether an instance created during a phase runs that phase.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum JoinPhase {
    /// Never: each phase walks the snapshot it took at its start (the plan's earlier default).
    Never,
    /// Only when its place in the phase order lies in a later group than the instance whose
    /// handler created it (under `RankThenCreation`: a higher rank, never the creator's own
    /// rank). Default: Confirmed (recorder), traces/rely-the-needle-4-2/tr-room-03-b-log.csv
    /// row 8, traces/wanna/a3-t17-b.csv row 36, traces/rely-the-needle-4-2/t-evo-1.csv row 3.
    /// The untraced case "an object whose turn already passed" gets no event, as this rule says.
    #[default]
    LaterInOrder,
    /// Always: a new instance is put at its place among the instances not yet visited, or right
    /// after the current one when its place has passed.
    Always,
}

/// I1: the `keyboard_check_direct` latch.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DirectLatch {
    /// One "pressed since the last direct read" bit per action, set by every press and consumed
    /// by every direct read (CLONE_SPEC input, Inferred). Default.
    #[default]
    Latch,
    /// No latch: a direct read is the held state. What the recorder models
    /// (traces/rely-the-needle-4-2/t4-input-direct-tap-a.csv), for trace comparison.
    None,
}

/// What `Ctx::change_kind` (`instance_change`) takes from the new kind. Not in the spec
/// (Design); the new kind's rank, classes and built-in state always apply.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ChangeKindProps {
    /// Also sprite, mask, solid, visible, depth and persistent. Default (Design).
    #[default]
    KindDefaults,
    /// Keep the instance's own sprite, mask, solid, visible, depth and persistent.
    KeepInstance,
}
