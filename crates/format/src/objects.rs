//! Object files: base, params, sprite, mask, solid, visible, depth, persistent, classes
//! (docs/plans/sim-core.md 8.2 step 3, 8.3; docs/format/v0.md, Objects).
//!
//! `objects/<id>.toml` defines `@<id>`: a preset (`base` plus params) or a scripted object
//! (`script`, with an optional `base`). Keys other than the reserved ones below are params of the
//! base built-in, checked against `builtins::param_specs` by `params::check` (WP7b).
//!
//! Collision classes stand in for GM parents (plan 1.1): each built-in has default classes
//! ([`default_classes`]), some params pick a class ([`param_classes`]), and `counts_as` adds
//! more. The defaults are Design, read off the class names and the plan 6.1 table; a built-in
//! owner who needs another default changes it here.

use std::collections::BTreeMap;
use std::ops::Range;

use iwbtg_sim::def::{Builtin, ClassMask, ParamValue, Params};
use iwbtg_sim::num::Real;
use toml::Spanned;

use crate::diag::Diagnostic;
use crate::manifest::toml_error;
use crate::number::Number;

/// Keys of an object file that are not params.
pub const RESERVED_KEYS: [&str; 12] = [
    "base",
    "script",
    "sprite",
    "mask",
    "solid",
    "visible",
    "depth",
    "persistent",
    "counts_as",
    "collides_with",
    "roles",
    "params",
];

/// One parsed object file.
#[derive(Clone, Debug, Default)]
pub struct ObjectFile {
    /// Object id (file stem); the kind is `@<id>`.
    pub id: String,
    /// Built-in base.
    pub base: Option<Spanned<String>>,
    /// Script id (`scripts/<id>.rhai`).
    pub script: Option<Spanned<String>>,
    /// Default sprite.
    pub sprite: Option<Spanned<String>>,
    /// Object-level collision mask sprite (GM object `mask`).
    pub mask: Option<Spanned<String>>,
    /// GM `solid`; default false.
    pub solid: Option<bool>,
    /// GM `visible`; default true.
    pub visible: Option<bool>,
    /// GM `depth`; default 0.
    pub depth: Option<Spanned<f64>>,
    /// GM `persistent`; default false.
    pub persistent: Option<bool>,
    /// Extra collision classes (plan 1.1): named classes or package class names.
    pub counts_as: Vec<Spanned<String>>,
    /// Classes whose touch runs the script's `collide(other)`: named classes or package class names.
    pub collides_with: Vec<Spanned<String>>,
    /// Legacy converter key (`roles = ["block"]`), read only in `LoadMode::Trace` (CR10).
    pub roles: Vec<Spanned<String>>,
    /// Params, in key order.
    pub params: BTreeMap<String, Spanned<toml::Value>>,
}

/// Parses `objects/<id>.toml`. `path` and `text` are for diagnostics.
pub fn parse_object(id: &str, path: &str, text: &str) -> Result<ObjectFile, Diagnostic> {
    let map: BTreeMap<Spanned<String>, Spanned<toml::Value>> =
        toml::from_str(text).map_err(|e| toml_error(path, text, &e))?;
    let err = |span: Range<usize>, msg: String| Diagnostic::error_at(path, text, span.start, msg);
    let string = |key: &str, v: &Spanned<toml::Value>| match v.get_ref() {
        toml::Value::String(s) => Ok(Spanned::new(v.span(), s.clone())),
        other => Err(err(
            v.span(),
            format!("`{key}` must be a string, not a {}", other.type_str()),
        )),
    };
    let boolean = |key: &str, v: &Spanned<toml::Value>| match v.get_ref() {
        toml::Value::Boolean(b) => Ok(*b),
        other => Err(err(
            v.span(),
            format!("`{key}` must be true or false, not a {}", other.type_str()),
        )),
    };
    let strings = |key: &str, v: &Spanned<toml::Value>| match v.get_ref() {
        toml::Value::Array(items) => items
            .iter()
            .map(|i| match i {
                toml::Value::String(s) => Ok(Spanned::new(v.span(), s.clone())),
                _ => Err(err(v.span(), format!("`{key}` must be a list of strings"))),
            })
            .collect::<Result<Vec<_>, _>>(),
        _ => Err(err(v.span(), format!("`{key}` must be a list of strings"))),
    };
    let mut out = ObjectFile {
        id: id.to_owned(),
        ..ObjectFile::default()
    };
    for (key, value) in &map {
        match key.get_ref().as_str() {
            "base" => out.base = Some(string("base", value)?),
            "script" => out.script = Some(string("script", value)?),
            "sprite" => out.sprite = Some(string("sprite", value)?),
            "mask" => out.mask = Some(string("mask", value)?),
            "solid" => out.solid = Some(boolean("solid", value)?),
            "visible" => out.visible = Some(boolean("visible", value)?),
            "persistent" => out.persistent = Some(boolean("persistent", value)?),
            "depth" => {
                let d = number(value.get_ref()).map_err(|e| err(value.span(), format!("depth: {e}")))?;
                out.depth = Some(Spanned::new(value.span(), d));
            }
            "counts_as" => out.counts_as = strings("counts_as", value)?,
            "collides_with" => out.collides_with = strings("collides_with", value)?,
            "roles" => out.roles = strings("roles", value)?,
            "params" => match value.get_ref() {
                toml::Value::Table(t) => {
                    for (k, v) in t {
                        if RESERVED_KEYS.contains(&k.as_str()) || out.params.contains_key(k) {
                            return Err(err(
                                value.span(),
                                format!("param `{k}` is set twice or is a reserved key"),
                            ));
                        }
                        out.params
                            .insert(k.clone(), Spanned::new(value.span(), v.clone()));
                    }
                }
                _ => return Err(err(value.span(), "`params` must be a table".to_owned())),
            },
            other => {
                if out.params.contains_key(other) {
                    return Err(err(key.span(), format!("param `{other}` is set twice")));
                }
                out.params.insert(other.to_owned(), value.clone());
            }
        }
    }
    Ok(out)
}

