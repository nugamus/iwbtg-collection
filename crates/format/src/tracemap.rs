//! `tests/trace-map.toml`: original GameMaker names to our package ids, written by the research
//! converter (docs/plans/sim-core.md 8.1, 9; research `converter/FORMAT_NOTES.md` section 12).
//!
//! The trace map is research data: it is never shipped or hashed (`tests/`). Tools use it to map
//! trace columns and rooms. The loader reads it only in `LoadMode::Trace`, to accept current
//! converter output (object status for CR3, room file speeds, built-in default sprites, room
//! order). Unknown keys are allowed here, unlike shipped data files, because the research side
//! adds columns as traces need them.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::diag::Diagnostic;
use crate::manifest::toml_error;

/// Package path of the trace map.
pub const TRACE_MAP_PATH: &str = "tests/trace-map.toml";

/// The parsed trace map.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TraceMap {
    /// Original room name to room data.
    pub rooms: BTreeMap<String, TraceRoom>,
    /// Original object name to object data.
    pub objects: BTreeMap<String, TraceObject>,
    /// Original sprite name to package sprite id.
    pub sprites: BTreeMap<String, TraceResource>,
    /// Original background name to package id.
    pub backgrounds: BTreeMap<String, TraceResource>,
    /// Original sound name to package id.
    pub sounds: BTreeMap<String, TraceResource>,
    /// Original font name to package id.
    pub fonts: BTreeMap<String, TraceResource>,
    /// Original path name to package id.
    pub paths: BTreeMap<String, TraceResource>,
    /// Original timeline name to package id.
    pub timelines: BTreeMap<String, TraceResource>,
    /// Audio files loaded by DLLs, by original file path.
    pub external_audio: BTreeMap<String, TraceResource>,
}

/// One original room.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TraceRoom {
    /// Package room id.
    pub id: String,
    /// Position in the original room order.
    pub order: Option<u32>,
    /// Placed instances in the original file.
    pub instances: Option<u32>,
    /// Room speed stored in the original room file (live until the world's Room Start, R14).
    pub speed: Option<u32>,
}

/// One original object.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TraceObject {
    /// GameMaker object index (the object order source, plan 1.3).
    pub index: Option<u32>,
    /// Our kind (`block`, `@spike_up`); empty for engine objects with no package kind.
    pub kind: String,
    /// How the converter mapped it.
    pub status: TraceStatus,
    /// Built-in base of a preset.
    pub base: Option<String>,
    /// Built-in params the mapping sets (`dir = "up"`).
    pub params: BTreeMap<String, TraceParam>,
    /// Package id of the object's sprite.
    pub sprite: Option<String>,
}

/// How the converter mapped an original object.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TraceStatus {
    /// A bare built-in kind.
    Builtin,
    /// A preset object (`base = ...`).
    Preset,
    /// Engine plumbing (world, init, menus, blood, bow, ...): CR3.
    Infra,
    /// No built-in; needs a script.
    #[default]
    Unmapped,
    /// Any other status word.
    Other,
}

/// A param value in the trace map.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TraceParam {
    /// Text.
    Str(String),
    /// Integer.
    Int(i64),
    /// Flag.
    Bool(bool),
    /// A decimal, as IEEE bits.
    Float(u64),
}

/// A non-object resource (sprite, sound, ...).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TraceResource {
    /// Package id.
    pub id: String,
    /// Converter status, if any (`not_converted`).
    pub status: Option<String>,
    /// Target folder for audio (`sounds`, `music`).
    pub to: Option<String>,
    /// Original file name.
    pub file: Option<String>,
}

impl TraceMap {
    /// The object whose package kind is `kind` (`@world`, `block`) with the lowest original index.
    pub fn object_for_kind(&self, kind: &str) -> Option<(&str, &TraceObject)> {
        self.objects
            .iter()
            .filter(|(_, o)| o.kind == kind)
            .min_by_key(|(name, o)| (o.index.unwrap_or(u32::MAX), name.as_str()))
            .map(|(name, o)| (name.as_str(), o))
    }

    /// The trace-map row of the room whose package id is `id`.
    pub fn room_by_id(&self, id: &str) -> Option<(&str, &TraceRoom)> {
        self.rooms
            .iter()
            .find(|(_, r)| r.id == id)
            .map(|(name, r)| (name.as_str(), r))
    }
}

#[derive(Deserialize)]
struct RawMap {
    #[serde(default)]
    rooms: BTreeMap<String, RawRoom>,
    #[serde(default)]
    objects: BTreeMap<String, RawObject>,
    #[serde(default)]
    sprites: BTreeMap<String, RawResource>,
    #[serde(default)]
    backgrounds: BTreeMap<String, RawResource>,
    #[serde(default)]
    sounds: BTreeMap<String, RawResource>,
    #[serde(default)]
    fonts: BTreeMap<String, RawResource>,
    #[serde(default)]
    paths: BTreeMap<String, RawResource>,
    #[serde(default)]
    timelines: BTreeMap<String, RawResource>,
    #[serde(default)]
    external_audio: BTreeMap<String, RawResource>,
}

