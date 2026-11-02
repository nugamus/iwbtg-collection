//! Built-in kinds other than the kid (docs/plans/sim-core.md 6).
//!
//! One file per built-in, documented in `docs/format/builtins/<kind>.md`. Each file has the same
//! four entry points (`param_specs`, `event_set`, `init_state`, `on_event`), and WP0 wrote the
//! routing to all of them here, so WP6b and WP6c never edit this file. WP6a owns this file and
//! `state.rs` (WP6b and WP6c add one variant each there and own their built-ins' files).

pub mod state;

// Tier 1 (WP6a).
pub mod block;
pub mod blood;
pub mod blood_emitter;
pub mod bow;
pub mod camera;
pub mod difficulty_warp;
pub mod game_over;
pub mod kid_bullet;
pub mod kid_start;
pub mod killer;
pub mod platform;
pub mod save;
pub mod spike;
pub mod warp;
pub mod world;

// Tier 2 (WP6b).
pub mod boss_gate;
pub mod boss_item;
pub mod bullet_blocker;
pub mod drop_platform;
pub mod edge_warp;
pub mod fake_block;
pub mod game_clear;
pub mod gravity_flip;
pub mod hidden_block;
pub mod item_viewer;
pub mod moving_platform;
pub mod music;
pub mod playerface;
pub mod secret_item;
pub mod slope;
pub mod trap;
pub mod trigger;
pub mod vine;
pub mod water;

// Tier 3 (WP6c).
pub mod breakable_block;
pub mod conveyor;
pub mod decor;
pub mod jump_refresher;
pub mod kid_param_zone;
pub mod ladder;
pub mod orbit;
pub mod path_platform;
pub mod scripted;
pub mod sign;
pub mod spring;
pub mod time_limit;
pub mod trap_stopper;

use serde::{Deserialize, Serialize};

pub use state::BuiltinState;

use crate::def::{Builtin, KindDef, KindId, ParamValue, Params};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::num::Real;
use crate::world::{Ctx, Flow, InstId, KindState};

/// Mechanism globals: trigger models, boss and item flags. WP6a adds the fields; WP4b added the
/// two flag words the save slot holds.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct MechGlobals {
    /// Boss flags, one bit per boss (`SaveSlot::bosses`).
    pub bosses: u16,
    /// Item flags, one bit per item (`SaveSlot::items`).
    pub items: u16,
}

/// Built-in effects for the client. Visual only: the sim never reads them back.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum BuiltinOutput {
    /// A `blood_emitter` in `particles` mode made one Step's particle stream (family B death).
    DeathParticles {
        /// Emitter x.
        x: Real,
        /// Emitter y.
        y: Real,
        /// Particles this Step.
        count: u32,
        /// The kid was flipped: the particles fall up.
        flipped: bool,
    },
}

/// One parameter a built-in accepts; `crates/format` validates package params against these.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ParamSpec {
    /// Param name in the package.
    pub name: &'static str,
    /// Its type.
    pub ty: ParamType,
    /// Must be given (no default).
    pub required: bool,
}

/// Type of a built-in parameter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParamType {
    /// A real.
    Real,
    /// A flag.
    Bool,
    /// One of these words.
    Enum(&'static [&'static str]),
    /// A room id.
    Room,
    /// A kind id.
    Kind,
    /// A sound id.
    Sound,
}

/// A param set a built-in cannot use.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
#[error("param {param}: {message}")]
pub struct ParamError {
    /// The param name.
    pub param: String,
    /// What is wrong.
    pub message: String,
}

/// A real param, `None` when absent. A param of another type is a [`ParamError`].
pub(crate) fn param_real(p: &Params, name: &str) -> Result<Option<Real>, ParamError> {
    match p.0.get(name) {
        None => Ok(None),
        Some(ParamValue::Real(r)) if r.is_finite() => Ok(Some(*r)),
        Some(_) => Err(param_error(name, "must be a finite number")),
    }
}

