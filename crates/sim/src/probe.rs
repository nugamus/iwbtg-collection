//! Read-only trace sampling, compiled into every build (docs/plans/sim-core.md 9.3).
//!
//! Sample point: after tick row 13, before the room change, which is where `Sim::step` returns
//! (the recorder writes its rows "after End Step, drawing and the animation update, before a
//! queued room change is applied", research `tooling/opengmk-trace/README.md`). One row per
//! alive, active instance of the traced kind, in dispatch order (`events::dispatch_order`).
//!
//! Reading is side-effect free except for [`KeyRead::Direct`], which consumes the
//! `keyboard_check_direct` latch the way the recorder's own column read did (I1). WP0 declares;
//! WP8a owns.

pub mod registry;

use crate::builtins::state::BuiltinState;
use crate::collide::Target;
use crate::def::{GameDef, NameId, VarId};
use crate::events;
use crate::input::Action;
use crate::kid::api::{self, KidField};
use crate::num::Real;
use crate::rules::TickRules;
use crate::rules::tick::FirstRule;
use crate::sim::Sim;
use crate::trace;
use crate::world::{InstId, Instance, KindState, Value, World};

/// What to sample each frame.
#[derive(Clone, PartialEq, Debug)]
pub struct Probe {
    /// The traced object: one row per alive instance.
    pub object: Target,
    /// Columns after the recorder's fixed ones. Each row [`sample`] returns holds the 18 fixed
    /// columns in `trace::FIXED_COLUMNS` order, then one value per entry here.
    pub columns: Vec<Column>,
}

/// One probe column.
#[derive(Clone, PartialEq, Debug)]
pub enum Column {
    /// One of the recorder's fixed columns (`trace::FIXED_COLUMNS`).
    Fixed(FixedColumn),
    /// A field of some instance.
    Field {
        /// Whose field.
        target: ProbeTarget,
        /// Which field.
        field: FieldRef,
    },
    /// A global.
    Global(VarId),
    /// `instance_number(...)`.
    Count(Target),
    /// `keyboard_check*(...)`.
    Key(KeyRead),
    /// A save slot field (decoded save files, plan 9.1 kind 3).
    Save(SaveField),
    /// Engine state the original keeps in a global or a built-in variable (`global.death`,
    /// `room_speed`), when the package declares no variable of that name.
    Engine(EngineVar),
}

/// Engine state a probe column can read without the package declaring a variable for it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EngineVar {
    /// `global.difficulty`.
    Difficulty,
    /// `global.death`.
    Death,
    /// `global.time`.
    Time,
    /// `global.savenum`, the save slot number.
    Savenum,
    /// `global.clear`.
    Clear,
    /// `global.frozen`.
    Frozen,
    /// `global.frozen2`.
    Frozen2,
    /// `global.reverse` (family B gravity flag).
    Reverse,
    /// `global.grav` (flipped-kid flag of the games that swap kid kinds).
    Grav,
    /// `global.paused`: the `world` built-in's pause flag; 0 when the game has none.
    Paused,
    /// `room_speed`.
    RoomSpeed,
}

impl EngineVar {
    /// The engine variable a GM global name (without `global.`) names.
    pub fn global(name: &str) -> Option<EngineVar> {
        Some(match name {
            "difficulty" => EngineVar::Difficulty,
            "death" => EngineVar::Death,
            "time" => EngineVar::Time,
            "savenum" => EngineVar::Savenum,
            "clear" => EngineVar::Clear,
            "frozen" => EngineVar::Frozen,
            "frozen2" => EngineVar::Frozen2,
            "reverse" => EngineVar::Reverse,
            "grav" => EngineVar::Grav,
            "paused" => EngineVar::Paused,
            _ => return None,
        })
    }
}

/// The recorder's fixed columns, in `trace::FIXED_COLUMNS` order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FixedColumn {
    /// `frame`.
    Frame,
    /// `room`.
    Room,
    /// `object`.
    Object,
    /// `id` (ignored by diffs).
    Id,
    /// `x`.
    X,
    /// `y`.
    Y,
    /// `xprevious`.
    Xprevious,
    /// `yprevious`.
    Yprevious,
    /// `hspeed`.
    Hspeed,
    /// `vspeed`.
    Vspeed,
    /// `speed`.
    Speed,
    /// `direction`.
    Direction,
    /// `gravity`.
    Gravity,
    /// `gravity_direction`.
    GravityDirection,
    /// `friction`.
    Friction,
    /// `image_index`.
    ImageIndex,
    /// `sprite_index`.
    SpriteIndex,
    /// `sprite_name`.
    SpriteName,
}

