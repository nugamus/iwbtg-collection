//! The `SIM_VERSION` bump check (docs/architecture/versioning.md, #783).
//!
//! After the freeze point (`iwbtg_sim::SIM_VERSION_FREEZE` is `Some`), every pinned golden hash
//! (the `FINAL_HASH` of `wp1_determinism.rs`, the state hash vectors and the golden corpus cases)
//! is recorded in `sim_version_pins.txt` together with the `SIM_VERSION` it was pinned under. A
//! pin that changes while `SIM_VERSION` stays the same fails this test: an outcome changed and old
//! replays would stop verifying. Bump `SIM_VERSION`, then `cargo xtask repin` records the new pins.
//! Before the freeze point the check is inert and the record stays empty.
//!
//! `UPDATE_SIM_PINS=1` (set by `cargo xtask repin`) rewrites the record, but never over a pin
//! that changed without a bump.

use std::collections::BTreeMap;

/// The record, compiled in like the golden corpus's expected lines.
const RECORD: &str = include_str!("sim_version_pins.txt");
const WP1: &str = include_str!("wp1_determinism.rs");
const GOLDEN: &str = include_str!("golden/expected.txt");

/// Pinned hashes by name (`final_hash`, `vector <name>`, `case <name>`) under one `SIM_VERSION`.
#[derive(Debug, PartialEq, Eq)]
struct Pins {
    sim_version: u32,
    pins: BTreeMap<String, String>,
}

/// Why the check failed.
#[derive(Debug, PartialEq, Eq)]
enum Failure {
    /// Pins changed under the recorded `SIM_VERSION`: the one failure a repin must not paper over.
    ChangedWithoutBump(Vec<String>),
    /// The record is missing, older than `SIM_VERSION`, or lacks or has extra pins: a repin fixes it.
    Stale(String),
    /// `SIM_VERSION` is below the freeze point or below the record.
    Version(String),
}

fn is_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// The pins as the test files hold them now.
fn current() -> BTreeMap<String, String> {
    let mut pins = BTreeMap::new();
    let prefix = "const FINAL_HASH: &str = \"";
    let at = WP1.find(prefix).expect("FINAL_HASH in wp1_determinism.rs") + prefix.len();
    pins.insert("final_hash".to_owned(), WP1[at..at + 64].to_owned());
    for (name, hash) in super::hash_vectors::VECTORS {
        pins.insert(format!("vector {name}"), hash.to_owned());
    }
    // Case lines are `name ticks status hash`; coverage lines have three fields.
    for line in GOLDEN.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if let [name, _, _, hash] = f[..] {
            if is_hash(hash) {
                pins.insert(format!("case {name}"), hash.to_owned());
            }
        }
    }
    pins
}

/// Parses the record; `None` when no pins have been recorded (before the freeze point).
fn parse(text: &str) -> Option<Pins> {
    let mut sim_version = None;
    let mut pins = BTreeMap::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(v) = line.strip_prefix("sim_version ") {
            sim_version = Some(v.parse().expect("sim_version is a number"));
        } else {
            let (name, hash) = line.rsplit_once(' ').expect("`<name> <hash>` line");
            assert!(is_hash(hash), "bad hash in sim_version_pins.txt: {line}");
            pins.insert(name.to_owned(), hash.to_owned());
        }
    }
    sim_version.map(|sim_version| Pins { sim_version, pins })
}

fn render(p: &Pins) -> String {
    let mut s = String::from(
        "# Golden pins recorded under one SIM_VERSION after the freeze point (#783).\n\
         # Written by `cargo xtask repin`; never edit by hand.\n",
    );
    s.push_str(&format!("sim_version {}\n", p.sim_version));
    for (name, hash) in &p.pins {
        s.push_str(&format!("{name} {hash}\n"));
    }
    s
}