/// A number from a data value: an integer, a float or `{ bits = "..." }`. Non-finite values are
/// refused (plan 10.2).
pub fn number(v: &toml::Value) -> Result<f64, String> {
    let n = match v {
        toml::Value::Integer(i) => {
            let f = *i as f64;
            if f as i128 != i128::from(*i) {
                return Err(format!(
                    "integer {i} cannot be stored exactly as a double; write it with {{ bits = ... }}"
                ));
            }
            f
        }
        toml::Value::Float(f) => *f,
        toml::Value::Table(t) if t.len() == 1 => match t.get("bits") {
            Some(toml::Value::String(hex)) => Number::from_bits_hex(hex)?.0,
            _ => return Err("expected a number or { bits = \"<16 hex digits>\" }".to_owned()),
        },
        other => return Err(format!("expected a number, not a {}", other.type_str())),
    };
    if n.is_finite() {
        Ok(n)
    } else {
        Err("numbers must be finite".to_owned())
    }
}

/// A param value by its TOML type: numbers are reals, booleans flags, strings text. Reference
/// params (rooms, kinds, sounds) are resolved later from the built-in's `ParamSpec`.
pub fn param_value(v: &toml::Value) -> Result<ParamValue, String> {
    match v {
        toml::Value::Boolean(b) => Ok(ParamValue::Bool(*b)),
        toml::Value::String(s) => Ok(ParamValue::Str(s.clone())),
        toml::Value::Integer(_) | toml::Value::Float(_) | toml::Value::Table(_) => {
            number(v).map(|n| ParamValue::Real(Real::new(n)))
        }
        other => Err(format!(
            "param values are numbers, booleans or strings, not a {}",
            other.type_str()
        )),
    }
}

/// Default collision classes of a built-in (Design; plan 1.1 and 6.1).
pub fn default_classes(b: Builtin) -> ClassMask {
    match b {
        Builtin::Kid => ClassMask::KID,
        Builtin::KidBullet => ClassMask::KID_BULLET,
        Builtin::Blood => ClassMask::BLOOD,
        Builtin::Bow => ClassMask::BOW,
        Builtin::Block | Builtin::BreakableBlock => ClassMask::BLOCK,
        Builtin::Spike | Builtin::Killer => ClassMask::KILLER,
        Builtin::Save => ClassMask::SAVE,
        // GM warps and warpStart are roomChanger children.
        Builtin::Warp | Builtin::DifficultyWarp => ClassMask::ROOM_CHANGER,
        // The platform's parent is block in 17 games (CLONE_SPEC collision); the other two
        // games give their platform preset `counts_as = ["solidblock"]`.
        Builtin::Platform | Builtin::MovingPlatform | Builtin::DropPlatform | Builtin::PathPlatform => {
            ClassMask::PLATFORM | ClassMask::BLOCK
        }
        Builtin::Water => ClassMask::WATER1,
        Builtin::FakeBlock => ClassMask::FAKE_BLOCK,
        Builtin::Trigger => ClassMask::TRIGGER,
        Builtin::EdgeWarp => ClassMask::EDGE_WARP,
        Builtin::BulletBlocker => ClassMask::BULLET_BLOCKER,
        Builtin::Slope => ClassMask::SLOPE,
        Builtin::Conveyor => ClassMask::CONVEYOR,
        Builtin::KidParamZone => ClassMask::PARAM_ZONE,
        Builtin::TrapStopper => ClassMask::TRAP_STOPPER,
        Builtin::Ladder => ClassMask::LADDER,
        _ => ClassMask::NONE,
    }
}

