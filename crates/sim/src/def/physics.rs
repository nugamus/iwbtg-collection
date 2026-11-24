//! The kid's physics profile, `physics.toml` (docs/architecture/sim/kid.md).
//!
//! Every section is an explicit struct with named fields. The `[ice]`,
//! `[slope]`, `[ladder]` and `[flip]` sections live in `physics_ext.rs`. Every struct
//! denies unknown fields, so a typo in a profile is a load error, not a silently ignored key.

use serde::{Deserialize, Deserializer};

use super::class::ClassMask;
use super::ids::{ScriptId, SoundId, SpriteId};
use crate::input::Action;
use crate::kid::{KidAction, KidCollisionRun};
use crate::num::Real;

pub use super::physics_ext::{
    BlockSlopeStep, Downhill, FlipKind, FlipModel, IceModel, IceRules, LadderEntry, LadderRules,
    MirrorContact, SlopeKind, SlopeModel, SlopeRulesDraft,
};

/// The kid engine family a profile starts from (`preset = "A1"`): documentation and defaults
/// (`kid::presets::apply`). Nothing in the sim branches on it.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
pub enum KidFamily {
    /// kid/A1.md.
    #[default]
    A1,
    /// kid/A2.md.
    A2,
    /// kid/A3.md.
    A3,
    /// kid/A4.md.
    A4,
    /// kid/A5.md.
    A5,
    /// kid/B.md.
    B,
    /// kid/C.md.
    C,
}

/// A full physics profile, after `kid::presets::apply` filled the defaults.
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PhysicsProfile {
    /// Family the defaults came from.
    pub preset: KidFamily,
    /// `[numbers]`.
    pub numbers: KidNumbers,
    /// `[input]`.
    pub input: KidInput,
    /// `[events]`: ordered action lists per event.
    pub events: KidEvents,
    /// `[jump]`.
    pub jump: JumpRules,
    /// `[fall_cap]`.
    pub fall_cap: FallCap,
    /// `[gravity]`.
    pub gravity: GravityRules,
    /// `[platform]`.
    pub platform: PlatformRules,
    /// `[vine]`.
    pub vine: VineRules,
    /// `[water]`.
    pub water: WaterRules,
    /// `[ice]`.
    pub ice: IceRules,
    /// `[slope]`.
    pub slope: SlopeModel,
    /// `[ladder]`.
    pub ladder: LadderRules,
    /// `[flip]`.
    pub flip: FlipModel,
    /// `[death]`.
    pub death: DeathRules,
    /// `[bullet]`.
    pub bullet: BulletRules,
    /// `[save]`.
    pub save: SaveRules,
    /// `[misc]`.
    pub misc: KidMisc,
    /// `[sprites]`: the kid's sprites by role (resolved from names at load).
    pub sprites: KidSprites,
    /// `[sounds]`: the kid's sounds by role (resolved from names at load).
    pub sounds: KidSounds,
    /// `[script]`: the script file whose functions the event lists may run at a slot, with the
    /// custom variable names that script may read and write (decision 0004).
    pub script: KidScript,
}

/// `[script]`: the kid profile's script file and the variables it may use (decision 0004).
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct KidScript {
    /// `file`: `scripts/<id>.rhai`, resolved by the loader to the script's index. `None` when the
    /// profile names no script, in which case no event list may have a script slot.
    pub file: Option<ScriptId>,
    /// `vars`: names the script may read and write as `this.<name>`. Names are package content and
    /// enter `gameplay_hash`; a name not listed here is an undeclared-variable error.
    pub vars: Vec<String>,
}

