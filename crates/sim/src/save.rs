//! The save slot (docs/architecture/sim/rooms.md, N5-N8).
//!
//! The sim never touches files: the save is sim state, the client stores its bytes (postcard).
//!
//! Spec: CLONE_SPEC rooms, "Save files at the runner level" and "Saved positions per group".
//! Not here: the file bytes (the client stores the slot), the first-play save (kid Create) and the kid's own fields. A save reads the kid through its instance only (position,
//! previous position, class, `image_xscale`). "Flipped" is the class `ClassMask::KID_FLIPPED`
//! (games that flip by object swap); games that flip by a flag are OPEN until `kid::api` has
//! an accessor (follow-up issue).

use serde::{Deserialize, Serialize};

use crate::collide::query::place_free;
use crate::def::{EngineKind, GameDef, KidFamily, RoomId, RoomRole};
use crate::error::SimError;
use crate::num::{self, Real};
use crate::rules::NumRules;
use crate::world::{Ctx, InstId, SpawnInit};

/// How a save wrote its position: the exact `f64` (`standard@1`), or one of the original
/// formats' quantizations (`gm8`, selected by the save point or the game's `[save]` model).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum SavePrecision {
    /// The exact `f64` (`standard@1`).
    Exact,
    /// Base-100 bytes (13 games; last byte through `byte_from_real`, N5).
    #[default]
    XyBase100Bytes,
    /// Base-256 div/mod bytes (little-runner, N8).
    XyBase256Bytes,
    /// `string()` with two decimals (machinary, eat-the-new-year, N6).
    XyText2Dec,
    /// `xprevious`/`yprevious` exactly (kaenbyou, eat-the-lemon, N7).
    XyPrevious,
    /// A fixed point (poronkusema).
    FixedPoint,
    /// Floor with the three-step +1 nudge using `place_free` (xia).
    FloorNudge,
}

impl SavePrecision {
    /// The precision a saved position with `model` was written at.
    pub fn of_model(model: SavedPosition) -> SavePrecision {
        match model {
            SavedPosition::XyBase100Bytes => SavePrecision::XyBase100Bytes,
            SavedPosition::XyBase256Bytes => SavePrecision::XyBase256Bytes,
            SavedPosition::XyText2Dec => SavePrecision::XyText2Dec,
            SavedPosition::XyPrevious => SavePrecision::XyPrevious,
            SavedPosition::FixedPoint => SavePrecision::FixedPoint,
            SavedPosition::FloorNudge => SavePrecision::FloorNudge,
        }
    }

    /// The quantization model, or `None` for [`SavePrecision::Exact`].
    pub fn model(self) -> Option<SavedPosition> {
        match self {
            SavePrecision::Exact => None,
            SavePrecision::XyBase100Bytes => Some(SavedPosition::XyBase100Bytes),
            SavePrecision::XyBase256Bytes => Some(SavedPosition::XyBase256Bytes),
            SavePrecision::XyText2Dec => Some(SavedPosition::XyText2Dec),
            SavePrecision::XyPrevious => Some(SavedPosition::XyPrevious),
            SavePrecision::FixedPoint => Some(SavedPosition::FixedPoint),
            SavePrecision::FloorNudge => Some(SavedPosition::FloorNudge),
        }
    }
}

