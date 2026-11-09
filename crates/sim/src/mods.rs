//! Mods: the table the client, the UI and the verifier read, and the set a replay header carries
//! (docs/product.md, Mods; docs/architecture/replay.md, "Mods").
//!
//! Every mod has a fixed bit in [`ModSet`]. A bit, once given, is never reused or moved: stored
//! replays name their mods by it. No mod changes what the sim computes; Sudden Death only says
//! where a run ends ([`ModSet::ends_run`]), and the rest act on the client (drawing, clock,
//! save states).

use serde::{Deserialize, Serialize};

/// How the mod select sheet groups mods.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModKind {
    /// Makes a run harder.
    Challenge,
    /// Changes only what is drawn.
    Visual,
    /// Helps practice; never ranked.
    Practice,
}

/// One mod. The discriminant is its bit in [`ModSet`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum Mod {
    /// One death ends the run.
    SuddenDeath = 0,
    /// Draws the hitboxes.
    Hitboxes = 1,
    /// Plain backdrop instead of the room's background.
    NoBackground = 2,
    /// Save and load the whole state anywhere.
    SaveStates = 3,
    /// The clock runs slower.
    SlowMotion = 4,
    /// One tick per key press.
    FrameAdvance = 5,
}

/// What the UI shows for a mod and whether a run with it can be ranked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ModDef {
    /// The mod.
    pub id: Mod,
    /// Two-letter code, shown on mod tiles and in board names (`SD`).
    pub code: &'static str,
    /// Name on the mod select sheet.
    pub name: &'static str,
    /// One-line description on the mod select sheet.
    pub description: &'static str,
    /// The sheet's column.
    pub kind: ModKind,
    /// A run with this mod can be ranked (on its own board, as every combination is).
    pub ranked: bool,
}

/// Every mod, in sheet order (by kind, then as listed).
pub const MODS: [ModDef; 6] = [
    ModDef {
        id: Mod::SuddenDeath,
        code: "SD",
        name: "Sudden Death",
        description: "One death ends the run.",
        kind: ModKind::Challenge,
        ranked: true,
    },
    ModDef {
        id: Mod::Hitboxes,
        code: "HB",
        name: "Hitboxes",
        description: "Draw the kid's and every hazard's hitbox.",
        kind: ModKind::Visual,
        ranked: true,
    },
    ModDef {
        id: Mod::NoBackground,
        code: "NB",
        name: "No background",
        description: "Plain backdrop behind the room.",
        kind: ModKind::Visual,
        ranked: true,
    },
    ModDef {
        id: Mod::SaveStates,
        code: "SS",
        name: "Save states",
        description: "Save and load anywhere.",
        kind: ModKind::Practice,
        ranked: false,
    },
    ModDef {
        id: Mod::SlowMotion,
        code: "SL",
        name: "Slow motion",
        description: "Play at a lower speed.",
        kind: ModKind::Practice,
        ranked: false,
    },
    ModDef {
        id: Mod::FrameAdvance,
        code: "FA",
        name: "Frame advance",
        description: "Step one tick at a time.",
        kind: ModKind::Practice,
        ranked: false,
    },
];

impl Mod {
    /// The mod's table row.
    pub fn def(self) -> &'static ModDef {
        // MODS lists every mod once, so the search always finds it.
        MODS.iter()
            .find(|d| d.id == self)
            .expect("MODS lists every mod")
    }

    /// The mod whose code is `code` (case ignored).
    pub fn from_code(code: &str) -> Option<Mod> {
        MODS.iter()
            .find(|d| d.code.eq_ignore_ascii_case(code))
            .map(|d| d.id)
    }

    fn bit(self) -> u32 {
        1 << (self as u8)
    }
}

/// A set of mods: the replay header's `mods` field, one bit per [`Mod`]. Encoded as a plain `u32`
/// (a varint in the replay).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModSet(u32);

impl ModSet {
    /// No mods: a plain run.
    pub const NONE: ModSet = ModSet(0);

    /// The bits of every mod in [`MODS`].
    pub const KNOWN_BITS: u32 = (1 << MODS.len()) - 1;

    /// The set these bits stand for, or `None` when a bit names no mod.
    pub fn from_bits(bits: u32) -> Option<ModSet> {
        (bits & !Self::KNOWN_BITS == 0).then_some(ModSet(bits))
    }

    /// The raw bits.
    pub fn bits(self) -> u32 {
        self.0
    }