/// A flag param, `None` when absent.
pub(crate) fn param_bool(p: &Params, name: &str) -> Result<Option<bool>, ParamError> {
    match p.0.get(name) {
        None => Ok(None),
        Some(ParamValue::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(param_error(name, "must be true or false")),
    }
}

/// An enum param: one of `allowed`, `None` when absent.
pub(crate) fn param_enum<'a>(
    p: &'a Params,
    name: &str,
    allowed: &[&str],
) -> Result<Option<&'a str>, ParamError> {
    match p.0.get(name) {
        None => Ok(None),
        Some(ParamValue::Str(s)) if allowed.contains(&s.as_str()) => Ok(Some(s.as_str())),
        Some(_) => Err(param_error(
            name,
            &format!("must be one of {}", allowed.join(", ")),
        )),
    }
}

/// A kind param, `None` when absent.
pub(crate) fn param_kind(p: &Params, name: &str) -> Result<Option<KindId>, ParamError> {
    match p.0.get(name) {
        None => Ok(None),
        Some(ParamValue::Kind(k)) => Ok(Some(*k)),
        Some(_) => Err(param_error(name, "must name a kind")),
    }
}

/// A [`ParamError`].
pub(crate) fn param_error(name: &str, message: &str) -> ParamError {
    ParamError {
        param: name.to_owned(),
        message: message.to_owned(),
    }
}

/// The built-in state of instance `me`, if it has one.
pub(crate) fn state_of<'a>(ctx: &'a Ctx<'_>, me: InstId) -> Option<&'a BuiltinState> {
    match &ctx.w.get(me)?.state {
        KindState::Builtin(s) => Some(s),
        _ => None,
    }
}

/// The built-in state of instance `me`, for changing it.
pub(crate) fn state_mut<'a>(ctx: &'a mut Ctx<'_>, me: InstId) -> Option<&'a mut BuiltinState> {
    match &mut ctx.w.get_mut(me)?.state {
        KindState::Builtin(s) => Some(s),
        _ => None,
    }
}

/// The base built-in of instance `me`'s kind. An unknown instance or kind is an invariant break.
fn base_of(ctx: &Ctx, me: InstId) -> Result<Builtin, SimError> {
    let inst = ctx
        .w
        .get(me)
        .ok_or_else(|| SimError::invariant(format!("event for unknown instance {}", me.0)))?;
    let kind = ctx
        .def
        .kind(inst.kind)
        .ok_or_else(|| SimError::invariant(format!("instance {} has unknown kind {}", me.0, inst.kind.0)))?;
    Ok(kind.base)
}