macro_rules! profile_section {
    ($($(#[$doc:meta])* $name:ident;)*) => {
        $(
            $(#[$doc])*
            #[derive(Clone, PartialEq, Debug, Default, Deserialize)]
            #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
            #[serde(deny_unknown_fields, default)]
            pub struct $name {}
        )*
    };
}

profile_section! {
    /// `[water]`.
    WaterRules;
}

/// A class list written as names (`["block", "solidblock", "water1"]`) into one [`ClassMask`].
/// An unknown name is a load error.
fn class_names<'de, D: Deserializer<'de>>(d: D) -> Result<ClassMask, D::Error> {
    let names = Vec::<String>::deserialize(d)?;
    let mut mask = ClassMask::NONE;
    for n in &names {
        let c = ClassMask::from_name(n)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown class `{n}`")))?;
        mask |= c;
    }
    Ok(mask)
}

/// One class written as a name (`"block"`) into a [`ClassMask`].
fn class_name<'de, D: Deserializer<'de>>(d: D) -> Result<ClassMask, D::Error> {
    let n = String::deserialize(d)?;
    ClassMask::from_name(&n).ok_or_else(|| serde::de::Error::custom(format!("unknown class `{n}`")))
}

/// `[numbers]`: the kid's constants (A1 Create, "Numeric constants"). A preset fills them
/// (`kid::presets::apply`); `create_defaults` copies them into the kid.
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct KidNumbers {
    /// Ground jump speed, then the air jumps in order (positive magnitudes; A1 `[8.5, 7.0]`).
    pub jumps: Vec<Real>,
    /// Gravity the kid runs at (0.4).
    pub gravity: Real,
    /// Run speed, `maxSpeed` (3).
    pub run_speed: Real,
    /// Speed clamp, `maxVspeed` (9).
    pub max_vspeed: Real,
    /// `maxFallSpeed`: read only by `fall_cap = "shift_then_clamp"` (A4).
    pub max_fall_speed: Real,
    /// The jump release multiplier (0.45).
    pub release_mult: Real,
    /// Vertical speed beyond which the jump or fall sprite shows (0.05).
    pub sprite_threshold: Real,
    /// `image_speed` while standing, at Create and in the air (0.2).
    pub image_speed_idle: Real,
    /// `image_speed` while running and sliding on a vine (0.5).
    pub image_speed_run: Real,
    /// With a positive value (family B: 2), `air_sprite` shows the fall sprite only when no
    /// `slope_wall` instance lies this many pixels in the gravity direction. 0 turns it off.
    pub air_wall_probe: Real,
}

/// How the walk action reads the direction keys.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WalkRead {
    /// `keyboard_check_direct`: held, or latched since the last direct read
    /// (`InputState::direct`; trick.restart_glitch).
    #[default]
    DirectLatch,
    /// The tick's sampled held state.
    Sampled,
}

/// How the vine action reads its kick and drop keys.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VineKeys {
    /// This tick's key press edge (A1, A2, B).
    #[default]
    Edges,
    /// Game-side latched flags (wanna, rori, fairy-tale and showdown).
    LatchedFlags,
}

/// `[input]`: how the kid reads keys.
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct KidInput {
    /// `walk = "direct_latch"` or `"sampled"`.
    pub walk: WalkRead,
    /// `vine_keys = "edges"` or `"latched_flags"`.
    pub vine_keys: VineKeys,
}

/// `[events]`: ordered action lists per event (docs/architecture/sim/kid.md). Collisions, keys, alarms and the
/// boundary arrive with the collision runs and death.
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct KidEvents {
    /// Create.
    pub create: Vec<KidAction>,
    /// Room Start.
    pub room_start: Vec<KidAction>,
    /// Step.
    pub step: Vec<KidAction>,
    /// End Step.
    pub end_step: Vec<KidAction>,
    /// Collision handlers, in handler order (`{ with = "block", run = "block_standard" }`).
    pub collisions: Vec<KidCollision>,
    /// Alarm handlers (`{ n = 0, run = ["play_timer"] }`).
    pub alarms: Vec<KidAlarm>,
    /// Key handlers (`{ action = "suicide", on = "press", run = ["kill"] }`).
    pub keys: Vec<KidKey>,
    /// What happens at the room edge.
    pub boundary: BoundaryModel,
}

/// `events.boundary`: how the kid handles the room edge (docs/architecture/sim/kid.md edge row).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryModel {
    /// Nothing: the kid is not told about the edge.
    #[default]
    None,
    /// Intersect Boundary tests the origin: outside the room (strictly) kills the kid unless an
    /// `edge_warp` overlaps it, which warps it; then the kid wraps to the opposite edge (A1).
    IntersectOrigin,
    /// Like `intersect_origin` with machinary's fixed numbers (A4): the edges are 800 by 608
    /// whatever the room, the wrap adds or subtracts 800 sideways, adds 608 at the top and
    /// subtracts 600 (not 608) at the bottom.
    IntersectOriginFixed,
    /// Outside Room kills the kid unless a room changer overlaps (family B).
    OutsideRoomKill,
    /// The origin test runs in End Step.
    EndStepOrigin,
}

/// One `events.alarms` row.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KidAlarm {
    /// The alarm number (0-11).
    pub n: u8,
    /// The actions to run.
    pub run: Vec<KidAction>,
}

