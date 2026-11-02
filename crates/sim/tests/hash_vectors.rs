//! State hash test vectors (#92): fixed worlds with pinned BLAKE3 hashes, so any change to the
//! canonical encoding (docs/architecture.md, State hash; `hash::HASH_LAYOUT`) fails here.
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
        "ba11ff3270d1237bc6e7732a9ce779a2fd646321a44bdc6df5bf0471892b4828",
    ),
    (
        "instances",
        "9da97379bd6e20b46c953eab26a9b5a0f933d60102818f5827558cb0b2cc90dc",
    ),
    (
        "globals",
        "bd56737012c6598848366f97d30392420bebaef9e69250d0e8506bf8a848e179",
    ),
    (
        "rng",
        "7fbeda1705ab2a728bf6294f97db3c53704860bf04a32ec8a7101888d9a5072f",
    ),
    (
        "reals",
        "9ef8add59f8341e4e6ef83428ad6e358b51326faeaa9a252a5d3fec83e7b9918",
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
