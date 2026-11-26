//! Per-instance built-in state (docs/architecture/sim/world.md).
//!
//! Each built-in with state adds one variant here that wraps a struct from
//! its own `builtins/<kind>.rs`.

use serde::{Deserialize, Serialize};

use super::{
    block, blood, blood_emitter, bow, breakable_block, camera, conveyor, death_sound, difficulty_warp,
    drop_platform, edge_warp, fake_block, game_clear, game_over, jump_refresher, kid_bullet, kid_param_zone,
    kid_start, killer, moving_platform, music, orbit, particle_system, path_platform, save, sign, spike,
    spring, time_limit, trap, trap_stopper, trigger, warp, water, world,
};

/// A built-in instance's state.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum BuiltinState {
    /// No per-instance state.
    #[default]
    Stateless,
    /// `world`.
    World(world::WorldState),
    /// `kid_start`.
    KidStart(kid_start::KidStartState),
    /// `boss_gate` and `secret_item`: the flag index.
    Flag(u8),
    /// `boss_item`.
    BossItem {
        /// The boss flag index.
        index: u8,
        /// The boss kind it waits for.
        watched: Option<crate::def::KindId>,
    },
    /// `item_viewer`.
    ItemViewer {
        /// Shows a boss flag (else an item flag).
        boss: bool,
        /// The flag index.
        index: u8,
    },
    /// `game_clear`.
    GameClear(game_clear::GameClearState),
    /// `music`.
    Music(music::MusicState),
    /// `death_sound` (family B death).
    DeathSound(death_sound::DeathSoundState),
    /// `drop_platform`.
    DropPlatform(drop_platform::DropState),
    /// `gravity_flip`: flips up.
    GravityFlip {
        /// Sets `global.grav` (up) or clears it (down).
        up: bool,
        /// `yflag_with_view_rotation` (kaenbyou): rotate view 0, then flip every kid.
        rotate: bool,
        /// `rotate`: this activation has started (the original's `played`).
        played: bool,
    },
    /// `bullet_blocker`.
    BulletBlocker {
        /// Spares bullets while it overlaps a kid.
        except_overlapping: bool,
    },
    /// `edge_warp` per-edge table.
    EdgeTable(edge_warp::EdgeTable),
    /// `edge_warp` in `room_assign` mode.
    RoomAssign(edge_warp::RoomAssign),
    /// `fake_block`.
    FakeBlock(fake_block::FakeBlockState),
    /// `trigger`.
    Trigger(trigger::TriggerState),
    /// `trap`.
    Trap(trap::TrapState),
    /// `breakable_block`.
    Breakable(breakable_block::BreakableState),
    /// `trap_stopper`.
    Stopper(trap_stopper::StopperState),
    /// `orbit`.
    Orbit(orbit::OrbitState),
    /// `path_platform`.
    PathPlatform(path_platform::PathPlatformState),
    /// `jump_refresher`.
    Refresher(jump_refresher::RefresherState),
    /// `spring`.
    Spring(spring::SpringState),
    /// `conveyor`.
    Conveyor(conveyor::ConveyorState),
    /// `kid_param_zone`.
    ParamZone(kid_param_zone::ZoneState),
    /// `moving_platform`.
    MovingPlatform(moving_platform::MovingPlatformState),
    /// `water`.
    Water(water::WaterState),
    /// `block`.
    Block(block::BlockState),
    /// `spike`.
    Spike(spike::SpikeState),
    /// `kid_bullet`.
    KidBullet(kid_bullet::KidBulletState),
    /// `save`.
    Save(save::SaveState),
    /// `warp`.
    Warp(warp::WarpState),
    /// `difficulty_warp`.
    DifficultyWarp(difficulty_warp::DifficultyWarpState),
    /// `blood`.
    Blood(blood::BloodState),
    /// `blood_emitter`.
    BloodEmitter(blood_emitter::BloodEmitterState),
    /// `game_over`.
    GameOver(game_over::GameOverState),
    /// `bow`.
    Bow(bow::BowState),
    /// `camera`.
    Camera(camera::CameraState),
    /// `killer`.
    Killer(killer::KillerState),
    /// `sign`.
    Sign(sign::SignState),
    /// `time_limit`.
    TimeLimit(time_limit::TimeLimitState),
    /// `particle_system` (#765).
    ParticleSystem(particle_system::ParticleSystemState),
}