/// What the game's save file holds.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SaveSlot {
    /// Saved room.
    pub room: RoomId,
    /// Saved position, after [`quantize`].
    pub x: Real,
    /// Saved position, after [`quantize`].
    pub y: Real,
    /// How [`x`](Self::x)/[`y`](Self::y) were written: the save point's model, or the game's
    /// `[save]` model for the first-play save. Not serialized or hashed: the persisted
    /// [`Checkpoint`] carries it, and the runtime value is exactly what `save_now` used. A slot
    /// decoded from an older file takes the default until the checkpoint supplies the real one.
    #[serde(skip)]
    pub precision: SavePrecision,
    /// The save object's stable key (#1153 E1): `name:<declared name>` or
    /// `at:<room>/<kind>@<xbits>,<ybits>` from its exact spawn position. Not serialized or
    /// hashed; the persisted [`Checkpoint`] carries it and `set_point` fills it after
    /// [`save_now`]. A first-play or creation-code save leaves it `None`.
    #[serde(skip)]
    pub point: Option<String>,
    /// Gravity flipped, as the game stores it. A game that flips by swapping to a second kid kind
    /// or by a flag on the kid (families A and C) stores whether the kid was flipped. A game that
    /// flips by the `yflag` model (family B, `[flip] model = "yflag"`) stores `global.reverse`
    /// here; it never flips by kind, so the two meanings never meet in one game. Only
    /// [`flip_by_kind`] tells the loader whether this field also picks the flipped kid kind and
    /// the `+dy` offset.
    pub flipped: bool,
    /// Kid facing, -1 or 1.
    pub facing: i8,
    /// Difficulty index.
    pub difficulty: u8,
    /// Death counter (checked).
    pub deaths: u32,
    /// Play time counter (checked).
    pub time: u32,
    /// Boss flags.
    pub bosses: u16,
    /// Item flags.
    pub items: u16,
    /// Game cleared.
    pub clear: bool,
}

/// Why a legacy raw [`SaveSlot`] payload could not be decoded.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum SaveSlotError {
    /// The bytes do not decode.
    #[error("the save slot does not decode ({0})")]
    Corrupt(String),
    /// Bytes remain after the slot.
    #[error("{0} trailing bytes after the save slot")]
    TrailingBytes(u64),
}

impl SaveSlot {
    /// Decodes the raw slot a pre-#1145 client save and a replay format 5/6's `FromSave` kept
    /// (the format before the versioned checkpoint). The payload must be the whole byte range:
    /// trailing bytes are refused, so an envelope's version field or an appended field is never
    /// read as a slot (#1325).
    pub fn decode(bytes: &[u8]) -> Result<SaveSlot, SaveSlotError> {
        let (slot, rest) = postcard::take_from_bytes::<SaveSlot>(bytes)
            .map_err(|e| SaveSlotError::Corrupt(e.to_string()))?;
        if !rest.is_empty() {
            return Err(SaveSlotError::TrailingBytes(rest.len() as u64));
        }
        Ok(slot)
    }
}

/// What the original's save format does to a position (rooms Confirmed group table).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SavedPosition {
    /// Base-100 bytes (13 games; last byte through `byte_from_real`, N5).
    XyBase100Bytes,
    /// Base-256 div/mod bytes (little-runner, N8).
    XyBase256Bytes,
    /// `string()` with two decimals (machinary, eat-the-new-year, N6).
    XyText2Dec,
    /// `xprevious`/`yprevious` exactly (kaenbyou, eat-the-lemon, N7).
    XyPrevious,
    /// A fixed point (poronkusema).
    FixedPoint,
    /// Floor with the three-step +1 nudge using `place_free` (xia).
    FloorNudge,
}

/// Saves now from `kid` with the game's position model.
///
/// The slot gets the current room, the quantized position ([`quantize`]; `XyPrevious` reads
/// `xprevious`/`yprevious`, `FixedPoint` takes `fixed` and errors without it, `FloorNudge` runs
/// the three `place_free` checks of scrSaveGame first), the kid's class and facing, and the run
/// counters, flags and difficulty. The save point calls this from its own event, after the
/// kid's block correction (Confirmed (recorder), rely `num-byte-save-d.csv`), so the position
/// read here is the corrected one.
pub fn save_now(
    ctx: &mut Ctx,
    kid: InstId,
    model: SavedPosition,
    fixed: Option<(Real, Real)>,
) -> Result<(), SimError> {
    // Family B stores the first kid whichever kid touched the save point (R21).
    let kid = if ctx.physics().save.first_kid {
        ctx.w.kid(&ctx.rules.tick).unwrap_or(kid)
    } else {
        kid
    };
    let inst = ctx
        .w
        .get(kid)
        .ok_or_else(|| SimError::invariant(format!("save_now: unknown kid {}", kid.0)))?;
    let (x, y) = (inst.m.x(), inst.m.y());
    let (px, py) = (inst.m.xprevious(), inst.m.yprevious());
    let flipped = if by_flag(ctx) {
        ctx.w.engine.kid.b.reverse
    } else {
        crate::kid::api::flipped(ctx.w, kid)
    };
    let facing = crate::kid::api::saved_facing(ctx.w, kid);
    let (rx, ry) = match model {
        SavedPosition::XyPrevious => (px, py),
        SavedPosition::FixedPoint => fixed.ok_or_else(|| {
            SimError::invariant("save_now: SavedPosition::FixedPoint needs the fixed point")
        })?,
        SavedPosition::FloorNudge => nudge(ctx, kid, x, y)?,
        SavedPosition::XyBase100Bytes | SavedPosition::XyBase256Bytes | SavedPosition::XyText2Dec => (x, y),
    };
    let (qx, qy) = quantize(rx, ry, model, &ctx.rules.num);
    let run = &ctx.w.engine.run;
    let slot = SaveSlot {
        room: ctx.w.room,
        x: qx,
        y: qy,
        precision: SavePrecision::of_model(model),
        point: None,
        flipped,
        facing,
        difficulty: run.difficulty,
        deaths: run.deaths,
        time: run.time,
        bosses: ctx.w.engine.mech.bosses,
        items: ctx.w.engine.mech.items,
        clear: run.clear,
    };
    ctx.w.save = Some(slot);
    Ok(())
}