/// Runs one event on a built-in instance, in the built-in's own file (`match` on [`Builtin`]).
/// WP0 wrote every arm, so WP6b and WP6c only fill their files.
pub fn on_event(ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, SimError> {
    match base_of(ctx, me)? {
        Builtin::World => world::on_event(ctx, me, ev),
        Builtin::KidStart => kid_start::on_event(ctx, me, ev),
        Builtin::Block => block::on_event(ctx, me, ev),
        Builtin::Spike => spike::on_event(ctx, me, ev),
        Builtin::Killer => killer::on_event(ctx, me, ev),
        Builtin::KidBullet => kid_bullet::on_event(ctx, me, ev),
        Builtin::Blood => blood::on_event(ctx, me, ev),
        Builtin::BloodEmitter => blood_emitter::on_event(ctx, me, ev),
        Builtin::GameOver => game_over::on_event(ctx, me, ev),
        Builtin::Bow => bow::on_event(ctx, me, ev),
        Builtin::Save => save::on_event(ctx, me, ev),
        Builtin::Warp => warp::on_event(ctx, me, ev),
        Builtin::DifficultyWarp => difficulty_warp::on_event(ctx, me, ev),
        Builtin::Camera => camera::on_event(ctx, me, ev),
        Builtin::Platform => platform::on_event(ctx, me, ev),
        Builtin::Playerface => playerface::on_event(ctx, me, ev),
        Builtin::MovingPlatform => moving_platform::on_event(ctx, me, ev),
        Builtin::Water => water::on_event(ctx, me, ev),
        Builtin::Vine => vine::on_event(ctx, me, ev),
        Builtin::FakeBlock => fake_block::on_event(ctx, me, ev),
        Builtin::HiddenBlock => hidden_block::on_event(ctx, me, ev),
        Builtin::Trigger => trigger::on_event(ctx, me, ev),
        Builtin::Trap => trap::on_event(ctx, me, ev),
        Builtin::DropPlatform => drop_platform::on_event(ctx, me, ev),
        Builtin::GravityFlip => gravity_flip::on_event(ctx, me, ev),
        Builtin::EdgeWarp => edge_warp::on_event(ctx, me, ev),
        Builtin::BulletBlocker => bullet_blocker::on_event(ctx, me, ev),
        Builtin::Slope => slope::on_event(ctx, me, ev),
        Builtin::Music => music::on_event(ctx, me, ev),
        Builtin::BossGate => boss_gate::on_event(ctx, me, ev),
        Builtin::BossItem => boss_item::on_event(ctx, me, ev),
        Builtin::SecretItem => secret_item::on_event(ctx, me, ev),
        Builtin::ItemViewer => item_viewer::on_event(ctx, me, ev),
        Builtin::GameClear => game_clear::on_event(ctx, me, ev),
        Builtin::JumpRefresher => jump_refresher::on_event(ctx, me, ev),
        Builtin::Spring => spring::on_event(ctx, me, ev),
        Builtin::Conveyor => conveyor::on_event(ctx, me, ev),
        Builtin::KidParamZone => kid_param_zone::on_event(ctx, me, ev),
        Builtin::BreakableBlock => breakable_block::on_event(ctx, me, ev),
        Builtin::TrapStopper => trap_stopper::on_event(ctx, me, ev),
        Builtin::Orbit => orbit::on_event(ctx, me, ev),
        Builtin::PathPlatform => path_platform::on_event(ctx, me, ev),
        Builtin::TimeLimit => time_limit::on_event(ctx, me, ev),
        Builtin::Sign => sign::on_event(ctx, me, ev),
        Builtin::Ladder => ladder::on_event(ctx, me, ev),
        Builtin::Decor => decor::on_event(ctx, me, ev),
        Builtin::Scripted => scripted::on_event(ctx, me, ev),
        Builtin::Kid => crate::kid::on_event(ctx, me, ev),
        Builtin::Probe => crate::events::probe_on_event(ctx, me, ev),
        Builtin::Inert => Ok(Flow::Continue),
    }
}

/// What a kind with this base handles. The kid's set depends on the physics profile, so the
/// loader calls `kid::event_set(&def.physics)` for `Builtin::Kid`; this gives an empty set for
/// it, as for `Inert`.
pub fn event_set(b: Builtin, kind: &KindDef) -> EventSet {
    match b {
        Builtin::World => world::event_set(kind),
        Builtin::KidStart => kid_start::event_set(kind),
        Builtin::Block => block::event_set(kind),
        Builtin::Spike => spike::event_set(kind),
        Builtin::Killer => killer::event_set(kind),
        Builtin::KidBullet => kid_bullet::event_set(kind),
        Builtin::Blood => blood::event_set(kind),
        Builtin::BloodEmitter => blood_emitter::event_set(kind),
        Builtin::GameOver => game_over::event_set(kind),
        Builtin::Bow => bow::event_set(kind),
        Builtin::Save => save::event_set(kind),
        Builtin::Warp => warp::event_set(kind),
        Builtin::DifficultyWarp => difficulty_warp::event_set(kind),
        Builtin::Camera => camera::event_set(kind),
        Builtin::Platform => platform::event_set(kind),
        Builtin::Playerface => playerface::event_set(kind),
        Builtin::MovingPlatform => moving_platform::event_set(kind),
        Builtin::Water => water::event_set(kind),
        Builtin::Vine => vine::event_set(kind),
        Builtin::FakeBlock => fake_block::event_set(kind),
        Builtin::HiddenBlock => hidden_block::event_set(kind),
        Builtin::Trigger => trigger::event_set(kind),
        Builtin::Trap => trap::event_set(kind),
        Builtin::DropPlatform => drop_platform::event_set(kind),
        Builtin::GravityFlip => gravity_flip::event_set(kind),
        Builtin::EdgeWarp => edge_warp::event_set(kind),
        Builtin::BulletBlocker => bullet_blocker::event_set(kind),
        Builtin::Slope => slope::event_set(kind),
        Builtin::Music => music::event_set(kind),
        Builtin::BossGate => boss_gate::event_set(kind),
        Builtin::BossItem => boss_item::event_set(kind),
        Builtin::SecretItem => secret_item::event_set(kind),
        Builtin::ItemViewer => item_viewer::event_set(kind),
        Builtin::GameClear => game_clear::event_set(kind),
        Builtin::JumpRefresher => jump_refresher::event_set(kind),
        Builtin::Spring => spring::event_set(kind),
        Builtin::Conveyor => conveyor::event_set(kind),
        Builtin::KidParamZone => kid_param_zone::event_set(kind),
        Builtin::BreakableBlock => breakable_block::event_set(kind),
        Builtin::TrapStopper => trap_stopper::event_set(kind),
        Builtin::Orbit => orbit::event_set(kind),
        Builtin::PathPlatform => path_platform::event_set(kind),
        Builtin::TimeLimit => time_limit::event_set(kind),
        Builtin::Sign => sign::event_set(kind),
        Builtin::Ladder => ladder::event_set(kind),
        Builtin::Decor => decor::event_set(kind),
        Builtin::Scripted => scripted::event_set(kind),
        Builtin::Probe => crate::events::probe_event_set(),
        Builtin::Kid | Builtin::Inert => EventSet::default(),
    }
}

/// The params a built-in accepts. The kid takes none (its settings are the physics profile).
pub fn param_specs(b: Builtin) -> &'static [ParamSpec] {
    match b {
        Builtin::World => world::param_specs(),
        Builtin::KidStart => kid_start::param_specs(),
        Builtin::Block => block::param_specs(),
        Builtin::Spike => spike::param_specs(),
        Builtin::Killer => killer::param_specs(),
        Builtin::KidBullet => kid_bullet::param_specs(),
        Builtin::Blood => blood::param_specs(),
        Builtin::BloodEmitter => blood_emitter::param_specs(),
        Builtin::GameOver => game_over::param_specs(),
        Builtin::Bow => bow::param_specs(),
        Builtin::Save => save::param_specs(),
        Builtin::Warp => warp::param_specs(),
        Builtin::DifficultyWarp => difficulty_warp::param_specs(),
        Builtin::Camera => camera::param_specs(),
        Builtin::Platform => platform::param_specs(),
        Builtin::Playerface => playerface::param_specs(),
        Builtin::MovingPlatform => moving_platform::param_specs(),
        Builtin::Water => water::param_specs(),
        Builtin::Vine => vine::param_specs(),
        Builtin::FakeBlock => fake_block::param_specs(),
        Builtin::HiddenBlock => hidden_block::param_specs(),
        Builtin::Trigger => trigger::param_specs(),
        Builtin::Trap => trap::param_specs(),
        Builtin::DropPlatform => drop_platform::param_specs(),
        Builtin::GravityFlip => gravity_flip::param_specs(),
        Builtin::EdgeWarp => edge_warp::param_specs(),
        Builtin::BulletBlocker => bullet_blocker::param_specs(),
        Builtin::Slope => slope::param_specs(),
        Builtin::Music => music::param_specs(),
        Builtin::BossGate => boss_gate::param_specs(),
        Builtin::BossItem => boss_item::param_specs(),
        Builtin::SecretItem => secret_item::param_specs(),
        Builtin::ItemViewer => item_viewer::param_specs(),
        Builtin::GameClear => game_clear::param_specs(),
        Builtin::JumpRefresher => jump_refresher::param_specs(),
        Builtin::Spring => spring::param_specs(),
        Builtin::Conveyor => conveyor::param_specs(),
        Builtin::KidParamZone => kid_param_zone::param_specs(),
        Builtin::BreakableBlock => breakable_block::param_specs(),
        Builtin::TrapStopper => trap_stopper::param_specs(),
        Builtin::Orbit => orbit::param_specs(),
        Builtin::PathPlatform => path_platform::param_specs(),
        Builtin::TimeLimit => time_limit::param_specs(),
        Builtin::Sign => sign::param_specs(),
        Builtin::Ladder => ladder::param_specs(),
        Builtin::Decor => decor::param_specs(),
        Builtin::Scripted => scripted::param_specs(),
        Builtin::Kid | Builtin::Probe | Builtin::Inert => &[],
    }
}