#[derive(Deserialize)]
struct RawRoom {
    id: String,
    order: Option<u32>,
    instances: Option<u32>,
    speed: Option<u32>,
}

#[derive(Deserialize)]
struct RawObject {
    index: Option<u32>,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    status: String,
    base: Option<String>,
    #[serde(default)]
    params: BTreeMap<String, toml::Value>,
    sprite: Option<String>,
}

#[derive(Deserialize)]
struct RawResource {
    #[serde(default)]
    id: String,
    status: Option<String>,
    to: Option<String>,
    file: Option<String>,
}

/// Parses `tests/trace-map.toml`.
pub fn load_trace_map(text: &str) -> Result<TraceMap, Diagnostic> {
    let raw: RawMap = toml::from_str(text).map_err(|e| toml_error(TRACE_MAP_PATH, text, &e))?;
    let resources = |m: BTreeMap<String, RawResource>| {
        m.into_iter()
            .map(|(k, r)| {
                (
                    k,
                    TraceResource {
                        id: r.id,
                        status: r.status,
                        to: r.to,
                        file: r.file,
                    },
                )
            })
            .collect()
    };
    let mut objects = BTreeMap::new();
    for (name, o) in raw.objects {
        let mut params = BTreeMap::new();
        for (key, value) in o.params {
            let param = match value {
                toml::Value::String(s) => TraceParam::Str(s),
                toml::Value::Integer(i) => TraceParam::Int(i),
                toml::Value::Boolean(b) => TraceParam::Bool(b),
                toml::Value::Float(f) => TraceParam::Float(f.to_bits()),
                other => {
                    return Err(Diagnostic::file_error(
                        TRACE_MAP_PATH,
                        format!(
                            "object `{name}` param `{key}` is a {}, not a string, number or boolean",
                            other.type_str()
                        ),
                    ));
                }
            };
            params.insert(key, param);
        }
        let status = match o.status.as_str() {
            "builtin" => TraceStatus::Builtin,
            "preset" => TraceStatus::Preset,
            "infra" => TraceStatus::Infra,
            "unmapped" | "" => TraceStatus::Unmapped,
            _ => TraceStatus::Other,
        };
        objects.insert(
            name,
            TraceObject {
                index: o.index,
                kind: o.kind,
                status,
                base: o.base,
                params,
                sprite: o.sprite,
            },
        );
    }
    Ok(TraceMap {
        rooms: raw
            .rooms
            .into_iter()
            .map(|(k, r)| {
                (
                    k,
                    TraceRoom {
                        id: r.id,
                        order: r.order,
                        instances: r.instances,
                        speed: r.speed,
                    },
                )
            })
            .collect(),
        objects,
        sprites: resources(raw.sprites),
        backgrounds: resources(raw.backgrounds),
        sounds: resources(raw.sounds),
        fonts: resources(raw.fonts),
        paths: resources(raw.paths),
        timelines: resources(raw.timelines),
        external_audio: resources(raw.external_audio),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rooms_objects_and_resources() {
        let text = r#"
[rooms]
"room1" = { id = "room_1", order = 4, instances = 143, speed = 30 }

[objects]
"spikeUp" = { index = 14, kind = "@spike_up", status = "preset", base = "spike", params = { dir = "up" }, sprite = "spr_5" }
"bow" = { index = 25, kind = "", status = "infra", sprite = "bow" }
"menuSelect" = { index = 273, kind = "@menu_select", status = "infra", future_column = 1 }

[sprites]
"sprSpike" = { id = "spike" }
"#;
        let map = load_trace_map(text).unwrap();
        assert_eq!(map.rooms["room1"].speed, Some(30));
        assert_eq!(map.room_by_id("room_1").map(|(n, _)| n), Some("room1"));
        let spike = &map.objects["spikeUp"];
        assert_eq!(spike.status, TraceStatus::Preset);
        assert_eq!(spike.params["dir"], TraceParam::Str("up".into()));
        assert_eq!(map.objects["bow"].status, TraceStatus::Infra);
        assert_eq!(
            map.object_for_kind("@menu_select").map(|(n, _)| n),
            Some("menuSelect")
        );
        assert_eq!(map.sprites["sprSpike"].id, "spike");
    }

    #[test]
    fn syntax_errors_have_positions() {
        let d = load_trace_map("[rooms]\n\"a\" = { id = }\n").unwrap_err();
        assert_eq!(d.file, TRACE_MAP_PATH);
        assert_eq!(d.line, 2);
    }
}
