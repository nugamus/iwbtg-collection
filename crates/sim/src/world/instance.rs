//! Instances (docs/architecture/sim/world.md).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::anim::Image;
use crate::def::{Builtin, ClassMask, GroupId, KindDef, KindId, NameId, VarId};
use crate::error::SimError;
use crate::motion::Motion;
use crate::num::Real;
use crate::path::PathRun;
use crate::timeline::TimelineRun;
use crate::{builtins, events, kid, script};

/// Most elements a GML array holds (decision 0001 limits). The script host sets the same cap with
/// `set_max_array_size(10_000)` (`script/host.rs`), so a script array and a stored array agree.
pub const MAX_ARRAY_SIZE: usize = 10_000;

/// An instance id: a counter starting at [`InstId::FIRST`], never reused (manual: ids >= 100000).
/// Our ids never match the original's; trace diffs ignore the `id` column.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct InstId(pub u64);

impl InstId {
    /// The first id the sim hands out.
    pub const FIRST: InstId = InstId(100_001);
}

/// A GML value: a custom variable, a global, a param read by scripts.
///
/// Assignment copies the value (the GML 8 rule): an array belongs to its variable, so writing one
/// variable never changes another. `Clone` already copies the vector; [`Value::assigned`] names
/// the rule at the callers that assign one stored value to another.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Value {
    /// A real.
    Real(Real),
    /// A string.
    Str(String),
    /// An instance reference.
    Inst(InstId),
    /// No value (GML `noone`-like absence; reads of unset variables never give this, see
    /// `GameDef::uninit_reads_zero`).
    #[default]
    None,
    /// A dense GML array; nested arrays are allowed. At most [`MAX_ARRAY_SIZE`] elements: build
    /// and grow one through [`Value::array`] and [`Value::array_push`], which check the cap.
    ///
    /// The variant is last so the postcard encoding of the other four does not change.
    Array(Vec<Value>),
}

impl Value {
    /// An array value holding `items`; `Err` past [`MAX_ARRAY_SIZE`].
    pub fn array(items: Vec<Value>) -> Result<Value, SimError> {
        if items.len() > MAX_ARRAY_SIZE {
            return Err(array_cap_error());
        }
        Ok(Value::Array(items))
    }

    /// Appends `item` to this array; `Err` past [`MAX_ARRAY_SIZE`] or when `self` is not an array.
    pub fn array_push(&mut self, item: Value) -> Result<(), SimError> {
        let Value::Array(items) = self else {
            return Err(SimError::invariant("array_push on a value that is not an array"));
        };
        if items.len() >= MAX_ARRAY_SIZE {
            return Err(array_cap_error());
        }
        items.push(item);
        Ok(())
    }

    /// The elements, when this is an array.
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    /// The elements for changing them, when this is an array.
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    /// The value a variable gets when another is assigned to it: the GML 8 rule, an array is
    /// copied, never shared. Equal to `clone`, and named here so assignment sites say why.
    pub fn assigned(&self) -> Value {
        self.clone()
    }
}

/// The bare assignment `name = value` to the variable `id` of `vars` (a global or an instance
/// variable), the GM8 rule (research `creation-code.md`, Global arrays, Confirmed by recorder,
/// #1133; contract `script/global-array-bare-write`): the bare name is element 0, so when the
/// variable already holds an array the write goes to element 0 and the other elements keep their
/// values. Otherwise, or when `value` is itself an array, the variable takes `value`.
pub fn assign_bare(vars: &mut BTreeMap<VarId, Value>, id: VarId, value: Value) {
    if !matches!(value, Value::Array(_))
        && let Some(Value::Array(items)) = vars.get_mut(&id)
    {
        match items.first_mut() {
            Some(first) => *first = value,
            None => items.push(value),
        }
        return;
    }
    vars.insert(id, value);
}

/// The one error for an array past its cap, so the message and its wording stay in one place.
pub(crate) fn array_cap_error() -> SimError {
    SimError::unsupported(format!(
        "array cap exceeded: an array holds at most {MAX_ARRAY_SIZE} elements"
    ))
}

/// What a kind's instances do across a room change (decision 0005; E2 of #1153).
///
/// This is per instance/kind, and it is not the room's [`RoomDef::persistent`](crate::def::RoomDef::persistent)
/// flag: a persistent **room** keeps its own placed instances while the player is away and
/// restores them on return, while a `Run` instance follows the kid into any room. A `Save`
/// instance is a checkpoint writer; it is rebuilt with its room, not carried.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Persistence {
    /// Rebuilt by the room on entry; not carried. The default.
    #[default]
    Room,
    /// GM `persistent`: carried into the new room (`Instance::persistent`).
    Run,
    /// The `save` built-in: writes the checkpoint; not carried.
    Save,
}

