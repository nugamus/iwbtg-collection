//! Collision switches (docs/plans/sim-core.md 2.3, 3.4, register C2, C7b, C9, C10, C13, M7, R12).
//!
//! C1, C3, C4, C11, C12 and C14 live in their `collide` functions; C5 in `format::masks`.
//! WP0 declares; WP3 owns.

/// Rules for collision events and queries.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CollideRules {
    /// R12 (Confirmed (recorder), T-ORD-2): the order the collision phase runs its pairs in.
    pub pair_order: PairOrder,
    /// C7b: re-check a non-solid pair's overlap right before its event. Default true
    /// (Confirmed (recorder): the killer sees the post-block position, help-the-fairy-tale
    /// t-col-06).
    pub recheck_nonsolid: bool,
    /// C9 (UNKNOWN motion U7; showdown t-mot-07-e does not separate the two): what the final
    /// "still colliding" check of a solid pair tests.
    pub solid_recheck: SolidRecheck,
    /// M7: where a solid pair leaves the instance when the final check still collides.
    pub solid_keep: SolidKeep,
    /// C2 (Inferred / UNKNOWN): how a place test rounds.
    pub place_round: PlaceRound,
    /// C2 (UNKNOWN: CLONE_SPEC collision, "how the collision box rounds an exact .5 in x"):
    /// whether the collision box rounds the origin first or each edge after the mask offset.
    pub box_round: BoxRound,
    /// C10 (answered by the recorder, little-runner t-col-07-d): `move_contact_solid` from an
    /// overlapping start.
    pub mcs_start: McsStart,
    /// C13: which boxes `distance_to_object` measures.
    pub dist_boxes: DistBoxes,
    /// C13 ("+1 or not"): offset added to the gap. WP3 picked it with the flush-kid test.
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
            solid_keep: SolidKeep::default(),
            place_round: PlaceRound::default(),
            box_round: BoxRound::default(),
            mcs_start: McsStart::default(),
            dist_boxes: DistBoxes::default(),
            dist_offset: DistOffset::default(),
            dist_combine: DistCombine::default(),
        }
    }
}

/// R12: the order the collision phase runs its pairs in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PairOrder {
    /// Pairs of instances, lower kind first. Default; Confirmed (recorder), OpenGMK only
    /// (CLONE_SPEC collision "Order of collision events: pairs of instances, lower object
    /// first"; traces/rely-the-needle-4-2/t-ord-2.md). The outer instance `a` goes by its own
    /// rank, then id; the inner `b` is every other instance of rank greater than or equal to
    /// `a`'s, by rank then id; per overlapping pair `a`'s handler for `b` runs, then `b`'s
    /// handler for `a`. Two instances of one rank meet twice, both directions each time.
    #[default]
    LowerRankFirst,
    /// Instance by instance in dispatch order; one instance's pairs by the other's rank, then
    /// the other's id (the earlier Inferred reading, ruled out by T-ORD-2).
    OtherRankThenId,
    /// Instance by instance in dispatch order; one instance's pairs by its handler list, then
    /// the other in dispatch order.
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

/// M7: where "keeps the instance at its previous place" puts it after a solid pair's event.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SolidKeep {
    /// At `(xprevious, yprevious)`. Default: CLONE_SPEC collision Inferred "What previous place
    /// and new position mean" (corrected, Confirmed (recorder)): an event that moved the
    /// embedded kid from y 584 to 583 still ends every tick at 584 (showdown t-mot-07-e.csv
    /// rows 2-8).
    #[default]
    Previous,
    /// Where the event left it (the plan's earlier M7 reading).
    EventPosition,
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

/// C2: how the collision box rounds a fractional position (ties to even either way).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BoxRound {
    /// Round x and y first, then add the integer mask offsets:
    /// `bbox_left = round(x) + (left - origin_x)` (CLONE_SPEC collision Inferred "Bounding box
    /// at unit scale"). Default, the reading the spec favours: the kid rests at x 58.5 flush
    /// against a block at x 64 (rely-the-needle-4-2 trick-wall-align.md run d; round(58.5) =
    /// 58, right edge 63), while the reported `bbox_right` there reads 64.
    #[default]
    OriginFirst,
    /// Round each edge after adding its offset, `round(x + offset)`, like the reported `bbox_*`
    /// values (machinary t-col-08-a.csv: x 200.5 reads 196..206). t-col-08 cannot tell whether
    /// collisions use this box; kaenbyou's rotated spikes fit neither rule
    /// (famb-spike-mask.md). Still OPEN in CLONE_SPEC collision.
    AfterOffset,
}

/// C10: `move_contact_solid` when the start already overlaps a solid.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum McsStart {
    /// Do not move (the manual). Default; Confirmed (recorder) by little-runner
    /// t-col-07-d.csv rows 4-5.
    #[default]
    ManualNoMove,
    /// The start is not checked; step as usual.
    NotChecked,
}

/// C13: which boxes `distance_to_object` measures.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DistBoxes {
    /// The reported `bbox_*` boxes (`collide::bbox::reported_box`: corner rule, each edge
    /// rounded). Default: Confirmed (recorder), a kid facing left reads distance 1 to a vine
    /// behind him because his reported `bbox_left` is x-7 (showdown a3-t16-a.csv row 5).
    #[default]
    Reported,
    /// The rounded, inclusive collision boxes (C1).
    RoundedInclusive,
    /// The reported boxes without rounding, so sub-pixel positions give fractional distances.
    Unrounded,
}

/// C13: offset added to the box gap.
///
/// The gap on one axis is the number of free pixels between the two boxes, 0 when they touch
/// or overlap. `PlusOne` adds 1 on each axis where the boxes are apart, which makes the gap the
/// difference of the facing edge coordinates.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DistOffset {
    /// The free-pixel gap: touching boxes give 0.
    None,
    /// Plus one: touching boxes give 1. Default, picked by WP3's flush-kid test
    /// (`tests/wp3_query.rs`, kid A1 "a flush kid is at distance 1"), and Confirmed (recorder)
    /// by animus a-vine-right-a/b/c.csv and a-vine-left-a/b.csv row 3 (touching 1, one free
    /// pixel 2, overlap 0).
    #[default]
    PlusOne,
}

/// C13: how the x and y gaps combine. Confirmed (recorder): `sqrt(dx*dx + dy*dy)`
/// (trick-vine-kick-and-djump-d rows 108-110: sqrt(5), sqrt(26), sqrt(65)).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DistCombine {
    /// `sqrt(dx*dx + dy*dy)`. Default.
    #[default]
    Euclid,
    /// `max(dx, dy)`.
    Max,
}