    /// Whether every bit names a mod in [`MODS`].
    pub fn is_known(self) -> bool {
        self.0 & !Self::KNOWN_BITS == 0
    }

    /// The set with `m` added.
    #[must_use]
    pub fn with(self, m: Mod) -> ModSet {
        ModSet(self.0 | m.bit())
    }

    /// Whether `m` is in the set.
    pub fn contains(self, m: Mod) -> bool {
        self.0 & m.bit() != 0
    }

    /// The set's mods in [`MODS`] order.
    pub fn iter(self) -> impl Iterator<Item = &'static ModDef> {
        MODS.iter().filter(move |d| self.contains(d.id))
    }

    /// The first mod (in [`MODS`] order) that keeps a run with this set off the boards, for the
    /// loading card's Unranked pill; `None` when the set can be ranked.
    pub fn unranked_by(self) -> Option<&'static ModDef> {
        self.iter().find(|d| !d.ranked)
    }

    /// The codes joined by `+` (`SD+NB`), or `NM` (no mod) for the empty set. Names the set's
    /// board.
    pub fn codes(self) -> String {
        if self.0 == 0 {
            return "NM".to_owned();
        }
        self.iter().map(|d| d.code).collect::<Vec<_>>().join("+")
    }

    /// Reads codes separated by `,` or `+` (`sd,nb`, `SD+NB`; case and spaces ignored). An empty
    /// string or `NM` is the empty set.
    pub fn parse(text: &str) -> Result<ModSet, String> {
        let mut set = ModSet::NONE;
        for code in text.split([',', '+']).map(str::trim).filter(|c| !c.is_empty()) {
            if code.eq_ignore_ascii_case("NM") {
                continue;
            }
            let m = Mod::from_code(code).ok_or_else(|| {
                let known: Vec<_> = MODS.iter().map(|d| d.code).collect();
                format!("unknown mod `{code}` (known: {})", known.join(", "))
            })?;
            set = set.with(m);
        }
        Ok(set)
    }

    /// Whether the run ends after a tick that took the death counter (`global.death`) from
    /// `deaths_before` to `deaths_after`: with Sudden Death, any death does. The client stops
    /// the run on that tick and the recorded replay ends there; the verifier rejects a replay
    /// with inputs past it.
    pub fn ends_run(self, deaths_before: u32, deaths_after: u32) -> bool {
        self.contains(Mod::SuddenDeath) && deaths_after > deaths_before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mod_has_one_row_and_its_bit_matches_its_place() {
        for (i, d) in MODS.iter().enumerate() {
            assert_eq!(d.id as usize, i, "{}", d.code);
            assert_eq!(d.id.def(), d);
            assert_eq!(d.code.len(), 2);
            assert!(d.description.ends_with('.'));
        }
    }

    #[test]
    fn practice_mods_are_never_ranked() {
        for d in &MODS {
            if d.kind == ModKind::Practice {
                assert!(!d.ranked, "{}", d.code);
            }
        }
    }

    #[test]
    fn codes_parse_and_print() {
        let set = ModSet::parse("nb, SD").unwrap();
        assert_eq!(set.codes(), "SD+NB");
        assert_eq!(ModSet::parse("SD+NB").unwrap(), set);
        assert_eq!(ModSet::parse("").unwrap(), ModSet::NONE);
        assert_eq!(ModSet::parse("NM").unwrap().codes(), "NM");
        assert!(ModSet::parse("XX").unwrap_err().contains("unknown mod `XX`"));
    }

    #[test]
    fn unknown_bits_are_rejected() {
        assert_eq!(ModSet::from_bits(0b11), Some(ModSet::NONE.with(Mod::SuddenDeath).with(Mod::Hitboxes)));
        assert_eq!(ModSet::from_bits(1 << 31), None);
        assert!(!ModSet(1 << MODS.len()).is_known());
    }

    #[test]
    fn the_first_unranked_mod_is_the_reason() {
        assert_eq!(ModSet::parse("SD,NB").unwrap().unranked_by(), None);
        let set = ModSet::parse("FA,SD,SL").unwrap();
        assert_eq!(set.unranked_by().map(|d| d.code), Some("SL"));
    }

    #[test]
    fn sudden_death_ends_the_run_on_a_death() {
        let sd = ModSet::NONE.with(Mod::SuddenDeath);
        assert!(sd.ends_run(3, 4));
        assert!(!sd.ends_run(4, 4));
        // A reset that clears the counter is no death.
        assert!(!sd.ends_run(4, 0));
        assert!(!ModSet::NONE.ends_run(0, 1));
    }
}