/// Records the save object's stable key on the slot [`save_now`] just wrote (#1153 E1). A
/// room-file `save` instance carries the key from `Ctx::spawn`; a script-spawned save object,
/// the first-play save and `save_game` have none. Called by the save built-in after `save_now`.
pub fn set_point(ctx: &mut Ctx, me: InstId) {
    let point = ctx.w.get(me).and_then(|i| i.point.clone());
    if let Some(slot) = ctx.w.save.as_mut() {
        slot.point = point;
    }
}

/// A save with no kid (room creation code `save_game`, #629): the current room at x 0, y 0,
/// not flipped (by-flag games store `global.reverse`), facing 1, and the run counters, flags
/// and difficulty as [`save_now`] stores them. Confirmed (recorder) for showdown, whose save
/// stores no facing or gravity: kid/A3.md "Saves", a3-t20-a f3-4. The facing and flip of other
/// games' kid-less save are Inferred (no game of the 20 saves without a kid otherwise).
pub fn save_without_kid(ctx: &mut Ctx) {
    let flipped = by_flag(ctx) && ctx.w.engine.kid.b.reverse;
    let run = &ctx.w.engine.run;
    let precision = SavePrecision::of_model(ctx.physics().save.position.model());
    ctx.w.save = Some(SaveSlot {
        room: ctx.w.room,
        x: Real::ZERO,
        y: Real::ZERO,
        precision,
        point: None,
        flipped,
        facing: 1,
        difficulty: run.difficulty,
        deaths: run.deaths,
        time: run.time,
        bosses: ctx.w.engine.mech.bosses,
        items: ctx.w.engine.mech.items,
        clear: run.clear,
    });
}

/// True when the game flips gravity by a flag (`[flip] model = "yflag"`, family B): its save
/// stores `global.reverse` in [`SaveSlot::flipped`].
fn by_flag(ctx: &Ctx) -> bool {
    ctx.physics().flip.model == crate::def::physics::FlipKind::Yflag
}

/// True when [`SaveSlot::flipped`] means "the kid of the flipped kind, `flipped_load_dy` lower":
/// every game but the ones that flip by flag, whose `flipped` is `global.reverse` and whose kid
/// is the one kind at the saved spot.
pub fn flip_by_kind(ctx: &Ctx, slot: &SaveSlot) -> bool {
    slot.flipped && !by_flag(ctx)
}

/// A death-only save: counters only. The saved room, position and flags stay as they are
/// (saveDeathTime seeks past them; little-runner writes the old ones back). With no save there
/// is nothing to update, so it does nothing (OPEN: the original would create a file holding
/// only the counters, which no load can use).
pub fn save_counters(ctx: &mut Ctx) -> Result<(), SimError> {
    let (deaths, time) = (ctx.w.engine.run.deaths, ctx.w.engine.run.time);
    if let Some(slot) = ctx.w.save.as_mut() {
        slot.deaths = deaths;
        slot.time = time;
    }
    Ok(())
}

