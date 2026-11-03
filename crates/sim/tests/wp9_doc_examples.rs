//! Every ```rhai example in the docs passes the script validator (#367), so the docs name only
//! functions the host registers.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, KindDef, RoomDef, ScriptDef};
use iwbtg_sim::script::validate::validate;

const DOCS: [&str; 3] = [
    "docs/format/v0.md",
    "docs/architecture.md",
    "docs/decisions/0001-script-language.md",
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
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut seen = 0;
    for doc in DOCS {
        let text = std::fs::read_to_string(root.join(doc)).expect("doc");
        for (n, source) in examples(&text).into_iter().enumerate() {
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
