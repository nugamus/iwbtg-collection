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
//! | 0-27 | the named classes below |
//! | 28-31 | reserved for WP11 (a WP that needs a new named bit asks the integrator) |
//! | 32-47 | 16 package-defined classes ([`ClassMask::package`]) |
//! | 48-63 | reserved |
//!
//! WP0 declares; WP11 owns afterwards.

use core::ops::{BitAnd, BitOr, BitOrAssign};

use serde::{Deserialize, Serialize};

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

    /// Every named class with its package-format name, in bit order.
    pub const NAMED: [(ClassMask, &'static str); 28] = [
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
        assert_eq!(all.0, (1 << 28) - 1);
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
