//! Room files: `run` expansion in row-major order, params, views, file speed, role, `on_create`
//! (docs/plans/sim-core.md 4.1, 8.2 step 4, 8.3; docs/format/v0.md, Rooms).
//!
//! This module parses one `rooms/<id>.toml`; `load` resolves kinds, ranks and names. Instances
//! stay in file order, which is creation order (Inferred, CLONE_SPEC rooms "created in the order
//! they are listed"); `run` grids expand in place, row by row, and are never sorted or merged.

use std::collections::BTreeMap;
use std::ops::Range;

use serde::Deserialize;
use toml::Spanned;

use crate::diag::Diagnostic;
use crate::manifest::{RoleName, toml_error};
use crate::number::Number;
use crate::objects::number;

/// Most instances in one room after `run` expansion (docs/format/v0.md, Assets).
pub const MAX_INSTANCES_PER_ROOM: usize = 50_000;

/// Default `run` spacing (docs/format/v0.md; spacing default is still OPEN there).
pub const DEFAULT_STEP: (f64, f64) = (32.0, 32.0);

/// Keys of an instance that are not params. `depth` and `visible` are params: instance creation
/// code sets them like any other variable, so they travel with the other creation-code params.
pub const INSTANCE_KEYS: [&str; 10] = [
    "kind", "x", "y", "name", "group", "xscale", "yscale", "angle", "run", "step",
];

/// One parsed room file.
#[derive(Clone, Debug)]
pub struct RoomFile {
    /// Room id (file stem).
    pub id: String,
    /// Width and height.
    pub size: (u32, u32),
    /// `room_speed` on entry (R14); `None` uses the game's tick rate.
    pub file_speed: Option<Spanned<u32>>,
    /// Role (CR6); `None` is play.
    pub role: Option<RoleName>,
    /// R does nothing in this room (`no_restart`).
    pub no_restart: bool,
    /// GM persistent room.
    pub persistent: bool,
    /// GM "enable the use of views".
    pub views_enabled: bool,
    /// Up to 8 views.
    pub views: Vec<Spanned<ViewFile>>,
    /// Instances as written (before `run` expansion).
    pub instances: Vec<InstanceFile>,
    /// Room creation code, as data.
    pub on_create: Vec<Spanned<ActionFile>>,
}

/// One `[[views]]` entry.
#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ViewFile {
    /// Visible when the room starts.
    #[serde(default)]
    pub visible: bool,
    /// View in the room: x, y, width, height.
    pub view: [Number; 4],
    /// Port on the screen: x, y, width, height; default the view's size at (0, 0).
    #[serde(default)]
    pub port: Option<[Number; 4]>,
    /// Kind to follow.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<String>"))]
    pub follow: Option<Spanned<String>>,
    /// Horizontal and vertical border; default 32, 32.
    #[serde(default)]
    pub border: Option<[Number; 2]>,
    /// Horizontal and vertical speed limits; default -1, -1 (no limit).
    #[serde(default)]
    pub speed: Option<[Number; 2]>,
}

/// One room creation action (plan 4.2 step 6), `{ do = "...", ... }`.
#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "do", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionFile {
    /// `global.<var> = value`.
    SetGlobal {
        /// Global name.
        var: String,
        /// A number, string or boolean (true is 1).
        #[cfg_attr(feature = "schema", schemars(with = "serde_json::Value"))]
        value: toml::Value,
    },
    /// `instance_create(x, y, kind)`.
    Spawn {
        /// Kind.
        kind: String,
        /// Position.
        x: Number,
        /// Position.
        y: Number,
        /// Params of the new instance.
        #[serde(default)]
        #[cfg_attr(feature = "schema", schemars(with = "BTreeMap<String, serde_json::Value>"))]
        params: BTreeMap<String, toml::Value>,
    },
    /// The engine's `loadGame` (R23).
    LoadGame,
    /// `instance_destroy()` (R24: `Unsupported` until the spec says which instance is self).
    DestroySelf,
}

/// One instance entry as written.
#[derive(Clone, Debug)]
pub struct InstanceFile {
    /// Where the entry is.
    pub span: Range<usize>,
    /// Kind reference.
    pub kind: Spanned<String>,
    /// Position of the first instance.
    pub x: f64,
    /// Position of the first instance.
    pub y: f64,
    /// Instance name.
    pub name: Option<Spanned<String>>,
    /// Instance group.
    pub group: Option<Spanned<String>>,
    /// `image_xscale`.
    pub xscale: f64,
    /// `image_yscale`.
    pub yscale: f64,
    /// `image_angle`.
    pub angle: f64,
    /// Grid size: columns, rows.
    pub run: (u32, u32),
    /// Grid spacing.
    pub step: (f64, f64),
    /// Params (instance creation code as data), in key order.
    pub params: BTreeMap<String, Spanned<toml::Value>>,
}