/// Which edge of a key a row reacts to.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyOn {
    /// Keyboard (held).
    Held,
    /// Key press.
    Press,
    /// Key release.
    Release,
}

/// One `events.keys` row.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KidKey {
    /// The client action by its package name (`"suicide"`).
    #[serde(deserialize_with = "action_name")]
    #[cfg_attr(feature = "schema", schemars(with = "Action"))]
    pub action: Action,
    /// The edge.
    pub on: KeyOn,
    /// The actions to run.
    pub run: Vec<KidAction>,
}

/// An action written as its package name.
fn action_name<'de, D: Deserializer<'de>>(d: D) -> Result<Action, D::Error> {
    let n = String::deserialize(d)?;
    Action::from_name(&n).ok_or_else(|| serde::de::Error::custom(format!("unknown action `{n}`")))
}

/// One script slot in a kid event list (`{ do = "script", fn = "dash" }`, decision 0004). The
/// function is looked up in the profile's `[script] file`: no argument in a list event, `other`
/// in a collision slot, with `this` bound to the kid.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KidScriptSlot {
    /// Must be `"script"`.
    #[serde(rename = "do")]
    pub action: ScriptSlotDo,
    /// The function of the profile's `[script] file`.
    #[serde(rename = "fn")]
    pub function: String,
}

/// The only value of a kid slot's `do`: `"script"`. A named table keeps a typo from being read as
/// some other action.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptSlotDo {
    /// A script function.
    Script,
}

/// One `events.collisions` row: the kid runs `run` when it collides with an instance of class
/// `with`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KidCollision {
    /// The handler class (`"block"`, `"platform"`, `"killer"`).
    #[serde(deserialize_with = "class_name")]
    #[cfg_attr(feature = "schema", schemars(with = "ClassMask"))]
    pub with: ClassMask,
    /// The named run.
    pub run: KidCollisionRun,
}

/// What `[jump] air` names as the air-jump model.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AirJump {
    /// The `djump` flag (A1-A5).
    #[default]
    Djump,
    /// The family B counter.
    CurJumps,
    /// `djump` then `tjump` (showdown).
    DjumpThenTjump,
}

/// When water 2 allows the air jump.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterAir {
    /// Never.
    None,
    /// A water 2 one pixel below allows the air jump even without `djump` (A1).
    #[default]
    Water2WithoutFlag,
    /// Family B.
    WaterUnlimited,
    /// Family C.
    Water3Keeps,
}

/// `[water] model`: the standard name for the water air-jump rule, the same value as
/// `[jump] water_air` (docs/format/physics.md "Standard names and aliases").
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterModel {
    /// Water never allows the air jump.
    #[default]
    None,
    /// Water 2 one pixel below allows the air jump even without `djump` (A1).
    #[serde(alias = "water2_without_flag")]
    JumpRefresh,
    /// Water allows any number of air jumps (family B).
    #[serde(alias = "water_unlimited")]
    FullJump,
    /// Water 2 and 3 allow the air jump and water 3 keeps `djump` (family C).
    Water3Keeps,
}

impl WaterModel {
    /// The `[jump] water_air` value this model names.
    pub fn water_air(self) -> WaterAir {
        match self {
            WaterModel::None => WaterAir::None,
            WaterModel::JumpRefresh => WaterAir::Water2WithoutFlag,
            WaterModel::FullJump => WaterAir::WaterUnlimited,
            WaterModel::Water3Keeps => WaterAir::Water3Keeps,
        }
    }
}

/// An extra ground-jump condition besides the classes.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundAlso {
    /// `onPlatform` counts as ground.
    PlatformFlag,
    /// `ladderjump` counts as ground.
    LadderJump,
}

/// `[jump]`.
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct JumpRules {
    /// Classes that count as ground one pixel in the gravity direction
    /// (`["block", "solidblock", "water1"]`).
    #[serde(deserialize_with = "class_names")]
    pub ground_classes: ClassMask,
    /// Extra ground conditions (`["platform_flag", "ladder_jump"]`).
    pub ground_also: Vec<GroundAlso>,
    /// A ground jump sets `djump` (false in A4).
    pub ground_sets_djump: bool,
    /// `air = "djump_then_tjump"` (showdown): `release` cuts a second time while `tjump` is set
    /// (`playerTJump`: `vspeed * release_mult` again when `tjump` and moving against gravity).
    pub release_twice_while_tjump: bool,
    /// The air-jump model.
    pub air: AirJump,
    /// When water 2 allows the air jump.
    pub water_air: WaterAir,
    /// `cur_jumps`: the last numbered jump (`maxJumps`, B: 2).
    pub max_jumps: u8,
    /// `cur_jumps`: `infJump` starts on (B: false).
    pub inf_jump: bool,
    /// A Jump press does nothing on a tick when Pause was pressed (kaenbyou).
    pub blocked_by_pause_press: bool,
    /// How the jump release is read.
    pub cancel: JumpCancel,
}