impl Persistence {
    /// The scope a kind's instances start with. The `save` built-in wins over a GM persistent
    /// flag (a save object writes the checkpoint; it is not a carried instance).
    pub fn of(kind: &KindDef) -> Persistence {
        Persistence::from_flags(kind.base, kind.persistent)
    }

    /// The scope from a kind's base and the instance's effective persistent flag. Used by
    /// `change_kind`, where the kind may keep the old instance's persistent flag.
    pub(crate) fn from_flags(base: Builtin, persistent: bool) -> Persistence {
        if base == Builtin::Save {
            Persistence::Save
        } else if persistent {
            Persistence::Run
        } else {
            Persistence::Room
        }
    }
}

/// One instance.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Instance {
    /// Its id.
    pub id: InstId,
    /// Built-in kind or package object (preset or scripted).
    pub kind: KindId,
    /// Dispatch rank from `object_order`, fixed at creation.
    pub rank: u16,
    /// Instance name from the room file (`PlacedInstance::name`): triggers find instances by
    /// it, scripts read it, the probe resolves `ProbeTarget::Named` through it.
    pub name: Option<NameId>,
    /// A room-file `save` object's stable checkpoint key (#1153 E1): `name:<name>` when the
    /// instance is named, else `at:<room>/<kind>@<xbits>,<ybits>` from its exact spawn position.
    /// Not serialized or hashed; it is provenance for [`Checkpoint::point`](crate::save::Checkpoint).
    #[serde(skip)]
    pub point: Option<String>,
    /// Instance group from the room file (`PlacedInstance::group`).
    pub group: Option<GroupId>,
    /// GM parent emulation: what collision targets and queries see.
    pub classes: ClassMask,
    /// False after destroy; swept at the end of the tick.
    pub alive: bool,
    /// False while deactivated (xia pause, machinary).
    pub active: bool,
    /// GM `persistent`: the carry flag the room change reads.
    pub persistent: bool,
    /// The kind's persistence scope (E2 of #1153). Not serialized or hashed; `persistent` stays
    /// the behavior the carry reads. Derived at spawn and on `change_kind`.
    #[serde(skip)]
    pub persistence: Persistence,
    /// GM `solid`, per instance; may change at run time (Confirmed).
    pub solid: bool,
    /// GM `visible`.
    pub visible: bool,
    /// GM `depth`, which is also the draw-order key (E3 of #1153): larger is farther back, and
    /// equal depths tie-break by creation order (`events::draw_order`). The value comes from
    /// declared data (the kind's default, a room-file instance override, or a `set depth`
    /// script); the instance's numeric `id` is a runtime handle and tiebreak, never a layer.
    pub depth: Real,
    /// Position, speeds, gravity, friction.
    pub m: Motion,
    /// Sprite, mask and image variables.
    pub img: Image,
    /// `alarm[0..12]`; a value `<= 0` is idle. Default -1 (Inferred).
    pub alarms: [Real; 12],
    /// The running timeline, if any.
    pub timeline: Option<TimelineRun>,
    /// The path being followed, if any.
    pub path: Option<PathRun>,
    /// Per-owner state (docs/architecture/sim/world.md).
    pub state: KindState,
    /// Custom variables (scripts, creation-code params).
    pub vars: BTreeMap<VarId, Value>,
}

impl Instance {
    /// A fresh alive, active, visible instance at `(x, y)` with GM defaults: no name or group,
    /// not solid, not persistent, depth 0, no classes, default image, alarms idle (-1), no state, no variables.
    /// Spawning (`Ctx::spawn`) then applies the kind's defaults and runs Create.
    pub fn new(id: InstId, kind: KindId, rank: u16, x: Real, y: Real) -> Instance {
        Instance {
            id,
            kind,
            rank,
            name: None,
            point: None,
            group: None,
            classes: ClassMask::NONE,
            alive: true,
            active: true,
            persistent: false,
            persistence: Persistence::Room,
            solid: false,
            visible: true,
            depth: Real::ZERO,
            m: Motion::new_at(x, y),
            img: Image::default(),
            alarms: [Real::new(-1.0); 12],
            timeline: None,
            path: None,
            state: KindState::None,
            vars: BTreeMap::new(),
        }
    }
}

