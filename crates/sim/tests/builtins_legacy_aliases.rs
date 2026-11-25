//! The survey's **move** params are `ParamSpec` legacy aliases (#1076, #1375).
//!
//! `docs/format/builtins-survey.md` ("Moves") marks 56 parameters that come from one game and
//! are not a general concept. Format 0 keeps them as documented aliases, so the marker holds the
//! games that may use each and the issue that replaces it. This test parses the survey so the
//! table and the code cannot drift: the marked set must be exactly the survey's, with the same
//! game and the issue from the survey's Follow-ups grouping.

use std::collections::BTreeMap;

use iwbtg_sim::builtins::{self, LegacyParam};
use iwbtg_sim::def::Builtin;

/// The survey, embedded so a moved table is a compile-time input to this test.
const SURVEY: &str = include_str!("../../../docs/format/builtins-survey.md");

/// The `Moves` table: `kind.param` to the game its row names.
fn survey_moves() -> BTreeMap<(String, String), String> {
    let start = SURVEY
        .find("\n## Moves\n")
        .expect("the survey has a Moves section")
        + 1;
    let rest = &SURVEY[start..];
    let end = rest.find("\n## ").expect("a section after Moves");
    let mut kind = String::new();
    let mut out = BTreeMap::new();
    for line in rest[..end].lines() {
        if let Some(heading) = line.strip_prefix("### `")
            && let Some((name, _)) = heading.split_once('`')
        {
            kind = name.to_owned();
            continue;
        }
        let Some(row) = line.strip_prefix("| `") else {
            continue;
        };
        let Some((param, tail)) = row.split_once('`') else {
            continue;
        };
        // `| \`param\` | game | meaning |`: the game is the second cell.
        let mut cells = tail.split('|');
        let _ = cells.next();
        let game = cells.next().unwrap_or("").trim();
        assert!(!kind.is_empty(), "param `{param}` outside a kind heading");
        assert!(!game.is_empty(), "`{kind}.{param}` names no game");
        out.insert((kind.clone(), param.to_owned()), game.to_owned());
    }
    out
}

/// The issue the survey's Follow-ups section assigns to a kind's moves.
fn expected_issue(kind: &str) -> u16 {
    match kind {
        "trap" => 1267,
        "water" | "edge_warp" => 1268,
        _ => 1269,
    }
}

/// The marked params in the code: `kind.param` to its marker.
fn marked() -> BTreeMap<(String, String), LegacyParam> {
    let mut out = BTreeMap::new();
    for base in Builtin::ALL {
        for spec in builtins::param_specs(base) {
            if let Some(legacy) = spec.legacy {
                out.insert((base.name().to_owned(), spec.name.to_owned()), legacy);
            }
        }
    }
    out
}

#[test]
fn the_marked_set_is_the_surveys_move_table() {
    let survey = survey_moves();
    let code = marked();
    assert_eq!(
        survey.len(),
        56,
        "the survey's Moves section should hold 56 params"
    );
    assert_eq!(code.len(), survey.len(), "marked params: {code:?}");

    let survey_keys: Vec<_> = survey.keys().cloned().collect();
    let code_keys: Vec<_> = code.keys().cloned().collect();
    assert_eq!(code_keys, survey_keys, "the marked set drifted from the survey");

    for (key, legacy) in &code {
        let (kind, param) = key;
        let game = &survey[key];
        assert_eq!(legacy.games, [game.as_str()], "`{kind}.{param}` must name {game}");
        assert_eq!(
            legacy.issue,
            expected_issue(kind),
            "`{kind}.{param}` must point at its Follow-ups issue"
        );
    }
}

#[test]
fn a_marker_accepts_the_converter_and_slug_package_ids() {
    let legacy = LegacyParam {
        games: &["rely-the-needle-4-2"],
        issue: 1269,
    };
    // A hand-written port may keep the slug's underscores; the converter drops the version.
    assert!(legacy.allows("rely_the_needle_4_2"));
    assert!(legacy.allows("rely_the_needle"));
    assert!(!legacy.allows("showcase"));
    assert!(!legacy.allows("rely_the_needle_extra"));
}