impl FixedColumn {
    /// The fixed column with this recorder name.
    pub fn from_name(name: &str) -> Option<FixedColumn> {
        FixedColumn::ALL.into_iter().find(|c| c.name() == name)
    }

    /// Every fixed column, in `trace::FIXED_COLUMNS` order.
    pub const ALL: [FixedColumn; 18] = [
        FixedColumn::Frame,
        FixedColumn::Room,
        FixedColumn::Object,
        FixedColumn::Id,
        FixedColumn::X,
        FixedColumn::Y,
        FixedColumn::Xprevious,
        FixedColumn::Yprevious,
        FixedColumn::Hspeed,
        FixedColumn::Vspeed,
        FixedColumn::Speed,
        FixedColumn::Direction,
        FixedColumn::Gravity,
        FixedColumn::GravityDirection,
        FixedColumn::Friction,
        FixedColumn::ImageIndex,
        FixedColumn::SpriteIndex,
        FixedColumn::SpriteName,
    ];

    /// The column's name in the recorder CSV.
    pub fn name(self) -> &'static str {
        match self {
            FixedColumn::Frame => "frame",
            FixedColumn::Room => "room",
            FixedColumn::Object => "object",
            FixedColumn::Id => "id",
            FixedColumn::X => "x",
            FixedColumn::Y => "y",
            FixedColumn::Xprevious => "xprevious",
            FixedColumn::Yprevious => "yprevious",
            FixedColumn::Hspeed => "hspeed",
            FixedColumn::Vspeed => "vspeed",
            FixedColumn::Speed => "speed",
            FixedColumn::Direction => "direction",
            FixedColumn::Gravity => "gravity",
            FixedColumn::GravityDirection => "gravity_direction",
            FixedColumn::Friction => "friction",
            FixedColumn::ImageIndex => "image_index",
            FixedColumn::SpriteIndex => "sprite_index",
            FixedColumn::SpriteName => "sprite_name",
        }
    }
}

/// Whose field a column reads.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ProbeTarget {
    /// The instance the row is for.
    Row,
    /// The kid (`World::kid`, R21).
    Kid,
    /// The first instance of a target (`World::first_of`).
    First(Target),
    /// The instance with this room-file name.
    Named(NameId),
}

/// A field, resolved from its GM name by `registry::resolve`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FieldRef {
    /// A built-in variable that is also a fixed column.
    Fixed(FixedColumn),
    /// `alarm[n]`.
    Alarm(u8),
    /// `image_speed`, `image_xscale` and the other image variables not in the fixed columns.
    Image(ImageField),
    /// A custom variable.
    Var(VarId),
    /// A kid field (`kid::api::read_field`), so the probe never depends on `KidState` layout.
    Kid(KidField),
}

/// Image variables a probe can read besides the fixed columns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImageField {
    /// `image_speed`.
    Speed,
    /// `image_xscale`.
    Xscale,
    /// `image_yscale`.
    Yscale,
    /// `image_angle`.
    Angle,
    /// `image_alpha`.
    Alpha,
    /// `visible`.
    Visible,
    /// `solid`.
    Solid,
    /// `depth`.
    Depth,
}

/// A keyboard read column.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyRead {
    /// `keyboard_check`.
    Held(Action),
    /// `keyboard_check_pressed`.
    Pressed(Action),
    /// `keyboard_check_released`.
    Released(Action),
    /// `keyboard_check_direct`: **consumes** the latch, as the recorder's read did (I1).
    Direct(Action),
}

/// A save slot field.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SaveField {
    /// A save exists.
    Exists,
    /// Saved room.
    Room,
    /// Saved x.
    X,
    /// Saved y.
    Y,
    /// Saved flip.
    Flipped,
    /// Saved facing.
    Facing,
    /// Difficulty.
    Difficulty,
    /// Deaths.
    Deaths,
    /// Time.
    Time,
    /// Boss flags.
    Bosses,
    /// Item flags.
    Items,
    /// Clear flag.
    Clear,
}