/// How the jump cut works (`[jump] cancel`).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JumpCancel {
    /// The `release` action reads the release edge (every game with the cut switched on).
    #[default]
    Edge,
    /// B's switch for jump cancelling off: a first jump arms Alarm 0 each tick and the
    /// `cancel_poll` action cuts the jump when the key is up. The `release` action does nothing.
    PollAlarm,
}

/// What `[fall_cap] rule` does to `vspeed`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallCapRule {
    /// `abs(vspeed) > max_vspeed` gives `sign(vspeed) * max_vspeed` (A1).
    #[default]
    ClampAbs,
    /// Clamp against gravity only (B).
    ClampAgainstGravity,
    /// Clamp downward only (spook).
    ClampDownOnly,
    /// Shift, then clamp (A4).
    ShiftThenClamp,
}

/// `[fall_cap]`.
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FallCap {
    /// The rule.
    pub rule: FallCapRule,
}

/// When gravity is set back to its constant.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GravityReapply {
    /// Every Step, unless the kid is on a ladder (A1: Step action 2.4).
    #[default]
    Step,
    /// Other models.
    EndStepSlope,
    /// Only at Create.
    CreateOnly,
}

/// `[gravity] model`: the standard name for the gravity reapply rule, the same value as
/// `[gravity] reapply`. Gravity's orientation when it flips is `[flip] model`, not this.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GravityModel {
    /// Gravity is the constant `numbers.gravity`, reapplied every Step (A1).
    #[default]
    Constant,
    /// Same as `constant`.
    Step,
    /// Gravity is set by the End Step slope code (family B).
    EndStepSlope,
    /// Gravity is set only at Create (A4, A5, C).
    CreateOnly,
}

impl GravityModel {
    /// The `[gravity] reapply` value this model names.
    pub fn reapply(self) -> GravityReapply {
        match self {
            GravityModel::Constant | GravityModel::Step => GravityReapply::Step,
            GravityModel::EndStepSlope => GravityReapply::EndStepSlope,
            GravityModel::CreateOnly => GravityReapply::CreateOnly,
        }
    }
}

/// `[gravity]`.
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct GravityRules {
    /// When gravity is reapplied.
    pub reapply: GravityReapply,
    /// Landing on a block (`block_standard`) sets gravity to 0 (A1; the Step sets it back).
    pub block_landing_zeroes: bool,
}

/// `[platform]`.
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PlatformRules {
    /// Pixels below the kid probed for a platform to keep `onPlatform` (4).
    pub hold_probe: Real,
    /// When the kid lands on a platform.
    pub rule: PlatformRule,
    /// Pixels above the platform's `y` the kid is put at (9).
    pub snap: Real,
    /// What the kid takes from the platform's motion.
    pub take: PlatformTake,
    /// The reversed kid (`flip.model = "yflag"`, `yflag` -1) lands only when `y - vspeed / 2` is
    /// at least this many pixels below the platform's `y` (family B 15).
    pub reversed_above: Real,
    /// ... and is then put this many pixels below the platform's `y` (family B 24).
    pub reversed_snap: Real,
}

/// When a platform collision lands the kid.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformRule {
    /// `y - vspeed / 2 <= platform.y`: the kid was at or above the top half a step ago (A1).
    #[default]
    HalfVspeed,
}

/// What a landing copies from the platform.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformTake {
    /// The platform's `vspeed` (A1).
    #[default]
    Vspeed,
    /// The platform's `y - yprevious` (family B).
    DeltaY,
    /// Family C.
    YspeedFlipped,
}

/// Vine model.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VineModel {
    /// No vines.
    None,
    /// Left vine below `left_below`, right vine exactly `right_equal` (A, B).
    #[default]
    DistanceAsym,
    /// Both sides within one pixel (C).
    DistanceSymEarly,
}

