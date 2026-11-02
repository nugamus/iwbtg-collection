//! The JSON Schemas in `docs/format/schema/v0/` (#212): not stale, and the examples in
//! `docs/format/v0.md` and the valid test packages validate against them.
//!
//! `UPDATE_SCHEMAS=1 cargo test -p iwbtg-format --test schemas` rewrites the schema files.

use std::fs;
use std::path::{Path, PathBuf};

use iwbtg_format::schema;

fn client_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn schema_dir() -> PathBuf {
    client_root().join("docs/format/schema/v0")
}

fn load_schema(stem: &str) -> serde_json::Value {
    let path = schema_dir().join(format!("{stem}.schema.json"));
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).expect("schema file is JSON")
}

/// Parses a TOML document into the JSON value a schema is checked against.
fn toml_to_json(text: &str) -> serde_json::Value {
    fn convert(v: toml::Value) -> serde_json::Value {
        use serde_json::Value;
        match v {
            toml::Value::String(s) => Value::String(s),
            toml::Value::Integer(i) => Value::from(i),
            toml::Value::Float(f) => Value::from(f),
            toml::Value::Boolean(b) => Value::Bool(b),
            // Schema tools see a TOML date as a string.
            toml::Value::Datetime(d) => Value::String(d.to_string()),
            toml::Value::Array(a) => Value::Array(a.into_iter().map(convert).collect()),
            toml::Value::Table(t) => Value::Object(t.into_iter().map(|(k, v)| (k, convert(v))).collect()),
        }
    }
    convert(toml::from_str(text).expect("example is valid TOML"))
}

fn assert_valid(schema_stem: &str, what: &str, text: &str) {
    let schema = load_schema(schema_stem);
    let validator = jsonschema::validator_for(&schema).expect("schema compiles");
    let instance = toml_to_json(text);
    let errors: Vec<String> = validator
        .iter_errors(&instance)
        .map(|e| format!("  {} at {}", e, e.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "{what} does not validate against {schema_stem}.schema.json:\n{}",
        errors.join("\n")
    );
}

#[test]
fn schema_files_are_not_stale() {
    let update = std::env::var_os("UPDATE_SCHEMAS").is_some();
    let mut stale = Vec::new();
    for (stem, text) in schema::generate() {
        let path = schema_dir().join(format!("{stem}.schema.json"));
        if update {
            fs::create_dir_all(schema_dir()).unwrap();
            fs::write(&path, &text).unwrap();
            continue;
        }
        // Git may check the file out with CRLF line endings.
        let on_disk = fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        if on_disk != text {
            stale.push(path.display().to_string());
        }
    }
    assert!(
        stale.is_empty(),
        "stale or missing schemas: {stale:?}\nrun `UPDATE_SCHEMAS=1 cargo test -p iwbtg-format --test schemas` and commit the result"
    );
}

/// The ```toml blocks of v0.md that start with a `#:schema` line, as `(schema stem, text)`.
fn v0_examples() -> Vec<(String, String)> {
    let md = fs::read_to_string(client_root().join("docs/format/v0.md")).unwrap();
    let mut out = Vec::new();
    let mut block: Option<Vec<String>> = None;
    for line in md.lines() {
        let trimmed = line.trim_start();
        match &mut block {
            None if trimmed == "```toml" => block = Some(Vec::new()),
            None => {}
            Some(_) if trimmed == "```" => {
                let lines = block.take().unwrap();
                let first = lines.first().map(|l| l.trim_start()).unwrap_or("");
                if let Some(rest) = first.strip_prefix("#:schema ")
                    && let Some(stem) = rest
                        .rsplit('/')
                        .next()
                        .and_then(|f| f.strip_suffix(".schema.json"))
                {
                    out.push((stem.to_owned(), lines.join("\n")));
                }
            }
            Some(lines) => lines.push(line.trim_start_matches("  ").to_owned()),
        }
    }
    out
}

#[test]
fn v0_examples_validate() {
    let examples = v0_examples();
    // game, object (x2), room, tiles, timeline, sprite, and physics.
    for stem in ["game", "object", "room", "tiles", "timeline", "sprite"] {
        assert!(
            examples.iter().any(|(s, _)| s == stem),
            "v0.md has no example for {stem}"
        );
    }
    for (stem, text) in &examples {
        assert_valid(stem, &format!("a v0.md example:\n{text}\n"), text);
    }
}

#[test]
fn valid_test_packages_validate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/valid");
    let mut checked = 0;
    for pkg in fs::read_dir(&root).unwrap() {
        let pkg = pkg.unwrap().path();
        for (dir, stem) in [("objects", "object"), ("rooms", "room"), ("sprites", "sprite")] {
            let Ok(entries) = fs::read_dir(pkg.join(dir)) else {
                continue;
            };
            for e in entries {
                let path = e.unwrap().path();
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                if !name.ends_with(".toml") {
                    continue;
                }
                let stem = if name.ends_with(".tiles.toml") {
                    "tiles"
                } else {
                    stem
                };
                assert_valid(
                    stem,
                    &path.display().to_string(),
                    &fs::read_to_string(&path).unwrap(),
                );
                checked += 1;
            }
        }
        assert_valid(
            "game",
            &pkg.display().to_string(),
            &fs::read_to_string(pkg.join("game.toml")).unwrap(),
        );
        checked += 1;
    }
    assert!(checked >= 3, "found only {checked} files");
}

#[test]
fn schemas_reject_what_the_loader_rejects() {
    // An unknown key in game.toml is an error (docs/format/v0.md, Common rules).
    let schema = load_schema("game");
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(!validator.is_valid(&serde_json::json!({ "bogus": 1 })));
    let room = load_schema("room");
    let validator = jsonschema::validator_for(&room).unwrap();
    assert!(!validator.is_valid(&serde_json::json!({ "size": [800, 608], "bogus": 1 })));
    assert!(validator.is_valid(&serde_json::json!({ "size": [800, 608] })));
}

/// A new runner in the sim must get a schema value: this match stops compiling until it does.
#[test]
fn runner_schema_covers_every_runner() {
    use iwbtg_sim::rules::Runner;
    let all = |r: Runner| match r {
        Runner::Gm80 | Runner::Gm81 => (),
    };
    all(Runner::Gm80);
    let schema = load_schema("game");
    let runner = serde_json::to_string(&schema).unwrap();
    for name in ["gm80", "gm81"] {
        assert!(runner.contains(name), "game schema lacks runner {name}");
    }
}

/// The rely example (`docs/format/examples/rely-physics.toml`) validates against the physics
/// schema. That schema is written by hand (WP5a.5): generating it needs a `schema` feature on
/// the sim's profile types.
#[test]
fn the_rely_physics_example_validates() {
    let text = fs::read_to_string(client_root().join("docs/format/examples/rely-physics.toml")).unwrap();
    assert_valid("physics", "docs/format/examples/rely-physics.toml", &text);
}

#[test]
fn the_physics_schema_rejects_mistakes() {
    let validator = jsonschema::validator_for(&load_schema("physics")).unwrap();
    assert!(validator.is_valid(&serde_json::json!({ "preset": "A1", "numbers": { "run_speed": 4 } })));
    for bad in [
        serde_json::json!({ "bogus": 1 }),
        serde_json::json!({ "numbers": { "run_spead": 3 } }),
        serde_json::json!({ "events": { "step": ["wlak"] } }),
        serde_json::json!({ "jump": { "ground_classes": ["blokc"] } }),
        serde_json::json!({ "input": { "walk": "telepathy" } }),
    ] {
        assert!(!validator.is_valid(&bad), "{bad} should be rejected");
    }
}
