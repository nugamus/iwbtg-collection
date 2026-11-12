//! Kinds, parameters and dispatch ranks (docs/plans/sim-core.md 1.1, 1.3, 8.2).
//!
//! WP0 declares; WP7a owns afterwards.

use std::collections::BTreeMap;

use super::builtin::Builtin;
use super::class::ClassMask;
use super::ids::{KindId, MaskId, RoomId, ScriptId, SoundId, SpriteId};
use crate::events::EventSet;
use crate::num::Real;

/// One kind: a built-in, a preset (a built-in with params) or a scripted package object.
#[derive(Clone, PartialEq, Debug)]
pub struct KindDef {
    /// Package name (`block`, `@boss1`, ...).
    pub name: String,
    /// The built-in whose handlers run for this kind.
    pub base: Builtin,
    /// A script whose handlers run after the base handler (WP9).
    pub script: Option<ScriptId>,
    /// Default sprite.
    pub sprite: Option<SpriteId>,
    /// Object-level mask; `None` uses the sprite's masks.
    pub mask: Option<MaskId>,
    /// Default `solid`.
    pub solid: bool,
    /// Default `visible`.
    pub visible: bool,
    /// Default depth.
    pub depth: Real,
    /// GM `persistent`.
    pub persistent: bool,
    /// Classes, the built-in's defaults plus `counts_as`.
    pub classes: ClassMask,
    /// Default params; a placed instance's params override them.
    pub params: Params,
    /// What the kind handles, built once at load (`events::event_set` reads it).
    pub events: EventSet,
}

impl KindDef {
    /// A kind with GM defaults: no sprite, not solid, visible, depth 0, not persistent, no
    /// classes, no params, no events.
    pub fn new(name: impl Into<String>, base: Builtin) -> KindDef {
        KindDef {
            name: name.into(),
            base,
            script: None,
            sprite: None,
            mask: None,
            solid: false,
            visible: true,
            depth: Real::ZERO,
            persistent: false,
            classes: ClassMask::NONE,
            params: Params::default(),
            events: EventSet::default(),
        }
    }
}

/// Instance or kind parameters (object params, instance creation code as data).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Params(pub BTreeMap<String, ParamValue>);

/// One parameter value, resolved at load.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ParamValue {
    /// A real.
    Real(Real),
    /// A flag.
    Bool(bool),
    /// Text (enum-like params such as `mode = "screen_snap"`).
    Str(String),
    /// A room reference (`room_to`).
    Room(RoomId),
    /// A kind reference (spawners, traps).
    Kind(KindId),
    /// A sound reference.
    Sound(SoundId),
}

/// One `[defaults.<kind>]` variant of a bare built-in (docs/format/v0.md, `[defaults.<kind>]`):
/// an instance of `kind` whose params match `when` uses `variant`, the kind the loader derives
/// from it (`spike[dir=down]`, with the variant's sprite and mask). A placed instance and a
/// script `instance_create` take it alike (#701).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct KindVariant {
    /// The bare built-in kind.
    pub kind: KindId,
    /// The params the variant matches, in `game.toml` order.
    pub when: Vec<(String, ParamValue)>,
    /// The derived kind.
    pub variant: KindId,
}

/// Whether an instance with `params` of a kind with `kind_params` matches a variant's `when`:
/// every `when` param, the instance's or else the kind's, has that value. Reals compare by bits,
/// so `2` and `2.0` match. The loader and script spawns share it (#701).
pub fn variant_matches(when: &[(String, ParamValue)], params: &Params, kind_params: &Params) -> bool {
    when.iter().all(|(k, v)| {
        params
            .0
            .get(k)
            .or_else(|| kind_params.0.get(k))
            .is_some_and(|p| param_matches(p, v))
    })
}

fn param_matches(a: &ParamValue, b: &ParamValue) -> bool {
    match (a, b) {
        (ParamValue::Real(x), ParamValue::Real(y)) => x.to_bits() == y.to_bits(),
        _ => a == b,
    }
}

/// An `object_order` selector on a kind's variant (`{ kind = "spike", dir = "up" }`,
/// `{ kind = "kid", flipped = true }`), so collapsed original objects keep their original index
/// order (converter OPEN 13).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VariantSel {
    /// Param name (`dir`, `flipped`).
    pub key: String,
    /// Param value it must have.
    pub value: ParamValue,
}

/// One resolved `object_order` entry.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RankEntry {
    /// The kind.
    pub kind: KindId,
    /// Variant selector, if the entry has one.
    pub variant: Option<VariantSel>,
    /// Dispatch rank.
    pub rank: u16,
}

/// Every kind's dispatch rank, from `object_order` (`crates/format`, `resolve_ranks`).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct RankTable {
    /// Entries in `object_order` order.
    pub entries: Vec<RankEntry>,
}

impl RankTable {
    /// The rank of `kind`: the entry whose selector matches `variant`, else the kind's entry
    /// without a selector.
    pub fn rank_for(&self, kind: KindId, variant: Option<&VariantSel>) -> Option<u16> {
        let of_kind = || self.entries.iter().filter(move |e| e.kind == kind);
        variant
            .and_then(|v| of_kind().find(|e| e.variant.as_ref() == Some(v)))
            .or_else(|| of_kind().find(|e| e.variant.is_none()))
            .map(|e| e.rank)
    }
}
