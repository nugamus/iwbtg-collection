//! Every ```rhai example in the docs passes the script validator (#367), so the docs name only
//! functions the host registers.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, KindDef, RoomDef, ScriptDef};
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::host::Iws;
use iwbtg_sim::script::validate::{EVENT_FNS, validate};

const ARCHITECTURE: &str = include_str!("../../../docs/architecture.md");
const DOCS: [(&str, &str); 3] = [
    ("docs/format/v0.md", include_str!("../../../docs/format/v0.md")),
    ("docs/architecture.md", ARCHITECTURE),
    (
        "docs/decisions/0001-script-language.md",
        include_str!("../../../docs/decisions/0001-script-language.md"),
    ),
];

/// The bodies of the ```rhai fences of a markdown text.
fn examples(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Option<String> = None;
    for line in text.lines() {
        match (&mut cur, line.trim_end()) {
            (None, "```rhai") => cur = Some(String::new()),
            (Some(_), "```") => out.extend(cur.take()),
            (Some(body), l) => {
                body.push_str(l);
                body.push('\n');
            }
            (None, _) => {}
        }
    }
    out
}

#[test]
fn every_rhai_example_in_the_docs_validates() {
    let mut seen = 0;
    for (doc, text) in DOCS {
        for (n, source) in examples(text).into_iter().enumerate() {
            let mut b = GameDefBuilder::new("docs");
            let room = b.room(RoomDef::new("room", 320, 240));
            b.kind(KindDef::new("@orb", Builtin::Block));
            b.kind(KindDef::new("kid_bullet", Builtin::KidBullet));
            b.start_room(room);
            let mut def = b.build().expect("def");
            def.scripts.push(ScriptDef {
                name: format!("{doc}#{n}"),
                source: source.clone(),
            });
            let issues = validate(&def);
            assert!(issues.is_empty(), "{doc} example {n}: {issues:#?}\n{source}");
            seen += 1;
        }
    }
    assert!(seen >= 2, "found only {seen} examples");
}

/// The function names the Script API section of the architecture page uses without marking them
/// *planned* are registered (or are event functions): the docs and the registry agree.
#[test]
fn the_architecture_script_api_names_only_registered_functions_or_marks_them_planned() {
    let text = ARCHITECTURE;
    let start = text.find("### Script API (first version)").expect("section");
    let end = text[start..]
        .find(
            "
## ",
        )
        .expect("next section")
        + start;
    let iws = Iws::new(RunnerRules::default().num);
    let mut registered: std::collections::BTreeSet<String> = iws
        .rhai_engine()
        .collect_fn_metadata(None, |f| Some(f.metadata.name.to_string()), true)
        .into_iter()
        .collect();
    registered.extend(EVENT_FNS.iter().map(|(n, _)| (*n).to_owned()));
    registered.extend(["trigger", "draw", "moment_"].map(str::to_owned));
    let mut checked = 0;
    for line in text[start..end].lines() {
        let lower = line.to_lowercase();
        if line.starts_with('|') && lower.contains("planned") {
            continue;
        }
        let head = lower.find("planned").map_or(line, |i| &line[..i]);
        let mut rest = head;
        while let Some(open) = rest.find('`') {
            let Some(len) = rest[open + 1..].find('`') else {
                break;
            };
            let code = &rest[open + 1..open + 1 + len];
            rest = &rest[open + len + 2..];
            let Some(paren) = code.find('(') else { continue };
            let name = code[..paren].rsplit('.').next().unwrap_or("");
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            checked += 1;
            assert!(
                registered.contains(name) || name == "moment_<name>",
                "`{name}` is on the architecture page without *planned*, but it is not registered: {line}"
            );
        }
    }
    assert!(checked > 25, "only {checked} names checked");
}
