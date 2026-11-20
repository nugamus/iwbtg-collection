//! Read-only trace sampling, compiled into every build (docs/architecture/traces.md).
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
use crate::collide::bbox::{BboxRead, bbox_read, mask_sprite};
use crate::collide::query::Q;
use crate::def::{GameDef, NameId, VarId};
use crate::events;
use crate::input::Action;
use crate::kid::api::{self, KidField};
use crate::num::Real;
use crate::rules::RunnerRules;
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
    /// A save slot field (decoded save files, docs/architecture/traces.md kind 3).
    Save(SaveField),
    /// Engine state the original keeps in a global or a built-in variable (`global.death`,
    /// `room_speed`), when the package declares no variable of that name.
    Engine(EngineVar),
    /// GML `place_free(x, y)` called by the row's instance: no solid instance meets it placed at
    /// the two evaluated coordinates (`collide::place_free`; CLONE_SPEC collision, "Place
    /// functions test a temporary position"). Read-only: the instance never moves.
    PlaceFree {
        /// The tested x.
        x: RowExpr,
        /// The tested y.
        y: RowExpr,
    },
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
    /// GML `fps` as the sampled frame saw it: 0 during the trace runner's warm-up
    /// (`TickRules::fps_zero_ticks`), then the tick rate (R28, `tick::real_time_fps`).
    Fps,
    /// `global.key_left`: the latched Left vine-key flag (`KidGlobalsExt::key_left`; showdown
    /// `global.button01`). 0 in games without `input.vine_keys = "latched_flags"`.
    KeyLeft,
    /// `global.key_right`: the latched Right flag (showdown `global.button02`).
    KeyRight,
    /// `global.key_jump`: the latched Jump flag (showdown `global.button03`).
    KeyJump,
    /// GML `random_get_seed()`: the gameplay generator's state (`World::rng`) read as a signed
    /// 32-bit integer, as the recorder prints it (`-669280590`, num-rng-a).
    RandomSeed,
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
            "key_left" => EngineVar::KeyLeft,
            "key_right" => EngineVar::KeyRight,
            "key_jump" => EngineVar::KeyJump,
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
    /// The instance at this 0-based index among the alive, active instances of a target, in
    /// the order [`TickRules::first_rule`] picks "the first" by (R21): GML
    /// `instance_find(obj, n)`. `Nth(t, 0)` is `First(t)` for a kind.
    Nth(Target, u32),
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
    /// `bbox_left`, `bbox_right`, `bbox_top`, `bbox_bottom`: what GML reads
    /// (`collide::bbox::bbox_read`) at the instance's position after the tick.
    Bbox(BboxEdge),
    /// `mask_index`, `image_number`, `sprite_width`, `sprite_height` ([`sprite_field`]).
    Sprite(SpriteField),
}

/// Read-only sprite variables of an instance (and `mask_index`, which the probe only reads).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpriteField {
    /// `mask_index`.
    MaskIndex,
    /// `image_number`.
    ImageNumber,
    /// `sprite_width`.
    Width,
    /// `sprite_height`.
    Height,
}

impl SpriteField {
    /// Every sprite field.
    pub const ALL: [SpriteField; 4] = [
        SpriteField::MaskIndex,
        SpriteField::ImageNumber,
        SpriteField::Width,
        SpriteField::Height,
    ];

    /// The GML variable name.
    pub fn name(self) -> &'static str {
        match self {
            SpriteField::MaskIndex => "mask_index",
            SpriteField::ImageNumber => "image_number",
            SpriteField::Width => "sprite_width",
            SpriteField::Height => "sprite_height",
        }
    }

    /// The field with this GML name.
    pub fn from_name(name: &str) -> Option<SpriteField> {
        SpriteField::ALL.into_iter().find(|f| f.name() == name)
    }
}

