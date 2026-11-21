//! The room's declared render layers, back to front, and the world band's draw order.
//!
//! A room draws as these bands, in order ([`Layer`]): the room colour, the background layers in
//! file order, the world band, the foreground layers in file order. The world band is one z
//! range shared by tile layers and instances; its contents are placed by the sim's draw order
//! (`iwbtg_sim::events::draw_order`: depth descending, then creation), never by a float of the
//! depth, so the client and the sim cannot disagree (#789). Tiles and backgrounds are layer
//! entries, not constants scattered through the art code: [`Layer`] is the one declaration and
//! [`Ladder`] places the world band inside it.
//!
//! Tie rules, each with one answer and one test:
//! - later background and foreground layers draw on top (`index` in file order);
//! - tile layers of the same depth keep file order, so a later layer draws on top;
//! - a tile layer at an instance's own depth draws over it (OpenGMK's picture, CLONE_SPEC
//!   TR-ROOM-13, partial);
//! - instances draw in the sim's order, and z strictly rises along it.
//!
//! The world band's tile layers are fixed rungs, spread evenly over the range; the instances
//! between two rungs (by depth) share the gap between them, evenly by their place in the draw
//! order. Each gap is at least `WORLD_Z` / (layers + 1) wide, so thousands of instances still
//! get z values that `f32` keeps apart, at any depth.

use bevy::prelude::*;
use iwbtg_sim::world::{InstId, Instance, World};

/// The room's plain colour, behind every other layer.
pub(crate) const COLOUR_Z: f32 = -996.0;
/// Background layer 0; later layers add `LAYER_STEP` (`Layer::Background` does the arithmetic).
pub(crate) const BACKGROUND_Z: f32 = -995.0;
/// Foreground layer 0; later layers add `LAYER_STEP` (`Layer::Foreground` does the arithmetic).
pub(crate) const FOREGROUND_Z: f32 = 995.0;

/// The z range instances and tiles use: in front of the room colour and backgrounds
/// (`BACKGROUND_Z`, -995), behind the foregrounds (`FOREGROUND_Z`, 995), hitboxes, HUD and
/// overlay.
pub(crate) const WORLD_Z: (f32, f32) = (-990.0, 990.0);

/// The gap between two background or foreground layers, so their file order is a stable tie
/// rule (later on top). Smaller than one pixel of any real drawable, and far from the overlay.
const LAYER_STEP: f32 = 1e-3;

/// A declared room layer, back to front. The world band is one entry; `Ladder` places its tile
/// rungs and instances. `index` is the layer's file index, so later entries draw on top.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Layer {
    /// The room's plain colour.
    Colour,
    /// A background layer (not a foreground one) at its file index.
    Background(usize),
    /// The shared tile/instance band, `WORLD_Z`.
    World,
    /// A foreground layer at its file index.
    Foreground(usize),
}

impl Layer {
    /// The band this layer occupies. Point layers have a zero-width band; the world band spans
    /// [`WORLD_Z`].
    pub(crate) fn band(self) -> (f32, f32) {
        match self {
            Layer::Colour => (COLOUR_Z, COLOUR_Z),
            Layer::Background(i) => {
                let z = BACKGROUND_Z + i as f32 * LAYER_STEP;
                (z, z)
            }
            Layer::World => WORLD_Z,
            Layer::Foreground(i) => {
                let z = FOREGROUND_Z + i as f32 * LAYER_STEP;
                (z, z)
            }
        }
    }

    /// The layer's own z. [`Layer::World`] reports its back edge (`WORLD_Z.0`); use [`band`] for
    /// the whole range.
    ///
    /// [`band`]: Layer::band
    pub(crate) fn z(self) -> f32 {
        self.band().0
    }
}

