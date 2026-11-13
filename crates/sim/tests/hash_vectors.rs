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

const VECTORS: [(&str, &str); 5] = [
    (
        "empty",
        "38f25bd28b6c80877388754e0dacdcd99058316c15adff89845e770a700791ef",
    ),
    (
        "instances",
        "e35b7d739dc58f67a296f473c2549fd8214e335dfc8487bc990725dafde3873b",
    ),
    (
        "globals",
        "d8c700641f81f648304e580393939f223d67afb57d6b4d8a4e5fef685894cfff",
    ),
    (
        "rng",
        "b3e1a61a4f126a25c39c7150647343a9009088251794b5ba84473b0967260932",
    ),
    (
        "reals",
        "6b7c478a26c9afb5477ba65b703627667296fdd8c9ad277915d285318b10fb9d",
    ),
];

fn world(name: &str) -> World {
    match name {
        "empty" => empty(),
        "instances" => instances(),
        "globals" => globals(),
        "rng" => rng(),
        "reals" => reals(f64::NAN.to_bits()),
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