/// `[vine] model` as written: the standard model names, with `latched_keys` also selecting the
/// key-reading model (`[input] vine_keys = "latched_flags"`).
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VineMode {
    /// No vines.
    #[default]
    None,
    /// Left vine below `left_below`, right vine exactly `right_equal` (A, B).
    DistanceAsym,
    /// The game's latched key flags drive the vine keys (A3); the distance rule is asymmetric.
    LatchedKeys,
    /// Both sides within one pixel (C).
    #[serde(alias = "distance_sym_early")]
    SymmetricEarly,
}

impl VineMode {
    /// The distance model this mode names.
    pub fn vine_model(self) -> VineModel {
        match self {
            VineMode::None => VineModel::None,
            VineMode::DistanceAsym | VineMode::LatchedKeys => VineModel::DistanceAsym,
            VineMode::SymmetricEarly => VineModel::DistanceSymEarly,
        }
    }

    /// True when the mode reads the game's latched key flags (A3).
    pub fn latches_keys(self) -> bool {
        matches!(self, VineMode::LatchedKeys)
    }
}

/// `[vine]`.
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct VineRules {
    /// A3 (wanna, showdown): `walk` ignores vines (no facing or movement rule near them); the vine
    /// action alone sets `hspeed`.
    pub walk_ignores_vines: bool,
    /// The model.
    pub model: VineModel,
    /// Left vine: grabs when the distance is below this (2).
    pub left_below: Real,
    /// Right vine: grabs when the distance equals this (1).
    pub right_equal: Real,
    /// Vertical speed while sliding (2).
    pub slide: Real,
    /// Kick: horizontal speed and vertical speed (15, -9).
    pub kick: [Real; 2],
    /// Drop: horizontal speed away from the vine (3).
    pub drop: Real,
    /// The vine tests look one pixel down (`place_free(x, y + 1)`) whatever the gravity, as
    /// Kotaro's flipped player2 does (kid/A2.md); otherwise they look in the gravity direction.
    pub probe_down_fixed: bool,
}

/// `[misc]`.
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct KidMisc {
    /// The kid starts transparent and fades in (rely: alpha 0, +`fade_step` per Step).
    pub fade_in: bool,
    /// Alpha added per Step while fading in (0.1).
    pub fade_step: Real,
    /// `walk` shows the idle sprite only with a `jump.ground_classes` instance 1 px below
    /// (spook house).
    pub idle_needs_block: bool,
}

/// `[sounds]`: the kid's sounds by role, played through `sound::play`. Package files give names;
/// the loader resolves them to ids. A role the game does not have stays `None` and the action
/// plays nothing.
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct KidSounds {
    /// A ground jump (`playerJump`).
    pub jump: Option<SoundId>,
    /// An air jump.
    pub double_jump: Option<SoundId>,
    /// A shot.
    pub shoot: Option<SoundId>,
    /// A vine kick.
    pub vine_kick: Option<SoundId>,
    /// `killPlayer`'s `sound` step.
    pub death: Option<SoundId>,
}

/// `[sprites]`: the kid's sprites by role. Package files give names; the loader resolves
/// them to ids. A role the game does not have stays `None` and the action leaves the sprite
/// alone.
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct KidSprites {
    /// Standing.
    pub idle: Option<SpriteId>,
    /// Running.
    pub running: Option<SpriteId>,
    /// Rising or double jumping.
    pub jump: Option<SpriteId>,
    /// Falling.
    pub fall: Option<SpriteId>,
    /// Sliding on a vine.
    pub sliding: Option<SpriteId>,
    /// On a ladder (`sprPlayerBack`).
    pub back: Option<SpriteId>,
    /// Moving on a ladder (`sprPlayerLadderUpDown`).
    pub ladder_up_down: Option<SpriteId>,
}

/// `[bullet]`: what `shoot` does.
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct BulletRules {
    /// Shooting needs fewer than this many bullets in the room (4).
    pub max: u32,
    /// Bullet speed; the bullet's `hspeed` is `speed * facing` (16).
    pub speed: Real,
    /// Where the bullet appears, relative to the kid: `x + offset[0] * facing`, `y - offset[1] *
    /// yflag` (family B `[2, 2]`; A1 `[0, 0]`).
    pub offset: [Real; 2],
}

/// One step of the death sequence (kid/A1.md "Death"). `[death] order` lists them in the order a
/// family runs them.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeathStep {
    /// The death sound, played by the death step `sound`.
    Sound,
    /// Family B: creates the `death_sound` engine object (music fade, death sound a tick later).
    DeathSound,
    /// A blood emitter at the kid's position.
    BloodEmitter,
    /// Destroy the kid.
    DestroyKid,
    /// Destroy every kid instance (family B: "destroy every player instance").
    DestroyAllKids,
    /// `global.death` plus 1 (checked).
    DeathsPlusOne,
    /// The death-only save (`saveDeathTime`): counters into the save slot.
    SaveCounters,
    /// The GAMEOVER display. It places itself at view 0 plus its own `offset_x`/`offset_y`
    /// params in its Create (`game_over` built-in), so the offset is not a kid field.
    GameOver,
}

