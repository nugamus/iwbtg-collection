//! The built-in kinds (docs/plans/sim-core.md 6.1). WP0 lists every variant so the loader can
//! parse kinds; WP6a owns the file afterwards. Dispatch lives in `builtins/mod.rs`, where WP0
//! routed every variant to its own file, so WP6b and WP6c edit neither file.

/// Every built-in kind. A package kind is one of these, with params (a preset) or a script on
/// top (`Scripted`, or a base built-in plus a script).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Builtin {
    // Engine kinds the sim spawns by role (`EngineKind`).
    /// The engine controller: restart, reset, pause, mute, timers (tier 1).
    World,
    /// The kid (WP5a-WP5c).
    Kid,
    /// The kid's bullet (tier 1).
    KidBullet,
    /// Blood drop: draws its frame and gravity from the gameplay RNG (tier 1).
    Blood,
    /// Spawns blood each Step until its alarm ends it (tier 1).
    BloodEmitter,
    /// The GAMEOVER display (tier 1).
    GameOver,
    /// The bow that follows the kid (tier 1).
    Bow,
    /// The face on the death screen: one `irandom(3)` draw in Create (tier 2).
    Playerface,

    // Tier 1.
    /// Places the kid.
    KidStart,
    /// Solid block.
    Block,
    /// Spike.
    Spike,
    /// Generic killer, also the cherry.
    Killer,
    /// Save point.
    Save,
    /// Warp to a room.
    Warp,
    /// Difficulty select warp (`warpStart`).
    DifficultyWarp,
    /// Camera object.
    Camera,
    /// Jump-through platform.
    Platform,

    // Tier 2.
    /// Moving platform.
    MovingPlatform,
    /// Water.
    Water,
    /// Vine.
    Vine,
    /// Fake block.
    FakeBlock,
    /// Hidden block.
    HiddenBlock,
    /// Trap trigger.
    Trigger,
    /// Trap.
    Trap,
    /// Platform that drops.
    DropPlatform,
    /// Gravity flipper.
    GravityFlip,
    /// Edge warp.
    EdgeWarp,
    /// Bullet blocker.
    BulletBlocker,
    /// Slope.
    Slope,
    /// Music player.
    Music,
    /// Family B `objDeathSound`: fades the music and plays the death sound a tick later.
    DeathSound,
    /// Boss gate.
    BossGate,
    /// Boss item.
    BossItem,
    /// Secret item.
    SecretItem,
    /// Item viewer.
    ItemViewer,
    /// Game clear: raises the `run_end` timing trigger.
    GameClear,

    // Tier 3.
    /// Jump refresher.
    JumpRefresher,
    /// Spring.
    Spring,
    /// Conveyor.
    Conveyor,
    /// Kid parameter zone.
    KidParamZone,
    /// Breakable block.
    BreakableBlock,
    /// Trap stopper.
    TrapStopper,
    /// Orbiting object.
    Orbit,
    /// Platform on a path.
    PathPlatform,
    /// Time limit.
    TimeLimit,
    /// Sign: raises a client text box.
    Sign,
    /// Ladder.
    Ladder,
    /// Decoration, no behavior.
    Decor,
    /// A kind whose behavior is a script (WP9).
    Scripted,

    // Not in the package format.
    /// Test built-in that logs every event and can move, spawn, destroy and request a room
    /// (WP1 tests, WP11 golden probe tests). Never loaded from a package.
    Probe,
    /// `LoadMode::Trace`: an unmapped script kind loaded with no events; its mask still collides.
    Inert,
}