/// The probe's rows for this frame. Takes `&mut Sim` because `KeyRead::Direct` consumes the
/// latch.
///
/// One row per alive, active instance of `probe.object`, in dispatch order; each row holds the
/// 18 fixed columns, then `probe.columns` left to right. Columns are read row by row, so a
/// `Direct` column consumes the latch on the first row and later rows see only the held state,
/// as the recorder's per-instance evaluation would. A value the sim does not have (a variable
/// never set, no instance to read, a kid field this kid lacks) is an empty cell, as the recorder
/// writes a `--vars` expression that fails. Strings the definition does not carry
/// (`sprite_name`) are empty cells too.
pub fn sample(sim: &mut Sim, probe: &Probe, frame: i64) -> Vec<Vec<trace::Value>> {
    let ids = {
        let w = &sim.world;
        events::dispatch_order(w, &sim.rules.tick, |i| {
            i.alive && i.active && matches(sim.def(), i, probe.object)
        })
    };
    let mut rows = Vec::with_capacity(ids.len());
    for id in ids {
        let mut row = Vec::with_capacity(FixedColumn::ALL.len() + probe.columns.len());
        match sim.world.get(id) {
            Some(inst) => {
                row.extend(FixedColumn::ALL.map(|c| fixed_value(sim.def(), &sim.world, inst, c, frame)));
            }
            None => row.extend(FixedColumn::ALL.map(|_| trace::Value::Empty)),
        }
        for column in &probe.columns {
            let value = read_column(sim, id, column, frame);
            row.push(value);
        }
        rows.push(row);
    }
    rows
}

/// One column for the row of instance `row`.
fn read_column(sim: &mut Sim, row: InstId, column: &Column, frame: i64) -> trace::Value {
    if let Column::Key(KeyRead::Direct(a)) = column {
        return truth(sim.world.input.direct(*a));
    }
    let (w, def, rules) = (&sim.world, &*sim.def, &sim.rules.tick);
    match column {
        Column::Fixed(c) => match w.get(row) {
            Some(inst) => fixed_value(def, w, inst, *c, frame),
            None => trace::Value::Empty,
        },
        Column::Field { target, field } => match target_instance(def, w, rules, row, target) {
            Some(id) => read_field(def, w, id, field, frame),
            // `player.x` with no player reads 0 under "treat uninitialized variables as 0"
            // (kaenbyou t5-input-restart-latch-b.csv f32-f34: id, x and hspeed all 0.0).
            None if def.uninit_reads_zero => trace::Value::Real(0.0),
            None => trace::Value::Empty,
        },
        Column::Global(v) => w.globals.get(v).map_or(trace::Value::Empty, from_value),
        Column::Count(t) => count(def, w, *t),
        Column::Key(k) => match k {
            KeyRead::Held(a) => truth(w.input.held(*a)),
            // The recorder writes its rows after the frame, when the pressed and released flags
            // are already cleared: `keyboard_check_pressed` reads 0 on the press frame
            // (traces/kaenbyou/t5-input-restart-latch.md, f42).
            KeyRead::Pressed(_) | KeyRead::Released(_) => truth(false),
            KeyRead::Direct(_) => trace::Value::Empty,
        },
        Column::Save(f) => save_value(def, w, *f),
        Column::Engine(v) => engine_value(w, *v),
    }
}

/// An engine variable as a GML real.
fn engine_value(w: &World, v: EngineVar) -> trace::Value {
    let g = &w.engine;
    match v {
        EngineVar::Difficulty => trace::Value::Real(f64::from(g.run.difficulty)),
        EngineVar::Death => trace::Value::Real(f64::from(g.run.deaths)),
        EngineVar::Time => trace::Value::Real(f64::from(g.run.time)),
        EngineVar::Savenum => trace::Value::Real(f64::from(g.run.slot)),
        EngineVar::Clear => truth(g.run.clear),
        EngineVar::Frozen => truth(g.kid.frozen),
        EngineVar::Frozen2 => truth(g.kid.frozen2),
        EngineVar::Reverse => truth(g.kid.b.reverse),
        EngineVar::Grav => truth(g.kid.grav),
        EngineVar::Paused => truth(w.iter_alive().any(|i| crate::builtins::world::is_paused(w, i.id))),
        EngineVar::RoomSpeed => real(w.room_speed),
    }
}

