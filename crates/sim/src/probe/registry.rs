//! GM variable names to probe fields (docs/plans/sim-core.md 9.3): `djump`, `curJumps`,
//! `jump[1]`, `alarm[0]`, `image_index`, ... WP0 declares; WP8a owns.
//!
//! [`resolve`] maps one variable name to a [`FieldRef`]. [`column`] reads a whole probe column
//! expression in our names (a trace case's `vars` values): a variable of the row's instance, of
//! the kid, of the first instance of a kind or of a named instance, a global, a count, a key read
//! or a save field. Original GameMaker names never reach the sim: `iwbtg trace import` translates
//! them through the package's trace map first.

use super::{Column, EngineVar, FieldRef, FixedColumn, ImageField, KeyRead, ProbeTarget, SaveField};
use crate::collide::Target;
use crate::def::{ClassMask, GameDef, KindId, NameId};
use crate::input::Action;
use crate::kid::api::KidField;

/// GM built-in instance variables the probe cannot read yet (no `FieldRef` for them). They never
/// resolve to a kid field or a custom variable.
const UNSUPPORTED_BUILTINS: [&str; 24] = [
    "xstart",
    "ystart",
    "bbox_left",
    "bbox_right",
    "bbox_top",
    "bbox_bottom",
    "mask_index",
    "image_number",
    "image_single",
    "image_blend",
    "sprite_width",
    "sprite_height",
    "sprite_xoffset",
    "sprite_yoffset",
    "object_index",
    "persistent",
    "timeline_index",
    "timeline_position",
    "timeline_running",
    "timeline_speed",
    "path_index",
    "path_position",
    "path_speed",
    "alarm",
];

/// The field a GM variable name refers to; `None` when unknown.
///
/// Built-in variables first: the fixed columns that are instance variables (`x` ...
/// `image_index`, `sprite_index`, `id`), `alarm[0]` to `alarm[11]`, the image variables and
/// `visible`, `solid`, `depth`. Then a custom variable the package declares
/// (`GameDef::var_names`, no index). Any other identifier, with or without one index
/// (`djump`, `jump[1]`), is a kid field read through `kid::api::read_field`, which gives no value
/// on an instance that is not a kid. Built-ins the probe cannot read yet (`xstart`, `bbox_left`,
/// `timeline_position`, ...) and anything that is not a plain name give `None`.
pub fn resolve(name: &str, def: &GameDef) -> Option<FieldRef> {
    let (base, index) = split_index(name)?;
    if let Some(field) = builtin(base, index) {
        return Some(field);
    }
    if UNSUPPORTED_BUILTINS.contains(&base) || FixedColumn::from_name(base).is_some() || is_image_name(base) {
        return None;
    }
    if index.is_none()
        && let Some(v) = def.var_names.get(base)
    {
        return Some(FieldRef::Var(*v));
    }
    Some(FieldRef::Kid(KidField {
        name: base.to_string(),
        index,
    }))
}