/// The initial state of a new built-in instance from its params. The kid's state is
/// `KindState::Kid` (WP5a), so `Kid` gives `Stateless` here, as do `Probe` and `Inert`.
pub fn init_state(b: Builtin, params: &Params) -> Result<BuiltinState, ParamError> {
    match b {
        Builtin::World => world::init_state(params),
        Builtin::KidStart => kid_start::init_state(params),
        Builtin::Block => block::init_state(params),
        Builtin::Spike => spike::init_state(params),
        Builtin::Killer => killer::init_state(params),
        Builtin::KidBullet => kid_bullet::init_state(params),
        Builtin::Blood => blood::init_state(params),
        Builtin::BloodEmitter => blood_emitter::init_state(params),
        Builtin::GameOver => game_over::init_state(params),
        Builtin::Bow => bow::init_state(params),
        Builtin::Save => save::init_state(params),
        Builtin::Warp => warp::init_state(params),
        Builtin::DifficultyWarp => difficulty_warp::init_state(params),
        Builtin::Camera => camera::init_state(params),
        Builtin::Platform => platform::init_state(params),
        Builtin::Playerface => playerface::init_state(params),
        Builtin::MovingPlatform => moving_platform::init_state(params),
        Builtin::Water => water::init_state(params),
        Builtin::Vine => vine::init_state(params),
        Builtin::FakeBlock => fake_block::init_state(params),
        Builtin::HiddenBlock => hidden_block::init_state(params),
        Builtin::Trigger => trigger::init_state(params),
        Builtin::Trap => trap::init_state(params),
        Builtin::DropPlatform => drop_platform::init_state(params),
        Builtin::GravityFlip => gravity_flip::init_state(params),
        Builtin::EdgeWarp => edge_warp::init_state(params),
        Builtin::BulletBlocker => bullet_blocker::init_state(params),
        Builtin::Slope => slope::init_state(params),
        Builtin::Music => music::init_state(params),
        Builtin::BossGate => boss_gate::init_state(params),
        Builtin::BossItem => boss_item::init_state(params),
        Builtin::SecretItem => secret_item::init_state(params),
        Builtin::ItemViewer => item_viewer::init_state(params),
        Builtin::GameClear => game_clear::init_state(params),
        Builtin::JumpRefresher => jump_refresher::init_state(params),
        Builtin::Spring => spring::init_state(params),
        Builtin::Conveyor => conveyor::init_state(params),
        Builtin::KidParamZone => kid_param_zone::init_state(params),
        Builtin::BreakableBlock => breakable_block::init_state(params),
        Builtin::TrapStopper => trap_stopper::init_state(params),
        Builtin::Orbit => orbit::init_state(params),
        Builtin::PathPlatform => path_platform::init_state(params),
        Builtin::TimeLimit => time_limit::init_state(params),
        Builtin::Sign => sign::init_state(params),
        Builtin::Ladder => ladder::init_state(params),
        Builtin::Decor => decor::init_state(params),
        Builtin::Scripted => scripted::init_state(params),
        Builtin::Kid | Builtin::Probe | Builtin::Inert => Ok(BuiltinState::Stateless),
    }
}