/// What the original's save format does to a position (rooms Confirmed group table).
///
/// - `XyBase100Bytes`: per coordinate `b1 = floor(v / 10000)`, `b2 = floor((v - b1 * 10000) /
///   100)`, `b3 = v - b1 * 10000 - b2 * 100`, each written through `byte_from_real` (N5, which
///   rounds half to even, so 567.4 gives 567 and 48.5 gives 48), read back as `b1 * 10000 + b2 *
///   100 + b3`. Negative values wrap like the byte write (OPEN, not traced).
/// - `XyBase256Bytes`: `b1 = min(v div 256, 255)`, `v = v mod 256`, `b2 = min(v div 1, 255)`,
///   read back as `b1 * 256 + b2`: truncation (Confirmed (recorder) 936.6 to 936, N8).
/// - `XyText2Dec`: `string(v)` read back (N6): two decimals unless integral.
/// - `XyPrevious`, `FixedPoint`: exact (the source of the numbers is `save_now`'s).
/// - `FloorNudge`: floors both (the `place_free` nudge needs a world: see `save_now`).
pub fn quantize(x: Real, y: Real, model: SavedPosition, r: &NumRules) -> (Real, Real) {
    match model {
        SavedPosition::XyBase100Bytes => (base100(x, r), base100(y, r)),
        SavedPosition::XyBase256Bytes => (base256(x, r), base256(y, r)),
        SavedPosition::XyText2Dec => (text2(x, r), text2(y, r)),
        SavedPosition::XyPrevious | SavedPosition::FixedPoint => (x, y),
        SavedPosition::FloorNudge => (num::floor(x, r), num::floor(y, r)),
    }
}

fn byte_real(v: Real, r: &NumRules) -> Real {
    Real::new(f64::from(num::byte_from_real(v, r)))
}

fn base100(v: Real, r: &NumRules) -> Real {
    let ten_k = Real::new(10_000.0);
    let hundred = Real::new(100.0);
    let hi = num::floor(v / ten_k, r);
    let mid = num::floor((v - hi * ten_k) / hundred, r);
    let lo = v - hi * ten_k - mid * hundred;
    byte_real(hi, r) * ten_k + byte_real(mid, r) * hundred + byte_real(lo, r)
}

fn base256(v: Real, r: &NumRules) -> Real {
    let cap = Real::new(255.0);
    let place = Real::new(256.0);
    // A zero divisor is impossible here, so the fallbacks never run.
    let hi = num::gm_div(v, place).unwrap_or(Real::ZERO);
    let rest = num::gm_mod(v, place).unwrap_or(Real::ZERO);
    let lo = num::gm_div(rest, Real::ONE).unwrap_or(Real::ZERO);
    num::gm_min(hi, cap, r) * place + num::gm_min(lo, cap, r)
}

fn text2(v: Real, r: &NumRules) -> Real {
    num::gm_string_real(v, r).parse::<f64>().map_or(v, Real::new)
}

/// Group F's three-step nudge (scrSaveGame): each check uses the kid's mask and `place_free`.
fn nudge(ctx: &Ctx, kid: InstId, x: Real, y: Real) -> Result<(Real, Real), SimError> {
    let r = &ctx.rules.num;
    let (mut x, mut y) = (x, y);
    if !place_free(ctx, kid, num::floor(x, r), y)? {
        x += Real::ONE;
    }
    if !place_free(ctx, kid, x, num::floor(y, r))? {
        y += Real::ONE;
    }
    if !place_free(ctx, kid, num::floor(x, r), num::floor(y, r))? {
        x += Real::ONE;
        y += Real::ONE;
    }
    Ok((num::floor(x, r), num::floor(y, r)))
}

