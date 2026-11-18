//! The `game.toml` `[tricks]` keys (docs/format/game.md). The table is
//! `research/specs/tricks.md`, section "Format keys (`[tricks]`)": a key is a trick id without
//! its `trick.`, `input.` or `bug.` prefix.

/// What a `[tricks]` key is (research `specs/tricks.md`, Format keys).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrickType {
    /// A separate engine behavior: the package declares it on or off.
    Switch,
    /// Follows from the physics profile, kid family and objects: the package can only label it,
    /// never turn it off.
    Emergent,
    /// No source defines it yet, so it is not a key until classified.
    Unknown,
}

use TrickType::{Emergent, Switch, Unknown};

/// Every listed key with its type, in the research table's order.
pub const TRICKS: &[(&str, TrickType)] = &[
    ("h_align", Emergent),
    ("wall_align", Emergent),
    ("ad_align", Switch),
    ("v_align", Emergent),
    ("height_dependency", Emergent),
    ("respawn_valign", Emergent),
    ("hdiet", Emergent),
    ("drift_28_second", Emergent),
    ("vdiet", Switch),
    ("n_frame_jump", Emergent),
    ("one_frame_jump", Emergent),
    ("jump_cancel", Emergent),
    ("cancel_bounce", Emergent),
    ("low_cancel", Emergent),
    ("cactus", Emergent),
    ("chain_cancel", Emergent),
    ("cactus_phantom_release", Switch),
    ("dual_shift", Switch),
    ("numpad_cancel", Switch),
    ("disable_cancels", Switch),
    ("coyote_time", Switch),
    ("jump_buffer", Switch),
    ("needle_rng", Emergent),
    ("shift_buffering", Unknown),
    ("bunny_hop", Emergent),
    ("bunny_fall", Emergent),
    ("ledge_grab", Emergent),
    ("corner_stop", Emergent),
    ("x_block", Emergent),
    ("full_jump_setup", Emergent),
    ("corner_clip", Emergent),
    ("vine_slide", Emergent),
    ("vine_jump", Emergent),
    ("vine_hold_kick", Emergent),
    ("vine_drop", Emergent),
    ("vine_climb", Emergent),
    ("vine_keep_djump", Emergent),
    ("right_vine_miss", Emergent),
    ("vine_on_ground", Emergent),
    ("distance_vines", Emergent),
    ("water_jump", Emergent),
    ("water_slow_fall", Emergent),
    ("water_keep_djump", Emergent),
    ("water_full_jump_1f", Switch),
    ("water1_low_check", Switch),
    ("ptj", Emergent),
    ("platform_valign_reset", Emergent),
    ("platform_snap", Emergent),
    ("platform_djump_refresh", Emergent),
    ("platform_walk_off_catch", Emergent),
    ("platform_ceiling_push", Emergent),
    ("restart_glitch", Switch),
    ("sub_frame_tap", Switch),
    ("save_jump", Emergent),
    ("save_in_wall", Emergent),
    ("restart_f2_same_frame", Switch),
    ("edge_kill_timing", Emergent),
    ("killer_check_timing", Emergent),
    ("spike_face_stand", Emergent),
    ("cherry_upswing", Emergent),
];

/// The type of a `[tricks]` key; `None` when the key is not listed.
pub fn trick_type(key: &str) -> Option<TrickType> {
    TRICKS.iter().find(|(k, _)| *k == key).map(|&(_, t)| t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::check_id;

    #[test]
    fn table_matches_the_research_counts() {
        let count = |t| TRICKS.iter().filter(|(_, x)| *x == t).count();
        assert_eq!(TRICKS.len(), 60);
        assert_eq!(count(Switch), 13);
        assert_eq!(count(Emergent), 46);
        assert_eq!(count(Unknown), 1);
    }

    #[test]
    fn keys_are_ids_and_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for (key, _) in TRICKS {
            assert!(check_id(key).is_ok(), "{key}");
            assert!(seen.insert(*key), "duplicate {key}");
        }
    }
}
