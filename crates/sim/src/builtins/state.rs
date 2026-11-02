//! Per-instance built-in state (docs/plans/sim-core.md 1.5).
//!
//! WP6a owns this file. Each built-in with state adds one variant here that wraps a struct from
//! its own `builtins/<kind>.rs`; WP6b and WP6c add one line each, merged in wave order.

use serde::{Deserialize, Serialize};

use super::{
    block, blood, blood_emitter, bow, camera, difficulty_warp, game_over, kid_bullet, kid_start, killer,
    save, spike, warp, world,
};

/// A built-in instance's state.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum BuiltinState {
    /// No per-instance state.
    #[default]
    Stateless,
    /// `world` (WP6a).
    World(world::WorldState),
    /// `kid_start` (WP6a).
    KidStart(kid_start::KidStartState),
    /// `block` (WP6a).
    Block(block::BlockState),
    /// `spike` (WP6a).
    Spike(spike::SpikeState),
    /// `kid_bullet` (WP6a).
    KidBullet(kid_bullet::KidBulletState),
    /// `save` (WP6a).
    Save(save::SaveState),
    /// `warp` (WP6a).
    Warp(warp::WarpState),
    /// `difficulty_warp` (WP6a).
    DifficultyWarp(difficulty_warp::DifficultyWarpState),
    /// `blood` (WP6a).
    Blood(blood::BloodState),
    /// `blood_emitter` (WP6a).
    BloodEmitter(blood_emitter::BloodEmitterState),
    /// `game_over` (WP6a).
    GameOver(game_over::GameOverState),
    /// `bow` (WP6a).
    Bow(bow::BowState),
    /// `camera` (WP6a).
    Camera(camera::CameraState),
    /// `killer` (WP6a).
    Killer(killer::KillerState),
}
