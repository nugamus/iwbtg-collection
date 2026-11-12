//! Collision classes: the sim's stand-in for GM parent objects (docs/plans/sim-core.md 1.1).
//!
//! A collision event or query "with object X" becomes "with class X". Each built-in declares its
//! default classes; a package object may add classes (`counts_as`). A collision event with a
//! parent also fires for its children (Confirmed, manual 205_00), which class sets reproduce
//! without a parent tree.
//!
//! Bit layout, fixed by WP0 (no later appends):
//!
//! | Bits | Use |
//! |---|---|
//! | 0-28 | the named classes below (28, `slope_mirror`, added by WP5c.4c) |
//! | 29-31 | reserved for WP11 (a WP that needs a new named bit asks the integrator) |
//! | 32-47 | 16 package-defined classes ([`ClassMask::package`]) |
//! | 48-63 | reserved |
//!
//! WP0 declares; WP11 owns afterwards.

use core::ops::{BitAnd, BitOr, BitOrAssign};

use serde::{Deserialize, Serialize};

use super::builtin::Builtin;
use super::kind::{ParamValue, Params};

/// A set of collision classes, one bit each.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize)]
pub struct ClassMask(pub u64);

impl ClassMask {
    /// No class.
    pub const NONE: ClassMask = ClassMask(0);
    /// The kid.
    pub const KID: ClassMask = ClassMask(1 << 0);
    /// The flipped kid object of games that flip by swapping objects.
    pub const KID_FLIPPED: ClassMask = ClassMask(1 << 1);
    /// The kid's bullet.
    pub const KID_BULLET: ClassMask = ClassMask(1 << 2);
    /// Blocks the kid stands on and collides with.
    pub const BLOCK: ClassMask = ClassMask(1 << 3);
    /// The engines' second solid block parent (eat-the-rori-girl, help-the-fairy-tale platforms).
    pub const SOLIDBLOCK: ClassMask = ClassMask(1 << 4);
    /// Jump-through platforms.
    pub const PLATFORM: ClassMask = ClassMask(1 << 5);
    /// Anything that kills the kid on contact (spikes, cherries).
    pub const KILLER: ClassMask = ClassMask(1 << 6);
    /// Water, first variant.
    pub const WATER1: ClassMask = ClassMask(1 << 7);
    /// Water, second variant.
    pub const WATER2: ClassMask = ClassMask(1 << 8);
    /// Water, third variant.
    pub const WATER3: ClassMask = ClassMask(1 << 9);
    /// Vine on the kid's left.
    pub const VINE_L: ClassMask = ClassMask(1 << 10);
    /// Vine on the kid's right.
    pub const VINE_R: ClassMask = ClassMask(1 << 11);
    /// Edge room changers (`roomChanger`).
    pub const ROOM_CHANGER: ClassMask = ClassMask(1 << 12);
    /// Edge warps.
    pub const EDGE_WARP: ClassMask = ClassMask(1 << 13);
    /// Save points.
    pub const SAVE: ClassMask = ClassMask(1 << 14);
    /// Slip (ice) blocks.
    pub const SLIP: ClassMask = ClassMask(1 << 15);
    /// Slopes.
    pub const SLOPE: ClassMask = ClassMask(1 << 16);
    /// Slope walls.
    pub const SLOPE_WALL: ClassMask = ClassMask(1 << 17);
    /// Ladders.
    pub const LADDER: ClassMask = ClassMask(1 << 18);
    /// Fake blocks.
    pub const FAKE_BLOCK: ClassMask = ClassMask(1 << 19);
    /// Objects that stop kid bullets.
    pub const BULLET_BLOCKER: ClassMask = ClassMask(1 << 20);
    /// Trap triggers.
    pub const TRIGGER: ClassMask = ClassMask(1 << 21);
    /// Trap stoppers.
    pub const TRAP_STOPPER: ClassMask = ClassMask(1 << 22);
    /// Moving blocks.
    pub const MOVING_BLOCK: ClassMask = ClassMask(1 << 23);
    /// Conveyors.
    pub const CONVEYOR: ClassMask = ClassMask(1 << 24);
    /// Kid parameter zones.
    pub const PARAM_ZONE: ClassMask = ClassMask(1 << 25);
    /// The bow.
    pub const BOW: ClassMask = ClassMask(1 << 26);
    /// Blood.
    pub const BLOOD: ClassMask = ClassMask(1 << 27);
    /// Slopes that rise to the left (`nanameblockR`, `yaneM`); `slope` rises to the right.
    pub const SLOPE_MIRROR: ClassMask = ClassMask(1 << 28);