/// Loads a slot as loadGame and saveExe do: restores the counters, flags and difficulty from
/// the slot, creates a persistent kid at the saved spot and returns it. A flipped slot spawns
/// `EngineKind::KidFlipped` when the game has one (else the kid kind) and adds
/// `RestartModel::flipped_load_dy` to y (tempExe, +4 in 10 Group A games). The kid is created
/// at (0, 0) and then moved, as the original does (its `xprevious` stays 0, Confirmed
/// (recorder) timeline N+1), so its Create runs at (0, 0). The room change is **not** requested
/// here: the caller does (`restart`, the room sequence's `LoadGame`).
pub fn load_into(ctx: &mut Ctx, slot: &SaveSlot) -> Result<InstId, SimError> {
    ctx.w.engine.run.difficulty = slot.difficulty;
    ctx.w.engine.run.deaths = slot.deaths;
    ctx.w.engine.run.time = slot.time;
    ctx.w.engine.run.clear = slot.clear;
    ctx.w.engine.mech.bosses = slot.bosses;
    ctx.w.engine.mech.items = slot.items;
    let by_kind = flip_by_kind(ctx, slot);
    let role = match (by_kind, ctx.def.engine_kind(EngineKind::KidFlipped)) {
        (true, Some(_)) => EngineKind::KidFlipped,
        _ => EngineKind::Kid,
    };
    let id = ctx
        .spawn_engine(role, Real::ZERO, Real::ZERO, SpawnInit::default())?
        .ok_or_else(|| SimError::invariant("load_into: the game has no kid kind"))?;
    let dy = if by_kind {
        ctx.def.restart.flipped_load_dy
    } else {
        Real::ZERO
    };
    if let Some(inst) = ctx.w.get_mut(id) {
        inst.persistent = true;
        inst.m.set_x(slot.x)?;
        inst.m.set_y(slot.y + dy)?;
    }
    crate::kid::api::restore_orientation(ctx, id, slot.flipped, slot.facing)?;
    Ok(id)
}

// ---- The checkpoint: the persisted save ----

/// A room's stable identity in a save, replay or board: its package name (`RoomDef::name`),
/// never the runtime [`RoomId`] index. A reorder or insert cannot change the room a checkpoint
/// names.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub struct RoomKey(pub String);

/// The ruleset a checkpoint was written under. Format 0 maps it to the runner and built-in
/// revision (`gm80@0`); format 1 makes `ruleset@N` explicit (#1160).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RulesetId(pub String);

impl RulesetId {
    /// The ruleset this build reads `def` as. Through format 0 every package is `gm8` at its
    /// `engine_rev` (decision 0005); format 1 writes `ruleset@N` explicitly (#1160).
    pub fn of(def: &GameDef) -> RulesetId {
        RulesetId(format!("gm8@{}", def.engine_rev))
    }
}

/// The compatibility profile a checkpoint was written under. Until format 1 names one
/// (`[compat] profile`, #1160), the physics family it starts from names it (`gm8/a1@1`).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ProfileId(pub String);

impl ProfileId {
    /// The profile of `room`: its physics family at version 1.
    pub fn for_room(def: &GameDef, room: RoomId) -> ProfileId {
        ProfileId(format!("gm8/{}@1", family_name(def.physics_in(room).preset)))
    }
}

fn family_name(family: KidFamily) -> &'static str {
    match family {
        KidFamily::A1 => "a1",
        KidFamily::A2 => "a2",
        KidFamily::A3 => "a3",
        KidFamily::A4 => "a4",
        KidFamily::A5 => "a5",
        KidFamily::B => "b",
        KidFamily::C => "c",
    }
}

/// One versioned checkpoint: the payload of the client save file and of a replay's
/// `StartMode::FromSave` (replay format 7 and later). The runtime keeps the numeric [`SaveSlot`];
/// the checkpoint names the room by [`RoomKey`] and is resolved when it is loaded.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    /// [`Checkpoint::FORMAT`].
    pub format: u16,
    /// The ruleset it was written under.
    pub ruleset: RulesetId,
    /// The compatibility profile it was written under.
    pub profile: ProfileId,
    /// The saved room's stable name.
    pub room: RoomKey,
    /// The save object's stable key (#1153 E1): `name:<declared name>` for a named placed
    /// instance, else `at:<room>/<kind>@<xbits>,<ybits>` from its exact spawn position. `None`
    /// for a first-play or creation-code save, and for a script-spawned save object that has no
    /// stable placed identity. [`Checkpoint::to_slot`] resolves it or refuses.
    pub point: Option<String>,
    /// The stored position, already written at [`precision`](Self::precision).
    pub x: Real,
    /// The stored position, already written at [`precision`](Self::precision).
    pub y: Real,
    /// How the position was written.
    pub precision: SavePrecision,
    /// Gravity flipped, as the game stores it (`SaveSlot::flipped`).
    pub flipped: bool,
    /// Kid facing, -1 or 1.
    pub facing: i8,
    /// Difficulty index.
    pub difficulty: u8,
    /// Death counter.
    pub deaths: u32,
    /// Play time counter.
    pub time: u32,
    /// Boss flags.
    pub bosses: u16,
    /// Item flags.
    pub items: u16,
    /// Game cleared.
    pub clear: bool,
}