/// The rule. Pure, so the tests below exercise it before the freeze point too.
fn check(
    freeze: Option<u32>,
    sim_version: u32,
    record: Option<&Pins>,
    current: &BTreeMap<String, String>,
) -> Result<(), Failure> {
    let Some(freeze) = freeze else {
        return Ok(());
    };
    if sim_version < freeze {
        return Err(Failure::Version(format!(
            "SIM_VERSION {sim_version} is below the freeze point {freeze}"
        )));
    }
    let Some(record) = record else {
        return Err(Failure::Stale(
            "no pins recorded since the freeze point: run `cargo xtask repin`".into(),
        ));
    };
    if record.sim_version > sim_version {
        return Err(Failure::Version(format!(
            "SIM_VERSION {sim_version} is below the recorded {}: it never goes down",
            record.sim_version
        )));
    }
    if record.sim_version < sim_version {
        return Err(Failure::Stale(format!(
            "SIM_VERSION is now {sim_version}, the pins were recorded under {}: run `cargo xtask repin`",
            record.sim_version
        )));
    }
    let changed: Vec<String> = record
        .pins
        .iter()
        .filter(|(name, hash)| current.get(*name).is_some_and(|now| now != *hash))
        .map(|(name, _)| name.clone())
        .collect();
    if !changed.is_empty() {
        return Err(Failure::ChangedWithoutBump(changed));
    }
    if record.pins.keys().ne(current.keys()) {
        return Err(Failure::Stale(
            "pins were added or removed: run `cargo xtask repin` to record them".into(),
        ));
    }
    Ok(())
}

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "the test, not the sim, reads UPDATE_SIM_PINS and rewrites the record"
)]
fn golden_pins_change_only_with_a_sim_version_bump() {
    let current = current();
    let record = parse(RECORD);
    let result = check(
        iwbtg_sim::SIM_VERSION_FREEZE,
        iwbtg_sim::SIM_VERSION,
        record.as_ref(),
        &current,
    );
    if let Err(Failure::Stale(_)) = result {
        if std::env::var_os("UPDATE_SIM_PINS").is_some() {
            let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/sim_version_pins.txt");
            let pins = Pins {
                sim_version: iwbtg_sim::SIM_VERSION,
                pins: current,
            };
            std::fs::write(path, render(&pins)).expect("write sim_version_pins.txt");
            return;
        }
    }
    match result {
        Ok(()) => {}
        Err(Failure::ChangedWithoutBump(names)) => panic!(
            "golden hashes changed after the freeze point without a SIM_VERSION bump: {}. \
             Old replays would stop verifying. Bump SIM_VERSION in crates/sim/src/lib.rs, then \
             run `cargo xtask repin` (docs/architecture/versioning.md)",
            names.join(", ")
        ),
        Err(Failure::Stale(why) | Failure::Version(why)) => panic!("{why}"),
    }
}

/// Before the freeze point the record stays empty, so nothing is enforced by accident.
#[test]
fn no_pins_are_recorded_before_the_freeze_point() {
    if iwbtg_sim::SIM_VERSION_FREEZE.is_none() {
        assert_eq!(
            parse(RECORD),
            None,
            "sim_version_pins.txt must hold no pins before the freeze"
        );
    }
}

#[test]
fn every_kind_of_golden_pin_is_read() {
    let pins = current();
    assert!(pins.contains_key("final_hash"));
    assert!(pins.contains_key("vector empty"));
    assert!(pins.contains_key("case kid_a1_basics"));
    assert!(!pins.keys().any(|k| k.starts_with("case builtin")));
}

fn pins(v: u32, entries: &[(&str, &str)]) -> Pins {
    Pins {
        sim_version: v,
        pins: entries
            .iter()
            .map(|(n, h)| ((*n).to_owned(), (*h).to_owned()))
            .collect(),
    }
}

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[test]
fn the_check_is_inert_before_the_freeze_point() {
    let rec = pins(1, &[("case x", A)]);
    let now = pins(1, &[("case x", B)]).pins;
    assert_eq!(check(None, 1, Some(&rec), &now), Ok(()));
    assert_eq!(check(None, 1, None, &now), Ok(()));
}

#[test]
fn a_changed_pin_without_a_bump_fails() {
    let rec = pins(2, &[("case x", A), ("case y", A)]);
    let now = pins(2, &[("case x", B), ("case y", A)]).pins;
    assert_eq!(
        check(Some(2), 2, Some(&rec), &now),
        Err(Failure::ChangedWithoutBump(vec!["case x".into()]))
    );
}

#[test]
fn a_bump_asks_for_a_repin_and_the_repin_passes() {
    let rec = pins(2, &[("case x", A)]);
    let now = pins(3, &[("case x", B)]);
    assert!(matches!(
        check(Some(2), 3, Some(&rec), &now.pins),
        Err(Failure::Stale(_))
    ));
    let rewritten = parse(&render(&now)).expect("record parses");
    assert_eq!(rewritten, now);
    assert_eq!(check(Some(2), 3, Some(&rewritten), &now.pins), Ok(()));
}

#[test]
fn a_new_case_needs_a_repin_but_no_bump() {
    let rec = pins(2, &[("case x", A)]);
    let now = pins(2, &[("case x", A), ("case new", B)]).pins;
    assert!(matches!(
        check(Some(2), 2, Some(&rec), &now),
        Err(Failure::Stale(_))
    ));
}

#[test]
fn versions_never_go_below_the_freeze_or_the_record() {
    let rec = pins(3, &[("case x", A)]);
    let now = rec.pins.clone();
    assert!(matches!(
        check(Some(2), 1, Some(&rec), &now),
        Err(Failure::Version(_))
    ));
    assert!(matches!(
        check(Some(2), 2, Some(&rec), &now),
        Err(Failure::Version(_))
    ));
    assert!(matches!(check(Some(2), 2, None, &now), Err(Failure::Stale(_))));
}
