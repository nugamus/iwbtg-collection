//! Step 9: `gameplay_hash` (docs/format/versioning.md).

use super::*;

/// `game.toml` keys that cannot change the sim (docs/format/versioning.md).
const COSMETIC_MANIFEST_KEYS: [&str; 9] = [
    "title",
    "version",
    "source_version",
    "creators",
    "source",
    "third_party",
    "music",
    "key_art",
    "controls",
];

/// `[[maps]]` keys that are only shown: renaming a map keeps its board.
const COSMETIC_MAP_KEYS: [&str; 3] = ["name", "difficulty", "category"];

/// BLAKE3 over the canonical parsed form of everything that can change the sim
/// (docs/format/versioning.md; architecture "Versions"): the gameplay fields of `game.toml`,
/// the physics profile, objects, rooms (not tiles), timelines, paths, sound and sprite sidecars,
/// scripts, the built mask bits, and the mask rule ([`ALPHA_IN_MASK`]). Data files hash as their
/// parsed values with sorted keys, so comments and formatting do not count; scripts hash as text
/// with LF line ends.
pub fn gameplay_hash(files: &PackageFiles, manifest: &Manifest, def: &GameDef) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"iwbtg-gameplay-hash v0\0");
    h.update(&def.engine_rev.to_le_bytes());
    h.update(match ALPHA_IN_MASK {
        AlphaRule::Greater => b"alpha>",
        AlphaRule::GreaterOrEqual => b"alpha>=",
    });
    let physics = manifest.physics.get_ref().as_str();
    for (path, bytes) in files.iter() {
        let data = path == MANIFEST_PATH
            || path == physics
            || path.starts_with("physics/")
            || crate::engine_pack::is_gameplay_file(path)
            || [
                "objects/",
                "timelines/",
                "paths/",
                "particles/",
                "sprites/",
                "sounds/",
                "music/",
            ]
            .iter()
            .any(|d| path.starts_with(d))
            || (path.starts_with("rooms/") && !path.ends_with(".tiles.toml"));
        let script = path.starts_with("scripts/") && path.ends_with(".rhai");
        if script {
            let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
            hash_bytes(&mut h, path.as_bytes());
            hash_bytes(&mut h, text.as_bytes());
        } else if data && path.ends_with(".toml") {
            let text = String::from_utf8_lossy(bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes)).into_owned();
            let Ok(mut table) = toml::from_str::<toml::Table>(&text) else {
                continue;
            };
            if path == MANIFEST_PATH {
                for k in COSMETIC_MANIFEST_KEYS {
                    table.remove(k);
                }
                normalize_compat(&mut table, *manifest.format.get_ref());
                if let Some(toml::Value::Array(maps)) = table.get_mut("maps") {
                    for map in maps.iter_mut().filter_map(toml::Value::as_table_mut) {
                        for k in COSMETIC_MAP_KEYS {
                            map.remove(k);
                        }
                    }
                }
            }
            hash_bytes(&mut h, path.as_bytes());
            canonical(&mut h, &toml::Value::Table(table));
        }
    }
    h.update(&(def.masks.len() as u64).to_le_bytes());
    for m in &def.masks {
        for e in [m.bbox.left, m.bbox.top, m.bbox.right, m.bbox.bottom] {
            h.update(&e.to_le_bytes());
        }
        match &m.shape {
            MaskShape::Rect => {
                h.update(b"r");
            }
            MaskShape::Empty => {
                h.update(b"e");
            }
            MaskShape::Bits(b) => {
                h.update(b"b");
                h.update(&b.width().to_le_bytes());
                h.update(&b.height().to_le_bytes());
                let mut byte = 0u8;
                let mut n = 0u32;
                for y in 0..i64::from(b.height()) {
                    for x in 0..i64::from(b.width()) {
                        byte = byte << 1 | u8::from(b.get(x, y));
                        n += 1;
                        if n == 8 {
                            h.update(&[byte]);
                            (byte, n) = (0, 0);
                        }
                    }
                }
                h.update(&[byte, u8::try_from(n).unwrap_or(0)]);
            }
        }
    }
    *h.finalize().as_bytes()
}

/// Removes `ruleset` and `[compat]` from a manifest table **only in format 0**. There the loader
/// rejects any declaration that does not resolve to the package's existing behavior (`gm8` at
/// `engine_rev`, the physics preset's family, a deviation equal to the physics value), so the
/// explicit form must keep the absent-field hash. A declaration that changes behavior cannot
/// reach the hash in format 0 because it is an error; at format 1 it must be hashed (user
/// decision 2, decision 0005), which is the format-1 path still in #1160.
fn normalize_compat(table: &mut toml::Table, format: u32) {
    if format == 0 {
        table.remove("ruleset");
        table.remove("compat");
    }
}

fn hash_bytes(h: &mut blake3::Hasher, b: &[u8]) {
    h.update(&(b.len() as u64).to_le_bytes());
    h.update(b);
}

/// A TOML value in a fixed binary form: type tag, then content; tables with sorted keys; floats
/// as bits.
fn canonical(h: &mut blake3::Hasher, v: &toml::Value) {
    match v {
        toml::Value::String(s) => {
            h.update(b"s");
            hash_bytes(h, s.as_bytes());
        }
        toml::Value::Integer(i) => {
            h.update(b"i");
            h.update(&i.to_le_bytes());
        }
        toml::Value::Float(f) => {
            h.update(b"f");
            h.update(&f.to_bits().to_le_bytes());
        }
        toml::Value::Boolean(b) => {
            h.update(if *b { b"T" } else { b"F" });
        }
        toml::Value::Datetime(d) => {
            h.update(b"d");
            hash_bytes(h, d.to_string().as_bytes());
        }
        toml::Value::Array(a) => {
            h.update(b"a");
            h.update(&(a.len() as u64).to_le_bytes());
            for item in a {
                canonical(h, item);
            }
        }
        toml::Value::Table(t) => {
            h.update(b"t");
            h.update(&(t.len() as u64).to_le_bytes());
            let mut keys: Vec<&String> = t.keys().collect();
            keys.sort();
            for k in keys {
                hash_bytes(h, k.as_bytes());
                if let Some(v) = t.get(k) {
                    canonical(h, v);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_compat;

    /// The hash boundary: format 0 normalizes only declarations that must resolve to the
    /// package's existing behavior; format 1 must hash them (decision 0005, user decision 2).
    #[test]
    fn compat_is_normalized_only_in_format_zero() {
        let mut table: toml::Table =
            toml::from_str("format = 0\nruleset = \"gm8@0\"\n[compat]\nprofile = \"gm8/a1@1\"\n")
                .expect("table");
        normalize_compat(&mut table, 0);
        assert!(!table.contains_key("ruleset"));
        assert!(!table.contains_key("compat"));

        let mut table: toml::Table =
            toml::from_str("format = 1\nruleset = \"gm8@1\"\n[compat]\nprofile = \"gm8/a1@1\"\n")
                .expect("table");
        normalize_compat(&mut table, 1);
        assert!(table.contains_key("ruleset"), "format 1 hashes the ruleset");
        assert!(
            table.contains_key("compat"),
            "format 1 hashes the profile and its deviations"
        );
    }
}