/// `[death]`: what `kid::api::kill` does.
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct DeathRules {
    /// The steps, in order. A1: sound, blood emitter, destroy kid, GAMEOVER, deaths + 1,
    /// death-only save.
    pub order: Vec<DeathStep>,
    /// In a `Select` room a kill goes to the same room again and destroys the kid, and nothing
    /// else happens (rely's special case).
    pub select_room_reset: bool,
    /// Death stops the music. Not built; `true` is `Unsupported`.
    pub stop_music: bool,
    /// The blood emitter and GAMEOVER use the first kid's position and gravity instead of the
    /// killed kid's (family B with several kids, R21).
    pub first_kid: bool,
    /// `killer_check` does nothing in Boot, Title and Menu rooms, and in a Select room it moves
    /// the kid to `xstart`, `ystart` instead of killing it (A3 wanna End Step).
    pub killer_check_menu_rooms: bool,
}

/// How a save stores the position (`save.rs` `SavedPosition`), by its package name.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SavePosition {
    /// Base-100 bytes (13 games).
    #[default]
    XyBase100Bytes,
    /// Base-256 bytes (little-runner).
    XyBase256Bytes,
    /// Text with two decimals (machinary, eat-the-new-year).
    XyText2Dec,
    /// `xprevious` and `yprevious` (kaenbyou, eat-the-lemon).
    XyPrevious,
    /// A fixed point (poronkusema); needs `save.fixed`.
    FixedPoint,
    /// Floor with the `place_free` nudge (xia).
    FloorNudge,
}

impl SavePosition {
    /// The save module's model.
    pub fn model(self) -> crate::save::SavedPosition {
        use crate::save::SavedPosition as S;
        match self {
            SavePosition::XyBase100Bytes => S::XyBase100Bytes,
            SavePosition::XyBase256Bytes => S::XyBase256Bytes,
            SavePosition::XyText2Dec => S::XyText2Dec,
            SavePosition::XyPrevious => S::XyPrevious,
            SavePosition::FixedPoint => S::FixedPoint,
            SavePosition::FloorNudge => S::FloorNudge,
        }
    }
}

/// `[save]`: the kid's first-play save.
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SaveRules {
    /// The position model.
    pub position: SavePosition,
    /// The fixed point for `fixed_point`.
    pub fixed: Option<[Real; 2]>,
    /// A save stores the first kid whichever kid touched the save point (family B, R21).
    pub first_kid: bool,
}

// ---- The draft: a profile as written, before its preset fills the defaults ----

