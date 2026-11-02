//! Per-instance built-in state (docs/plans/sim-core.md 1.5).
//!
//! WP6a owns this file. Each built-in with state adds one variant here that wraps a struct from
//! its own `builtins/<kind>.rs`; WP6b and WP6c add one line each, merged in wave order.

use serde::{Deserialize, Serialize};

use super::{
    block, blood, blood_emitter, bow, camera, difficulty_warp, drop_platform, edge_warp, fake_block,
    game_clear, game_over, kid_bullet, kid_start, killer, moving_platform, music, save, spike, trap, trigger,
    warp, water, world,
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
    /// `boss_gate` and `secret_item` (WP6b): the flag index.
    Flag(u8),
    /// `boss_item` (WP6b).
    BossItem {
        /// The boss flag index.
        index: u8,
        /// The boss kind it waits for.
        watched: Option<crate::def::KindId>,
    },
    /// `item_viewer` (WP6b).
    ItemViewer {
        /// Shows a boss flag (else an item flag).
        boss: bool,
        /// The flag index.
        index: u8,
    },
    /// `game_clear` (WP6b).
    GameClear(game_clear::GameClearState),
    /// `music` (WP6b).
    Music(music::MusicState),
    /// `drop_platform` (WP6b).
    DropPlatform(drop_platform::DropState),
    /// `gravity_flip` (WP6b): flips up.
    GravityFlip {
        /// Sets `global.grav` (up) or clears it (down).
        up: bool,
    },
    /// `bullet_blocker` (WP6b).
    BulletBlocker {
        /// Spares bullets while it overlaps a kid.
        except_overlapping: bool,
    },
    /// `edge_warp` per-edge table (WP6b).
    EdgeTable(edge_warp::EdgeTable),
    /// `edge_warp` in `room_assign` mode (WP6b).
    RoomAssign(edge_warp::RoomAssign),
    /// `fake_block` (WP6b).
    FakeBlock(fake_block::FakeBlockState),
    /// `trigger` (WP6b).
    Trigger(trigger::TriggerState),
    /// `trap` (WP6b).
    Trap(trap::TrapState),
    /// `moving_platform` (WP6b).
    MovingPlatform(moving_platform::MovingPlatformState),
    /// `water` (WP6b).
    Water(water::WaterState),
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
