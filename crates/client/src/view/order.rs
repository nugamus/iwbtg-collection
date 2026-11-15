//! Draw order as z: instances and tile layers share one z range, placed by the sim's draw order
//! (`iwbtg_sim::events::draw_order`: depth descending, then creation), never by a float of the
//! depth, so the client and the sim cannot disagree (#789).
//!
//! The room's tile layers are fixed rungs, spread evenly over the range; the instances between
//! two rungs (by depth) share the gap between them, evenly by their place in the draw order.
//! Each gap is at least `WORLD_Z` / (layers + 1) wide, so thousands of instances still get z
//! values that `f32` keeps apart, at any depth.

use bevy::prelude::*;
use iwbtg_sim::world::{InstId, Instance, World};

/// The z range instances and tiles use: in front of the room colour and backgrounds
/// (`art::BACKGROUND_Z`, -995), behind the foregrounds (995), hitboxes, HUD and overlay.
pub(crate) const WORLD_Z: (f32, f32) = (-990.0, 990.0);

/// The tile layer depths of the room on screen, in draw order (deepest first; equal depths in
/// file order, so a later layer draws on top).
#[derive(Resource, Default, Clone, Debug, PartialEq)]
pub(crate) struct Ladder {
    /// Each rung's depth, in draw order.
    depths: Vec<f64>,
    /// File index of each layer to its rung.
    rung_of: Vec<usize>,
}

impl Ladder {
    /// The ladder of tile layers with these depths, in file order.
    pub(crate) fn new(depths: impl IntoIterator<Item = f64>) -> Self {
        let file: Vec<f64> = depths.into_iter().collect();
        let mut by_draw: Vec<usize> = (0..file.len()).collect();
        // Stable: equal depths keep file order.
        by_draw.sort_by(|&a, &b| file[b].total_cmp(&file[a]));
        let mut rung_of = vec![0; file.len()];
        for (rung, &layer) in by_draw.iter().enumerate() {
            rung_of[layer] = rung;
        }
        Self {
            depths: by_draw.iter().map(|&l| file[l]).collect(),
            rung_of,
        }
    }

    fn gap(&self) -> f32 {
        (WORLD_Z.1 - WORLD_Z.0) / (self.depths.len() + 1) as f32
    }

    /// The z of tile layer `layer` (its index in the room's file).
    pub(crate) fn tile_z(&self, layer: usize) -> f32 {
        WORLD_Z.0 + (self.rung_of[layer] + 1) as f32 * self.gap()
    }

    /// The gap an instance at `depth` sits in: below every tile layer of the same or a smaller
    /// depth, so a tile layer at the instance's own depth draws over it (OpenGMK's picture,
    /// CLONE_SPEC TR-ROOM-13, partial).
    fn gap_of(&self, depth: f64) -> usize {
        self.depths.iter().filter(|&&d| d.total_cmp(&depth).is_gt()).count()
    }

    /// The z of each instance, given in draw order (depth descending, then creation) as its
    /// depth: z strictly rising along the list.
    pub(crate) fn z_in_order(&self, depths: &[f64]) -> Vec<f32> {
        let gaps: Vec<usize> = depths.iter().map(|&d| self.gap_of(d)).collect();
        let mut count = vec![0usize; self.depths.len() + 1];
        for &g in &gaps {
            count[g] += 1;
        }
        let mut seen = vec![0usize; self.depths.len() + 1];
        let gap = self.gap();
        gaps.iter()
            .map(|&g| {
                seen[g] += 1;
                let within = seen[g] as f32 / (count[g] + 1) as f32;
                WORLD_Z.0 + (g as f32 + within) * gap
            })
            .collect()
    }
}

/// The instances the sim draws (alive, active, visible, `filter`), in its draw order, each with
/// its z among the `ladder`'s tile layers.
pub(crate) fn instance_z(
    w: &World,
    ladder: &Ladder,
    filter: impl Fn(&Instance) -> bool,
) -> Vec<(InstId, f32)> {
    let order = iwbtg_sim::events::draw_order(w, filter);
    let depths: Vec<f64> = order
        .iter()
        .map(|&id| w.get(id).map_or(0.0, |i| i.depth.get()))
        .collect();
    order.into_iter().zip(ladder.z_in_order(&depths)).collect()
}

#[cfg(test)]
mod tests {
    use iwbtg_sim::def::{KindId, RoomId};
    use iwbtg_sim::num::Real;

    use super::*;

    fn rising(z: &[f32]) -> bool {
        z.windows(2).all(|w| w[0] < w[1])
    }

    #[test]
    fn same_depth_instances_draw_in_creation_order_across_a_thousand() {
        // Ids 100999 and 101000 at the same depth: the sim's draw order puts the older first,
        // and z follows the list, whatever the ids (#789; the old tie-break used id % 1000).
        let mut w = World::new(RoomId(0), 1);
        w.next_id = 100_999;
        for _ in 0..2 {
            let id = w.alloc_id().unwrap();
            let r = Real::new(16.0);
            w.insert(Instance::new(id, KindId(0), 0, r, r)).unwrap();
        }
        let z = instance_z(&w, &Ladder::default(), |_| true);
        assert_eq!(z[0].0, InstId(100_999));
        assert_eq!(z[1].0, InstId(101_000));
        assert!(z[0].1 < z[1].1, "the older instance under the newer one");
    }

    #[test]
    fn large_depths_keep_a_stable_order() {
        // Overlay-style depths, and thousands of equal ones: every z distinct and in order.
        let mut depths = vec![2e9, 1e6 + 1.0, 1e6];
        depths.extend(std::iter::repeat_n(-1e6, 20_000));
        depths.push(-1e6 - 1.0);
        let ladder = Ladder::new([1e6, 1e6, -1e7]);
        let z = ladder.z_in_order(&depths);
        assert!(rising(&z));
        assert!(z.iter().all(|&z| z > WORLD_Z.0 && z < WORLD_Z.1));
    }

    #[test]
    fn instances_and_tiles_interleave_by_depth() {
        // File order: front layer, two layers at 1000000 (the later on top), one at -100.
        let ladder = Ladder::new([0.0, 1e6, 1e6, -100.0]);
        let back = ladder.tile_z(1);
        assert!(back < ladder.tile_z(2), "same depth: later layer on top");
        assert!(ladder.tile_z(2) < ladder.tile_z(0));
        assert!(ladder.tile_z(0) < ladder.tile_z(3));
        let z = ladder.z_in_order(&[2e6, 1e6, 50.0, 0.0, -5.0, -100.0, -200.0]);
        assert!(rising(&z));
        assert!(z[0] < back, "deeper than every layer: behind them");
        assert!(z[1] < back, "equal depth: the tile layer on top");
        assert!(ladder.tile_z(2) < z[2] && z[3] < ladder.tile_z(0));
        assert!(ladder.tile_z(0) < z[4] && z[5] < ladder.tile_z(3));
        assert!(ladder.tile_z(3) < z[6] && z[6] < WORLD_Z.1);
    }
}
