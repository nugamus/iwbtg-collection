//! The behaviour taxonomy and the one trait table (docs/architecture/sim/builtins.md; decision
//! 0005, "A small standard built-in set"; #1159 T1).
//!
//! The shared set is a small number of native behaviours plus presets and scripts. This table is
//! the single place that says, per kind, what behaviour it is, which params pick its classes,
//! which params pick its event set, which classes it carries and which events it handles.
//! `def::class` and `builtins` delegate to it, so the two hand-written selector tables are gone.
//!
//! The table is a pure refactor: every entry is the value the old tables gave, so a resolved
//! `KindDef`, `EventSet` and initial `BuiltinState` is field-identical and no package, hash or
//! replay moves. The 50 kind names stay the creator vocabulary; `base = "<behaviour>"` is T2.
//!
//! The behaviours are the twelve of decision 0005 (spawn, checkpoint, portal, trigger, hazard,
//! pickup, overlay, emitter, solid/surface, mover/projectile, kid zone, goal) plus the controller
//! (`world`), the kid (Area C) and the data bases (`decor`, `scripted`). `probe` and `inert` are
//! not package kinds; they appear for exhaustiveness and map to the data base.

use super::Builtin;
use super::class::ClassMask;
use super::kind::KindDef;
use crate::events::EventSet;

/// One of the standard built-in behaviours (decision 0005), or a kind outside the shared set.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Behaviour {
    /// A player spawn (`kid_start`).
    Spawn,
    /// A checkpoint (`save`).
    Checkpoint,
    /// A room-to-room door (`warp`, `edge_warp`, `difficulty_warp`).
    Portal,
    /// A touch zone that fires a trap id (`trigger`).
    Trigger,
    /// Something that kills the kid (`spike`, `killer`, `orbit`).
    Hazard,
    /// A collectable or its flag (`boss_item`, `secret_item`, `item_viewer`).
    Pickup,
    /// A display or screen effect (`game_over`, `playerface`, `bow`, `sign`, `music`,
    /// `death_sound`, `camera`).
    Overlay,
    /// Something that makes instances (`blood`, `blood_emitter`, `particle_system`).
    Emitter,
    /// A solid or surface (`block`, `platform`, `slope`, `ladder`, `vine`, `fake_block`,
    /// `hidden_block`, `breakable_block`, `conveyor`, `bullet_blocker`, `trap_stopper`,
    /// `boss_gate`).
    Solid,
    /// A moving object or projectile (`moving_platform`, `drop_platform`, `path_platform`,
    /// `trap`, `kid_bullet`).
    Mover,
    /// A zone that changes the kid (`water`, `gravity_flip`, `kid_param_zone`,
    /// `jump_refresher`, `spring`).
    KidZone,
    /// A run goal (`game_clear`, `time_limit`).
    Goal,
    /// The engine controller (`world`).
    Controller,
    /// The kid (Area C; its events are the physics profile's lists).
    Kid,
    /// A data base: no native behaviour (`decor`, `scripted`; `probe`/`inert` for exhaustiveness).
    Data,
}

/// One kind's place in the taxonomy and its trait set.
#[derive(Clone, Copy, Debug)]
pub struct Traits {
    /// The native behaviour (or the controller, the kid, or a data base).
    pub behaviour: Behaviour,
    /// The params that pick the kind's classes (`class::param_classes`).
    pub class_params: &'static [&'static str],
    /// The params that pick the kind's event set.
    pub event_params: &'static [&'static str],
    /// The classes the kind carries by default (`class::default_classes`).
    pub classes: ClassMask,
    /// The kind's events, built from its params.
    pub events: fn(&KindDef) -> EventSet,
}

/// The events of a kind with no native behaviour (the kid, `inert`).
fn no_events(_kind: &KindDef) -> EventSet {
    EventSet::default()
}

/// `probe`'s event set (a test kind; every event).
fn probe_events(_kind: &KindDef) -> EventSet {
    crate::events::probe_event_set()
}

macro_rules! traits_table {
    ($($builtin:ident : $behaviour:ident, $classes:expr, [$($class_param:literal),*], $event_params:expr, $events:path;)*) => {
        static TRAITS: [Traits; 52] = [
            $(Traits {
                behaviour: Behaviour::$behaviour,
                class_params: &[$($class_param),*],
                event_params: $event_params,
                classes: $classes,
                events: $events,
            },)*
        ];
    };
}

