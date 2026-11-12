//! JSON Schemas for the data files (docs/format/v0.md, Common rules for data files).
//!
//! Every data file starts with a `#:schema` line that points at one of the files this module
//! generates into `docs/format/schema/v0/`. Editors use them for completion and validation, and
//! the converter checks its output against them. They are generated, never hand-written:
//!
//! - `game`, `room` and `sprite` come straight from the types the loader deserializes
//!   ([`Manifest`], `RawRoom`, `RawSidecar`). Their `toml::Spanned` fields carry a
//!   `schemars(with = ...)` override with the plain type.
//! - `physics` comes from `iwbtg_sim::def::physics::PhysicsProfileDraft` and the section drafts
//!   under it, through the sim crate's own `schema` feature. The action, collision-run and class
//!   names come from the enums and tables the kid reads (`Action::ALL`, `ClassMask::NAMED`, the
//!   per-file action enums), so there is no second list.
//! - `tiles` and `background` (the tileset sidecar) come from the types `art` deserializes
//!   ([`TilesFile`], [`TilesetFile`]).
//! - `object` and `timeline` come from the shape types below. The loader reads object
//!   files key by key (every key that is not reserved is a param of the base), so the shape lists
//!   the reserved keys and leaves the rest open. `timeline`, `path` and `sound` are
//!   shape types here that mirror the private file structs of `timelines.rs`, `paths.rs` and
//!   `sounds.rs` (`tests/schemas.rs` validates the loader's accepted files and rejects its
//!   rejected ones, so they cannot drift unseen).
//!
//! `tests/schemas.rs` regenerates and diffs the files (`UPDATE_SCHEMAS=1 cargo test -p
//! iwbtg-format --test schemas` rewrites them), validates the examples in v0.md and the valid
//! test packages against them.

#![allow(
    clippy::disallowed_types,
    reason = "serde's `flatten` expansion names f32; no value of that type is used"
)]

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use iwbtg_sim::def::physics::PhysicsProfileDraft;

pub use crate::art::{TilesFile, TilesetFile};
use crate::engine_pack::EnginePack;
use crate::manifest::Manifest;
use crate::number::Number;
use crate::rooms::RawRoom;
use crate::sprites::RawSidecar;

/// The `runner` values (`iwbtg_sim::rules::Runner`, which lives in the sim crate and has no
/// schema). `tests/schemas.rs` fails to compile when a runner is added there without a variant
/// here.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RunnerSchema {
    /// GameMaker 8.0.
    Gm80,
    /// GameMaker 8.1.
    Gm81,
}

/// One entry of `instances` in a room file. Keys other than the common ones are params of the
/// kind, checked against the kind's own param list at load.
#[derive(Deserialize, JsonSchema)]
pub struct InstanceShape {
    /// Kind: a built-in name, or `@<object id>`.
    pub kind: String,
    /// Position of the first instance.
    pub x: Number,
    /// Position of the first instance.
    pub y: Number,
    /// Instance name, for warps and scripts.
    pub name: Option<String>,
    /// Group tag, for triggers and scripts.
    pub group: Option<String>,
    /// `image_xscale`.
    pub xscale: Option<Number>,
    /// `image_yscale`.
    pub yscale: Option<Number>,
    /// `image_angle`.
    pub angle: Option<Number>,
    /// `[columns, rows]`: expands into a grid of identical instances.
    pub run: Option<[u32; 2]>,
    /// `[dx, dy]` spacing of the grid; default `[32, 32]`.
    pub step: Option<[Number; 2]>,
    /// GM8 instance id from the room file; unique in the room, never with `run`.
    #[schemars(range(max = 2_147_483_647))]
    pub gm_id: Option<u32>,
    /// Params named like an instance key (`params = { kind = 20 }`, GM creation code that sets
    /// `kind`); every other param is a key of its own.
    #[serde(rename = "params")]
    pub key_params: Option<BTreeMap<String, Value>>,
    /// Params of the kind (`dir`, `to`, ...).
    #[serde(flatten)]
    pub params: BTreeMap<String, Value>,
}