/// The world band's contents: the room's tile layer depths on screen, in draw order (deepest
/// first; equal depths in file order, so a later layer draws on top), with the instances placed
/// between the rungs by the sim's draw order.
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
        let (lo, hi) = Layer::World.band();
        (hi - lo) / (self.depths.len() + 1) as f32
    }

    /// The z of tile layer `layer` (its index in the room's file).
    pub(crate) fn tile_z(&self, layer: usize) -> f32 {
        Layer::World.z() + (self.rung_of[layer] + 1) as f32 * self.gap()
    }

    /// The gap an instance at `depth` sits in: below every tile layer of the same or a smaller
    /// depth, so a tile layer at the instance's own depth draws over it (OpenGMK's picture,
    /// CLONE_SPEC TR-ROOM-13, partial).
    fn gap_of(&self, depth: f64) -> usize {
        self.depths
            .iter()
            .filter(|&&d| d.total_cmp(&depth).is_gt())
            .count()
    }

    /// The z of something drawn at `depth` after every instance of the same depth (a particle
    /// system, [particles](../../../docs/architecture/client/particles.md)): inside the depth's
    /// gap between tile layers, between the instances `placed` there (each drawn instance's
    /// depth and z) that draw before it and those that draw after it.
    pub(crate) fn depth_z(&self, placed: &[(f64, f32)], depth: f64) -> f32 {
        let g = self.gap_of(depth);
        let mut lo = Layer::World.z() + g as f32 * self.gap();
        let mut hi = lo + self.gap();
        for &(d, z) in placed {
            if self.gap_of(d) != g {
                continue;
            }
            if d >= depth {
                lo = lo.max(z);
            } else {
                hi = hi.min(z);
            }
        }
        (lo + hi) / 2.0
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
        let (back, _) = Layer::World.band();
        let gap = self.gap();
        gaps.iter()
            .map(|&g| {
                seen[g] += 1;
                let within = seen[g] as f32 / (count[g] + 1) as f32;
                back + (g as f32 + within) * gap
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
    fn declared_layers_run_back_to_front() {
        // Colour, backgrounds, the world band, foregrounds: one answer for every pair.
        assert_eq!(Layer::Colour.band(), (COLOUR_Z, COLOUR_Z));
        assert_eq!(Layer::World.band(), WORLD_Z);
        assert!(Layer::Colour.z() < Layer::Background(0).z());
        assert!(
            Layer::Background(0).z() < Layer::Background(1).z(),
            "a later background draws on top"
        );
        assert!(Layer::Background(7).z() < Layer::World.z());
        assert!(Layer::World.z() < Layer::Foreground(0).z());
        assert!(
            Layer::Foreground(0).z() < Layer::Foreground(1).z(),
            "a later foreground draws on top"
        );
        assert!(Layer::Foreground(7).z() < 1000.0, "under the overlay");
    }

    #[test]
    fn every_tile_rung_and_instance_stays_inside_the_world_band() {
        let ladder = Ladder::new([1e9, 0.0, -1e9]);
        for n in 0..3 {
            let z = ladder.tile_z(n);
            assert!(WORLD_Z.0 < z && z < WORLD_Z.1, "tile {n} at {z}");
        }
        for z in ladder.z_in_order(&[1e9, 1.0, 0.0, -1.0, -1e9]) {
            assert!(WORLD_Z.0 < z && z < WORLD_Z.1, "instance at {z}");
        }
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
    fn a_depth_lands_after_its_own_depth_and_before_smaller_ones() {
        let ladder = Ladder::new([0.0]);
        let depths = [10.0, 5.0, 5.0, 1.0, -3.0];
        let z = ladder.z_in_order(&depths);
        let placed: Vec<(f64, f32)> = depths.iter().copied().zip(z.iter().copied()).collect();
        let at5 = ladder.depth_z(&placed, 5.0);
        assert!(z[2] < at5 && at5 < z[3], "over both 5s, under the 1");
        let back = ladder.depth_z(&placed, 100.0);
        assert!(back > WORLD_Z.0 && back < z[0]);
        // Behind the tile layer at 0 is the instance at 1; in front of it the one at -3.
        let at0 = ladder.depth_z(&placed, 0.0);
        assert!(z[3] < at0 && at0 < ladder.tile_z(0));
        assert!(ladder.depth_z(&placed, -5.0) > z[4]);
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