// The table is in `Builtin::ALL` declaration order (`traits` indexes by the discriminant).
traits_table! {
    // Engine kinds the sim spawns by role.
    World: Controller, ClassMask::NONE, [], &["mute", "clear_switches_on_room_end"], crate::builtins::world::event_set;
    Kid: Kid, ClassMask::KID, [], &[], no_events;
    KidBullet: Mover, ClassMask::KID_BULLET, [], &["block_check"], crate::builtins::kid_bullet::event_set;
    Blood: Emitter, ClassMask::BLOOD, [], &["sticks"], crate::builtins::blood::event_set;
    BloodEmitter: Emitter, ClassMask::NONE, [], &["mode", "destroyed_by_restart"], crate::builtins::blood_emitter::event_set;
    GameOver: Overlay, ClassMask::NONE, [], &[], crate::builtins::game_over::event_set;
    Bow: Overlay, ClassMask::BOW, [], &[], crate::builtins::bow::event_set;
    Playerface: Overlay, ClassMask::NONE, [], &[], crate::builtins::playerface::event_set;

    // Tier 1.
    KidStart: Spawn, ClassMask::NONE, [], &[], crate::builtins::kid_start::event_set;
    Block: Solid, ClassMask::BLOCK, ["kid_collides"], &["proximity_frame"], crate::builtins::block::event_set;
    Spike: Hazard, ClassMask::KILLER, [], &["destroys_bullets"], crate::builtins::spike::event_set;
    Killer: Hazard, ClassMask::KILLER, ["kill_outside_view"], &["bounce", "destroy_outside_room", "kill_outside_view"], crate::builtins::killer::event_set;
    Save: Checkpoint, ClassMask::SAVE, [], &["blink", "on_shot", "on_touch"], crate::builtins::save::event_set;
    Warp: Portal, ClassMask::ROOM_CHANGER, [], crate::builtins::warp::GATE_PARAMS, crate::builtins::warp::event_set;
    DifficultyWarp: Portal, ClassMask::ROOM_CHANGER, [], &[], crate::builtins::difficulty_warp::event_set;
    Camera: Overlay, ClassMask::NONE, [], &["mode", "on_step", "on_room_start", "on_room_end"], crate::builtins::camera::event_set;
    Platform: Solid, ClassMask(ClassMask::PLATFORM.0 | ClassMask::BLOCK.0), [], &[], crate::builtins::platform::event_set;

    // Tier 2.
    MovingPlatform: Mover, ClassMask(ClassMask::PLATFORM.0 | ClassMask::BLOCK.0), [], &["carry_rule", "start_on_touch", "marker_reverse"], crate::builtins::moving_platform::event_set;
    Water: KidZone, ClassMask::WATER1, ["variant"], &["destroy_outside_room", "angle_step", "area_left", "area_right", "area_top", "area_bottom"], crate::builtins::water::event_set;
    Vine: Solid, ClassMask::NONE, ["side"], &[], crate::builtins::vine::event_set;
    FakeBlock: Solid, ClassMask::FAKE_BLOCK, [], &["vanish_on", "stops_bullets"], crate::builtins::fake_block::event_set;
    HiddenBlock: Solid, ClassMask::NONE, [], &["stops_bullets", "destroyed_by_bullet", "destroyed_by_blood"], crate::builtins::hidden_block::event_set;
    Trigger: Trigger, ClassMask::TRIGGER, [], &[], crate::builtins::trigger::event_set;
    Trap: Mover, ClassMask::NONE, [], &["launch", "alm", "alm2", "alm3", "alm4", "destroy_outside_room", "stop_on_stopper"], crate::builtins::trap::event_set;
    DropPlatform: Mover, ClassMask(ClassMask::PLATFORM.0 | ClassMask::BLOCK.0), [], &["mode"], crate::builtins::drop_platform::event_set;
    GravityFlip: KidZone, ClassMask::NONE, [], &["transition"], crate::builtins::gravity_flip::event_set;
    EdgeWarp: Portal, ClassMask::EDGE_WARP, [], &["mode"], crate::builtins::edge_warp::event_set;
    BulletBlocker: Solid, ClassMask::BULLET_BLOCKER, [], &[], crate::builtins::bullet_blocker::event_set;
    Slope: Solid, ClassMask::SLOPE, ["shape", "scale_x"], &[], crate::builtins::slope::event_set;
    Music: Overlay, ClassMask::NONE, [], &["mode"], crate::builtins::music::event_set;
    DeathSound: Overlay, ClassMask::NONE, [], &[], crate::builtins::death_sound::event_set;
    BossGate: Solid, ClassMask::NONE, [], &[], crate::builtins::boss_gate::event_set;
    BossItem: Pickup, ClassMask::NONE, [], &[], crate::builtins::boss_item::event_set;
    SecretItem: Pickup, ClassMask::NONE, [], &[], crate::builtins::secret_item::event_set;
    ItemViewer: Pickup, ClassMask::NONE, [], &[], crate::builtins::item_viewer::event_set;
    GameClear: Goal, ClassMask::NONE, [], &[], crate::builtins::game_clear::event_set;

    // Tier 3.
    JumpRefresher: KidZone, ClassMask::NONE, [], &[], crate::builtins::jump_refresher::event_set;
    Spring: KidZone, ClassMask::NONE, [], &[], crate::builtins::spring::event_set;
    Conveyor: Solid, ClassMask::CONVEYOR, [], &["mode"], crate::builtins::conveyor::event_set;
    KidParamZone: KidZone, ClassMask::PARAM_ZONE, [], &[], crate::builtins::kid_param_zone::event_set;
    BreakableBlock: Solid, ClassMask::BLOCK, [], &["break_on"], crate::builtins::breakable_block::event_set;
    TrapStopper: Solid, ClassMask::TRAP_STOPPER, [], &["mode"], crate::builtins::trap_stopper::event_set;
    Orbit: Hazard, ClassMask::NONE, [], &["mode", "destroy_outside_room"], crate::builtins::orbit::event_set;
    PathPlatform: Mover, ClassMask(ClassMask::PLATFORM.0 | ClassMask::BLOCK.0), [], &["start_on_touch"], crate::builtins::path_platform::event_set;
    TimeLimit: Goal, ClassMask::NONE, [], &[], crate::builtins::time_limit::event_set;
    Sign: Overlay, ClassMask::NONE, [], &[], crate::builtins::sign::event_set;
    Ladder: Solid, ClassMask::LADDER, [], &[], crate::builtins::ladder::event_set;
    Decor: Data, ClassMask::NONE, [], &[], crate::builtins::decor::event_set;
    ParticleSystem: Emitter, ClassMask::NONE, [], &["on_room_start", "on_room_start_destroys_instance", "on_room_end"], crate::builtins::particle_system::event_set;
    Scripted: Data, ClassMask::NONE, [], &[], crate::builtins::scripted::event_set;

    // Not in the package format.
    Probe: Data, ClassMask::NONE, [], &[], probe_events;
    Inert: Data, ClassMask::NONE, [], &[], no_events;
}

