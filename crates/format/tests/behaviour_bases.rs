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
    let text = render(&r);
    assert!(
        text.contains("`hazrd` is not a built-in kind or a behaviour base"),
        "{text}"
    );
    assert!(text.contains("hazard"), "the diagnostic names the bases: {text}");
}

#[test]
fn a_behaviour_base_uses_its_reference_kind_params() {
    let order = ORDER.replace("\"@boss\",", "\"@boss\", \"@orb\",");
    let files = package(|f| {
        f.push((
            "objects/orb.toml".into(),
            b"base = \"hazard\"\nsprite = \"spike_up\"\nbounce = true\n".to_vec(),
        ));
        set(f, "game.toml", manifest(&order, ""));
    });
    load_ok(&files, STRICT);
}

#[test]
fn a_param_the_reference_kind_does_not_have_is_an_error() {
    // `destroys_bullets` is a `spike` param; the hazard reference kind is `killer`.
    let order = ORDER.replace("\"@boss\",", "\"@boss\", \"@orb\",");
    let files = package(|f| {
        f.push((
            "objects/orb.toml".into(),
            b"base = \"hazard\"\nsprite = \"spike_up\"\ndestroys_bullets = true\n".to_vec(),
        ));
        set(f, "game.toml", manifest(&order, ""));
    });
    let r = load_report(&files, STRICT);
    let text = render(&r);
    assert!(
        text.contains("unknown param `destroys_bullets`") && text.contains("(killer)"),
        "{text}"
    );
}

#[test]
fn a_param_from_another_behaviour_is_an_error() {
    // `bounce` is a `killer` param; `solid` resolves to `block`.
    let order = ORDER.replace("\"@boss\",", "\"@boss\", \"@orb\",");
    let files = package(|f| {
        f.push((
            "objects/orb.toml".into(),
            b"base = \"solid\"\nsprite = \"spike_up\"\nbounce = true\n".to_vec(),
        ));
        set(f, "game.toml", manifest(&order, ""));
    });
    let r = load_report(&files, STRICT);
    let text = render(&r);
    assert!(
        text.contains("unknown param `bounce`") && text.contains("(block)"),
        "{text}"
    );
}