/// A probe column from an expression in our names. The forms:
///
/// - `x`, `alarm[0]`, `djump`, `jump[1]`: a field of the row's instance ([`resolve`]);
/// - `kid.<field>`: a field of the kid (`World::kid`, R21);
/// - `<kind>.<field>`: a field of the first instance of that kind (R21);
/// - `named(<instance name>).<field>`: a field of the instance with that room-file name;
/// - `global.<name>`: a global the package declares, else engine state (`difficulty`, `death`, `time`,
///   `savenum`, `clear`, `frozen`, `frozen2`, `reverse`, `grav`, `paused`); `room_speed` is engine state too;
/// - `instance_number(<target>)`, where a target is a kind name or `class:<class name>`;
/// - `keyboard_check(<action>)`, `keyboard_check_pressed(...)`, `keyboard_check_released(...)`,
///   `keyboard_check_direct(...)`, with an action name (`jump`, `right`);
/// - `save.<field>`: (a slot field, else the field of the first `save` kind instance) `exists`, `room`, `x`, `y`, `flipped`, `facing`, `difficulty`, `deaths`,
///   `time`, `bosses`, `items`, `clear`.
///
/// The error says what could not be read.
pub fn column(expr: &str, def: &GameDef) -> Result<Column, String> {
    let expr = expr.trim();
    if let Some(arg) = call_arg(expr, "instance_number") {
        return target(arg, def).map(Column::Count);
    }
    for (func, read) in [
        ("keyboard_check", KeyRead::Held as fn(Action) -> KeyRead),
        ("keyboard_check_pressed", KeyRead::Pressed),
        ("keyboard_check_released", KeyRead::Released),
        ("keyboard_check_direct", KeyRead::Direct),
    ] {
        if let Some(arg) = call_arg(expr, func) {
            let action =
                Action::from_name(arg.trim()).ok_or_else(|| format!("{arg:?} is not an action name"))?;
            return Ok(Column::Key(read(action)));
        }
    }
    if let Some(name) = expr.strip_prefix("global.") {
        let (base, index) = split_index(name).ok_or_else(|| format!("{expr:?} is not a global name"))?;
        if index.is_some() {
            return Err(format!("{expr:?}: global arrays are not probe columns"));
        }
        // The engine's own globals first: `var_names` also holds instance parameters (a save
        // point's `difficulty`), which are not the global of that name.
        if let Some(v) = EngineVar::global(base) {
            return Ok(Column::Engine(v));
        }
        return def
            .var_names
            .get(base)
            .map(|v| Column::Global(*v))
            .ok_or_else(|| format!("{expr:?}: the package declares no global {base:?}"));
    }
    // A save slot field; a name that is none falls through, because `save` is also the save
    // point's kind (`save.image_index` is that kind's field).
    if let Some(f) = expr.strip_prefix("save.").and_then(save_field) {
        return Ok(Column::Save(f));
    }
    if let Some(rest) = expr.strip_prefix("named(") {
        let (inst, field) = rest
            .split_once(").")
            .ok_or_else(|| format!("{expr:?}: expected named(<instance>).<field>"))?;
        let name = def
            .names
            .iter()
            .position(|n| n == inst)
            .and_then(|i| u32::try_from(i).ok())
            .ok_or_else(|| format!("{expr:?}: no instance named {inst:?}"))?;
        return field_column(ProbeTarget::Named(NameId(name)), field, def, expr);
    }
    if let Some((owner, field)) = expr.split_once('.') {
        let owner_target = if owner == "kid" {
            ProbeTarget::Kid
        } else {
            ProbeTarget::First(Target::Kind(
                kind_named(owner, def).ok_or_else(|| format!("{expr:?}: no kind {owner:?}"))?,
            ))
        };
        return field_column(owner_target, field, def, expr);
    }
    if expr == "room_speed" {
        return Ok(Column::Engine(EngineVar::RoomSpeed));
    }
    field_column(ProbeTarget::Row, expr, def, expr)
}

/// A field column, or an error naming the expression.
fn field_column(target: ProbeTarget, field: &str, def: &GameDef, expr: &str) -> Result<Column, String> {
    let field = resolve(field, def).ok_or_else(|| format!("{expr:?}: no probe field {field:?}"))?;
    Ok(Column::Field { target, field })
}

/// A kind by package name.
fn kind_named(name: &str, def: &GameDef) -> Option<KindId> {
    def.kinds
        .iter()
        .position(|k| k.name == name)
        .and_then(|i| u32::try_from(i).ok())
        .map(KindId)
}

/// A query target: a kind name or `class:<name>`.
fn target(arg: &str, def: &GameDef) -> Result<Target, String> {
    let arg = arg.trim();
    if let Some(class) = arg.strip_prefix("class:") {
        return ClassMask::from_name(class)
            .map(Target::Class)
            .ok_or_else(|| format!("no class {class:?}"));
    }
    kind_named(arg, def)
        .map(Target::Kind)
        .ok_or_else(|| format!("no kind {arg:?}"))
}