    /// Every named class with its package-format name, in bit order.
    pub const NAMED: [(ClassMask, &'static str); 29] = [
        (ClassMask::KID, "kid"),
        (ClassMask::KID_FLIPPED, "kid_flipped"),
        (ClassMask::KID_BULLET, "kid_bullet"),
        (ClassMask::BLOCK, "block"),
        (ClassMask::SOLIDBLOCK, "solidblock"),
        (ClassMask::PLATFORM, "platform"),
        (ClassMask::KILLER, "killer"),
        (ClassMask::WATER1, "water1"),
        (ClassMask::WATER2, "water2"),
        (ClassMask::WATER3, "water3"),
        (ClassMask::VINE_L, "vine_l"),
        (ClassMask::VINE_R, "vine_r"),
        (ClassMask::ROOM_CHANGER, "room_changer"),
        (ClassMask::EDGE_WARP, "edge_warp"),
        (ClassMask::SAVE, "save"),
        (ClassMask::SLIP, "slip"),
        (ClassMask::SLOPE, "slope"),
        (ClassMask::SLOPE_WALL, "slope_wall"),
        (ClassMask::LADDER, "ladder"),
        (ClassMask::FAKE_BLOCK, "fake_block"),
        (ClassMask::BULLET_BLOCKER, "bullet_blocker"),
        (ClassMask::TRIGGER, "trigger"),
        (ClassMask::TRAP_STOPPER, "trap_stopper"),
        (ClassMask::MOVING_BLOCK, "moving_block"),
        (ClassMask::CONVEYOR, "conveyor"),
        (ClassMask::PARAM_ZONE, "param_zone"),
        (ClassMask::BOW, "bow"),
        (ClassMask::BLOOD, "blood"),
        (ClassMask::SLOPE_MIRROR, "slope_mirror"),
    ];

    /// First bit of the package-defined classes.
    pub const PACKAGE_FIRST_BIT: u32 = 32;
    /// Number of package-defined classes.
    pub const PACKAGE_COUNT: u32 = 16;

    /// Package-defined class `n` (`0..16`), or `None` when `n` is out of range.
    pub const fn package(n: u32) -> Option<ClassMask> {
        if n < ClassMask::PACKAGE_COUNT {
            Some(ClassMask(1 << (ClassMask::PACKAGE_FIRST_BIT + n)))
        } else {
            None
        }
    }

    /// The named class with this package-format name.
    pub fn from_name(name: &str) -> Option<ClassMask> {
        ClassMask::NAMED.iter().find(|(_, n)| *n == name).map(|(c, _)| *c)
    }

    /// True when no bit is set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// True when every bit of `other` is set here.
    pub const fn contains(self, other: ClassMask) -> bool {
        self.0 & other.0 == other.0
    }

    /// True when the two sets share a bit.
    pub const fn intersects(self, other: ClassMask) -> bool {
        self.0 & other.0 != 0
    }
}

/// The schema of one class as written in a data file: its package name, from
/// [`ClassMask::NAMED`] (a list of classes is `Vec<ClassMask>`).
#[cfg(feature = "schema")]
impl schemars::JsonSchema for ClassMask {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ClassName".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let names: Vec<&str> = ClassMask::NAMED.iter().map(|(_, n)| *n).collect();
        schemars::json_schema!({
            "description": "A collision class by its package name.",
            "type": "string",
            "enum": names
        })
    }
}

impl BitOr for ClassMask {
    type Output = ClassMask;
    fn bitor(self, rhs: ClassMask) -> ClassMask {
        ClassMask(self.0 | rhs.0)
    }
}