/// True when `inst` is one of the instances `t` names. A kind names its derived variants too
/// (`save[min_difficulty_removed=2]` is a `save`), as a GM object index covers its children.
fn matches(def: &GameDef, inst: &Instance, t: Target) -> bool {
    match t {
        Target::Class(c) => inst.classes.intersects(c),
        Target::Kind(k) => inst.kind == k || is_variant(def, inst.kind, k),
        Target::Inst(id) => inst.id == id,
        Target::Any => true,
    }
}

/// Whether `kind` is a variant the loader derived from `base` (`<base>[<params>]`).
fn is_variant(def: &GameDef, kind: crate::def::KindId, base: crate::def::KindId) -> bool {
    match (def.kind(kind), def.kind(base)) {
        (Some(v), Some(b)) => v
            .name
            .strip_prefix(b.name.as_str())
            .is_some_and(|rest| rest.starts_with('[')),
        _ => false,
    }
}

/// The instance a field column reads.
fn target_instance(
    def: &GameDef,
    w: &World,
    rules: &TickRules,
    row: InstId,
    t: &ProbeTarget,
) -> Option<InstId> {
    match t {
        ProbeTarget::Row => Some(row),
        ProbeTarget::Kid => w.kid(rules),
        ProbeTarget::First(Target::Class(c)) => w.first_of(*c, rules),
        ProbeTarget::First(t) => first_of_target(def, w, rules, *t),
        ProbeTarget::Named(n) => w
            .iter_alive()
            .find(|i| i.active && i.name == Some(*n))
            .map(|i| i.id),
    }
}

/// "The first" instance of a kind, one id or anything, by [`TickRules::first_rule`] (R21), as
/// `World::first_of` picks one for a class.
pub(crate) fn first_of_target(def: &GameDef, w: &World, rules: &TickRules, t: Target) -> Option<InstId> {
    match rules.first_rule {
        FirstRule::DispatchOrder => {
            events::dispatch_order(w, rules, |i| i.alive && i.active && matches(def, i, t))
                .first()
                .copied()
        }
        FirstRule::LowestId => w
            .iter_alive()
            .filter(|i| i.active && matches(def, i, t))
            .map(|i| i.id)
            .min(),
    }
}

/// GML `instance_number`: alive, active instances `t` names.
fn count(def: &GameDef, w: &World, t: Target) -> trace::Value {
    let n = match t {
        Target::Class(c) => w.count_of(c),
        t => {
            let n = w.iter_alive().filter(|i| i.active && matches(def, i, t)).count();
            u32::try_from(n).unwrap_or(u32::MAX)
        }
    };
    trace::Value::Real(f64::from(n))
}

/// One field of instance `id`.
fn read_field(def: &GameDef, w: &World, id: InstId, field: &FieldRef, frame: i64) -> trace::Value {
    let Some(inst) = w.get(id) else {
        return trace::Value::Empty;
    };
    match field {
        FieldRef::Fixed(c) => fixed_value(def, w, inst, *c, frame),
        FieldRef::Alarm(n) => inst
            .alarms
            .get(usize::from(*n))
            .map_or(trace::Value::Empty, |a| real(*a)),
        FieldRef::Image(f) => match f {
            ImageField::Speed => real(inst.img.speed()),
            ImageField::Xscale => real(inst.img.xscale()),
            ImageField::Yscale => real(inst.img.yscale()),
            ImageField::Angle => real(inst.img.angle()),
            ImageField::Alpha => real(inst.img.alpha()),
            ImageField::Visible => truth(inst.visible),
            ImageField::Solid => truth(inst.solid),
            ImageField::Depth => real(inst.depth),
        },
        FieldRef::Var(v) => inst.vars.get(v).map_or(trace::Value::Empty, from_value),
        FieldRef::Kid(f) => match inst.state {
            KindState::Kid(_) => api::read_field(w, id, f)
                .or_else(|| api::read_field_def(def, f))
                .map_or(trace::Value::Empty, |v| from_value(&v)),
            // The save point's cooldown, the original's `saveTimer`.
            KindState::Builtin(BuiltinState::Save(ref s)) if f.name == "saveTimer" && f.index.is_none() => {
                real(s.timer)
            }
            _ => trace::Value::Empty,
        },
    }
}