/// The traits of a built-in. The table is in [`Builtin::ALL`] order.
#[must_use]
pub fn traits(b: Builtin) -> &'static Traits {
    &TRAITS[b as usize]
}

/// The behaviour of a built-in.
#[must_use]
pub fn behaviour(b: Builtin) -> Behaviour {
    traits(b).behaviour
}

/// The reference concrete kind of each standard behaviour: what `base = "<behaviour>"` resolves
/// to. The other kinds of the same behaviour keep their own names: a creator who needs an
/// `edge_warp` writes `edge_warp`, not `portal`, and a spike writes `spike`, not `hazard`.
pub const BEHAVIOUR_BASES: [(&str, Behaviour, Builtin); 12] = [
    ("spawn", Behaviour::Spawn, Builtin::KidStart),
    ("checkpoint", Behaviour::Checkpoint, Builtin::Save),
    ("portal", Behaviour::Portal, Builtin::Warp),
    ("trigger", Behaviour::Trigger, Builtin::Trigger),
    ("hazard", Behaviour::Hazard, Builtin::Killer),
    ("pickup", Behaviour::Pickup, Builtin::BossItem),
    ("overlay", Behaviour::Overlay, Builtin::GameOver),
    ("emitter", Behaviour::Emitter, Builtin::BloodEmitter),
    ("solid", Behaviour::Solid, Builtin::Block),
    ("mover", Behaviour::Mover, Builtin::MovingPlatform),
    ("kid_zone", Behaviour::KidZone, Builtin::Water),
    ("goal", Behaviour::Goal, Builtin::GameClear),
];

/// The reference kind of a behaviour, or `None` for the controller, the kid and the data bases,
/// which are not package bases.
#[must_use]
pub fn base_kind(behaviour: Behaviour) -> Option<Builtin> {
    BEHAVIOUR_BASES
        .iter()
        .find(|(_, b, _)| *b == behaviour)
        .map(|(_, _, kind)| *kind)
}

/// The kind a behaviour base name resolves to (`base = "hazard"`), if `name` is one. The
/// behaviour names are additive aliases; the 50 kind names and their behavior do not change.
#[must_use]
pub fn base_from_name(name: &str) -> Option<Builtin> {
    BEHAVIOUR_BASES
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, _, kind)| *kind)
}

