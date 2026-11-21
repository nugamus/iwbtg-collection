//! Collision classes: the sim's stand-in for GM parent objects (docs/architecture/sim/world.md).
//!
//! A collision event or query "with object X" becomes "with class X". Each built-in declares its
//! default classes; a package object may add classes (`counts_as`). A collision event with a
//! parent also fires for its children (Confirmed, manual 205_00), which class sets reproduce
//! without a parent tree.
//!
//! Bit layout (decision 0004; WP0 fixed the named range):
//!
//! | Bits | Use |
//! |---|---|
//! | 0-28 | the named classes below (bit 28, `slope_mirror`, added by WP5c.4c) |
//! | 29-31 | reserved (a new named bit asks the integrator) |
//! | 32-127 | package-defined classes ([`ClassMask::package`], 96) |
//!
//! The first 16 package bits (32-47) keep the positions WP0 gave them, so every package that
//! fits in the old 16 classes hashes to the same bytes (postcard writes `u64` and `u128` with the
//! same LEB128 varint). Decision 0004 raised the cap from 16 to 96.

use core::ops::{BitAnd, BitOr, BitOrAssign};

use serde::{Deserialize, Serialize};

use super::builtin::Builtin;
use super::kind::{KindDef, ParamValue, Params};

/// A set of collision classes, one bit each.
///
/// A `u128` since decision 0004 so a package can define up to 96 classes. Values below 2^64
/// serialize exactly as the old `u64` did (postcard's LEB128 varint), so existing state hashes
/// and replays do not move.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize)]
pub struct ClassMask(pub u128);

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
    /// Number of package-defined classes (decision 0004 raised this from 16).
    pub const PACKAGE_COUNT: u32 = 96;

    /// Package-defined class `n` (`0..96`), or `None` when `n` is out of range.
    pub const fn package(n: u32) -> Option<ClassMask> {
        if n < ClassMask::PACKAGE_COUNT {
            Some(ClassMask(1u128 << (ClassMask::PACKAGE_FIRST_BIT + n)))
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

    /// Per-class specificity for collision-handler selection (C8, R12b), declared as data: the
    /// matching handler class with the lowest number runs, then the first in handler order.
    /// The defaults reproduce the measured GM parent chains: `block` (2) is the platform's
    /// parent in 17 games and ranks last, `solidblock` (1) next in the other two, where the
    /// platform also keeps the `block` bit (CLONE_SPEC collision, "One event per pair: the most
    /// specific ancestor"). A class not listed has [`ClassMask::DEFAULT_SPECIFICITY`].
    pub const SPECIFICITY: [(ClassMask, u8); 2] = [(ClassMask::BLOCK, 2), (ClassMask::SOLIDBLOCK, 1)];

    /// The specificity of every class not listed in [`ClassMask::SPECIFICITY`]: most specific.
    pub const DEFAULT_SPECIFICITY: u8 = 0;

    /// The value [`ClassMask::generality`] gives a mask that matches no class bit. It can match
    /// no handler, so the value is unobservable; it keeps the old last rank.
    const EMPTY_SPECIFICITY: u8 = 2;

    /// How far up a GM parent chain this handler class sits, for picking one collision handler
    /// per pair (C8, R12b): lower is more specific. This reads the [`ClassMask::SPECIFICITY`]
    /// data; a handler mask with several bits takes its lowest number, and any bit outside the
    /// declared classes takes [`ClassMask::DEFAULT_SPECIFICITY`].
    pub const fn generality(self) -> u8 {
        // The declared classes, so a bit outside them can take the default.
        let mut declared = ClassMask::NONE;
        let mut i = 0;
        while i < Self::SPECIFICITY.len() {
            declared = ClassMask(declared.0 | Self::SPECIFICITY[i].0.0);
            i += 1;
        }
        let mut low = if self.intersects(ClassMask(!declared.0)) {
            Self::DEFAULT_SPECIFICITY
        } else {
            Self::EMPTY_SPECIFICITY
        };
        let mut i = 0;
        while i < Self::SPECIFICITY.len() {
            let (class, specificity) = Self::SPECIFICITY[i];
            if self.intersects(class) && specificity < low {
                low = specificity;
            }
            i += 1;
        }
        low
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

/// Default collision classes of a built-in (Design; docs/architecture/sim/world.md and
/// docs/architecture/sim/builtins.md).
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
/// script spawn that sets them uses that derived kind or fails (`script::inst`, #694). The params
/// that pick the event set work the same way ([`kind_params`]).
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

/// The params that pick a kind's classes ([`class_params`]) or its event set
/// (`builtins::event_set_params`, #853), each once, in that order.
pub fn kind_params(b: Builtin) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = class_params(b).to_vec();
    for p in crate::builtins::event_set_params(b) {
        if !out.contains(p) {
            out.push(p);
        }
    }
    out
}

/// Whether `merged` (`kind`'s params with an instance's [`kind_params`] values over them) picks
/// other classes or another event set than `kind`'s own; `extra` is the kind's `counts_as`
/// classes. Then the instance needs a kind derived for those values (#659, #853).
pub fn picks_other_kind(kind: &KindDef, merged: &Params, extra: ClassMask) -> bool {
    if kind_classes(kind.base, merged, extra) != kind.classes {
        return true;
    }
    let mut with = kind.clone();
    with.params = merged.clone();
    crate::builtins::event_set(kind.base, &with) != crate::builtins::event_set(kind.base, kind)
}

/// A kind's classes: the param-picked class or the built-in default, plus `extra`.
pub fn kind_classes(b: Builtin, params: &Params, extra: ClassMask) -> ClassMask {
    param_classes(b, params).unwrap_or_else(|| default_classes(b)) | extra
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(ty: crate::builtins::ParamType) -> Vec<ParamValue> {
        use crate::builtins::ParamType;
        use crate::num::Real;
        match ty {
            ParamType::Real => [0.0, 1.0, -1.0, 2.5]
                .iter()
                .map(|v| ParamValue::Real(Real::new(*v)))
                .collect(),
            ParamType::Bool => vec![ParamValue::Bool(false), ParamValue::Bool(true)],
            ParamType::Enum(words) => words.iter().map(|w| ParamValue::Str((*w).into())).collect(),
            _ => vec![ParamValue::Str("x".into())],
        }
    }

    /// #853: `builtins::event_set_params` lists every param `builtins::event_set` reads. Each
    /// unlisted param, at each sample value, leaves the event set alone, on a kind with no params
    /// and on one with any listed param at any sample value.
    #[test]
    fn event_set_params_cover_event_set() {
        use crate::builtins::{event_set, event_set_params, param_specs};
        let mut missing = std::collections::BTreeSet::new();
        for b in Builtin::ALL {
            let listed = event_set_params(b);
            let specs = param_specs(b);
            for name in listed {
                assert!(
                    specs.iter().any(|s| s.name == *name),
                    "{b:?} lists `{name}`, not one of its params"
                );
            }
            let mut bases = vec![Params::default()];
            for s in specs.iter().filter(|s| listed.contains(&s.name)) {
                for v in samples(s.ty) {
                    let mut p = Params::default();
                    p.0.insert(s.name.into(), v);
                    bases.push(p);
                }
            }
            for base in &bases {
                let mut kind = KindDef::new("k", b);
                kind.params = base.clone();
                let own = event_set(b, &kind);
                for s in specs.iter().filter(|s| !listed.contains(&s.name)) {
                    for v in samples(s.ty) {
                        let mut with = kind.clone();
                        with.params.0.insert(s.name.into(), v.clone());
                        if event_set(b, &with) != own {
                            missing.insert(format!("{b:?} `{}`", s.name));
                        }
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "these params change the event set; list them in builtins::event_set_params: {missing:#?}"
        );
    }

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
        // Decision 0004: bits 48-127 are now package classes too.
        assert_eq!(ClassMask::package(16), Some(ClassMask(1 << 48)));
        assert_eq!(ClassMask::package(95), Some(ClassMask(1u128 << 127)));
        assert_eq!(ClassMask::package(96), None);
        assert_eq!(ClassMask::from_name("platform"), Some(ClassMask::PLATFORM));
        assert_eq!(ClassMask::from_name("parent"), None);
        assert!((ClassMask::PLATFORM | ClassMask::BLOCK).contains(ClassMask::BLOCK));
    }

    /// #1075: the first 16 package bits keep their positions and postcard's LEB128 varint encodes
    /// a `u128` below 2^64 exactly as the old `u64`, so existing state hashes and replays do not
    /// move. The named bits are checked the same way.
    #[test]
    fn low_bits_encode_as_the_old_u64() {
        let encoded = |mask: ClassMask| postcard::to_allocvec(&mask).expect("serializes");
        let encoded_u64 = |bits: u64| postcard::to_allocvec(&bits).expect("serializes");
        for n in 0..16 {
            let mask = ClassMask::package(n).expect("package class");
            let bits = u64::try_from(mask.0).expect("fits below 2^64");
            assert_eq!(encoded(mask), encoded_u64(bits), "package class {n}");
        }
        for (mask, name) in ClassMask::NAMED {
            let bits = u64::try_from(mask.0).expect("fits below 2^64");
            assert_eq!(encoded(mask), encoded_u64(bits), "named class {name}");
        }
        // The first 32 package classes (bits 32-63) still fit the old `u64` bytes; a class at bit
        // 64 or above needs the wider varint, as a new package may.
        let high = ClassMask::package(32).expect("33rd class");
        assert_eq!(high.0, 1u128 << 64);
        assert_ne!(encoded(high), encoded_u64(0));
    }

    #[test]
    fn generality_puts_the_block_parents_last() {
        assert_eq!(
            ClassMask::SPECIFICITY,
            [(ClassMask::BLOCK, 2), (ClassMask::SOLIDBLOCK, 1)]
        );
        assert_eq!(ClassMask::DEFAULT_SPECIFICITY, 0);
        assert_eq!(ClassMask::PLATFORM.generality(), 0);
        assert_eq!(ClassMask::KILLER.generality(), 0);
        assert_eq!(ClassMask::package(0).unwrap().generality(), 0);
        assert_eq!(ClassMask::SOLIDBLOCK.generality(), 1);
        assert_eq!(ClassMask::BLOCK.generality(), 2);
        assert_eq!((ClassMask::BLOCK | ClassMask::SOLIDBLOCK).generality(), 1);
        assert_eq!((ClassMask::BLOCK | ClassMask::KILLER).generality(), 0);
    }
}