impl Builtin {
    /// Every built-in, in declaration order.
    pub const ALL: [Builtin; 51] = [
        Builtin::World,
        Builtin::Kid,
        Builtin::KidBullet,
        Builtin::Blood,
        Builtin::BloodEmitter,
        Builtin::GameOver,
        Builtin::Bow,
        Builtin::Playerface,
        Builtin::KidStart,
        Builtin::Block,
        Builtin::Spike,
        Builtin::Killer,
        Builtin::Save,
        Builtin::Warp,
        Builtin::DifficultyWarp,
        Builtin::Camera,
        Builtin::Platform,
        Builtin::MovingPlatform,
        Builtin::Water,
        Builtin::Vine,
        Builtin::FakeBlock,
        Builtin::HiddenBlock,
        Builtin::Trigger,
        Builtin::Trap,
        Builtin::DropPlatform,
        Builtin::GravityFlip,
        Builtin::EdgeWarp,
        Builtin::BulletBlocker,
        Builtin::Slope,
        Builtin::Music,
        Builtin::DeathSound,
        Builtin::BossGate,
        Builtin::BossItem,
        Builtin::SecretItem,
        Builtin::ItemViewer,
        Builtin::GameClear,
        Builtin::JumpRefresher,
        Builtin::Spring,
        Builtin::Conveyor,
        Builtin::KidParamZone,
        Builtin::BreakableBlock,
        Builtin::TrapStopper,
        Builtin::Orbit,
        Builtin::PathPlatform,
        Builtin::TimeLimit,
        Builtin::Sign,
        Builtin::Ladder,
        Builtin::Decor,
        Builtin::Scripted,
        Builtin::Probe,
        Builtin::Inert,
    ];

    /// The package-format name (`base = "..."`, `object_order` entries).
    pub const fn name(self) -> &'static str {
        match self {
            Builtin::World => "world",
            Builtin::Kid => "kid",
            Builtin::KidBullet => "kid_bullet",
            Builtin::Blood => "blood",
            Builtin::BloodEmitter => "blood_emitter",
            Builtin::GameOver => "game_over",
            Builtin::Bow => "bow",
            Builtin::Playerface => "playerface",
            Builtin::KidStart => "kid_start",
            Builtin::Block => "block",
            Builtin::Spike => "spike",
            Builtin::Killer => "killer",
            Builtin::Save => "save",
            Builtin::Warp => "warp",
            Builtin::DifficultyWarp => "difficulty_warp",
            Builtin::Camera => "camera",
            Builtin::Platform => "platform",
            Builtin::MovingPlatform => "moving_platform",
            Builtin::Water => "water",
            Builtin::Vine => "vine",
            Builtin::FakeBlock => "fake_block",
            Builtin::HiddenBlock => "hidden_block",
            Builtin::Trigger => "trigger",
            Builtin::Trap => "trap",
            Builtin::DropPlatform => "drop_platform",
            Builtin::GravityFlip => "gravity_flip",
            Builtin::EdgeWarp => "edge_warp",
            Builtin::BulletBlocker => "bullet_blocker",
            Builtin::Slope => "slope",
            Builtin::Music => "music",
            Builtin::DeathSound => "death_sound",
            Builtin::BossGate => "boss_gate",
            Builtin::BossItem => "boss_item",
            Builtin::SecretItem => "secret_item",
            Builtin::ItemViewer => "item_viewer",
            Builtin::GameClear => "game_clear",
            Builtin::JumpRefresher => "jump_refresher",
            Builtin::Spring => "spring",
            Builtin::Conveyor => "conveyor",
            Builtin::KidParamZone => "kid_param_zone",
            Builtin::BreakableBlock => "breakable_block",
            Builtin::TrapStopper => "trap_stopper",
            Builtin::Orbit => "orbit",
            Builtin::PathPlatform => "path_platform",
            Builtin::TimeLimit => "time_limit",
            Builtin::Sign => "sign",
            Builtin::Ladder => "ladder",
            Builtin::Decor => "decor",
            Builtin::Scripted => "scripted",
            Builtin::Probe => "probe",
            Builtin::Inert => "inert",
        }
    }

    /// The built-in a package may name, by its package-format name. `probe` and `inert` are not
    /// package names and give `None`.
    pub fn from_name(name: &str) -> Option<Builtin> {
        Builtin::ALL
            .iter()
            .copied()
            .filter(|b| !matches!(b, Builtin::Probe | Builtin::Inert))
            .find(|b| b.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_are_unique() {
        let mut names: Vec<&str> = Builtin::ALL.iter().map(|b| b.name()).collect();
        for b in Builtin::ALL {
            let expected = (!matches!(b, Builtin::Probe | Builtin::Inert)).then_some(b);
            assert_eq!(Builtin::from_name(b.name()), expected);
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Builtin::ALL.len());
    }
}