/// Why a checkpoint could not be built, decoded or resolved.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum CheckpointError {
    /// A format this build does not read.
    #[error("checkpoint format {0}, this build reads {current}", current = Checkpoint::FORMAT)]
    Format(u16),
    /// The room name is not in this package.
    #[error("the saved room `{0}` is not in this version of the game")]
    NoRoom(String),
    /// The room exists but is not gameplay.
    #[error("the saved room `{0}` is not gameplay")]
    NotPlay(String),
    /// The saved save object moved, was removed, was renamed, or the key does not parse.
    #[error("the saved point `{0}` is not in this version of the game")]
    NoPoint(String),
    /// More rooms than a `RoomId` can index.
    #[error("too many rooms")]
    TooManyRooms,
    /// The bytes do not decode.
    #[error("the checkpoint does not decode ({0})")]
    Corrupt(String),
    /// Bytes remain after the checkpoint.
    #[error("{0} trailing bytes after the checkpoint")]
    TrailingBytes(u64),
}

impl Checkpoint {
    /// The only checkpoint format this build writes.
    pub const FORMAT: u16 = 1;

    /// The checkpoint for `slot`, naming the room [`GameDef::room`] gives it. Reads the
    /// precision `save_now` recorded on the slot.
    pub fn from_slot(def: &GameDef, slot: &SaveSlot) -> Result<Checkpoint, CheckpointError> {
        let room = def
            .room(slot.room)
            .ok_or_else(|| CheckpointError::NoRoom(format!("room index {}", slot.room.0)))?;
        Self::from_parts(def, RoomKey(room.name.clone()), slot)
    }

    /// The checkpoint for `slot` under the stable `room` name: a version 1 or 2 save kept the
    /// name even when the numeric index moved (#1019).
    pub fn from_parts(def: &GameDef, room: RoomKey, slot: &SaveSlot) -> Result<Checkpoint, CheckpointError> {
        let at = resolve_room(def, &room)?;
        Ok(Checkpoint {
            format: Self::FORMAT,
            ruleset: RulesetId::of(def),
            profile: ProfileId::for_room(def, at),
            room,
            point: slot.point.clone(),
            x: slot.x,
            y: slot.y,
            precision: slot.precision,
            flipped: slot.flipped,
            facing: slot.facing,
            difficulty: slot.difficulty,
            deaths: slot.deaths,
            time: slot.time,
            bosses: slot.bosses,
            items: slot.items,
            clear: slot.clear,
        })
    }

    /// Resolves the checkpoint to the runtime slot. The named room must still exist and be
    /// gameplay, and the save point — when the checkpoint recorded one — must still resolve to
    /// the same placed instance. `ruleset`, `profile` and `precision` are provenance and do not
    /// change the stored position.
    ///
    /// Runs [`Checkpoint::validate`] first: the client reads the checkpoint inside its envelope
    /// (never through [`Checkpoint::decode`]), so this is where its disk path and the replay's
    /// `FromSave` path agree on the format (#1325).
    pub fn to_slot(&self, def: &GameDef) -> Result<SaveSlot, CheckpointError> {
        self.validate()?;
        let room = resolve_room(def, &self.room)?;
        if let Some(point) = &self.point {
            resolve_point(def, room, point)?;
        }
        Ok(SaveSlot {
            room,
            x: self.x,
            y: self.y,
            precision: self.precision,
            point: self.point.clone(),
            flipped: self.flipped,
            facing: self.facing,
            difficulty: self.difficulty,
            deaths: self.deaths,
            time: self.time,
            bosses: self.bosses,
            items: self.items,
            clear: self.clear,
        })
    }