/// `func(arg)` to `arg`.
fn call_arg<'e>(expr: &'e str, func: &str) -> Option<&'e str> {
    expr.strip_prefix(func)?.strip_prefix('(')?.strip_suffix(')')
}

/// A save slot field by name.
pub fn save_field(name: &str) -> Option<SaveField> {
    Some(match name {
        "exists" => SaveField::Exists,
        "room" => SaveField::Room,
        "x" => SaveField::X,
        "y" => SaveField::Y,
        "flipped" => SaveField::Flipped,
        "facing" => SaveField::Facing,
        "difficulty" => SaveField::Difficulty,
        "deaths" => SaveField::Deaths,
        "time" => SaveField::Time,
        "bosses" => SaveField::Bosses,
        "items" => SaveField::Items,
        "clear" => SaveField::Clear,
        _ => return None,
    })
}

/// A built-in variable the probe reads.
fn builtin(base: &str, index: Option<u32>) -> Option<FieldRef> {
    if base == "alarm" {
        return index
            .and_then(|i| u8::try_from(i).ok())
            .filter(|i| *i < 12)
            .map(FieldRef::Alarm);
    }
    if index.is_some() {
        return None;
    }
    if let Some(c) = FixedColumn::from_name(base)
        && !matches!(
            c,
            FixedColumn::Frame | FixedColumn::Room | FixedColumn::Object | FixedColumn::SpriteName
        )
    {
        return Some(FieldRef::Fixed(c));
    }
    image_field(base).map(FieldRef::Image)
}

/// The image variables and `visible`, `solid`, `depth`.
fn image_field(base: &str) -> Option<ImageField> {
    Some(match base {
        "image_speed" => ImageField::Speed,
        "image_xscale" => ImageField::Xscale,
        "image_yscale" => ImageField::Yscale,
        "image_angle" => ImageField::Angle,
        "image_alpha" => ImageField::Alpha,
        "visible" => ImageField::Visible,
        "solid" => ImageField::Solid,
        "depth" => ImageField::Depth,
        _ => return None,
    })
}

fn is_image_name(base: &str) -> bool {
    image_field(base).is_some()
}

