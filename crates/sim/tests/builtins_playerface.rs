//! playerface: the Create frame.

use iwbtg_sim::def::{Builtin, KindDef};
use iwbtg_sim::rng::Rng;
use iwbtg_sim::world::World;

use crate::builtins_playerface_water_support::*;

#[test]
fn playerface_create_draws_one_frame() {
    let g = game(vec![KindDef::new("face", Builtin::Playerface)]);
    let mut w = World::new(g.room, 11);
    let mut mirror = Rng::new(11);
    let frame = mirror.choose(3).unwrap();
    let id = spawn(&g, &mut w, kind_id(&g, "face"), 0.0, 0.0);
    assert_eq!(w.get(id).unwrap().img.index(), r(f64::from(frame)));
    assert_eq!(w.rng, mirror, "exactly one draw");
}
