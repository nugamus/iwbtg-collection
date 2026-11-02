//! The kid's physics profile, `physics.toml` (docs/plans/sim-core.md 5.1, 5.2).
//!
//! WP0 declares the skeleton: every section is an empty struct. WP5a owns the file and adds the
//! fields of 5.2 and 5.3 (WP5b asks WP5a for its fields in WP5a's sections). The `[ice]`,
//! `[slope]`, `[ladder]` and `[flip]` sections live in `physics_ext.rs` (WP5c). Every struct
//! denies unknown fields, so a typo in a profile is a load error, not a silently ignored key.

use serde::{Deserialize, Deserializer};

use super::class::ClassMask;
use super::ids::SpriteId;
use crate::kid::{KidAction, KidCollisionRun};
use crate::num::Real;

pub use super::physics_ext::{FlipKind, FlipModel, IceRules, LadderRules, SlopeModel};

/// The kid engine family a profile starts from (`preset = "A1"`): documentation and defaults
/// (`kid::presets::apply`). Nothing in the sim branches on it.
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
}

/// A profile as written, before the preset fills its defaults. `toml::from_str` target of
/// `crates/format` (WP7b); `kid::presets::apply` turns it into a [`PhysicsProfile`].
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PhysicsProfileDraft {
    /// `preset = "A1"`.
    pub preset: Option<KidFamily>,
}

macro_rules! profile_section {
    ($($(#[$doc:meta])* $name:ident;)*) => {
        $(
            $(#[$doc])*
            #[derive(Clone, PartialEq, Debug, Default, Deserialize)]
            #[serde(deny_unknown_fields, default)]
            pub struct $name {}
        )*
    };
}

profile_section! {
    /// `[water]` (WP5a).
    WaterRules;
    /// `[death]` (WP5a).
    DeathRules;
    /// `[save]` (WP5a).
    SaveRules;
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
}

/// How the walk action reads the direction keys.
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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VineKeys {
    /// This tick's key press edge (A1, A2, B).
    #[default]
    Edges,
    /// Game-side latched flags (wanna, rori, fairy-tale, showdown; WP5c).
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

/// `[events]`: ordered action lists per event (plan 5.2). Collisions, keys, alarms and the
/// boundary arrive with the collision runs and death (WP5a.3, WP5a.4).
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
}

/// One `events.collisions` row: the kid runs `run` when it collides with an instance of class
/// `with`.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KidCollision {
    /// The handler class (`"block"`, `"platform"`, `"killer"`).
    #[serde(deserialize_with = "class_name")]
    pub with: ClassMask,
    /// The named run.
    pub run: KidCollisionRun,
}

/// What `[jump] air` names as the air-jump model.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AirJump {
    /// The `djump` flag (A1-A5).
    #[default]
    Djump,
    /// The family B counter (WP5b).
    CurJumps,
    /// `djump` then `tjump` (showdown, WP5c).
    DjumpThenTjump,
}

/// When water 2 allows the air jump.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterAir {
    /// Never.
    None,
    /// A water 2 one pixel below allows the air jump even without `djump` (A1).
    #[default]
    Water2WithoutFlag,
    /// Family B (WP5b).
    WaterUnlimited,
    /// Family C (WP5c).
    Water3Keeps,
}

/// An extra ground-jump condition besides the classes.
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
    /// The air-jump model.
    pub air: AirJump,
    /// When water 2 allows the air jump.
    pub water_air: WaterAir,
}

/// What `[fall_cap] rule` does to `vspeed`.
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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GravityReapply {
    /// Every Step, unless the kid is on a ladder (A1: Step action 2.4).
    #[default]
    Step,
    /// Other models (WP5b, WP5c).
    EndStepSlope,
    /// Only at Create.
    CreateOnly,
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
}

/// When a platform collision lands the kid.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformRule {
    /// `y - vspeed / 2 <= platform.y`: the kid was at or above the top half a step ago (A1).
    #[default]
    HalfVspeed,
}

/// What a landing copies from the platform.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformTake {
    /// The platform's `vspeed` (A1).
    #[default]
    Vspeed,
    /// The platform's `y - yprevious` (family B, WP5b).
    DeltaY,
    /// Family C (WP5c).
    YspeedFlipped,
}

/// Vine model.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VineModel {
    /// No vines.
    None,
    /// Left vine below `left_below`, right vine exactly `right_equal` (A, B).
    #[default]
    DistanceAsym,
    /// Both sides within one pixel (C, WP5c).
    DistanceSymEarly,
}

/// `[vine]`.
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct VineRules {
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
}

/// `[misc]`.
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct KidMisc {
    /// The kid starts transparent and fades in (rely: alpha 0, +`fade_step` per Step).
    pub fade_in: bool,
    /// Alpha added per Step while fading in (0.1).
    pub fade_step: Real,
}

/// `[sprites]`: the kid's sprites by role. Package files give names; the loader (WP7b) resolves
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
}

/// `[bullet]`: what `shoot` does.
#[derive(Clone, PartialEq, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct BulletRules {
    /// Shooting needs fewer than this many bullets in the room (4).
    pub max: u32,
    /// Bullet speed; the bullet's `hspeed` is `speed * facing` (16).
    pub speed: Real,
}
