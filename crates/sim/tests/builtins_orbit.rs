//! orbit: the mirrored start, the radians wrap, the spawner and the param checks.

use iwbtg_sim::builtins;
use iwbtg_sim::def::{Builtin, KindId, ParamValue, Params};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::world::InstId;

use crate::builtins_tier3_support::*;

#[test]
fn orbiter_starts_mirrored_across_the_center_then_turns() {
    // object761: placed at (1840, 912), center (1840, 976): first position (1840, 1040).
    let mut g = orbit_game(&[
        ("center_x", real(1840.0)),
        ("center_y", real(976.0)),
        ("speed", real(1.0)),
    ]);
    let o = spawn(&mut g, "orb", 1840.0, 912.0);
    fire(&mut g, o, &EventKind::Step);
    let (x, y) = pos(&g, o);
    assert!(
        (x - 1840.0).abs() < 1e-9 && (y - 1040.0).abs() < 1e-9,
        "({x}, {y})"
    );
    fire(&mut g, o, &EventKind::Step);
    let (x2, y2) = pos(&g, o);
    // One degree on, radius 64: x = cx + r cos(271), y = cy - r sin(271).
    let a = deg(271.0);
    assert!((x2 - (1840.0 + 64.0 * cosr(a))).abs() < 1e-9);
    assert!((y2 - (976.0 - 64.0 * sinr(a))).abs() < 1e-9);
}

#[test]
fn radians_orbiter_wraps_its_angle_into_zero_to_two_pi() {
    let mut g = orbit_game(&[
        ("center_x", real(100.0)),
        ("center_y", real(100.0)),
        ("radius", real(50.0)),
        ("angle", real(6.0)),
        ("speed", real(1.0)),
        ("unit", word("radians")),
    ]);
    let o = spawn(&mut g, "orb", 0.0, 0.0);
    fire(&mut g, o, &EventKind::Step);
    let (x, y) = pos(&g, o);
    assert!((x - (100.0 + 50.0 * cosr(6.0))).abs() < 1e-9);
    assert!((y - (100.0 - 50.0 * sinr(6.0))).abs() < 1e-9);
    fire(&mut g, o, &EventKind::Step);
    let (x2, _) = pos(&g, o);
    let wrapped = 7.0 - std::f64::consts::TAU;
    assert!((x2 - (100.0 + 50.0 * cosr(wrapped))).abs() < 1e-9);
}

#[test]
fn spawner_makes_evenly_spaced_orbiters_and_goes_away() {
    let child = kind_of(
        "orb",
        Builtin::Orbit,
        params(&[("center_x", real(0.0)), ("center_y", real(0.0))]),
    );
    // Kinds: kid 0, bullet 1, then orb 2 and the maker.
    let orb = KindId(2);
    let spawner = kind_of(
        "maker",
        Builtin::Orbit,
        params(&[
            ("mode", word("spawner")),
            ("child", ParamValue::Kind(orb)),
            ("count", real(4.0)),
            ("center_x", real(200.0)),
            ("center_y", real(200.0)),
            ("radius", real(32.0)),
            ("speed", real(2.0)),
        ]),
    );
    let mut g = game(vec![child, spawner]);
    assert_eq!(kind(&g, "orb"), orb);
    let before = g.w.instances().len();
    let maker = spawn(&mut g, "maker", 0.0, 0.0);
    assert!(!alive(&g, maker), "the spawner destroys itself");
    let kids: Vec<InstId> =
        g.w.instances()
            .iter()
            .filter(|i| i.alive && i.kind == orb)
            .map(|i| i.id)
            .collect();
    assert_eq!(kids.len(), 4);
    assert!(g.w.instances().len() > before);
    for (i, id) in kids.iter().enumerate() {
        fire(&mut g, *id, &EventKind::Step);
        let a = deg(90.0 * i as f64);
        let (x, y) = pos(&g, *id);
        assert!((x - (200.0 + 32.0 * cosr(a))).abs() < 1e-9, "x of {i}");
        assert!((y - (200.0 - 32.0 * sinr(a))).abs() < 1e-9, "y of {i}");
    }
}

#[test]
fn orbit_needs_a_center_and_a_spawner_needs_a_child() {
    assert!(builtins::init_state(Builtin::Orbit, &Params::default()).is_err());
    let p = params(&[
        ("mode", word("spawner")),
        ("center_x", real(0.0)),
        ("center_y", real(0.0)),
    ]);
    assert!(builtins::init_state(Builtin::Orbit, &p).is_err());
}