/// Generates `$draft`, the as-written form of `$full`: every field optional, and `over` puts the
/// fields that were written over a base (the preset's) section.
macro_rules! draft_section {
    ($(#[$doc:meta])* $draft:ident => $full:ident { $( $(#[$m:meta])* $f:ident : $t:ty ),* $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Debug, Default, Deserialize)]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
        #[serde(deny_unknown_fields, default)]
        pub struct $draft {
            $( $(#[$m])* pub $f: Option<$t>, )*
        }

        impl $draft {
            /// The base with every field written here replacing the base's.
            pub fn over(self, mut base: $full) -> $full {
                $( if let Some(v) = self.$f { base.$f = v; } )*
                base
            }
        }
    };
}

/// Like [`class_names`], for an optional field.
fn class_names_opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<ClassMask>, D::Error> {
    let names = Vec::<String>::deserialize(d)?;
    let mut mask = ClassMask::NONE;
    for n in &names {
        let c = ClassMask::from_name(n)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown class `{n}`")))?;
        mask |= c;
    }
    Ok(Some(mask))
}

draft_section! {
    /// `[numbers]` as written.
    KidNumbersDraft => KidNumbers {
        jumps: Vec<Real>, gravity: Real, run_speed: Real, max_vspeed: Real,
        #[serde(alias = "max_fall")]
        max_fall_speed: Real,
        release_mult: Real, sprite_threshold: Real, image_speed_idle: Real, image_speed_run: Real,
        air_wall_probe: Real,
    }
}
draft_section! {
    /// `[input]` as written.
    KidInputDraft => KidInput { walk: WalkRead, vine_keys: VineKeys }
}
draft_section! {
    /// `[events]` as written.
    KidEventsDraft => KidEvents {
        create: Vec<KidAction>, room_start: Vec<KidAction>, step: Vec<KidAction>,
        end_step: Vec<KidAction>, collisions: Vec<KidCollision>, alarms: Vec<KidAlarm>,
        keys: Vec<KidKey>, boundary: BoundaryModel,
    }
}
draft_section! {
    /// `[jump]` as written.
    JumpRulesDraft => JumpRules {
        #[serde(deserialize_with = "class_names_opt")]
        #[cfg_attr(feature = "schema", schemars(with = "Vec<ClassMask>"))]
        ground_classes: ClassMask,
        ground_also: Vec<GroundAlso>, ground_sets_djump: bool, release_twice_while_tjump: bool,
        air: AirJump, water_air: WaterAir,
        max_jumps: u8, inf_jump: bool, blocked_by_pause_press: bool, cancel: JumpCancel,
    }
}
draft_section! {
    /// `[fall_cap]` as written.
    FallCapDraft => FallCap { rule: FallCapRule }
}
/// `[gravity]` as written. `model` is the standard spelling of `reapply`; a written `reapply`
/// wins over it.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct GravityRulesDraft {
    /// `reapply = "step"`.
    pub reapply: Option<GravityReapply>,
    /// `model = "constant" | "step" | "end_step_slope" | "create_only"`.
    pub model: Option<GravityModel>,
    /// `block_landing_zeroes = true`.
    pub block_landing_zeroes: Option<bool>,
}

impl GravityRulesDraft {
    /// The base with every field written here replacing the base's. `model` applies before an
    /// explicit `reapply`, so `reapply` wins when both are written.
    pub fn over(self, mut base: GravityRules) -> GravityRules {
        if let Some(model) = self.model {
            base.reapply = model.reapply();
        }
        if let Some(reapply) = self.reapply {
            base.reapply = reapply;
        }
        if let Some(zeroes) = self.block_landing_zeroes {
            base.block_landing_zeroes = zeroes;
        }
        base
    }
}

/// `[water]` as written. Its only standard key is `model`, which names the same value as
/// `[jump] water_air`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct WaterRulesDraft {
    /// `model = "none" | "jump_refresh" | "full_jump" | "water3_keeps"`.
    pub model: Option<WaterModel>,
}

draft_section! {
    /// `[platform]` as written.
    PlatformRulesDraft => PlatformRules {
        hold_probe: Real, rule: PlatformRule, snap: Real, take: PlatformTake, reversed_above: Real,
        reversed_snap: Real,
    }
}
/// `[vine]` as written. `model` is the standard spelling of the vine model; `latched_keys` also
/// selects the key-reading model, which `apply` writes into `[input] vine_keys`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct VineRulesDraft {
    /// `walk_ignores_vines`.
    pub walk_ignores_vines: Option<bool>,
    /// `model = "none" | "distance_asym" | "latched_keys" | "symmetric_early"`.
    pub model: Option<VineMode>,
    /// `left_below`.
    pub left_below: Option<Real>,
    /// `right_equal`.
    pub right_equal: Option<Real>,
    /// `slide`.
    pub slide: Option<Real>,
    /// `kick`.
    pub kick: Option<[Real; 2]>,
    /// `drop`.
    pub drop: Option<Real>,
    /// `probe_down_fixed`.
    pub probe_down_fixed: Option<bool>,
}

impl VineRulesDraft {
    /// The base with every field written here replacing the base's.
    pub fn over(self, mut base: VineRules) -> VineRules {
        if let Some(walk) = self.walk_ignores_vines {
            base.walk_ignores_vines = walk;
        }
        if let Some(model) = self.model {
            base.model = model.vine_model();
        }
        if let Some(v) = self.left_below {
            base.left_below = v;
        }
        if let Some(v) = self.right_equal {
            base.right_equal = v;
        }
        if let Some(v) = self.slide {
            base.slide = v;
        }
        if let Some(v) = self.kick {
            base.kick = v;
        }
        if let Some(v) = self.drop {
            base.drop = v;
        }
        if let Some(v) = self.probe_down_fixed {
            base.probe_down_fixed = v;
        }
        base
    }
}
draft_section! {
    /// `[death]` as written.
    DeathRulesDraft => DeathRules {
        order: Vec<DeathStep>, select_room_reset: bool,
        stop_music: bool, first_kid: bool, killer_check_menu_rooms: bool,
    }
}
draft_section! {
    /// `[bullet]` as written.
    BulletRulesDraft => BulletRules { max: u32, speed: Real, offset: [Real; 2] }
}
draft_section! {
    /// `[save]` as written.
    SaveRulesDraft => SaveRules { position: SavePosition, fixed: Option<[Real; 2]>, first_kid: bool }
}
draft_section! {
    /// `[misc]` as written.
    KidMiscDraft => KidMisc { fade_in: bool, fade_step: Real, idle_needs_block: bool }
}