/// A fixed column of `inst`, as the recorder writes it: integers for `frame`, `id` and
/// `sprite_index` (-1 for no sprite), quoted names for `room` and `object`.
fn fixed_value(def: &GameDef, w: &World, inst: &Instance, c: FixedColumn, frame: i64) -> trace::Value {
    let m = &inst.m;
    match c {
        FixedColumn::Frame => trace::Value::Int(frame),
        FixedColumn::Room => def
            .room(w.room)
            .map_or(trace::Value::Empty, |r| trace::Value::Str(r.name.clone())),
        FixedColumn::Object => def
            .kind(inst.kind)
            .map_or(trace::Value::Empty, |k| trace::Value::Str(k.name.clone())),
        FixedColumn::Id => trace::Value::Int(i64::try_from(inst.id.0).unwrap_or(i64::MAX)),
        FixedColumn::X => real(m.x()),
        FixedColumn::Y => real(m.y()),
        FixedColumn::Xprevious => real(m.xprevious()),
        FixedColumn::Yprevious => real(m.yprevious()),
        FixedColumn::Hspeed => real(m.hspeed()),
        FixedColumn::Vspeed => real(m.vspeed()),
        FixedColumn::Speed => real(m.speed()),
        FixedColumn::Direction => real(m.direction()),
        FixedColumn::Gravity => real(m.gravity()),
        FixedColumn::GravityDirection => real(m.gravity_direction()),
        FixedColumn::Friction => real(m.friction()),
        FixedColumn::ImageIndex => real(inst.img.index()),
        FixedColumn::SpriteIndex => trace::Value::Int(inst.img.sprite().map_or(-1, |s| i64::from(s.0))),
        // `GameDef` carries no sprite names; cases ignore this column.
        FixedColumn::SpriteName => trace::Value::Empty,
    }
}

/// A save slot field; every field but `Exists` is empty while no save exists.
fn save_value(def: &GameDef, w: &World, f: SaveField) -> trace::Value {
    let Some(s) = &w.save else {
        return match f {
            SaveField::Exists => truth(false),
            _ => trace::Value::Empty,
        };
    };
    match f {
        SaveField::Exists => truth(true),
        SaveField::Room => def
            .room(s.room)
            .map_or(trace::Value::Empty, |r| trace::Value::Str(r.name.clone())),
        SaveField::X => real(s.x),
        SaveField::Y => real(s.y),
        SaveField::Flipped => truth(s.flipped),
        SaveField::Facing => trace::Value::Real(f64::from(s.facing)),
        SaveField::Difficulty => trace::Value::Real(f64::from(s.difficulty)),
        SaveField::Deaths => trace::Value::Real(f64::from(s.deaths)),
        SaveField::Time => trace::Value::Real(f64::from(s.time)),
        SaveField::Bosses => trace::Value::Real(f64::from(s.bosses)),
        SaveField::Items => trace::Value::Real(f64::from(s.items)),
        SaveField::Clear => truth(s.clear),
    }
}

/// A real cell.
fn real(r: Real) -> trace::Value {
    trace::Value::Real(r.get())
}

/// A GML boolean: 1 or 0, as a real (the recorder writes `1.0` and `0.0`).
fn truth(b: bool) -> trace::Value {
    trace::Value::Real(if b { 1.0 } else { 0.0 })
}

