//! Hand-built game definitions for tests and golden rooms (docs/plans/sim-core.md 10.6).
//!
//! WP0 ships a working builder; WP7a owns it afterwards. No IO, so tests on every target
//! (wasm32 included) can use it.
//!
//! ```
//! use iwbtg_sim::def::testing::GameDefBuilder;
//! use iwbtg_sim::def::{Builtin, ClassMask, EngineKind, KindDef, RoomDef};
//!
//! let mut b = GameDefBuilder::new("one-room");
//! let kid_sprite = b.sprite_rect((11, 21), (5, 12));
//! let block_sprite = b.sprite_bits(&["##", "#."], (0, 0));
//! let mut kid = KindDef::new("kid", Builtin::Kid);
//! kid.sprite = Some(kid_sprite);
//! kid.classes = ClassMask::KID;
//! let kid = b.kind(kid);
//! let mut block = KindDef::new("block", Builtin::Block);
//! block.sprite = Some(block_sprite);
//! block.solid = true;
//! block.classes = ClassMask::BLOCK;
//! let block = b.kind(block);
//! b.engine_kind(EngineKind::Kid, kid);
//! let room = b.room(RoomDef::new("room_1", 800, 608));
//! b.place(room, kid, 96.0, 567.4)?;
//! b.place(room, block, 64.0, 576.0)?;
//! let def = b.build()?;
//!
//! assert_eq!(def.rooms.len(), 1);
//! assert_eq!(def.rooms[0].instances.len(), 2);
//! assert_eq!(def.rooms[0].instances[1].rank, 1);
//! assert_eq!(def.engine_kind(EngineKind::Kid), Some(kid));
//! # Ok::<(), iwbtg_sim::def::testing::BuildError>(())
//! ```

use super::game::{EngineKind, GameDef, ResetModel, RestartModel};
use super::ids::{KindId, MaskId, RoomId, SpriteId};
use super::kind::{KindDef, RankEntry, RankTable};
use super::physics::PhysicsProfile;
use super::room::{PlacedInstance, RoomDef};
use super::sprite::{BitMask, IRect, Mask, MaskSet, MaskShape, SpriteDef};
use crate::num::Real;
use crate::rules::Runner;
use crate::rules::num::FloatModel;
use crate::timing::TimingDef;

/// Why [`GameDefBuilder::build`] or [`GameDefBuilder::place`] failed.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
#[error("game def builder: {0}")]
pub struct BuildError(pub String);

/// Builds a [`GameDef`] in code. Kinds get ranks in the order they are added, unless
/// [`GameDefBuilder::order`] sets another order. The first room added is the start room and the
/// new-game room unless set otherwise.
#[derive(Clone, Debug)]
pub struct GameDefBuilder {
    def: GameDef,
    order: Option<Vec<KindId>>,
}