    /// The canonical bytes: the replay's `FromSave` payload and the client save's payload.
    pub fn encode(&self) -> Result<Vec<u8>, CheckpointError> {
        postcard::to_allocvec(self).map_err(|e| CheckpointError::Corrupt(e.to_string()))
    }

    /// Checks the parts every reader must agree on before using a checkpoint: the format. Shared
    /// by [`Checkpoint::decode`] and [`Checkpoint::to_slot`], so a foreign format is refused
    /// identically whether it arrived as a replay payload or inside the client's envelope.
    pub fn validate(&self) -> Result<(), CheckpointError> {
        if self.format != Self::FORMAT {
            return Err(CheckpointError::Format(self.format));
        }
        Ok(())
    }

    /// Decodes the payload exactly: trailing bytes are refused, so an appended or unknown field
    /// is never silently ignored (#1325), and then checks the format with
    /// [`Checkpoint::validate`]. The bytes a v0 replay kept are a raw [`SaveSlot`], not a
    /// checkpoint: use [`SaveSlot::decode`] for those.
    pub fn decode(bytes: &[u8]) -> Result<Checkpoint, CheckpointError> {
        let (checkpoint, rest) = postcard::take_from_bytes::<Checkpoint>(bytes)
            .map_err(|e| CheckpointError::Corrupt(e.to_string()))?;
        if !rest.is_empty() {
            return Err(CheckpointError::TrailingBytes(rest.len() as u64));
        }
        checkpoint.validate()?;
        Ok(checkpoint)
    }
}

/// The runtime index of a stable room key, or the error when it is gone or not gameplay.
fn resolve_room(def: &GameDef, room: &RoomKey) -> Result<RoomId, CheckpointError> {
    let at = def
        .rooms
        .iter()
        .position(|r| r.name == room.0)
        .ok_or_else(|| CheckpointError::NoRoom(room.0.clone()))?;
    if def.rooms[at].role != RoomRole::Play {
        return Err(CheckpointError::NotPlay(room.0.clone()));
    }
    u32::try_from(at)
        .map(RoomId)
        .map_err(|_| CheckpointError::TooManyRooms)
}

/// Resolves a persisted save-point key against the package data (#1153 E1). A key that no longer
/// names a placed instance — moved, removed, renamed, or malformed — is refused, so the load
/// warns once and starts a new game instead of resuming at a different instance.
fn resolve_point(def: &GameDef, room: RoomId, key: &str) -> Result<(), CheckpointError> {
    let no = || CheckpointError::NoPoint(key.to_owned());
    if let Some(name) = key.strip_prefix("name:") {
        let found = def
            .room(room)
            .is_some_and(|r| r.instances.iter().any(|p| point_name(def, p) == Some(name)));
        return if found { Ok(()) } else { Err(no()) };
    }
    let Some(place) = key.strip_prefix("at:") else {
        return Err(no());
    };
    let Some((place, position)) = place.split_once('@') else {
        return Err(no());
    };
    let Some((room_name, kind_name)) = place.split_once('/') else {
        return Err(no());
    };
    let Some((x, y)) = position.split_once(',') else {
        return Err(no());
    };
    let Ok(x) = u64::from_str_radix(x, 16) else {
        return Err(no());
    };
    let Ok(y) = u64::from_str_radix(y, 16) else {
        return Err(no());
    };
    let found = def.rooms.iter().find(|r| r.name == room_name).is_some_and(|r| {
        r.instances.iter().any(|p| {
            def.kind(p.kind).is_some_and(|k| k.name == kind_name)
                && p.x.get().to_bits() == x
                && p.y.get().to_bits() == y
        })
    });
    if found { Ok(()) } else { Err(no()) }
}

/// The declared name of a placed instance, resolved through the package's name table.
fn point_name<'a>(def: &'a GameDef, p: &crate::def::room::PlacedInstance) -> Option<&'a str> {
    p.name
        .and_then(|n| n.index())
        .and_then(|i| def.names.get(i))
        .map(String::as_str)
}