#[cfg(test)]
mod tests {
    use super::Behaviour::*;
    use super::*;

    #[test]
    fn the_table_is_in_builtin_all_order() {
        assert_eq!(TRAITS.len(), Builtin::ALL.len());
        for (i, b) in Builtin::ALL.iter().enumerate() {
            assert_eq!(*b as usize, i, "Builtin::ALL is not declaration order");
            assert_eq!(traits(*b).behaviour, behaviour(*b));
        }
    }

    #[test]
    fn the_package_kinds_cover_the_standard_behaviours() {
        let mut seen = std::collections::BTreeSet::new();
        for b in Builtin::ALL {
            if Builtin::from_name(b.name()).is_none() {
                continue;
            }
            seen.insert(behaviour(b));
        }
        for expected in [
            Behaviour::Spawn,
            Behaviour::Checkpoint,
            Behaviour::Portal,
            Behaviour::Trigger,
            Behaviour::Hazard,
            Behaviour::Pickup,
            Behaviour::Overlay,
            Behaviour::Emitter,
            Behaviour::Solid,
            Behaviour::Mover,
            Behaviour::KidZone,
            Behaviour::Goal,
            Behaviour::Controller,
            Behaviour::Kid,
            Behaviour::Data,
        ] {
            assert!(seen.contains(&expected), "no package kind is {expected:?}");
        }
    }

    #[test]
    fn the_taxonomy_maps_the_named_kinds() {
        for (name, expected) in [
            ("world", Controller),
            ("kid", Kid),
            ("kid_start", Spawn),
            ("save", Checkpoint),
            ("warp", Portal),
            ("trigger", Trigger),
            ("spike", Hazard),
            ("boss_item", Pickup),
            ("bow", Overlay),
            ("blood", Emitter),
            ("block", Solid),
            ("trap", Mover),
            ("water", KidZone),
            ("game_clear", Goal),
            ("decor", Data),
            ("scripted", Data),
        ] {
            let b = Builtin::from_name(name).expect("package kind");
            assert_eq!(behaviour(b), expected, "{name}");
        }
    }