impl BitOrAssign for ClassMask {
    fn bitor_assign(&mut self, rhs: ClassMask) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for ClassMask {
    type Output = ClassMask;
    fn bitand(self, rhs: ClassMask) -> ClassMask {
        ClassMask(self.0 & rhs.0)
    }
}

/// Default collision classes of a built-in (Design; plan 1.1 and 6.1).
pub fn default_classes(b: Builtin) -> ClassMask {
    match b {
        Builtin::Kid => ClassMask::KID,
        Builtin::KidBullet => ClassMask::KID_BULLET,
        Builtin::Blood => ClassMask::BLOOD,
        Builtin::Bow => ClassMask::BOW,
        Builtin::Block | Builtin::BreakableBlock => ClassMask::BLOCK,
        Builtin::Spike | Builtin::Killer => ClassMask::KILLER,
        Builtin::Save => ClassMask::SAVE,
        // GM warps and warpStart are roomChanger children.
        Builtin::Warp | Builtin::DifficultyWarp => ClassMask::ROOM_CHANGER,
        // The platform's parent is block in 17 games (CLONE_SPEC collision); the other two
        // games give their platform preset `counts_as = ["solidblock"]`.
        Builtin::Platform | Builtin::MovingPlatform | Builtin::DropPlatform | Builtin::PathPlatform => {
            ClassMask::PLATFORM | ClassMask::BLOCK
        }
        Builtin::Water => ClassMask::WATER1,
        Builtin::FakeBlock => ClassMask::FAKE_BLOCK,
        Builtin::Trigger => ClassMask::TRIGGER,
        Builtin::EdgeWarp => ClassMask::EDGE_WARP,
        Builtin::BulletBlocker => ClassMask::BULLET_BLOCKER,
        Builtin::Slope => ClassMask::SLOPE,
        Builtin::Conveyor => ClassMask::CONVEYOR,
        Builtin::KidParamZone => ClassMask::PARAM_ZONE,
        Builtin::TrapStopper => ClassMask::TRAP_STOPPER,
        Builtin::Ladder => ClassMask::LADDER,
        _ => ClassMask::NONE,
    }
}

/// Classes that a kind's params pick (Design): water `variant` 2 and 3, vine `side`, slope
/// `shape` and `scale_x`, a killer with `kill_outside_view` (no class: it kills in its own collision), and a
/// block with `kid_collides = false`, which has no class at all so the kid has no collision
/// event with it (docs/format/builtins/block.md).
pub fn param_classes(b: Builtin, params: &Params) -> Option<ClassMask> {
    let get = |k: &str| params.0.get(k);
    match b {
        Builtin::Block if matches!(get("kid_collides"), Some(ParamValue::Bool(false))) => {
            Some(ClassMask::NONE)
        }
        Builtin::Water => match get("variant") {
            Some(ParamValue::Real(v)) if v.to_bits() == 2.0f64.to_bits() => Some(ClassMask::WATER2),
            Some(ParamValue::Real(v)) if v.to_bits() == 3.0f64.to_bits() => Some(ClassMask::WATER3),
            _ => None,
        },
        Builtin::Killer if matches!(get("kill_outside_view"), Some(ParamValue::Bool(true))) => {
            Some(ClassMask::NONE)
        }
        // `shape` names which way the sprite's mask rises and a negative `scale_x` mirrors that
        // (docs/format/builtins/slope.md): rising left is the `slope_mirror` class, the kid's
        // yaneM or nanameblockR event (kid/A4.md, kid/A1.md RBF). The mask stays the sprite's.
        Builtin::Slope => {
            let left = matches!(get("shape"), Some(ParamValue::Str(s)) if s == "rise_left");
            let mirrored = matches!(get("scale_x"), Some(ParamValue::Real(v)) if v.get() < 0.0);
            Some(if left != mirrored {
                ClassMask::SLOPE_MIRROR
            } else {
                ClassMask::SLOPE
            })
        }
        Builtin::Vine => match get("side") {
            Some(ParamValue::Str(s)) if s == "left" => Some(ClassMask::VINE_L),
            Some(ParamValue::Str(s)) if s == "right" => Some(ClassMask::VINE_R),
            _ => None,
        },
        _ => None,
    }
}

/// The params [`param_classes`] reads for a built-in. A placed instance that sets them to pick
/// other classes than its kind's gets a derived kind with those classes (the loader, #659); a
/// script spawn that sets them uses that derived kind or fails (`script::inst`, #694).
pub fn class_params(b: Builtin) -> &'static [&'static str] {
    match b {
        Builtin::Block => &["kid_collides"],
        Builtin::Water => &["variant"],
        Builtin::Killer => &["kill_outside_view"],
        Builtin::Slope => &["shape", "scale_x"],
        Builtin::Vine => &["side"],
        _ => &[],
    }
}

/// A kind's classes: the param-picked class or the built-in default, plus `extra`.
pub fn kind_classes(b: Builtin, params: &Params, extra: ClassMask) -> ClassMask {
    param_classes(b, params).unwrap_or_else(|| default_classes(b)) | extra
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_bits_are_distinct_and_below_the_reserved_range() {
        let mut all = ClassMask::NONE;
        for (i, (c, _)) in ClassMask::NAMED.iter().enumerate() {
            assert_eq!(c.0, 1 << i, "bit order");
            assert!(!all.intersects(*c));
            all |= *c;
        }
        assert_eq!(all.0, (1 << 29) - 1);
    }

    #[test]
    fn package_bits() {
        assert_eq!(ClassMask::package(0), Some(ClassMask(1 << 32)));
        assert_eq!(ClassMask::package(15), Some(ClassMask(1 << 47)));
        assert_eq!(ClassMask::package(16), None);
        assert_eq!(ClassMask::from_name("platform"), Some(ClassMask::PLATFORM));
        assert_eq!(ClassMask::from_name("parent"), None);
        assert!((ClassMask::PLATFORM | ClassMask::BLOCK).contains(ClassMask::BLOCK));
    }
}