impl InstanceFile {
    /// The positions of the expanded grid in row-major order: row 0 left to right, then row 1.
    pub fn positions(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        let (cols, rows) = self.run;
        (0..rows).flat_map(move |r| {
            (0..cols).map(move |c| {
                (
                    self.x + f64::from(c) * self.step.0,
                    self.y + f64::from(r) * self.step.1,
                )
            })
        })
    }

    /// Number of instances after expansion.
    pub fn count(&self) -> u64 {
        u64::from(self.run.0) * u64::from(self.run.1)
    }
}

#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct RawRoom {
    #[cfg_attr(feature = "schema", schemars(with = "[u32; 2]"))]
    size: Spanned<[u32; 2]>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<u32>"))]
    file_speed: Option<Spanned<u32>>,
    #[serde(default)]
    role: Option<RoleName>,
    #[serde(default)]
    no_restart: bool,
    #[serde(default)]
    persistent: bool,
    #[serde(default)]
    music: Option<String>,
    #[serde(default)]
    background: Option<String>,
    #[serde(default)]
    background_color: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Vec<serde_json::Value>"))]
    backgrounds: Vec<toml::Value>,
    #[serde(default)]
    views_enabled: bool,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Vec<ViewFile>"))]
    views: Vec<Spanned<ViewFile>>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Vec<crate::schema::InstanceShape>"))]
    instances: Vec<Spanned<BTreeMap<Spanned<String>, Spanned<toml::Value>>>>,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Vec<ActionFile>"))]
    on_create: Vec<Spanned<ActionFile>>,
}

/// Parses `rooms/<id>.toml`. `path` and `text` are for diagnostics.
pub fn parse_room(id: &str, path: &str, text: &str) -> Result<RoomFile, Diagnostic> {
    let raw: RawRoom = toml::from_str(text).map_err(|e| toml_error(path, text, &e))?;
    let err = |span: Range<usize>, msg: String| Diagnostic::error_at(path, text, span.start, msg);
    // Visual and audio keys: read so unknown keys stay errors, but not part of the sim.
    let _ = (
        &raw.music,
        &raw.background,
        &raw.background_color,
        &raw.backgrounds,
    );
    let [w, h] = *raw.size.get_ref();
    if w == 0 || h == 0 {
        return Err(err(raw.size.span(), "size must be at least 1 x 1".to_owned()));
    }
    if let Some(s) = &raw.file_speed
        && *s.get_ref() == 0
    {
        return Err(err(s.span(), "file_speed must be at least 1".to_owned()));
    }
    if raw.views.len() > 8 {
        return Err(err(raw.views[8].span(), "a room has at most 8 views".to_owned()));
    }
    let mut instances = Vec::with_capacity(raw.instances.len());
    let mut total: u64 = 0;
    for entry in &raw.instances {
        let inst = parse_instance(entry, &err)?;
        total = total.saturating_add(inst.count());
        if total > MAX_INSTANCES_PER_ROOM as u64 {
            return Err(err(
                entry.span(),
                format!("the room has more than {MAX_INSTANCES_PER_ROOM} instances"),
            ));
        }
        instances.push(inst);
    }
    Ok(RoomFile {
        id: id.to_owned(),
        size: (w, h),
        file_speed: raw.file_speed,
        role: raw.role,
        no_restart: raw.no_restart,
        persistent: raw.persistent,
        views_enabled: raw.views_enabled,
        views: raw.views,
        instances,
        on_create: raw.on_create,
    })
}