    /// The refactor guard: the trait table's classes and selection params are pinned literally,
    /// as the old `default_classes`/`class_params`/`event_set_params` gave them. A change here is
    /// a deliberate selection change, not an accident of the table.
    #[test]
    fn the_selection_is_pinned() {
        let platform = ClassMask(ClassMask::PLATFORM.0 | ClassMask::BLOCK.0);
        let cases: &[(Builtin, ClassMask, &[&str], &[&str])] = &[
            (
                Builtin::World,
                ClassMask::NONE,
                &[],
                &["mute", "clear_switches_on_room_end"],
            ),
            (Builtin::Kid, ClassMask::KID, &[], &[]),
            (Builtin::KidBullet, ClassMask::KID_BULLET, &[], &["block_check"]),
            (Builtin::Blood, ClassMask::BLOOD, &[], &["sticks"]),
            (
                Builtin::BloodEmitter,
                ClassMask::NONE,
                &[],
                &["mode", "destroyed_by_restart"],
            ),
            (Builtin::Bow, ClassMask::BOW, &[], &[]),
            (
                Builtin::Block,
                ClassMask::BLOCK,
                &["kid_collides"],
                &["proximity_frame"],
            ),
            (Builtin::Spike, ClassMask::KILLER, &[], &["destroys_bullets"]),
            (
                Builtin::Killer,
                ClassMask::KILLER,
                &["kill_outside_view"],
                &["bounce", "destroy_outside_room", "kill_outside_view"],
            ),
            (
                Builtin::Save,
                ClassMask::SAVE,
                &[],
                &["blink", "on_shot", "on_touch"],
            ),
            (
                Builtin::Warp,
                ClassMask::ROOM_CHANGER,
                &[],
                crate::builtins::warp::GATE_PARAMS,
            ),
            (Builtin::DifficultyWarp, ClassMask::ROOM_CHANGER, &[], &[]),
            (
                Builtin::Camera,
                ClassMask::NONE,
                &[],
                &["mode", "on_step", "on_room_start", "on_room_end"],
            ),
            (Builtin::Platform, platform, &[], &[]),
            (
                Builtin::MovingPlatform,
                platform,
                &[],
                &["carry_rule", "start_on_touch", "marker_reverse"],
            ),
            (
                Builtin::Water,
                ClassMask::WATER1,
                &["variant"],
                &[
                    "destroy_outside_room",
                    "angle_step",
                    "area_left",
                    "area_right",
                    "area_top",
                    "area_bottom",
                ],
            ),
            (Builtin::Vine, ClassMask::NONE, &["side"], &[]),
            (
                Builtin::FakeBlock,
                ClassMask::FAKE_BLOCK,
                &[],
                &["vanish_on", "stops_bullets"],
            ),
            (
                Builtin::HiddenBlock,
                ClassMask::NONE,
                &[],
                &["stops_bullets", "destroyed_by_bullet", "destroyed_by_blood"],
            ),
            (Builtin::Trigger, ClassMask::TRIGGER, &[], &[]),
            (
                Builtin::Trap,
                ClassMask::NONE,
                &[],
                &[
                    "launch",
                    "alm",
                    "alm2",
                    "alm3",
                    "alm4",
                    "destroy_outside_room",
                    "stop_on_stopper",
                ],
            ),
            (Builtin::DropPlatform, platform, &[], &["mode"]),
            (Builtin::GravityFlip, ClassMask::NONE, &[], &["transition"]),
            (Builtin::EdgeWarp, ClassMask::EDGE_WARP, &[], &["mode"]),
            (Builtin::BulletBlocker, ClassMask::BULLET_BLOCKER, &[], &[]),
            (Builtin::Slope, ClassMask::SLOPE, &["shape", "scale_x"], &[]),
            (Builtin::Music, ClassMask::NONE, &[], &["mode"]),
            (Builtin::Conveyor, ClassMask::CONVEYOR, &[], &["mode"]),
            (Builtin::KidParamZone, ClassMask::PARAM_ZONE, &[], &[]),
            (Builtin::BreakableBlock, ClassMask::BLOCK, &[], &["break_on"]),
            (Builtin::TrapStopper, ClassMask::TRAP_STOPPER, &[], &["mode"]),
            (
                Builtin::Orbit,
                ClassMask::NONE,
                &[],
                &["mode", "destroy_outside_room"],
            ),
            (Builtin::PathPlatform, platform, &[], &["start_on_touch"]),
            (Builtin::Ladder, ClassMask::LADDER, &[], &[]),
            (
                Builtin::ParticleSystem,
                ClassMask::NONE,
                &[],
                &["on_room_start", "on_room_start_destroys_instance", "on_room_end"],
            ),
        ];
        let mut pinned = std::collections::BTreeSet::new();
        for (b, classes, class_params, event_params) in cases {
            let t = traits(*b);
            assert_eq!(t.classes, *classes, "{b:?} classes");
            assert_eq!(t.class_params, *class_params, "{b:?} class params");
            assert_eq!(t.event_params, *event_params, "{b:?} event params");
            pinned.insert(*b);
        }
        for b in Builtin::ALL {
            if pinned.contains(&b) {
                continue;
            }
            let t = traits(b);
            assert_eq!(t.classes, ClassMask::NONE, "{b:?} classes");
            assert!(t.class_params.is_empty(), "{b:?} class params");
            assert!(t.event_params.is_empty(), "{b:?} event params");
        }
    }

    #[test]
    fn the_behaviour_bases_are_additive_aliases() {
        for (name, expected, kind) in BEHAVIOUR_BASES {
            assert_eq!(base_from_name(name), Some(kind), "{name}");
            assert_eq!(base_kind(expected), Some(kind), "{name}");
            assert_eq!(behaviour(kind), expected, "{name}");
            assert_eq!(Builtin::from_name(name), Some(kind), "{name}");
            // A behaviour base is not also a kind name, except `trigger`, where the behaviour
            // and the kind share the word.
            if name != "trigger" {
                assert!(
                    Builtin::ALL.iter().all(|b| b.name() != name),
                    "`{name}` is already a kind name"
                );
            }
        }
    }

    #[test]
    fn every_native_behaviour_has_a_reference_kind() {
        for expected in [
            Spawn, Checkpoint, Portal, Trigger, Hazard, Pickup, Overlay, Emitter, Solid, Mover, KidZone, Goal,
        ] {
            assert!(base_kind(expected).is_some(), "{expected:?}");
        }
        for expected in [Controller, Kid, Data] {
            assert_eq!(base_kind(expected), None, "{expected:?}");
        }
    }

    #[test]
    fn every_trait_param_is_a_param_of_its_kind() {
        for b in Builtin::ALL {
            let t = traits(b);
            let specs = crate::builtins::param_specs(b);
            for name in t.class_params.iter().chain(t.event_params) {
                assert!(
                    specs.iter().any(|s| s.name == *name),
                    "{b:?} trait lists `{name}`, not one of its params"
                );
            }
        }
    }
}
