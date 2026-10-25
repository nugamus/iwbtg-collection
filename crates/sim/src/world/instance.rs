//! Instances (docs/plans/sim-core.md 1.1, 1.5). WP0 declares; WP1 owns.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::anim::Image;
use crate::def::{ClassMask, GroupId, KindId, NameId, VarId};
use crate::motion::Motion;
use crate::num::Real;
use crate::path::PathRun;
use crate::timeline::TimelineRun;
use crate::{builtins, events, kid, script};

/// An instance id: a counter starting at [`InstId::FIRST`], never reused (manual: ids >= 100000).
/// Our ids never match the original's; trace diffs ignore the `id` column.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct InstId(pub u64);

impl InstId {
    /// The first id the sim hands out.
    pub const FIRST: InstId = InstId(100_001);
}

/// A GML value: a custom variable, a global, a param read by scripts.
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
    /// Instance group from the room file (`PlacedInstance::group`).
    pub group: Option<GroupId>,
    /// GM parent emulation: what collision targets and queries see.
    pub classes: ClassMask,
    /// False after destroy; swept at the end of the tick.
    pub alive: bool,
    /// False while deactivated (xia pause, machinary).
    pub active: bool,
    /// GM `persistent`.
    pub persistent: bool,
    /// GM `solid`, per instance; may change at run time (Confirmed).
    pub solid: bool,
    /// GM `visible`.
    pub visible: bool,
    /// GM `depth`.
    pub depth: Real,
    /// Position, speeds, gravity, friction (WP2).
    pub m: Motion,
    /// Sprite, mask and image variables (WP1).
    pub img: Image,
    /// `alarm[0..12]`; a value `<= 0` is idle. Default -1 (Inferred).
    pub alarms: [Real; 12],
    /// The running timeline, if any.
    pub timeline: Option<TimelineRun>,
    /// The path being followed, if any.
    pub path: Option<PathRun>,
    /// Per-owner state (plan 1.5).
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
            group: None,
            classes: ClassMask::NONE,
            alive: true,
            active: true,
            persistent: false,
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

/// Per-owner instance state (plan 1.5). Each part's type lives in its owner's module and only
/// its owner adds fields.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub enum KindState {
    /// No state (blocks, spikes, decor).
    #[default]
    None,
    /// The kid (WP5a, `kid::KidState`).
    Kid(Box<kid::KidState>),
    /// A built-in's state (WP6a, `builtins::BuiltinState`).
    Builtin(builtins::BuiltinState),
    /// A script's state (WP9, `script::ScriptState`).
    Script(script::ScriptState),
    /// The test probe's state (WP1, `events::ProbeState`).
    Probe(events::ProbeState),
}
