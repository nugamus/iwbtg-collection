//! Behaviour base names (#1159 T2, decision 0005): `base = "hazard"` is an additive alias for
//! the behaviour's reference kind; the concrete kind names keep their behavior.

use crate::wp7a_load::*;
use iwbtg_format::load::load_report;
use iwbtg_sim::def::Builtin;

#[test]
fn a_behaviour_base_resolves_to_its_reference_kind() {
    let order = ORDER.replace("\"@boss\",", "\"@boss\", \"@orb\",");
    let files = package(|f| {
        f.push((
            "objects/orb.toml".into(),
            b"base = \"hazard\"\nsprite = \"spike_up\"\n".to_vec(),
        ));
        set(f, "game.toml", manifest(&order, ""));
    });
    let (def, _) = load_ok(&files, STRICT);
    let (_, orb) = kind_named(&def, "@orb");
    assert_eq!(orb.base, Builtin::Killer, "hazard resolves to the reference kind");
}

#[test]
fn every_behaviour_base_names_its_reference_kind() {
    for (base, expected) in [
        ("spawn", Builtin::KidStart),
        ("checkpoint", Builtin::Save),
        ("portal", Builtin::Warp),
        ("trigger", Builtin::Trigger),
        ("hazard", Builtin::Killer),
        ("pickup", Builtin::BossItem),
        ("overlay", Builtin::GameOver),
        ("emitter", Builtin::BloodEmitter),
        ("solid", Builtin::Block),
        ("mover", Builtin::MovingPlatform),
        ("kid_zone", Builtin::Water),
        ("goal", Builtin::GameClear),
    ] {
        assert_eq!(Builtin::from_name(base), Some(expected), "{base}");
    }
}

#[test]
fn an_unknown_base_is_still_an_error() {
    let files = package(|f| {
        f.push(("objects/bad.toml".into(), b"base = \"hazrd\"\n".to_vec()));
    });
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r)
            .iter()
            .any(|e| e.contains("`hazrd` is not a built-in kind")),
        "{}",
        render(&r)
    );
}