/// What a sprite field of `inst` reads, as the recorder writes it (reals); an empty cell where
/// the spec has no rule.
///
/// - `mask_index`: the sprite that owns the instance's mask (`collide::bbox::mask_sprite`), -1
///   with no mask (CLONE_SPEC collision: "-1 means same as sprite_index"). Like `sprite_index` it
///   is our sprite id, not the original's.
/// - `image_number`: the sprite's frame count, 0 for a sprite with no frames (CLONE_SPEC
///   collision, bbox-empty-mask.csv k1) and with no sprite (CLONE_SPEC sprites,
///   traces/little-trap/anim-nosprite.csv).
/// - `sprite_width`, `sprite_height`: the frame width (height) times `image_xscale`
///   (`image_yscale`), signed and exact under any scale, and independent of `image_angle`
///   (research#231, CLONE_SPEC sprites "Sprite size variables": maskNothing 2x2 at scale 3 and
///   angle 30 reads 6x6, bbox-empty-mask.csv k12; a 48-wide player at xscale -1 reads -48,
///   mask-mirror.csv frame 25). The sprite decides, not the mask. A sprite with no frames reads
///   0 at any scale (CLONE_SPEC collision, bbox-empty-mask.csv k1). With no sprite the spec has
///   no rule (`sprite_index = -1` is OPEN, trace request `NUM-SPRITE-SIZE`): an empty cell.
pub fn sprite_field(def: &GameDef, inst: &Instance, f: SpriteField) -> trace::Value {
    let count = |n: u32| trace::Value::Real(f64::from(n));
    if f == SpriteField::MaskIndex {
        return match inst.img.mask() {
            None => trace::Value::Real(-1.0),
            Some(m) => mask_sprite(def, m).map_or(trace::Value::Empty, |s| count(s.0)),
        };
    }
    let Some(s) = inst.img.sprite() else {
        return match f {
            SpriteField::ImageNumber => count(0),
            _ => trace::Value::Empty,
        };
    };
    let Some(sprite) = def.sprite(s) else {
        return trace::Value::Empty;
    };
    let size = |n: u32, scale: Real| {
        if sprite.frames == 0 {
            count(0)
        } else {
            // research#231: the signed frame size times the scale, exact at any scale; the
            // angle does not enter.
            trace::Value::Real(f64::from(n) * scale.get())
        }
    };
    match f {
        SpriteField::ImageNumber => count(u32::from(sprite.frames)),
        SpriteField::Width => size(sprite.size.0, inst.img.xscale()),
        SpriteField::Height => size(sprite.size.1, inst.img.yscale()),
        SpriteField::MaskIndex => trace::Value::Empty,
    }
}

/// What `bbox_*` reads for an instance with no sprite and no mask.
const BBOX_NO_MASK: f64 = -100_000.0;

/// One edge of the reported bounding box (GML `bbox_*`, read-only).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BboxEdge {
    /// `bbox_left`.
    Left,
    /// `bbox_right`.
    Right,
    /// `bbox_top`.
    Top,
    /// `bbox_bottom`.
    Bottom,
}

impl BboxEdge {
    /// The GML variable name.
    pub fn name(self) -> &'static str {
        match self {
            BboxEdge::Left => "bbox_left",
            BboxEdge::Right => "bbox_right",
            BboxEdge::Top => "bbox_top",
            BboxEdge::Bottom => "bbox_bottom",
        }
    }

    /// The edge with this GML name.
    pub fn from_name(name: &str) -> Option<BboxEdge> {
        [BboxEdge::Left, BboxEdge::Right, BboxEdge::Top, BboxEdge::Bottom]
            .into_iter()
            .find(|e| e.name() == name)
    }
}

/// A coordinate argument of a place test column: terms added and subtracted left to right in
/// binary64, as GML evaluates `x+hspeed` before the call.
#[derive(Clone, PartialEq, Debug)]
pub struct RowExpr {
    /// `(negate, term)` pairs; the first term may be negated too (`-1+x`).
    pub terms: Vec<(bool, Term)>,
}

