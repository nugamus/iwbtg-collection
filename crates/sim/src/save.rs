//! The save slot (docs/plans/sim-core.md 4.4, N5-N8).
//!
//! The sim never touches files: the save is sim state, the client stores its bytes (postcard).
//! WP0 declares; WP4b owns.
//!
//! Spec: CLONE_SPEC rooms, "Save files at the runner level" and "Saved positions per group".
//! Not here: the file bytes (the client stores the slot), the first-play save (kid Create,
//! WP5a) and the kid's own fields. A save reads the kid through its instance only (position,
//! previous position, class, `image_xscale`). "Flipped" is the class `ClassMask::KID_FLIPPED`
//! (games that flip by object swap); games that flip by a flag are OPEN until `kid::api` has
//! an accessor (follow-up issue).

use serde::{Deserialize, Serialize};

use crate::collide::query::place_free;
use crate::def::{EngineKind, RoomId};
use crate::error::SimError;
use crate::num::{self, Real};
use crate::rules::NumRules;
use crate::world::{Ctx, InstId, SpawnInit};

/// What the game's save file holds.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SaveSlot {
    /// Saved room.
    pub room: RoomId,
    /// Saved position, after [`quantize`].
    pub x: Real,
    /// Saved position, after [`quantize`].
    pub y: Real,
    /// Kid was flipped.
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
    let inst = ctx
        .w
        .get(kid)
        .ok_or_else(|| SimError::invariant(format!("save_now: unknown kid {}", kid.0)))?;
    let (x, y) = (inst.m.x(), inst.m.y());
    let (px, py) = (inst.m.xprevious(), inst.m.yprevious());
    let flipped = crate::kid::api::flipped(ctx.w, kid);
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
    let role = match (slot.flipped, ctx.def.engine_kind(EngineKind::KidFlipped)) {
        (true, Some(_)) => EngineKind::KidFlipped,
        _ => EngineKind::Kid,
    };
    let id = ctx
        .spawn_engine(role, Real::ZERO, Real::ZERO, SpawnInit::default())?
        .ok_or_else(|| SimError::invariant("load_into: the game has no kid kind"))?;
    let dy = if slot.flipped {
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