fn parse_instance(
    entry: &Spanned<BTreeMap<Spanned<String>, Spanned<toml::Value>>>,
    err: &impl Fn(Range<usize>, String) -> Diagnostic,
) -> Result<InstanceFile, Diagnostic> {
    let map = entry.get_ref();
    let get = |k: &str| map.iter().find(|(key, _)| key.get_ref() == k).map(|(_, v)| v);
    let num = |k: &str, default: Option<f64>| -> Result<f64, Diagnostic> {
        match get(k) {
            Some(v) => number(v.get_ref()).map_err(|e| err(v.span(), format!("`{k}`: {e}"))),
            None => default.ok_or_else(|| err(entry.span(), format!("instance needs `{k}`"))),
        }
    };
    let text = |k: &str| -> Result<Option<Spanned<String>>, Diagnostic> {
        match get(k) {
            None => Ok(None),
            Some(v) => match v.get_ref() {
                toml::Value::String(s) => Ok(Some(Spanned::new(v.span(), s.clone()))),
                _ => Err(err(v.span(), format!("`{k}` must be a string"))),
            },
        }
    };
    let pair = |k: &str| -> Result<Option<(f64, f64)>, Diagnostic> {
        match get(k) {
            None => Ok(None),
            Some(v) => match v.get_ref() {
                toml::Value::Array(a) if a.len() == 2 => {
                    let a0 = number(&a[0]).map_err(|e| err(v.span(), format!("`{k}`: {e}")))?;
                    let a1 = number(&a[1]).map_err(|e| err(v.span(), format!("`{k}`: {e}")))?;
                    Ok(Some((a0, a1)))
                }
                _ => Err(err(v.span(), format!("`{k}` must be a list of two numbers"))),
            },
        }
    };
    let kind = text("kind")?.ok_or_else(|| err(entry.span(), "instance needs `kind`".to_owned()))?;
    let run = match pair("run")? {
        None => (1, 1),
        Some((c, r)) => {
            let whole = |n: f64| (1.0..=f64::from(u32::MAX)).contains(&n) && n.fract() == 0.0;
            if !whole(c) || !whole(r) {
                let span = get("run").map_or(entry.span(), Spanned::span);
                return Err(err(
                    span,
                    "`run` must be two whole numbers of at least 1".to_owned(),
                ));
            }
            // Checked just above: whole numbers in 1..=u32::MAX.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let run = (c as u32, r as u32);
            run
        }
    };
    let mut params = BTreeMap::new();
    for (key, value) in map {
        if !INSTANCE_KEYS.contains(&key.get_ref().as_str()) {
            params.insert(key.get_ref().clone(), value.clone());
        }
    }
    Ok(InstanceFile {
        span: entry.span(),
        kind,
        x: num("x", None)?,
        y: num("y", None)?,
        name: text("name")?,
        group: text("group")?,
        xscale: num("xscale", Some(1.0))?,
        yscale: num("yscale", Some(1.0))?,
        angle: num("angle", Some(0.0))?,
        run,
        step: pair("step")?.unwrap_or(DEFAULT_STEP),
        params,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_expands_row_major_in_place() {
        let text = r#"size = [800, 608]
instances = [
  { kind = "block", x = 0, y = 576, run = [3, 2], step = [32, -32] },
  { kind = "spike", x = 320, y = 544, dir = "up", depth = 5 },
]
"#;
        let r = parse_room("r", "rooms/r.toml", text).unwrap();
        let pos: Vec<_> = r.instances[0].positions().collect();
        assert_eq!(
            pos,
            [
                (0.0, 576.0),
                (32.0, 576.0),
                (64.0, 576.0),
                (0.0, 544.0),
                (32.0, 544.0),
                (64.0, 544.0)
            ]
        );
        assert_eq!(r.instances[1].params.keys().collect::<Vec<_>>(), ["depth", "dir"]);
        assert_eq!(r.instances[1].count(), 1);
    }

    #[test]
    fn views_role_speed_and_actions() {
        let text = r#"size = [800, 608]
file_speed = 30
role = "select"
no_restart = true
views_enabled = true
on_create = [
  { do = "set_global", var = "difficulty", value = 1 },
  { do = "spawn", kind = "@boss", x = 10, y = 20, params = { hp = 3 } },
  { do = "load_game" },
]

[[views]]
visible = true
view = [0, 0, 800, 608]
follow = "kid"
"#;
        let r = parse_room("r", "rooms/r.toml", text).unwrap();
        assert_eq!(r.file_speed.as_ref().map(|s| *s.get_ref()), Some(30));
        assert_eq!(r.role, Some(RoleName::Select));
        assert!(r.no_restart);
        assert_eq!(r.views.len(), 1);
        assert_eq!(r.on_create.len(), 3);
        assert!(matches!(r.on_create[2].get_ref(), ActionFile::LoadGame));
    }

    #[test]
    fn bad_rooms() {
        assert!(parse_room("r", "p", "size = [0, 10]\n").is_err());
        let d = parse_room("r", "p", "size = [8, 8]\ninstances = [ { x = 1, y = 2 } ]\n").unwrap_err();
        assert!(d.message.contains("kind"), "{}", d.message);
        let d = parse_room(
            "r",
            "p",
            "size = [8, 8]\ninstances = [ { kind = \"block\", x = 1, y = 2, run = [0, 1] } ]\n",
        )
        .unwrap_err();
        assert_eq!(d.line, 2);
        let too_many =
            "size = [8, 8]\ninstances = [ { kind = \"block\", x = 0, y = 0, run = [50001, 1] } ]\n";
        assert!(parse_room("r", "p", too_many).is_err());
        assert!(parse_room("r", "p", "size = [8, 8]\nspeed = 50\n").is_err());
    }
}