/// A GML value as a trace cell. Instance ids are reals in GML; ours never match the original's.
fn from_value(v: &Value) -> trace::Value {
    match v {
        Value::Real(r) => real(*r),
        Value::Str(s) => trace::Value::Str(s.clone()),
        Value::Inst(id) => trace::Value::Int(i64::try_from(id.0).unwrap_or(i64::MAX)),
        Value::None => trace::Value::Empty,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::def::testing::GameDefBuilder;
    use crate::def::{Builtin, ClassMask, KindDef, RoomDef, RoomId};
    use crate::room::StartPoint;
    use crate::rules::RunnerRules;
    use crate::save::SaveSlot;
    use crate::script::ScriptRuntime;
    use crate::sim::SimOptions;
    use crate::sound::DllMode;
    use crate::trace::FIXED_COLUMNS;
    use crate::world::World;

    #[test]
    fn fixed_columns_match_the_recorder() {
        let names: Vec<&str> = FixedColumn::ALL.iter().map(|c| c.name()).collect();
        assert_eq!(names, FIXED_COLUMNS);
        for c in FixedColumn::ALL {
            assert_eq!(FixedColumn::from_name(c.name()), Some(c));
        }
        assert_eq!(FixedColumn::from_name("djump"), None);
    }

    /// A sim in one room with a world instance and two blocks, built without `Sim::new` so the
    /// test runs before WP1's bodies exist. The instances are stored directly.
    fn sim_with_blocks() -> (Sim, Vec<InstId>) {
        let mut b = GameDefBuilder::new("probe-test");
        let world_kind = b.kind(KindDef::new("world", Builtin::World));
        let mut block = KindDef::new("block", Builtin::Block);
        block.classes = ClassMask::BLOCK;
        let block = b.kind(block);
        b.room(RoomDef::new("room_1", 800, 608));
        let def = b.build().expect("def");
        let mut w = World::new(RoomId(0), 0);
        let mut ids = Vec::new();
        for (kind, x) in [(world_kind, 0.0), (block, 32.0), (block, 64.0)] {
            let id = w.alloc_id().expect("id");
            let mut inst = Instance::new(id, kind, 0, Real::new(x), Real::new(544.0));
            if kind == block {
                inst.classes = ClassMask::BLOCK;
            }
            w.insert(inst).expect("insert");
            ids.push(id);
        }
        let sim = Sim {
            def: Arc::new(def),
            rules: RunnerRules::default(),
            world: w,
            options: SimOptions {
                start: StartPoint::Room {
                    room: RoomId(0),
                    difficulty: 0,
                },
                seed: 0,
                dll_mode: DllMode::Model,
                mode: crate::stripped::PlayMode::Original,
            },
            scripts: ScriptRuntime::default(),
            rows_done: false,
        };
        (sim, ids)
    }

    #[test]
    fn fixed_values_and_save_fields_read_the_world() {
        let (mut sim, ids) = sim_with_blocks();
        let inst = sim.world.get(ids[1]).expect("block").clone();
        let row: Vec<trace::Value> = FixedColumn::ALL
            .map(|c| fixed_value(sim.def(), sim.world(), &inst, c, 7))
            .to_vec();
        assert_eq!(row[0], trace::Value::Int(7));
        assert_eq!(row[1], trace::Value::Str("room_1".into()));
        assert_eq!(row[2], trace::Value::Str("block".into()));
        assert_eq!(row[3], trace::Value::Int(100_002));
        assert_eq!(row[4], trace::Value::Real(32.0));
        assert_eq!(row[13], trace::Value::Real(270.0));
        assert_eq!(row[16], trace::Value::Int(-1));
        assert_eq!(row[17], trace::Value::Empty);
        assert_eq!(
            count(sim.def(), sim.world(), Target::Class(ClassMask::BLOCK)),
            trace::Value::Real(2.0)
        );
        assert_eq!(
            count(sim.def(), sim.world(), Target::Any),
            trace::Value::Real(3.0)
        );
        assert_eq!(
            save_value(sim.def(), sim.world(), SaveField::Exists),
            trace::Value::Real(0.0)
        );
        assert_eq!(
            save_value(sim.def(), sim.world(), SaveField::X),
            trace::Value::Empty
        );
        sim.world.save = Some(SaveSlot {
            room: RoomId(0),
            x: Real::new(96.0),
            y: Real::new(567.4),
            flipped: false,
            facing: -1,
            difficulty: 2,
            deaths: 5,
            time: 9,
            bosses: 0,
            items: 0,
            clear: false,
        });
        assert_eq!(
            save_value(sim.def(), sim.world(), SaveField::Exists),
            trace::Value::Real(1.0)
        );
        assert_eq!(
            save_value(sim.def(), sim.world(), SaveField::Y),
            trace::Value::Real(567.4)
        );
        assert_eq!(
            save_value(sim.def(), sim.world(), SaveField::Facing),
            trace::Value::Real(-1.0)
        );
        assert_eq!(
            save_value(sim.def(), sim.world(), SaveField::Room),
            trace::Value::Str("room_1".into())
        );
        let field = |f: &FieldRef| read_field(sim.def(), sim.world(), ids[2], f, 7);
        assert_eq!(field(&FieldRef::Alarm(0)), trace::Value::Real(-1.0));
        assert_eq!(
            field(&FieldRef::Image(ImageField::Visible)),
            trace::Value::Real(1.0)
        );
        assert_eq!(field(&FieldRef::Var(VarId(3))), trace::Value::Empty);
        assert_eq!(
            field(&FieldRef::Kid(KidField {
                name: "djump".into(),
                index: None
            })),
            trace::Value::Empty,
            "a kid field of a non-kid instance is empty"
        );
    }

    /// `sample` walks `events::dispatch_order`, and a `Direct` column consumes the latch (I1):
    /// a second sample in the same tick reads only the held state. Needs WP1's bodies of
    /// `dispatch_order`, `InputState::latch` and `InputState::direct`.
    #[test]
    #[ignore = "needs WP1: events::dispatch_order and InputState bodies"]
    fn sample_rows_and_direct_reads_consume_the_latch() {
        use crate::input::InputFrame;
        let (mut sim, _) = sim_with_blocks();
        let right = Action::Right.bit();
        sim.world.input.latch(InputFrame {
            held: 0,
            pressed: right,
            released: right,
        });
        let probe = Probe {
            object: Target::Class(ClassMask::BLOCK),
            columns: vec![
                Column::Key(KeyRead::Direct(Action::Right)),
                Column::Key(KeyRead::Pressed(Action::Right)),
                Column::Count(Target::Any),
            ],
        };
        let rows = sample(&mut sim, &probe, 4);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][4], trace::Value::Real(32.0));
        assert_eq!(rows[1][4], trace::Value::Real(64.0));
        assert_eq!(rows[0][18], trace::Value::Real(1.0), "the latch reads true once");
        assert_eq!(rows[1][18], trace::Value::Real(0.0), "the first row consumed it");
        assert_eq!(
            rows[1][19],
            trace::Value::Real(0.0),
            "pressed reads 0 after the frame"
        );
        assert_eq!(rows[0][20], trace::Value::Real(3.0));
        let again = sample(&mut sim, &probe, 4);
        assert_eq!(again[0][18], trace::Value::Real(0.0));
    }

    /// A field of an instance that does not exist reads 0 (uninitialized variables as 0), and the
    /// pressed flag reads 0 (the recorder writes its rows after the frame).
    #[test]
    fn missing_instance_reads_zero_and_pressed_reads_zero() {
        use crate::input::InputFrame;
        let (mut sim, ids) = sim_with_blocks();
        let col = Column::Field {
            target: ProbeTarget::Kid,
            field: FieldRef::Image(ImageField::Visible),
        };
        assert_eq!(read_column(&mut sim, ids[0], &col, 3), trace::Value::Real(0.0));
        sim.world.input.latch(InputFrame {
            held: 0,
            pressed: Action::Right.bit(),
            released: 0,
        });
        let pressed = Column::Key(KeyRead::Pressed(Action::Right));
        assert_eq!(
            read_column(&mut sim, ids[0], &pressed, 3),
            trace::Value::Real(0.0)
        );
    }

    /// A kind target covers the variants the loader derives from it (`save[min_difficulty_removed=2]`
    /// is a `save`): the original's `savePoint` probe reads the instance of its child object.
    #[test]
    fn kind_target_covers_derived_variants() {
        let mut b = GameDefBuilder::new("probe-variants");
        let plain = b.kind(KindDef::new("save", Builtin::Save));
        let variant = b.kind(KindDef::new("save[min_difficulty_removed=2]", Builtin::Save));
        let other = b.kind(KindDef::new("saver", Builtin::Save));
        b.room(RoomDef::new("room_1", 800, 608));
        let def = b.build().expect("def");
        let mut w = World::new(RoomId(0), 0);
        for kind in [variant, other] {
            let id = w.alloc_id().expect("id");
            w.insert(Instance::new(id, kind, 0, Real::ZERO, Real::ZERO))
                .expect("insert");
        }
        assert_eq!(count(&def, &w, Target::Kind(plain)), trace::Value::Real(1.0));
        assert_eq!(count(&def, &w, Target::Kind(variant)), trace::Value::Real(1.0));
        assert_eq!(count(&def, &w, Target::Kind(other)), trace::Value::Real(1.0));
    }
}