/// `name` or `name[n]` to its parts; `None` when it is neither (an expression, a call, a field
/// path).
fn split_index(name: &str) -> Option<(&str, Option<u32>)> {
    let (base, index) = match name.strip_suffix(']') {
        Some(head) => {
            let (base, n) = head.split_once('[')?;
            (base, Some(n.trim().parse::<u32>().ok()?))
        }
        None => (name, None),
    };
    let mut chars = base.chars();
    let first = chars.next()?;
    let ok =
        (first.is_ascii_alphabetic() || first == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    ok.then_some((base, index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::testing::GameDefBuilder;
    use crate::def::{Builtin, KindDef, RoomDef, VarId};

    fn def() -> GameDef {
        let mut b = GameDefBuilder::new("registry");
        b.kind(KindDef::new("kid", Builtin::Kid));
        b.kind(KindDef::new("@bullet", Builtin::KidBullet));
        b.room(RoomDef::new("room_1", 800, 608));
        b.def_mut().var_names.insert("hp".into(), VarId(0));
        b.def_mut().var_names.insert("difficulty".into(), VarId(1));
        b.def_mut().names.push("boss_door".into());
        b.build().expect("def")
    }

    fn kid(name: &str, index: Option<u32>) -> Option<FieldRef> {
        Some(FieldRef::Kid(KidField {
            name: name.into(),
            index,
        }))
    }

    #[test]
    fn resolves_builtins_vars_and_kid_fields() {
        let d = def();
        assert_eq!(resolve("x", &d), Some(FieldRef::Fixed(FixedColumn::X)));
        assert_eq!(
            resolve("image_index", &d),
            Some(FieldRef::Fixed(FixedColumn::ImageIndex))
        );
        assert_eq!(resolve("id", &d), Some(FieldRef::Fixed(FixedColumn::Id)));
        assert_eq!(resolve("alarm[0]", &d), Some(FieldRef::Alarm(0)));
        assert_eq!(resolve("alarm[11]", &d), Some(FieldRef::Alarm(11)));
        assert_eq!(resolve("alarm[12]", &d), None);
        assert_eq!(resolve("alarm", &d), None);
        assert_eq!(
            resolve("image_xscale", &d),
            Some(FieldRef::Image(ImageField::Xscale))
        );
        assert_eq!(resolve("visible", &d), Some(FieldRef::Image(ImageField::Visible)));
        assert_eq!(resolve("hp", &d), Some(FieldRef::Var(VarId(0))));
        assert_eq!(resolve("djump", &d), kid("djump", None));
        assert_eq!(resolve("curJumps", &d), kid("curJumps", None));
        assert_eq!(resolve("jump[1]", &d), kid("jump", Some(1)));
        for bad in [
            "bbox_left",
            "xstart",
            "room",
            "frame",
            "x[1]",
            "image_alpha[0]",
            "floor(image_index)",
            "a.b",
            "",
            "1x",
            "jump[x]",
        ] {
            assert_eq!(resolve(bad, &d), None, "{bad:?}");
        }
    }

    #[test]
    fn columns_in_our_names() {
        let d = def();
        let field = |target, field| Ok(Column::Field { target, field });
        assert_eq!(
            column("x", &d),
            field(ProbeTarget::Row, FieldRef::Fixed(FixedColumn::X))
        );
        assert_eq!(
            column("kid.x", &d),
            field(ProbeTarget::Kid, FieldRef::Fixed(FixedColumn::X))
        );
        assert_eq!(
            column("@bullet.alarm[0]", &d),
            field(ProbeTarget::First(Target::Kind(KindId(1))), FieldRef::Alarm(0))
        );
        assert_eq!(
            column("named(boss_door).visible", &d),
            field(
                ProbeTarget::Named(NameId(0)),
                FieldRef::Image(ImageField::Visible)
            )
        );
        // The engine's globals come first, even when the package interned a variable of that name
        // (a save point's `difficulty` parameter does).
        assert_eq!(column("global.difficulty", &d), Ok(Column::Engine(EngineVar::Difficulty)));
        assert_eq!(column("global.death", &d), Ok(Column::Engine(EngineVar::Death)));
        assert_eq!(column("global.paused", &d), Ok(Column::Engine(EngineVar::Paused)));
        assert_eq!(column("room_speed", &d), Ok(Column::Engine(EngineVar::RoomSpeed)));
        assert_eq!(column("global.hp", &d), Ok(Column::Global(VarId(0))));
        assert_eq!(
            column("instance_number(@bullet)", &d),
            Ok(Column::Count(Target::Kind(KindId(1))))
        );
        assert_eq!(
            column("instance_number(class:killer)", &d),
            Ok(Column::Count(Target::Class(ClassMask::KILLER)))
        );
        assert_eq!(
            column("keyboard_check(jump)", &d),
            Ok(Column::Key(KeyRead::Held(Action::Jump)))
        );
        assert_eq!(
            column("keyboard_check_direct(right)", &d),
            Ok(Column::Key(KeyRead::Direct(Action::Right)))
        );
        assert_eq!(
            column("keyboard_check_pressed(shoot)", &d),
            Ok(Column::Key(KeyRead::Pressed(Action::Shoot)))
        );
        assert_eq!(column("save.deaths", &d), Ok(Column::Save(SaveField::Deaths)));
        for bad in [
            "global.audio_music_volume",
            "global.a[1]",
            "player.x",
            "kid.bbox_left",
            "keyboard_check(vk_shift)",
            "instance_number(player)",
            "instance_number(class:nope)",
            "save.hp",
            "named(nobody).x",
            "named(boss_door)x",
            "floor(image_index)",
        ] {
            assert!(column(bad, &d).is_err(), "{bad:?}");
        }
    }
}