/// A `u32` id for the next entry of a table of length `len`.
fn next_id(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

impl GameDefBuilder {
    /// An empty game: 50 ticks per second, GM 8.0, engine_rev 0, binary64, uninitialized reads
    /// give 0, default physics, restart and reset models.
    pub fn new(id: impl Into<String>) -> GameDefBuilder {
        GameDefBuilder {
            def: GameDef {
                id: id.into(),
                tick_rate: 50,
                max_ticks: crate::replay::MAX_REPLAY_TICKS,
                runner: Runner::Gm80,
                engine_rev: 0,
                float_model: FloatModel::F64,
                uninit_reads_zero: true,
                first_runtime_id: None,
                sprites: Vec::new(),
                masks: Vec::new(),
                kinds: Vec::new(),
                rooms: Vec::new(),
                start_room: RoomId(0),
                new_game_room: RoomId(0),
                boot: Vec::new(),
                ranks: RankTable::default(),
                engine_kinds: Default::default(),
                var_names: Default::default(),
                names: Vec::new(),
                groups: Vec::new(),
                physics: PhysicsProfile::default(),
                restart: RestartModel::default(),
                reset: ResetModel::default(),
                sounds: Vec::new(),
                timelines: Vec::new(),
                paths: Vec::new(),
                scripts: Vec::new(),
                timing: TimingDef::default(),
                maps: Vec::new(),
                stripped: Default::default(),
            },
            order: None,
        }
    }

    /// The definition being built, for settings without a builder method.
    pub fn def_mut(&mut self) -> &mut GameDef {
        &mut self.def
    }

    fn add_mask(&mut self, mask: Mask) -> MaskId {
        let id = MaskId(next_id(self.def.masks.len()));
        self.def.masks.push(mask);
        id
    }

    fn add_sprite(&mut self, size: (u32, u32), origin: (i32, i32), mask: Mask) -> SpriteId {
        let mask = self.add_mask(mask);
        let id = SpriteId(next_id(self.def.sprites.len()));
        self.def.sprites.push(SpriteDef {
            frames: 1,
            size,
            origin,
            masks: MaskSet::Shared(mask),
        });
        id
    }

    /// A one-frame sprite whose mask is its whole `size` rectangle.
    pub fn sprite_rect(&mut self, size: (u32, u32), origin: (i32, i32)) -> SpriteId {
        let bbox = IRect {
            left: 0,
            top: 0,
            right: i64::from(size.0) - 1,
            bottom: i64::from(size.1) - 1,
        };
        let shape = if size.0 == 0 || size.1 == 0 {
            MaskShape::Empty
        } else {
            MaskShape::Rect
        };
        self.add_sprite(size, origin, Mask { bbox, shape })
    }

    /// A one-frame precise sprite from text rows: `#` is a solid pixel, anything else is clear.
    /// The bounding box is the tight box around the solid pixels; none gives an empty mask.
    pub fn sprite_bits(&mut self, rows: &[&str], origin: (i32, i32)) -> SpriteId {
        let width = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
        let size = (next_id(width), next_id(rows.len()));
        let solid: Vec<(i64, i64)> = rows
            .iter()
            .zip(0i64..)
            .flat_map(|(row, y)| {
                row.chars()
                    .zip(0i64..)
                    .filter(|(c, _)| *c == '#')
                    .map(move |(_, x)| (x, y))
            })
            .collect();
        let bbox = IRect {
            left: solid.iter().map(|p| p.0).min().unwrap_or(0),
            top: solid.iter().map(|p| p.1).min().unwrap_or(0),
            right: solid.iter().map(|p| p.0).max().unwrap_or(-1),
            bottom: solid.iter().map(|p| p.1).max().unwrap_or(-1),
        };
        let shape = if solid.is_empty() {
            MaskShape::Empty
        } else {
            let w = u32::try_from(bbox.right - bbox.left + 1).unwrap_or(0);
            let h = u32::try_from(bbox.bottom - bbox.top + 1).unwrap_or(0);
            let mut bits = BitMask::new(w, h).expect("test masks are far below MAX_MASK_WORDS");
            for (x, y) in &solid {
                bits.set(x - bbox.left, y - bbox.top, true);
            }
            MaskShape::Bits(bits)
        };
        self.add_sprite(size, origin, Mask { bbox, shape })
    }

    /// Adds a kind and returns its id.
    pub fn kind(&mut self, kind: KindDef) -> KindId {
        let id = KindId(next_id(self.def.kinds.len()));
        self.def.kinds.push(kind);
        id
    }

    /// Sets the dispatch order: `kinds[i]` gets rank `i`. Kinds left out get no rank.
    pub fn order(&mut self, kinds: &[KindId]) {
        self.order = Some(kinds.to_vec());
    }

    /// Adds a room and returns its id.
    pub fn room(&mut self, room: RoomDef) -> RoomId {
        let id = RoomId(next_id(self.def.rooms.len()));
        self.def.rooms.push(room);
        id
    }

    /// Places an instance of `kind` at `(x, y)` in `room`, after the room's other instances.
    /// Its rank is filled in by [`GameDefBuilder::build`].
    pub fn place(
        &mut self,
        room: RoomId,
        kind: KindId,
        x: f64,
        y: f64,
    ) -> Result<&mut PlacedInstance, BuildError> {
        let r = room
            .index()
            .and_then(|i| self.def.rooms.get_mut(i))
            .ok_or_else(|| BuildError(format!("no room {room:?}")))?;
        r.instances
            .push(PlacedInstance::at(kind, 0, Real::new(x), Real::new(y)));
        r.instances
            .last_mut()
            .ok_or_else(|| BuildError("instance list is empty after a push".into()))
    }

    /// Sets the kind the sim spawns for an engine role.
    pub fn engine_kind(&mut self, role: EngineKind, kind: KindId) {
        self.def.engine_kinds.insert(role, kind);
    }

    /// Adds a boot kind (created before the first room).
    pub fn boot(&mut self, kind: KindId) {
        self.def.boot.push(kind);
    }

    /// Sets the start room and the new-game room.
    pub fn start_room(&mut self, room: RoomId) {
        self.def.start_room = room;
        self.def.new_game_room = room;
    }

    /// Sets the physics profile.
    pub fn physics(&mut self, physics: PhysicsProfile) {
        self.def.physics = physics;
    }

    /// Finishes the definition: builds the rank table, fills placed ranks and checks every
    /// reference.
    pub fn build(mut self) -> Result<GameDef, BuildError> {
        let order = match self.order.take() {
            Some(order) => order,
            None => (0..next_id(self.def.kinds.len())).map(KindId).collect(),
        };
        let mut entries = Vec::with_capacity(order.len());
        for (kind, rank) in order.into_iter().zip(0u16..) {
            entries.push(RankEntry {
                kind,
                variant: None,
                rank,
            });
        }
        self.def.ranks = RankTable { entries };
        let def = &mut self.def;
        let kind_count = def.kinds.len();
        let ok_kind = |k: KindId| k.index().is_some_and(|i| i < kind_count);
        for room in &mut def.rooms {
            for inst in &mut room.instances {
                inst.rank = def
                    .ranks
                    .rank_for(inst.kind, None)
                    .ok_or_else(|| BuildError(format!("{:?} in {} has no rank", inst.kind, room.name)))?;
            }
        }
        if let Some(bad) = def
            .boot
            .iter()
            .chain(def.engine_kinds.values())
            .find(|k| !ok_kind(**k))
        {
            return Err(BuildError(format!("no kind {bad:?}")));
        }
        for kind in &def.kinds {
            let sprite_ok = kind
                .sprite
                .is_none_or(|s| s.index().is_some_and(|i| i < def.sprites.len()));
            let mask_ok = kind
                .mask
                .is_none_or(|m| m.index().is_some_and(|i| i < def.masks.len()));
            if !sprite_ok || !mask_ok {
                return Err(BuildError(format!(
                    "kind {} names a missing sprite or mask",
                    kind.name
                )));
            }
        }
        for room in [def.start_room, def.new_game_room] {
            if def.room(room).is_none() {
                return Err(BuildError(format!("no room {room:?}")));
            }
        }
        Ok(self.def)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::Builtin;

    #[test]
    fn bits_sprite_has_tight_box() {
        let mut b = GameDefBuilder::new("t");
        let s = b.sprite_bits(&["....", ".#..", "..#."], (0, 0));
        let k = b.kind(KindDef::new("k", Builtin::Probe));
        let room = b.room(RoomDef::new("r", 32, 32));
        b.place(room, k, 0.0, 0.0).unwrap();
        let def = b.build().unwrap();
        let mask = &def.masks[0];
        assert_eq!(
            mask.bbox,
            IRect {
                left: 1,
                top: 1,
                right: 2,
                bottom: 2
            }
        );
        let MaskShape::Bits(bits) = &mask.shape else {
            panic!("expected bits")
        };
        assert!(bits.get(0, 0) && bits.get(1, 1) && !bits.get(1, 0));
        assert_eq!(def.sprite(s).map(|s| s.size), Some((4, 3)));
    }

    #[test]
    fn build_rejects_missing_rooms_and_unranked_kinds() {
        assert!(GameDefBuilder::new("t").build().is_err());
        let mut b = GameDefBuilder::new("t");
        let a = b.kind(KindDef::new("a", Builtin::Probe));
        let c = b.kind(KindDef::new("c", Builtin::Probe));
        b.order(&[c]);
        let room = b.room(RoomDef::new("r", 32, 32));
        b.place(room, a, 0.0, 0.0).unwrap();
        assert!(b.place(RoomId(9), a, 0.0, 0.0).is_err());
        assert!(b.build().is_err());
    }
}
