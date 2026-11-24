//! State hash test vectors (#92): fixed worlds with pinned BLAKE3 hashes, so any change to the
//! canonical encoding (docs/architecture/sim/hash.md, State hash; `hash::HASH_LAYOUT`) fails here.
//!
//! A deliberate encoding change bumps `HASH_LAYOUT` and updates every vector in the same PR.

use iwbtg_sim::def::{KindId, RoomId, VarId};
use iwbtg_sim::hash::state_hash;
use iwbtg_sim::num::Real;
use iwbtg_sim::world::{Instance, Value, World};

fn hex(h: [u8; 32]) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

fn add(w: &mut World, kind: u32, x: f64, y: f64) {
    let id = w.alloc_id().expect("id");
    let rank = u16::try_from(kind).expect("small kind");
    w.insert(Instance::new(id, KindId(kind), rank, Real::new(x), Real::new(y)))
        .expect("insert");
}

/// Vector 1: a fresh world (room 0, seed 1).
fn empty() -> World {
    World::new(RoomId(0), 1)
}

/// Vector 2: three instances of two kinds at fractional positions.
fn instances() -> World {
    let mut w = World::new(RoomId(2), 1);
    add(&mut w, 0, 48.0, 552.4);
    add(&mut w, 1, 0.0, 576.0);
    add(&mut w, 1, 32.5, -16.25);
    w
}

/// Vector 3: globals of every value type, and a named variable.
fn globals() -> World {
    let mut w = World::new(RoomId(0), 1);
    w.var_names.insert("difficulty".into(), VarId(0));
    w.globals.insert(VarId(0), Value::Real(Real::new(2.0)));
    w.globals.insert(VarId(1), Value::Str("I wanna".into()));
    w.globals
        .insert(VarId(2), Value::Inst(iwbtg_sim::world::InstId::FIRST));
    w.globals.insert(VarId(3), Value::None);
    w
}

/// Vector 4: the RNG seeded 12345 and advanced 3 draws.
fn rng() -> World {
    let mut w = World::new(RoomId(0), 12345);
    for _ in 0..3 {
        w.rng.next_unit();
    }
    w
}

/// Vector 5: Real edge values: -0.0 (kept apart from 0.0), a NaN, 0.1 and the largest double.
fn reals(nan_bits: u64) -> World {
    let mut w = World::new(RoomId(0), 1);
    w.globals.insert(VarId(0), Value::Real(Real::new(-0.0)));
    w.globals
        .insert(VarId(1), Value::Real(Real::new(f64::from_bits(nan_bits))));
    w.globals.insert(VarId(2), Value::Real(Real::new(0.1)));
    w.globals.insert(VarId(3), Value::Real(Real::new(f64::MAX)));
    w
}

/// Vector 6: arrays (#984) in a global and an instance variable: flat, nested, empty and holding
/// `None`, so an array hashes by its elements, not by a single tag.
fn arrays() -> World {
    let mut w = World::new(RoomId(0), 1);
    add(&mut w, 0, 0.0, 0.0);
    let flat = Value::array(vec![Value::Real(Real::new(1.0)), Value::Str("a".into())]).expect("flat array");
    let nested = Value::array(vec![
        Value::array(vec![Value::Real(Real::new(2.0))]).expect("inner array"),
        Value::None,
    ])
    .expect("nested array");
    let outer = Value::array(vec![flat, nested, Value::array(Vec::new()).expect("empty array")])
        .expect("outer array");
    w.globals.insert(VarId(0), outer);
    let id = w.instances()[0].id;
    w.get_mut(id).expect("instance").vars.insert(
        VarId(1),
        Value::array(vec![Value::Real(Real::new(3.5))]).expect("var array"),
    );
    w
}

pub(crate) const VECTORS: [(&str, &str); 6] = [
    (
        "empty",
        "04daffeb241508b6ec5d56d69fcbfd7a931dad4ad2b26f1570f9d160902041ab",
    ),
    (
        "instances",
        "049662e24a21f03d3a94146155763180bf641cd7d88135cbd9f293aff179b3b2",
    ),
    (
        "globals",
        "55f3bc39d6599e9b6d83a66de0cba8209215ef6605e1199de2c289eb9363586b",
    ),
    (
        "rng",
        "6524371c03b2c44b64cab6b7f84ad9d47db64c2e52e075c1f4cbc218f5150036",
    ),
    (
        "reals",
        "0eb23c1c66e84f4bcc40b4569b43696daf8395130384d80a9ad48a1f456c9541",
    ),
    (
        "arrays",
        "5fd642c70ec82c5f057e0a3c3b93042109cf1377943110fb972a5d3ed0168ae1",
    ),
];

fn world(name: &str) -> World {
    match name {
        "empty" => empty(),
        "instances" => instances(),
        "globals" => globals(),
        "rng" => rng(),
        "reals" => reals(f64::NAN.to_bits()),
        "arrays" => arrays(),
        _ => unreachable!(),
    }
}

#[test]
fn hash_vectors_match() {
    let got: Vec<String> = VECTORS
        .iter()
        .map(|(n, _)| format!("{n}: {}", hex(state_hash(&world(n)))))
        .collect();
    let want: Vec<String> = VECTORS.iter().map(|(n, h)| format!("{n}: {h}")).collect();
    assert_eq!(
        got, want,
        "state hash encoding changed: bump HASH_LAYOUT and update the vectors"
    );
}

#[test]
fn hash_vectors_are_distinct() {
    let mut all: Vec<String> = VECTORS.iter().map(|(_, h)| (*h).to_owned()).collect();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), VECTORS.len());
}

/// The NaN rule: every NaN hashes alike, whatever its sign and payload.
#[test]
fn hash_nan_payloads_hash_alike() {
    let canonical = state_hash(&reals(f64::NAN.to_bits()));
    for bits in [
        0x7ff8_0000_0000_0001,
        0xfff8_0000_0000_0000,
        0x7ff0_0000_0000_0001,
    ] {
        assert_eq!(state_hash(&reals(bits)), canonical, "NaN {bits:#x}");
    }
}

/// -0.0 and 0.0 hash apart (they are different `Real` bits and GML can tell them apart).
#[test]
fn hash_negative_zero_is_not_zero() {
    let mut a = empty();
    let mut b = empty();
    a.globals.insert(VarId(0), Value::Real(Real::new(-0.0)));
    b.globals.insert(VarId(0), Value::Real(Real::new(0.0)));
    assert_ne!(state_hash(&a), state_hash(&b));
}

/// `World::particle_defs` is the client's record of which file built each particle system: the
/// sim never reads it, so it is not hashed (#866).
#[test]
fn hash_ignores_the_particle_defs_table() {
    let mut w = empty();
    let before = state_hash(&w);
    w.particle_defs.insert(
        iwbtg_sim::particles::PartSystemId(0),
        iwbtg_sim::def::PartSystemDefId(3),
    );
    assert_eq!(state_hash(&w), before);
}

/// `World::particle_clears` is the client's count of `clear` calls per particle system: the sim
/// never reads it, so it is not hashed (#883).
#[test]
fn hash_ignores_the_particle_clears_table() {
    let mut w = empty();
    let before = state_hash(&w);
    w.particle_clears.insert(iwbtg_sim::particles::PartSystemId(0), 2);
    assert_eq!(state_hash(&w), before);
}
