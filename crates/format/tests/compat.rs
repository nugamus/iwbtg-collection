//! `ruleset`, `[compat]` and the deviation catalogue (decision 0005, #1160).

use crate::wp7a_load::*;
use iwbtg_format::PackageFiles;
use iwbtg_format::compat;
use iwbtg_format::load::load_report;

/// The basic package with a root `ruleset` line and a trailing `[compat]` table.
fn with_compat(ruleset: &str, compat: &str) -> PackageFiles {
    package(|f| {
        let mut text = manifest(ORDER, compat);
        text = text.replace(
            "max_ticks = 180_000\n",
            &format!("max_ticks = 180_000\n{ruleset}\n"),
        );
        set(f, "game.toml", text);
    })
}

const SAVE_POSITION: &str = r#"
[compat]
profile = "gm8/a1@1"

[[compat.deviation]]
id = "save.position"
value = "xy_base100_bytes"
why = "the original stores the position in bytes"
impact = "F1"
evidence = "trace:test/room-1"
issue = 1234
"#;

#[test]
fn explicit_fields_that_name_the_resolved_behavior_keep_the_hash() {
    let base = load_ok(&package(|_| {}), STRICT).1.gameplay_hash.expect("hash");
    let explicit = load_ok(&with_compat("ruleset = \"gm8@0\"", SAVE_POSITION), STRICT)
        .1
        .gameplay_hash
        .expect("hash");
    assert_eq!(base, explicit, "a matching declaration is normalized away");
}

#[test]
fn the_resolved_summary_reports_the_inferred_values() {
    let (def, report) = load_ok(&with_compat("ruleset = \"gm8@0\"", SAVE_POSITION), STRICT);
    let resolved = report.compat.expect("resolved");
    assert_eq!(resolved.ruleset, "gm8@0");
    assert_eq!(resolved.profile, "gm8/a1@1");
    assert_eq!(resolved.deviations, ["save.position=xy_base100_bytes"]);
    assert!(resolved.explicit);
    assert_eq!(def.engine_rev, 0);
    assert_eq!(
        resolved.lines(),
        [
            "ruleset: gm8@0",
            "profile: gm8/a1@1",
            "deviation: save.position=xy_base100_bytes"
        ]
    );

    let absent = load_ok(&package(|_| {}), STRICT).1.compat.expect("resolved");
    assert!(!absent.explicit);
    assert_eq!(absent.ruleset, "gm8@0");
    assert_eq!(absent.profile, "gm8/a1@1");
    assert!(absent.deviations.is_empty());
}

#[test]
fn the_catalogue_is_closed() {
    let files = with_compat(
        "",
        r#"
[compat]
[[compat.deviation]]
id = "save.nope"
value = "xy_base100_bytes"
why = "x"
impact = "F3"
evidence = "x"
issue = 1
"#,
    );
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r)
            .iter()
            .any(|e| e.contains("unknown deviation `save.nope`")),
        "{}",
        render(&r)
    );
}

#[test]
fn a_deviation_needs_the_catalogue_type() {
    let files = with_compat(
        "",
        r#"
[compat]
[[compat.deviation]]
id = "save.position"
value = 3
why = "x"
impact = "F3"
evidence = "x"
issue = 1
"#,
    );
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r).iter().any(|e| e.contains("takes one of")),
        "{}",
        render(&r)
    );
}

#[test]
fn a_deviation_needs_provenance() {
    let files = with_compat(
        "",
        r#"
[compat]
[[compat.deviation]]
id = "save.position"
value = "xy_base100_bytes"
why = ""
impact = "F9"
evidence = ""
issue = 0
"#,
    );
    let r = load_report(&files, STRICT);
    let text = render(&r);
    assert!(text.contains("needs `why`"), "{text}");
    assert!(text.contains("needs `evidence`"), "{text}");
    assert!(text.contains("is not a feel tier"), "{text}");
    assert!(text.contains("needs the issue number"), "{text}");
}

#[test]
fn a_format_zero_ruleset_is_gm8_at_engine_rev() {
    let wrong_rev = load_report(&with_compat("ruleset = \"gm8@7\"", ""), STRICT);
    assert!(
        errors(&wrong_rev)
            .iter()
            .any(|e| e.contains("disagrees with `engine_rev`")),
        "{}",
        render(&wrong_rev)
    );
    let standard = load_report(&with_compat("ruleset = \"standard@1\"", ""), STRICT);
    assert!(
        errors(&standard)
            .iter()
            .any(|e| e.contains("is not implemented in format 0")),
        "{}",
        render(&standard)
    );
}

#[test]
fn a_profile_names_the_physics_preset_family() {
    let wrong_family = with_compat(
        "",
        r#"
[compat]
profile = "gm8/b@1"
"#,
    );
    let r = load_report(&wrong_family, STRICT);
    assert!(
        errors(&r)
            .iter()
            .any(|e| e.contains("names family `b`") && e.contains("preset is `a1`")),
        "{}",
        render(&r)
    );
    let unknown = load_report(&with_compat("", "[compat]\nprofile = \"gm8/z9@1\"\n"), STRICT);
    assert!(
        errors(&unknown).iter().any(|e| e.contains("is not one of")),
        "{}",
        render(&unknown)
    );
}

#[test]
fn a_deviation_must_name_the_value_the_package_resolves() {
    let files = with_compat(
        "",
        r#"
[compat]
[[compat.deviation]]
id = "save.position"
value = "floor_nudge"
why = "xia's format"
impact = "F1"
evidence = "trace:xia/room-1"
issue = 1
"#,
    );
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r).iter().any(|e| {
            e.contains("declares `floor_nudge`") && e.contains("resolves it to `xy_base100_bytes`")
        }),
        "{}",
        render(&r)
    );
}

#[test]
fn the_catalogue_covers_every_resolvable_id() {
    for spec in compat::DEVIATIONS {
        assert!(!spec.doc.is_empty(), "{} has no description", spec.id);
        // Every catalogue entry resolves from some preset (the A1 default is enough to prove
        // the resolver exists and returns a spelling the entry lists).
        let physics = iwbtg_sim::kid::presets::defaults(iwbtg_sim::def::KidFamily::A1);
        let value = compat::resolved(spec.id, &physics).unwrap_or_else(|| panic!("{}", spec.id));
        if let compat::Kind::Enum(values) = spec.kind {
            assert!(values.contains(&value), "{} resolves to `{value}`", spec.id);
        }
    }
}