/// Whether a built-in's kind is persistent by default: the kid is in all 20 games (CLONE_SPEC
/// rooms: "The player object is persistent in all 20 games"), so a restart that creates the kid
/// and then changes room keeps it and its id. An object file's `persistent` says otherwise.
pub fn default_persistent(b: Builtin) -> bool {
    matches!(b, Builtin::Kid)
}

/// Whether a built-in's kind is solid by default (GM `solid`, which `place_free` reads): the
/// block family and slopes are (mechanics `block`, `hidden_block`, `boss_gate`, `slope`); an
/// object file or the `solid` param says otherwise.
pub fn default_solid(b: Builtin) -> bool {
    matches!(
        b,
        Builtin::Block | Builtin::BreakableBlock | Builtin::HiddenBlock | Builtin::BossGate | Builtin::Slope
    )
}

/// Classes that a kind's params pick (Design): water `variant` 2 and 3, vine `side`, slope
/// `shape` and `scale_x`, a killer with `kill_outside_view` (no class: it kills in its own collision), and a
/// block with `kid_collides = false`, which has no class at all so the kid has no collision
/// event with it (docs/format/builtins/block.md).
pub fn param_classes(b: Builtin, params: &Params) -> Option<ClassMask> {
    let get = |k: &str| params.0.get(k);
    match b {
        Builtin::Block if matches!(get("kid_collides"), Some(ParamValue::Bool(false))) => {
            Some(ClassMask::NONE)
        }
        Builtin::Water => match get("variant") {
            Some(ParamValue::Real(v)) if v.to_bits() == 2.0f64.to_bits() => Some(ClassMask::WATER2),
            Some(ParamValue::Real(v)) if v.to_bits() == 3.0f64.to_bits() => Some(ClassMask::WATER3),
            _ => None,
        },
        Builtin::Killer if matches!(get("kill_outside_view"), Some(ParamValue::Bool(true))) => {
            Some(ClassMask::NONE)
        }
        // `shape` names which way the sprite's mask rises and a negative `scale_x` mirrors that
        // (docs/format/builtins/slope.md): rising left is the `slope_mirror` class, the kid's
        // yaneM or nanameblockR event (kid/A4.md, kid/A1.md RBF). The mask stays the sprite's.
        Builtin::Slope => {
            let left = matches!(get("shape"), Some(ParamValue::Str(s)) if s == "rise_left");
            let mirrored = matches!(get("scale_x"), Some(ParamValue::Real(v)) if v.get() < 0.0);
            Some(if left != mirrored {
                ClassMask::SLOPE_MIRROR
            } else {
                ClassMask::SLOPE
            })
        }
        Builtin::Vine => match get("side") {
            Some(ParamValue::Str(s)) if s == "left" => Some(ClassMask::VINE_L),
            Some(ParamValue::Str(s)) if s == "right" => Some(ClassMask::VINE_R),
            _ => None,
        },
        _ => None,
    }
}

/// A kind's classes: the param-picked class or the built-in default, plus `extra`.
pub fn kind_classes(b: Builtin, params: &Params, extra: ClassMask) -> ClassMask {
    param_classes(b, params).unwrap_or_else(|| default_classes(b)) | extra
}