/// `[script]` as written: the loader resolves `file` to a [`ScriptId`] and interns `vars`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct KidScriptDraft {
    /// `file = "<id>"`: `scripts/<id>.rhai`.
    pub file: Option<String>,
    /// `vars = ["dash_dir"]`.
    pub vars: Option<Vec<String>>,
}

/// A profile as written, before the preset fills its defaults. `toml::from_str` target of
/// `crates/format`; `kid::presets::apply` turns it into a [`PhysicsProfile`]: the
/// preset's profile with every field written here put over it. The sections of families that
/// have not landed (`[ice]`, `[slope]`, `[ladder]`, `[flip]`) replace the preset's whole.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PhysicsProfileDraft {
    /// `preset = "A1"`.
    pub preset: Option<KidFamily>,
    /// `[numbers]`.
    pub numbers: Option<KidNumbersDraft>,
    /// `[input]`.
    pub input: Option<KidInputDraft>,
    /// `[events]`.
    pub events: Option<KidEventsDraft>,
    /// `[jump]`.
    pub jump: Option<JumpRulesDraft>,
    /// `[fall_cap]`.
    pub fall_cap: Option<FallCapDraft>,
    /// `[gravity]`.
    pub gravity: Option<GravityRulesDraft>,
    /// `[platform]` (standard alias `[platforms]`).
    #[serde(alias = "platforms")]
    pub platform: Option<PlatformRulesDraft>,
    /// `[vine]` (standard alias `[vines]`).
    #[serde(alias = "vines")]
    pub vine: Option<VineRulesDraft>,
    /// `[water]`.
    pub water: Option<WaterRulesDraft>,
    /// `[ice]`.
    pub ice: Option<IceRules>,
    /// `[slope]` (standard alias `[slopes]`).
    #[serde(alias = "slopes")]
    pub slope: Option<SlopeRulesDraft>,
    /// `[ladder]` (standard alias `[ladders]`).
    #[serde(alias = "ladders")]
    pub ladder: Option<LadderRules>,
    /// `[flip]`.
    pub flip: Option<FlipModel>,
    /// `[death]`.
    pub death: Option<DeathRulesDraft>,
    /// `[bullet]`.
    pub bullet: Option<BulletRulesDraft>,
    /// `[save]`.
    pub save: Option<SaveRulesDraft>,
    /// `[misc]`.
    pub misc: Option<KidMiscDraft>,
    /// `[sprites]`: sprite names. `kid::presets::apply` leaves the profile's `sprites` empty;
    /// the loader resolves these names to ids and fills them in.
    pub sprites: Option<KidSpriteNames>,
    /// `[sounds]`: sound names; the loader resolves them to ids like the sprite names.
    pub sounds: Option<KidSoundNames>,
    /// `[script]`: the slot script file and its custom variables (decision 0004).
    pub script: Option<KidScriptDraft>,
}

/// `[sounds]` as written in a package: sound names by role, resolved to [`KidSounds`] ids by the
/// loader.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct KidSoundNames {
    /// A ground jump.
    pub jump: Option<String>,
    /// An air jump.
    pub double_jump: Option<String>,
    /// A shot.
    pub shoot: Option<String>,
    /// A vine kick.
    pub vine_kick: Option<String>,
    /// The death sound.
    pub death: Option<String>,
}

/// `[sprites]` as written in a package: sprite names by role, resolved to [`KidSprites`] ids by
/// the loader.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Clone, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct KidSpriteNames {
    /// Standing.
    pub idle: Option<String>,
    /// Running.
    pub running: Option<String>,
    /// Rising or double jumping.
    pub jump: Option<String>,
    /// Falling.
    pub fall: Option<String>,
    /// Sliding on a vine.
    pub sliding: Option<String>,
    /// On a ladder.
    pub back: Option<String>,
    /// Moving on a ladder.
    pub ladder_up_down: Option<String>,
}