/// Per-owner instance state (docs/architecture/sim/world.md). Each part's type lives in its owner's module and only
/// its owner adds fields.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum KindState {
    /// No state (blocks, spikes, decor).
    #[default]
    None,
    /// The kid (`kid::KidState`).
    Kid(Box<kid::KidState>),
    /// A built-in's state (`builtins::BuiltinState`).
    Builtin(builtins::BuiltinState),
    /// A script's state (`script::ScriptState`).
    Script(script::ScriptState),
    /// The test probe's state (`events::ProbeState`).
    Probe(events::ProbeState),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::RoomId;
    use crate::world::World;

    fn real(v: f64) -> Value {
        Value::Real(Real::new(v))
    }

    #[test]
    fn assignment_copies_an_array() {
        let mut w = World::new(RoomId(0), 1);
        let nested = Value::array(vec![real(7.0)]).expect("nested");
        w.globals
            .insert(VarId(0), Value::array(vec![real(1.0), nested]).expect("array"));
        let copy = w.globals[&VarId(0)].assigned();
        w.globals.insert(VarId(1), copy);

        // Change the first array, top level and nested: the assignment's copy stays as it was.
        let first = w
            .globals
            .get_mut(&VarId(0))
            .expect("global 0")
            .as_array_mut()
            .expect("array");
        first.push(real(2.0));
        first[1].as_array_mut().expect("nested").push(real(8.0));

        let second = w.globals[&VarId(1)].as_array().expect("global 1 is an array");
        assert_eq!(second.len(), 2, "no element was added to the copy");
        assert_eq!(second[0], real(1.0));
        assert_eq!(second[1].as_array().expect("nested").len(), 1);
    }

    /// Contract `script/global-array-bare-write` (#1133): a bare write keeps an array's other
    /// elements and writes element 0; a scalar or unset variable, or an array value, is replaced.
    #[test]
    fn a_bare_write_writes_element_zero_of_an_array() {
        let mut vars = BTreeMap::new();
        vars.insert(VarId(0), Value::array(vec![real(1.0), real(2.0)]).expect("array"));
        assign_bare(&mut vars, VarId(0), real(9.0));
        assert_eq!(vars[&VarId(0)], Value::Array(vec![real(9.0), real(2.0)]));

        vars.insert(VarId(1), Value::Array(Vec::new()));
        assign_bare(&mut vars, VarId(1), real(4.0));
        assert_eq!(vars[&VarId(1)], Value::Array(vec![real(4.0)]));

        assign_bare(&mut vars, VarId(2), real(5.0));
        assign_bare(&mut vars, VarId(2), real(6.0));
        assert_eq!(vars[&VarId(2)], real(6.0));

        let other = Value::array(vec![real(7.0)]).expect("array");
        assign_bare(&mut vars, VarId(0), other.clone());
        assert_eq!(vars[&VarId(0)], other);
    }

    #[test]
    fn arrays_nest() {
        let inner = Value::array(vec![real(1.0), Value::Str("x".into())]).expect("inner");
        let outer =
            Value::array(vec![inner.clone(), Value::array(Vec::new()).expect("empty")]).expect("outer");
        let items = outer.as_array().expect("outer");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].as_array().expect("inner")[0], real(1.0));
        assert_eq!(items[0].as_array().expect("inner")[1], Value::Str("x".into()));
        assert!(items[1].as_array().expect("empty").is_empty());
        assert_eq!(
            inner.as_array().expect("inner").len(),
            2,
            "the clone is independent"
        );
    }

    #[test]
    fn the_array_cap_rejects_the_10001st_element() {
        let full = Value::array(vec![Value::None; MAX_ARRAY_SIZE]).expect("at the cap");
        let mut growing = full.clone();
        let err = growing.array_push(real(1.0)).unwrap_err();
        assert!(err.to_string().contains("at most 10000 elements"), "{err}");
        let err = Value::array(vec![Value::None; MAX_ARRAY_SIZE + 1]).unwrap_err();
        assert!(err.to_string().contains("array cap exceeded"), "{err}");
        let mut not_an_array = real(1.0);
        assert!(not_an_array.array_push(real(2.0)).is_err());
    }

    #[test]
    fn arrays_round_trip_through_postcard() {
        let v = Value::array(vec![
            real(1.5),
            Value::array(vec![Value::Str("x".into())]).expect("nested"),
            Value::None,
        ])
        .expect("array");
        let bytes = postcard::to_allocvec(&v).expect("serializes");
        let back: Value = postcard::from_bytes(&bytes).expect("deserializes");
        assert_eq!(back, v);
    }
}
