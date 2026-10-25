//! Collision switches (docs/plans/sim-core.md 2.3, 3.4, register C2, C7b, C9, C10, C13, R12).
//!
//! C1, C3, C4, C11, C12 and C14 live in their `collide` functions; C5 in `format::masks`.
//! WP0 declares; WP3 owns.

/// Rules for collision events and queries.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CollideRules {
    /// R12 (Inferred / UNKNOWN): order of one instance's collision pairs.
    pub pair_order: PairOrder,
    /// C7b (UNKNOWN): re-check a non-solid pair's overlap right before its event. Default true
    /// (Design: one rule for solid and non-solid pairs).
    pub recheck_nonsolid: bool,
    /// C9 / M7 (UNKNOWN motion U7): what the final "still colliding" check of a solid pair
    /// tests.
    pub solid_recheck: SolidRecheck,
    /// C2 (Inferred / UNKNOWN): how a place test rounds.
    pub place_round: PlaceRound,
    /// C10 (UNKNOWN, sources disagree): `move_contact_solid` from an overlapping start.
    pub mcs_start: McsStart,
    /// C13: which boxes `distance_to_object` measures.
    pub dist_boxes: DistBoxes,
    /// C13 ("+1 or not", UNKNOWN): offset added to the gap. WP3 picks it with the flush-kid test.
    pub dist_offset: DistOffset,
    /// C13: how the x and y gaps combine (Design).
    pub dist_combine: DistCombine,
}

impl Default for CollideRules {
    fn default() -> CollideRules {
        CollideRules {
            pair_order: PairOrder::default(),
            recheck_nonsolid: true,
            solid_recheck: SolidRecheck::default(),
            place_round: PlaceRound::default(),
            mcs_start: McsStart::default(),
            dist_boxes: DistBoxes::default(),
            dist_offset: DistOffset::default(),
            dist_combine: DistCombine::default(),
        }
    }
}

/// R12: order of one instance's collision pairs.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PairOrder {
    /// By the other's rank, then the other's id (Inferred). Default.
    #[default]
    OtherRankThenId,
    /// By this instance's handler list, then the other in dispatch order.
    HandlerListOrder,
}

/// C9: what the final check after a solid pair's event tests.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SolidRecheck {
    /// Overlap with the other instance of the pair. Default.
    #[default]
    VsOther,
    /// Overlap with any solid.
    AnySolid,
}

/// C2: how a place test at `y + 1` rounds.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PlaceRound {
    /// Round the tested coordinate: `round(y + 1)` (Inferred "Standing heights"). Default.
    #[default]
    TestedCoord,
    /// Round, then add: `round(y) + 1`.
    RoundThenAdd,
}

/// C10: `move_contact_solid` when the start already overlaps a solid.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum McsStart {
    /// Do not move (the manual). Default.
    #[default]
    ManualNoMove,
    /// The start is not checked; step as usual.
    NotChecked,
}

/// C13: which boxes `distance_to_object` measures.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DistBoxes {
    /// The rounded, inclusive collision boxes (C1). Default.
    #[default]
    RoundedInclusive,
    /// Unrounded `x + offset` boxes, so sub-pixel positions give fractional distances.
    Unrounded,
}

/// C13: offset added to the box gap.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DistOffset {
    /// No offset. Default until WP3's flush-kid test picks.
    #[default]
    None,
    /// Plus one.
    PlusOne,
}

/// C13: how the x and y gaps combine.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DistCombine {
    /// `sqrt(dx*dx + dy*dy)` (Design). Default.
    #[default]
    Euclid,
    /// `max(dx, dy)`.
    Max,
}