/// The class a legacy converter role names (`roles = ["killer"]`, CR10).
pub fn legacy_role_classes(role: &str) -> Option<ClassMask> {
    match role {
        "block" => Some(ClassMask::BLOCK),
        "killer" => Some(ClassMask::KILLER),
        "platform" => Some(ClassMask::PLATFORM | ClassMask::BLOCK),
        "warp" => Some(ClassMask::ROOM_CHANGER),
        _ => ClassMask::from_name(role),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_keys_and_params() {
        let text = "base = \"spike\"\ndir = \"up\"\nsprite = \"spr_5\"\ndepth = 10\nparams = { size = 2 }\n";
        let o = parse_object("spike_up", "objects/spike_up.toml", text).unwrap();
        assert_eq!(o.base.as_ref().map(|b| b.get_ref().as_str()), Some("spike"));
        assert_eq!(o.depth.as_ref().map(|d| *d.get_ref()), Some(10.0));
        assert_eq!(o.params.keys().collect::<Vec<_>>(), ["dir", "size"]);
    }

    #[test]
    fn type_errors_point_at_the_value() {
        let d = parse_object("a", "objects/a.toml", "solid = \"no\"\n").unwrap_err();
        assert_eq!((d.line, d.column), (1, 9));
        assert!(parse_object("a", "objects/a.toml", "dir = 1\nparams = { dir = 2 }\n").is_err());
    }

    #[test]
    fn param_values_and_numbers() {
        assert_eq!(
            param_value(&toml::Value::Integer(2)),
            Ok(ParamValue::Real(Real::new(2.0)))
        );
        assert_eq!(
            param_value(&toml::Value::Boolean(true)),
            Ok(ParamValue::Bool(true))
        );
        assert!(param_value(&toml::Value::Array(vec![])).is_err());
        assert!(number(&toml::Value::Float(f64::INFINITY)).is_err());
        let mut bits = toml::map::Map::new();
        bits.insert("bits".into(), toml::Value::String("3FDCCCCCCCCCCCCD".into()));
        assert_eq!(
            number(&toml::Value::Table(bits)).unwrap().to_bits(),
            0x3FDC_CCCC_CCCC_CCCD
        );
    }

    #[test]
    fn slope_shape_and_scale_x_pick_the_class() {
        let class = |shape: Option<&str>, scale: Option<f64>| {
            let mut p = Params::default();
            if let Some(s) = shape {
                p.0.insert("shape".into(), ParamValue::Str(s.into()));
            }
            if let Some(v) = scale {
                p.0.insert("scale_x".into(), ParamValue::Real(Real::new(v)));
            }
            kind_classes(Builtin::Slope, &p, ClassMask::NONE)
        };
        assert_eq!(class(None, None), ClassMask::SLOPE);
        assert_eq!(class(Some("rise_right"), None), ClassMask::SLOPE);
        assert_eq!(class(Some("precise_sprite"), None), ClassMask::SLOPE);
        assert_eq!(class(Some("rise_left"), None), ClassMask::SLOPE_MIRROR);
        // eat-the-new-year yaneM: `scale_x = -1` (CR10).
        assert_eq!(class(None, Some(-1.0)), ClassMask::SLOPE_MIRROR);
        assert_eq!(class(None, Some(2.0)), ClassMask::SLOPE);
        assert_eq!(class(Some("rise_left"), Some(-1.0)), ClassMask::SLOPE);
        assert_eq!(class(Some("precise_sprite"), Some(-2.0)), ClassMask::SLOPE_MIRROR);
    }

    #[test]
    fn classes() {
        let mut p = Params::default();
        assert_eq!(
            kind_classes(Builtin::Water, &p, ClassMask::NONE),
            ClassMask::WATER1
        );
        p.0.insert("variant".into(), ParamValue::Real(Real::new(2.0)));
        assert_eq!(
            kind_classes(Builtin::Water, &p, ClassMask::NONE),
            ClassMask::WATER2
        );
        assert_eq!(
            kind_classes(Builtin::Platform, &Params::default(), ClassMask::SOLIDBLOCK),
            ClassMask::PLATFORM | ClassMask::BLOCK | ClassMask::SOLIDBLOCK
        );
        let mut b = Params::default();
        assert_eq!(
            kind_classes(Builtin::Block, &b, ClassMask::NONE),
            ClassMask::BLOCK
        );
        b.0.insert("kid_collides".into(), ParamValue::Bool(false));
        assert_eq!(kind_classes(Builtin::Block, &b, ClassMask::NONE), ClassMask::NONE);
        assert_eq!(
            kind_classes(Builtin::Block, &b, ClassMask::SOLIDBLOCK),
            ClassMask::SOLIDBLOCK,
            "counts_as still adds classes"
        );
        assert_eq!(legacy_role_classes("killer"), Some(ClassMask::KILLER));
        assert_eq!(legacy_role_classes("nope"), None);
    }

    /// The kid is persistent by default (all 20 games), no other built-in is.
    #[test]
    fn only_the_kid_is_persistent_by_default() {
        assert!(default_persistent(Builtin::Kid));
        assert!(!default_persistent(Builtin::Block));
        assert!(!default_persistent(Builtin::Save));
    }

    /// A killer with `kill_outside_view` (kaenbyou borderKiller) is not in the `killer` class.
    #[test]
    fn kill_outside_view_drops_the_killer_class() {
        let mut p = Params::default();
        assert_eq!(
            kind_classes(Builtin::Killer, &p, ClassMask::NONE),
            ClassMask::KILLER
        );
        p.0.insert("kill_outside_view".into(), ParamValue::Bool(true));
        assert_eq!(
            kind_classes(Builtin::Killer, &p, ClassMask::NONE),
            ClassMask::NONE
        );
    }
}