/// An object file (`objects/<id>.toml`): a preset or a scripted object.
#[derive(Deserialize, JsonSchema)]
pub struct ObjectShape {
    /// Built-in base kind.
    pub base: Option<String>,
    /// Script id (`scripts/<id>.rhai`).
    pub script: Option<String>,
    /// Default sprite id.
    pub sprite: Option<String>,
    /// Object-level collision mask sprite (GM object `mask`).
    pub mask: Option<String>,
    /// GM `solid`; default false.
    pub solid: Option<bool>,
    /// GM `visible`; default true.
    pub visible: Option<bool>,
    /// GM `persistent`; default false.
    pub persistent: Option<bool>,
    /// GM `depth`; default 0.
    pub depth: Option<Number>,
    /// Extra collision classes: named classes such as `block` or `killer`, or package class names.
    pub counts_as: Option<Vec<String>>,
    /// Classes whose touch runs the script's `collide(other)`: named classes or package class names.
    pub collides_with: Option<Vec<String>>,
    /// Legacy converter key, read only by trace builds. Write `counts_as` instead.
    pub roles: Option<Vec<String>>,
    /// Params with defaults; instances may override.
    pub params: Option<BTreeMap<String, Value>>,
    /// Every other key is a param of the base, checked against its param list at load.
    #[serde(flatten)]
    pub other_params: BTreeMap<String, Value>,
}

/// A timeline file (`timelines/<id>.toml`).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimelineShape {
    /// Start again after the last moment; default false.
    #[serde(rename = "loop")]
    pub looping: Option<bool>,
    /// Moments, in tick order.
    pub moments: Vec<MomentShape>,
}

/// One timeline moment.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MomentShape {
    /// Ticks from the timeline's start.
    pub t: u64,
    /// Runs `moment_<call>()` on the instance running the timeline.
    pub call: String,
}

/// A path file (`paths/<id>.toml`).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PathShape {
    /// `[x, y]` or `[x, y, speed factor]` (the factor defaults to 100); at least two points.
    #[schemars(length(min = 2))]
    pub points: Vec<PointShape>,
    /// The last point connects to the first; default false.
    pub closed: Option<bool>,
    /// Curved path (not supported by the sim yet: a load warning); default false.
    pub smooth: Option<bool>,
    /// Smoothing precision; default 4.
    pub precision: Option<u32>,
}

/// One path point.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum PointShape {
    /// `[x, y]`.
    Xy([Number; 2]),
    /// `[x, y, speed factor]`.
    Xys([Number; 3]),
}

/// The `kind` values of a sound sidecar.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SoundKindShape {
    /// Copies may overlap.
    Normal,
    /// Music: at most one copy plays (S2).
    Multimedia,
}

/// A sound or music sidecar (`sounds/<id>.toml`, `music/<id>.toml`).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SoundShape {
    /// Length in ticks (song length times the tick rate; converter request CR9). Without it the
    /// length is 0 and the load warns.
    pub length_ticks: Option<u32>,
    /// Default volume; default 1.
    pub volume: Option<Number>,
    /// Default pan; default 0.
    pub pan: Option<Number>,
    /// Overrides the folder's kind.
    pub kind: Option<SoundKindShape>,
}

/// Every schema as `(file stem, pretty JSON text)`. The file is `<stem>.schema.json`.
pub fn generate() -> Vec<(&'static str, String)> {
    fn text<T: JsonSchema>(title: &str) -> String {
        let mut value = serde_json::to_value(schemars::schema_for!(T)).expect("schema is JSON");
        value["title"] = Value::String(title.to_owned());
        let mut s = serde_json::to_string_pretty(&value).expect("schema is JSON");
        s.push('\n');
        s
    }
    vec![
        ("game", text::<Manifest>("IWBTG game.toml (format v0)")),
        (
            "engine",
            text::<EnginePack>("IWBTG engine pack engine.toml (format v0)"),
        ),
        (
            "object",
            text::<ObjectShape>("IWBTG objects/<id>.toml (format v0)"),
        ),
        (
            "physics",
            text::<PhysicsProfileDraft>("IWBTG physics.toml (kid physics profile, format v0)"),
        ),
        ("room", text::<RawRoom>("IWBTG rooms/<id>.toml (format v0)")),
        (
            "tiles",
            text::<TilesFile>("IWBTG rooms/<id>.tiles.toml (format v0)"),
        ),
        (
            "background",
            text::<TilesetFile>("IWBTG backgrounds/<id>.toml tileset sidecar (format v0)"),
        ),
        (
            "sprite",
            text::<RawSidecar>("IWBTG sprites/<id>.toml (format v0)"),
        ),
        (
            "timeline",
            text::<TimelineShape>("IWBTG timelines/<id>.toml (format v0)"),
        ),
        ("path", text::<PathShape>("IWBTG paths/<id>.toml (format v0)")),
        (
            "sound",
            text::<SoundShape>("IWBTG sounds/<id>.toml and music/<id>.toml (format v0)"),
        ),
    ]
}