/// One term of a [`RowExpr`].
#[derive(Clone, PartialEq, Debug)]
pub enum Term {
    /// A number literal.
    Num(Real),
    /// A field of the row's instance.
    Field(FieldRef),
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

/// One column read without a row, as at the end of a frame with no instance of the traced object
/// (the trace runner's `latch` series, #671). `None` for a column that needs the row's instance
/// (a fixed column, a `Row` field) or that consumes state when read (`keyboard_check_direct`).
pub fn read_unrowed(sim: &mut Sim, column: &Column, frame: i64) -> Option<trace::Value> {
    match column {
        Column::Fixed(_)
        | Column::Field {
            target: ProbeTarget::Row,
            ..
        }
        | Column::Key(KeyRead::Direct(_))
        | Column::PlaceFree { .. } => None,
        // `row` is never read for the remaining columns.
        _ => Some(read_column(sim, InstId(0), column, frame)),
    }
}

/// One column for the row of instance `row`.
fn read_column(sim: &mut Sim, row: InstId, column: &Column, frame: i64) -> trace::Value {
    if let Column::Key(KeyRead::Direct(a)) = column {
        return truth(sim.world.input.direct(*a));
    }
    let (w, def, all_rules) = (&sim.world, &*sim.def, &sim.rules);
    let rules = &all_rules.tick;
    if let Column::Engine(EngineVar::Fps) = column {
        // The sampled frame's tick: still `World::tick` between `step_rows` and `finish_step`
        // (a room-change row), one less after it.
        let ran = if sim.rows_done {
            w.tick
        } else {
            w.tick.saturating_sub(1)
        };
        return if ran < rules.fps_zero_ticks {
            trace::Value::Real(0.0)
        } else {
            trace::Value::Real(f64::from(def.tick_rate))
        };
    }
    match column {
        Column::Fixed(c) => match w.get(row) {
            Some(inst) => fixed_value(def, w, inst, *c, frame),
            None => trace::Value::Empty,
        },
        Column::Field { target, field } => match target_instance(def, w, rules, row, target) {
            Some(id) => read_field(def, w, all_rules, id, field, frame),
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
        Column::PlaceFree { x, y } => place_free_value(def, w, all_rules, row, x, y, frame),
    }
}

/// A [`Column::PlaceFree`] cell: 1 or 0, or empty when an argument is not a number or the query
/// fails. Reads the world only: `collide::query` places a copy of the mask, never the instance,
/// and draws no random numbers.
fn place_free_value(
    def: &GameDef,
    w: &World,
    rules: &RunnerRules,
    row: InstId,
    x: &RowExpr,
    y: &RowExpr,
    frame: i64,
) -> trace::Value {
    let (Some(x), Some(y)) = (
        eval_expr(def, w, rules, row, x, frame),
        eval_expr(def, w, rules, row, y, frame),
    ) else {
        return trace::Value::Empty;
    };
    if w.get(row).is_none() {
        return trace::Value::Empty;
    }
    let q = Q { def, rules, w };
    match q.place_hits(row, x, y, Target::Any, true) {
        Ok(hits) => truth(hits.is_empty()),
        Err(_) => trace::Value::Empty,
    }
}

/// A [`RowExpr`] for the row's instance, left to right in binary64; `None` when a field has no
/// real value.
fn eval_expr(
    def: &GameDef,
    w: &World,
    rules: &RunnerRules,
    row: InstId,
    e: &RowExpr,
    frame: i64,
) -> Option<Real> {
    let mut acc: Option<Real> = None;
    for (neg, term) in &e.terms {
        let v = match term {
            Term::Num(n) => *n,
            Term::Field(f) => match read_field(def, w, rules, row, f, frame) {
                trace::Value::Real(r) => Real::new(r),
                _ => return None,
            },
        };
        acc = Some(match (acc, *neg) {
            (None, false) => v,
            (None, true) => -v,
            (Some(a), false) => a + v,
            (Some(a), true) => a - v,
        });
    }
    acc
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
        // Read in `read_column`, which knows the sampled frame's tick.
        EngineVar::Fps => trace::Value::Empty,
        EngineVar::KeyLeft => truth(g.kid.ext.key_left),
        EngineVar::KeyRight => truth(g.kid.ext.key_right),
        EngineVar::KeyJump => truth(g.kid.ext.key_jump),
        EngineVar::RandomSeed => trace::Value::Real(f64::from(w.rng.seed.cast_signed())),
    }
}

/// True when `inst` is one of the instances `t` names. A kind names its derived variants too
/// (`save[min_difficulty_removed=2]` is a `save`), as a GM object index covers its children.
fn matches(def: &GameDef, inst: &Instance, t: Target) -> bool {
    match t {
        Target::Class(c) => inst.classes.intersects(c),
        Target::Kind(k) | Target::KindTree(k) => def.kind_covers(k, inst.kind),
        Target::Inst(id) => inst.id == id,
        Target::Any => true,
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
        ProbeTarget::Nth(t, n) => nth_of_target(def, w, rules, *t, *n),
    }
}

/// "The first" instance of a kind, one id or anything, by [`TickRules::first_rule`] (R21), as
/// `World::first_of` picks one for a class.
pub(crate) fn first_of_target(def: &GameDef, w: &World, rules: &TickRules, t: Target) -> Option<InstId> {
    match rules.first_rule {
        FirstRule::InstanceList => w.instance_list().into_iter().find(|id| {
            w.get(*id)
                .is_some_and(|i| i.alive && i.active && matches(def, i, t))
        }),
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

/// The `n`-th (0-based) alive, active instance `t` names, in the order
/// [`TickRules::first_rule`] picks "the first" by (R21): GML `instance_find(obj, n)`.
pub(crate) fn nth_of_target(
    def: &GameDef,
    w: &World,
    rules: &TickRules,
    t: Target,
    n: u32,
) -> Option<InstId> {
    let n = usize::try_from(n).ok()?;
    match rules.first_rule {
        FirstRule::InstanceList => w
            .instance_list()
            .into_iter()
            .filter(|id| {
                w.get(*id)
                    .is_some_and(|i| i.alive && i.active && matches(def, i, t))
            })
            .nth(n),
        FirstRule::DispatchOrder => events::dispatch_order(w, rules, |i| matches(def, i, t))
            .get(n)
            .copied(),
        FirstRule::LowestId => {
            let mut ids: Vec<InstId> = w
                .iter_alive()
                .filter(|i| i.active && matches(def, i, t))
                .map(|i| i.id)
                .collect();
            ids.sort_unstable();
            ids.get(n).copied()
        }
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
fn read_field(
    def: &GameDef,
    w: &World,
    rules: &RunnerRules,
    id: InstId,
    field: &FieldRef,
    frame: i64,
) -> trace::Value {
    let Some(inst) = w.get(id) else {
        return trace::Value::Empty;
    };
    match field {
        // `collide::bbox::bbox_read` (CLONE_SPEC collision): -100000 with no sprite and no mask,
        // the point of a 1x1 mask for a no-frame sprite, maskNothing's normalized box, another
        // inverted rectangle's raw edges. A box the spec has no rule for (a rotated inverted
        // rectangle, a fully transparent precise mask) stays an empty cell: UNKNOWN.
        FieldRef::Bbox(e) => match bbox_read(def, inst, (inst.m.x(), inst.m.y()), &rules.collide) {
            Ok(BboxRead::Box(b)) => {
                let v = match e {
                    BboxEdge::Left => b.left,
                    BboxEdge::Right => b.right,
                    BboxEdge::Top => b.top,
                    BboxEdge::Bottom => b.bottom,
                };
                #[allow(clippy::cast_precision_loss, reason = "room coordinates are far below 2^53")]
                trace::Value::Real(v as f64)
            }
            Ok(BboxRead::NoMask) => trace::Value::Real(BBOX_NO_MASK),
            Ok(BboxRead::Unknown) | Err(_) => trace::Value::Empty,
        },
        FieldRef::Fixed(c) => fixed_value(def, w, inst, *c, frame),
        FieldRef::Sprite(f) => sprite_field(def, inst, *f),
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
                .or_else(|| api::read_field_def(def, w.room, f))
                .map_or(trace::Value::Empty, |v| from_value(&v)),
            // The save point's cooldown, the original's `saveTimer`.
            KindState::Builtin(BuiltinState::Save(ref s)) if f.name == "saveTimer" && f.index.is_none() => {
                real(s.timer)
            }
            // A death sound's fade, the original's `start_vol` and `now_vol`.
            KindState::Builtin(BuiltinState::DeathSound(ref s)) if f.index.is_none() => {
                crate::builtins::death_sound::probe_field(s, &f.name).map_or(trace::Value::Empty, real)
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
        // One cell holds one value and the recorder has no array column. An empty cell would
        // make an array indistinguishable from an unset variable, so the whole array goes in
        // one string cell as its compact text form (`[1.0, [2.0], "x"]`, #1055) and `diff`
        // compares that text bit for bit.
        Value::Array(_) => trace::Value::Str(value_text(v)),
    }
}

/// The compact text form of a value inside an array cell (#1055).
///
/// Elements follow the cell rules: a real as the trace's shortest round-trip decimal
/// ([`trace::Value::decimal`]), an instance id as a decimal integer, a string quoted with `"`
/// doubled. `Value::None` (Rhai's unit, no GML value) is `undefined`. Nested arrays recurse,
/// separated by `, `. The form is deterministic: the same value always gives the same text,
/// and a changed element changes it.
fn value_text(v: &Value) -> String {
    let mut out = String::new();
    write_value_text(v, &mut out);
    out
}

fn write_value_text(v: &Value, out: &mut String) {
    match v {
        Value::Real(r) => out.push_str(&trace::Value::Real(r.get()).decimal()),
        Value::Str(s) => {
            out.push('"');
            out.push_str(&s.replace('"', "\"\""));
            out.push('"');
        }
        Value::Inst(id) => out.push_str(&i64::try_from(id.0).unwrap_or(i64::MAX).to_string()),
        Value::None => out.push_str("undefined"),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_value_text(item, out);
            }
            out.push(']');
        }
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
                map: None,
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
        let field = |f: &FieldRef| read_field(sim.def(), sim.world(), &sim.rules, ids[2], f, 7);
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

    /// An array variable records as one compact string cell, not an empty one (#1055): reals use
    /// the trace's shortest-decimal form, strings quote with `"` doubled, nested arrays recurse.
    /// The text is deterministic, so `diff` compares it bit for bit like any string cell.
    #[test]
    fn array_values_record_as_compact_text() {
        let (mut sim, ids) = sim_with_blocks();
        let nested = Value::array(vec![Value::Real(Real::new(2.0))]).expect("nested");
        let array = Value::array(vec![
            Value::Real(Real::ONE),
            nested,
            Value::Str("x".into()),
            Value::Inst(InstId(100_005)),
            Value::None,
            Value::Str("a\"b".into()),
            Value::array(Vec::new()).expect("empty"),
        ])
        .expect("array");
        sim.world
            .get_mut(ids[1])
            .expect("block")
            .vars
            .insert(VarId(0), array);
        let field = FieldRef::Var(VarId(0));
        let cell = read_field(sim.def(), sim.world(), &sim.rules, ids[1], &field, 1);
        let text = "[1.0, [2.0], \"x\", 100005, undefined, \"a\"\"b\", []]";
        assert_eq!(cell, trace::Value::Str(text.into()));
        assert!(
            !cell.same(&trace::Value::Empty),
            "an array no longer reads like an unset variable"
        );
        assert_eq!(
            value_text(&Value::Real(Real::new(1.5))),
            trace::Value::Real(1.5).decimal(),
            "an element real uses the trace's own decimal form"
        );
        assert_eq!(
            read_field(sim.def(), sim.world(), &sim.rules, ids[1], &field, 1),
            cell,
            "the same array always records the same text"
        );
        let items = sim
            .world
            .get_mut(ids[1])
            .expect("block")
            .vars
            .get_mut(&VarId(0))
            .expect("var")
            .as_array_mut()
            .expect("array");
        items[0] = Value::Real(Real::new(1.5));
        assert_ne!(
            read_field(sim.def(), sim.world(), &sim.rules, ids[1], &field, 1),
            cell,
            "a changed element changes the cell"
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

    /// An instance with no sprite and no mask reads -100000 for all four `bbox_*` at a fractional
    /// position (research#219).
    #[test]
    fn bbox_without_sprite_or_mask_reads_minus_100000() {
        let mut b = GameDefBuilder::new("probe-nobbox");
        let ctl = b.kind(KindDef::new("ctl", Builtin::World));
        b.room(RoomDef::new("room_1", 800, 608));
        let def = b.build().expect("def");
        let (mut sim, _) = sim_with_blocks();
        sim.def = Arc::new(def);
        sim.world = World::new(RoomId(0), 0);
        let me = sim.world.alloc_id().expect("id");
        let inst = Instance::new(me, ctl, 0, Real::new(10.5), Real::new(20.25));
        sim.world.insert(inst).expect("insert");
        let before = sim.world.clone();
        for e in [BboxEdge::Left, BboxEdge::Right, BboxEdge::Top, BboxEdge::Bottom] {
            let col = Column::Field {
                target: ProbeTarget::Row,
                field: FieldRef::Bbox(e),
            };
            assert_eq!(read_column(&mut sim, me, &col, 1), trace::Value::Real(-100_000.0));
        }
        assert_eq!(sim.world, before);
    }

    /// Sprite fields without a sprite: `image_number` 0 (little-trap anim-nosprite), `mask_index`
    /// -1, the sizes unknown; a mask no sprite owns reads an empty `mask_index`.
    #[test]
    fn sprite_fields_without_a_sprite() {
        use crate::def::{IRect, Mask, MaskId, MaskShape};
        let mut b = GameDefBuilder::new("probe-sprite-fields");
        let ctl = b.kind(KindDef::new("ctl", Builtin::World));
        b.room(RoomDef::new("room_1", 800, 608));
        b.def_mut().masks.push(Mask {
            bbox: IRect {
                left: 0,
                top: 0,
                right: 3,
                bottom: 3,
            },
            shape: MaskShape::Rect,
        });
        let def = b.build().expect("def");
        let mut inst = Instance::new(InstId(1), ctl, 0, Real::ZERO, Real::ZERO);
        let read = |inst: &Instance| SpriteField::ALL.map(|f| sprite_field(&def, inst, f));
        let r = |v: f64| trace::Value::Real(v);
        let e = trace::Value::Empty;
        assert_eq!(read(&inst), [r(-1.0), r(0.0), e.clone(), e.clone()]);
        inst.img.set_mask(Some(MaskId(0)));
        assert_eq!(read(&inst), [e.clone(), r(0.0), e.clone(), e]);
    }

    /// `sprite_width`/`sprite_height` are the signed frame size times the scale at any scale,
    /// independent of the angle (research#231: bbox-empty-mask.csv k12, mask-mirror.csv frame
    /// 25). A no-frame sprite reads 0 at any scale; no sprite stays an empty cell (OPEN).
    #[test]
    fn sprite_sizes_are_the_signed_frame_size_times_scale() {
        use crate::def::{MaskId, MaskSet, SpriteDef, SpriteId};
        let mut b = GameDefBuilder::new("probe-sprite-size");
        let ctl = b.kind(KindDef::new("ctl", Builtin::World));
        b.room(RoomDef::new("room_1", 800, 608));
        let small = b.sprite_rect((2, 2), (0, 0));
        let player = b.sprite_rect((48, 32), (0, 0));
        // A no-frame sprite keeps its frame size but reads 0; the size path never reads the mask.
        b.def_mut().sprites.push(SpriteDef {
            frames: 0,
            size: (5, 7),
            origin: (0, 0),
            masks: MaskSet::Shared(MaskId(0)),
        });
        let no_frames = SpriteId(2);
        let def = b.build().expect("def");
        let mut inst = Instance::new(InstId(1), ctl, 0, Real::ZERO, Real::ZERO);
        let read = |inst: &Instance, f: SpriteField| sprite_field(&def, inst, f);
        let r = |v: f64| trace::Value::Real(v);
        let e = trace::Value::Empty;

        // 2x2 at scale 3 and angle 30 reads 6x6 (bbox-empty-mask.csv k12).
        inst.img.set_sprite(Some(small));
        inst.img.set_xscale(Real::new(3.0)).expect("xscale");
        inst.img.set_yscale(Real::new(3.0)).expect("yscale");
        inst.img.set_angle(Real::new(30.0)).expect("angle");
        assert_eq!(read(&inst, SpriteField::Width), r(6.0));
        assert_eq!(read(&inst, SpriteField::Height), r(6.0));

        // 48 wide at xscale -1 reads -48 (mask-mirror.csv frame 25); height is signed too.
        inst.img.set_sprite(Some(player));
        inst.img.set_xscale(Real::new(-1.0)).expect("xscale");
        inst.img.set_yscale(Real::new(-1.0)).expect("yscale");
        assert_eq!(read(&inst, SpriteField::Width), r(-48.0));
        assert_eq!(read(&inst, SpriteField::Height), r(-32.0));

        // A fractional scale is the exact product: 2 * 1.5 = 3.
        inst.img.set_sprite(Some(small));
        inst.img.set_xscale(Real::new(1.5)).expect("xscale");
        inst.img.set_yscale(Real::new(1.5)).expect("yscale");
        assert_eq!(read(&inst, SpriteField::Width), r(3.0));
        assert_eq!(read(&inst, SpriteField::Height), r(3.0));

        // A sprite with no frames reads 0 at any scale, angle included.
        inst.img.set_sprite(Some(no_frames));
        assert_eq!(read(&inst, SpriteField::Width), r(0.0));
        assert_eq!(read(&inst, SpriteField::Height), r(0.0));

        // No sprite: OPEN (`sprite_index = -1`, NUM-SPRITE-SIZE), an empty cell.
        inst.img.set_sprite(None);
        assert_eq!(read(&inst, SpriteField::Width), e);
        assert_eq!(read(&inst, SpriteField::Height), e);
    }

    /// A no-frame sprite reads the point round(x), round(y); maskNothing its normalized 2x2 box;
    /// another inverted rectangle its raw, inverted edges (research#224, bbox-empty-mask.md).
    #[test]
    fn bbox_of_no_frame_sprite_and_empty_masks() {
        use crate::def::{IRect, Mask, MaskId, MaskSet, MaskShape, SpriteDef, SpriteId};
        let mut b = GameDefBuilder::new("probe-emptymask");
        let ctl = b.kind(KindDef::new("ctl", Builtin::World));
        b.room(RoomDef::new("room_1", 800, 608));
        let boxes = [
            (
                0,
                (1, 1),
                IRect {
                    left: 0,
                    top: 0,
                    right: -1,
                    bottom: -1,
                },
            ),
            (
                1,
                (2, 2),
                IRect {
                    left: 1,
                    top: 1,
                    right: 0,
                    bottom: 0,
                },
            ),
            (
                1,
                (32, 32),
                IRect {
                    left: 31,
                    top: 31,
                    right: 0,
                    bottom: 0,
                },
            ),
        ];
        for (i, (frames, size, bbox)) in boxes.into_iter().enumerate() {
            let d = b.def_mut();
            d.masks.push(Mask {
                bbox,
                shape: MaskShape::Empty,
            });
            d.sprites.push(SpriteDef {
                frames,
                size,
                origin: (0, 0),
                masks: MaskSet::Shared(MaskId(u32::try_from(i).expect("id"))),
            });
        }
        let def = b.build().expect("def");
        let (mut sim, _) = sim_with_blocks();
        sim.def = Arc::new(def);
        sim.world = World::new(RoomId(0), 0);
        let read = |sim: &mut Sim, sprite: u32| {
            let me = sim.world.alloc_id().expect("id");
            let mut inst = Instance::new(me, ctl, 0, Real::new(-2999.5), Real::new(-999.4));
            inst.img.set_sprite(Some(SpriteId(sprite)));
            sim.world.insert(inst).expect("insert");
            [BboxEdge::Left, BboxEdge::Top, BboxEdge::Right, BboxEdge::Bottom].map(|e| {
                let col = Column::Field {
                    target: ProbeTarget::Row,
                    field: FieldRef::Bbox(e),
                };
                read_column(sim, me, &col, 1)
            })
        };
        let r = |v: f64| trace::Value::Real(v);
        assert_eq!(read(&mut sim, 0), [r(-3000.0), r(-999.0), r(-3000.0), r(-999.0)]);
        assert_eq!(read(&mut sim, 1), [r(-3000.0), r(-999.0), r(-2998.0), r(-998.0)]);
        assert_eq!(read(&mut sim, 2), [r(-2968.0), r(-968.0), r(-3000.0), r(-999.0)]);
    }

    /// `bbox_*` read the reported box (rounded after the offset, ties to even) and `place_free`
    /// tests solids at the evaluated coordinates, without moving the instance.
    #[test]
    fn bbox_fields_and_place_free_columns() {
        let mut b = GameDefBuilder::new("probe-place");
        let cherry_sprite = b.sprite_rect((11, 11), (5, 5));
        let block_sprite = b.sprite_rect((32, 32), (0, 0));
        let cherry = b.kind(KindDef::new("cherry", Builtin::World));
        let block = b.kind(KindDef::new("block", Builtin::Block));
        b.room(RoomDef::new("room_1", 800, 608));
        let def = b.build().expect("def");
        let (mut sim, _) = sim_with_blocks();
        sim.def = Arc::new(def);
        sim.world = World::new(RoomId(0), 0);
        let me = sim.world.alloc_id().expect("id");
        let mut inst = Instance::new(me, cherry, 0, Real::new(200.5), Real::new(100.0));
        inst.img.set_sprite(Some(cherry_sprite));
        inst.m.set_hspeed(Real::new(8.0)).expect("hspeed");
        inst.m.set_vspeed(Real::new(-3.0)).expect("vspeed");
        sim.world.insert(inst).expect("insert");
        let wall = sim.world.alloc_id().expect("id");
        let mut inst = Instance::new(wall, block, 0, Real::new(212.0), Real::new(80.0));
        inst.img.set_sprite(Some(block_sprite));
        inst.solid = true;
        sim.world.insert(inst).expect("insert");
        let edge = |e| Column::Field {
            target: ProbeTarget::Row,
            field: FieldRef::Bbox(e),
        };
        // 200.5 - 5 = 195.5 rounds to 196, 205.5 to 206.
        assert_eq!(
            read_column(&mut sim, me, &edge(BboxEdge::Left), 1),
            trace::Value::Real(196.0)
        );
        assert_eq!(
            read_column(&mut sim, me, &edge(BboxEdge::Right), 1),
            trace::Value::Real(206.0)
        );
        assert_eq!(
            read_column(&mut sim, me, &edge(BboxEdge::Top), 1),
            trace::Value::Real(95.0)
        );
        assert_eq!(
            read_column(&mut sim, me, &edge(BboxEdge::Bottom), 1),
            trace::Value::Real(105.0)
        );
        let d = sim.def.clone();
        let col = |e: &str| registry::column(e, &d).expect("column");
        let before = sim.world.clone();
        assert_eq!(
            read_column(&mut sim, me, &col("place_free(x,y)"), 1),
            trace::Value::Real(1.0)
        );
        assert_eq!(
            read_column(&mut sim, me, &col("place_free(x+hspeed,y)"), 1),
            trace::Value::Real(0.0)
        );
        assert_eq!(
            read_column(&mut sim, me, &col("place_free(x,y+vspeed)"), 1),
            trace::Value::Real(1.0)
        );
        assert_eq!(
            read_column(&mut sim, me, &col("place_free(x+hspeed,y+vspeed+200)"), 1),
            trace::Value::Real(1.0)
        );
        assert_eq!(sim.world, before, "probe reads never change the world");
        assert_eq!(read_unrowed(&mut sim, &col("place_free(x,y)"), 1), None);
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
