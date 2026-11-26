//! The `ruleset`, `[compat]` profile and deviation catalogue (docs/format/game.md; decision
//! 0005, #1160).
//!
//! A format-0 package that names none of them means `gm8` at its `engine_rev`, with the profile
//! inferred from the physics preset. Naming them explicitly cannot change an outcome in format
//! 0: the engine's only ruleset is `gm8`, every profile bundle that exists is the family preset
//! the package already selected, and a deviation must declare the value the physics already
//! resolves to. The declarations therefore normalize to the same `gameplay_hash` as the absent
//! fields ([`crate::load::gameplay_hash`]). Format 1 requires `ruleset` and `[compat] profile`
//! and hashes them as written (docs/format/v1.md); the profile layer that lets a deviation
//! select another value is #1160.
//!
//! A deviation is package data, never code: it names one entry of [`DEVIATIONS`], a closed
//! catalogue of enum values the engine already implements, and says why it is there. The
//! catalogue grows with the compatibility moves of #1076; an id that is not here is an error.

use iwbtg_sim::def::physics::{
    AirJump, Downhill, FallCapRule, FlipKind, GravityReapply, IceModel, JumpCancel, MirrorContact,
    PlatformTake, SavePosition, VineModel, WalkRead, WaterAir,
};
use iwbtg_sim::def::{KidFamily, PhysicsProfile};
use serde::Deserialize;

/// The one ruleset this build implements. Format 0 uses it at the package's `engine_rev`.
pub const GM8: &str = "gm8";

/// The revision of each profile bundle this build knows. One revision exists today.
pub const PROFILE_REV: u16 = 1;

/// Kid families a profile may name, in [`KidFamily`] order. The profile's family is the physics
/// preset's family in format 0.
pub const FAMILIES: [&str; 7] = ["a1", "a2", "a3", "a4", "a5", "b", "c"];

/// The package name of a family preset (`a1`, `b`, ...).
#[must_use]
pub fn family_name(family: KidFamily) -> &'static str {
    match family {
        KidFamily::A1 => "a1",
        KidFamily::A2 => "a2",
        KidFamily::A3 => "a3",
        KidFamily::A4 => "a4",
        KidFamily::A5 => "a5",
        KidFamily::B => "b",
        KidFamily::C => "c",
    }
}

/// A parsed `ruleset = "<name>@<rev>"`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Ruleset {
    /// `gm8` in format 0.
    pub name: String,
    /// The built-in behavior revision.
    pub rev: u16,
}

impl Ruleset {
    /// Parses `<name>@<rev>`.
    pub fn parse(text: &str) -> Result<Ruleset, String> {
        let (name, rev) = text
            .split_once('@')
            .ok_or_else(|| format!("ruleset `{text}` must be `<name>@<rev>` (`gm8@0`)"))?;
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(format!("ruleset name `{name}` uses only a-z, 0-9 and `_`"));
        }
        let rev = rev
            .parse::<u16>()
            .map_err(|_| format!("ruleset `{text}`: `{rev}` is not a revision number"))?;
        Ok(Ruleset {
            name: name.to_owned(),
            rev,
        })
    }

    /// The canonical spelling.
    #[must_use]
    pub fn name_rev(&self) -> String {
        format!("{}@{}", self.name, self.rev)
    }
}

/// A parsed `[compat] profile = "<ruleset>/<family>@<n>"`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Profile {
    /// The ruleset the bundle belongs to (`gm8`).
    pub ruleset: String,
    /// The kid family (`a1`, `b`, ...).
    pub family: String,
    /// The bundle revision.
    pub rev: u16,
}

impl Profile {
    /// Parses `<ruleset>/<family>@<n>`.
    pub fn parse(text: &str) -> Result<Profile, String> {
        let (ruleset, rest) = text
            .split_once('/')
            .ok_or_else(|| format!("profile `{text}` must be `<ruleset>/<family>@<n>` (`gm8/a1@1`)"))?;
        let (family, rev) = rest
            .split_once('@')
            .ok_or_else(|| format!("profile `{text}` must be `<ruleset>/<family>@<n>` (`gm8/a1@1`)"))?;
        if !FAMILIES.contains(&family) {
            return Err(format!(
                "profile family `{family}` is not one of {}",
                FAMILIES.join(", ")
            ));
        }
        let rev = rev
            .parse::<u16>()
            .map_err(|_| format!("profile `{text}`: `{rev}` is not a revision number"))?;
        if rev != PROFILE_REV {
            return Err(format!(
                "profile `{text}`: this build knows revision {PROFILE_REV} only"
            ));
        }
        Ok(Profile {
            ruleset: ruleset.to_owned(),
            family: family.to_owned(),
            rev,
        })
    }

    /// The canonical spelling.
    #[must_use]
    pub fn id(&self) -> String {
        format!("{}/{}@{}", self.ruleset, self.family, self.rev)
    }
}

/// A deviation's value, as written in `game.toml`. The catalogue decides which shape is allowed.
#[derive(Clone, PartialEq, Debug, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum DeviationValue {
    /// A flag.
    Bool(bool),
    /// An integer.
    Integer(i64),
    /// A real.
    Real(f64),
    /// An enum value, by its package name.
    Str(String),
}

impl DeviationValue {
    /// The value as an enum spelling, when it is a string.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            DeviationValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// A short spelling for the `pkg check` line.
    #[must_use]
    pub fn show(&self) -> String {
        match self {
            DeviationValue::Bool(b) => b.to_string(),
            DeviationValue::Integer(i) => i.to_string(),
            DeviationValue::Real(r) => r.to_string(),
            DeviationValue::Str(s) => s.clone(),
        }
    }
}

/// What a catalogue entry accepts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// One of the listed enum spellings, written as a string.
    Enum(&'static [&'static str]),
    /// A bool.
    Bool,
}

/// One closed-catalogue deviation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DeviationSpec {
    /// The id a package writes (`save.position`).
    pub id: &'static str,
    /// What values it accepts.
    pub kind: Kind,
    /// What the value selects.
    pub doc: &'static str,
}

/// The closed deviation catalogue (decision 0005, "Package shape"). Every entry is an existing
/// enum in `physics.toml`; #1076 adds entries as compatibility moves are promoted. The format-0
/// loader requires the declared value to equal the value the package already resolves to, so a
/// declaration never changes a run until the profile layer lands.
pub const DEVIATIONS: [DeviationSpec; 14] = [
    DeviationSpec {
        id: "save.position",
        kind: Kind::Enum(&[
            "xy_base100_bytes",
            "xy_base256_bytes",
            "xy_text2_dec",
            "xy_previous",
            "fixed_point",
            "floor_nudge",
        ]),
        doc: "How a checkpoint stores the kid's position.",
    },
    DeviationSpec {
        id: "jump.cancel",
        kind: Kind::Enum(&["edge", "poll_alarm"]),
        doc: "How a jump release is read.",
    },
    DeviationSpec {
        id: "jump.air",
        kind: Kind::Enum(&["djump", "cur_jumps", "djump_then_tjump"]),
        doc: "The air-jump model.",
    },
    DeviationSpec {
        id: "jump.water_air",
        kind: Kind::Enum(&["none", "water2_without_flag", "water_unlimited", "water3_keeps"]),
        doc: "When water allows the air jump.",
    },
    DeviationSpec {
        id: "fall_cap.rule",
        kind: Kind::Enum(&[
            "clamp_abs",
            "clamp_against_gravity",
            "clamp_down_only",
            "shift_then_clamp",
        ]),
        doc: "How the vertical speed is clamped.",
    },
    DeviationSpec {
        id: "flip.model",
        kind: Kind::Enum(&["none", "swap_object", "yflag", "grav_sign"]),
        doc: "How gravity flipping works.",
    },
    DeviationSpec {
        id: "platform.take",
        kind: Kind::Enum(&["vspeed", "delta_y", "yspeed_flipped"]),
        doc: "What a platform landing copies from the platform.",
    },
    DeviationSpec {
        id: "ice.model",
        kind: Kind::Enum(&["none", "slip"]),
        doc: "How slip blocks affect walking.",
    },
    DeviationSpec {
        id: "input.walk",
        kind: Kind::Enum(&["direct_latch", "sampled"]),
        doc: "How the walk action reads the direction keys.",
    },
    DeviationSpec {
        id: "input.vine_keys",
        kind: Kind::Enum(&["edges", "latched_flags"]),
        doc: "How the vine action reads its keys.",
    },
    DeviationSpec {
        id: "vine.model",
        kind: Kind::Enum(&["none", "distance_asym", "distance_sym_early"]),
        doc: "The vine grab model.",
    },
    DeviationSpec {
        id: "gravity.reapply",
        kind: Kind::Enum(&["step", "end_step_slope", "create_only"]),
        doc: "When gravity is reapplied.",
    },
    DeviationSpec {
        id: "slope.mirror_contact",
        kind: Kind::Enum(&["normal", "left", "right"]),
        doc: "The contact a mirrored slope gives.",
    },
    DeviationSpec {
        id: "slope.downhill",
        kind: Kind::Enum(&["none", "por", "rainbow", "fairy_tale"]),
        doc: "How the kid snaps down a downhill slope.",
    },
];

/// The catalogue entry `id`, if there is one.
#[must_use]
pub fn spec(id: &str) -> Option<&'static DeviationSpec> {
    DEVIATIONS.iter().find(|d| d.id == id)
}

/// The value `id` resolves to in `physics`, by its package spelling; `None` for an id outside
/// the catalogue.
#[must_use]
pub fn resolved(id: &str, physics: &PhysicsProfile) -> Option<&'static str> {
    Some(match id {
        "save.position" => match physics.save.position {
            SavePosition::XyBase100Bytes => "xy_base100_bytes",
            SavePosition::XyBase256Bytes => "xy_base256_bytes",
            SavePosition::XyText2Dec => "xy_text2_dec",
            SavePosition::XyPrevious => "xy_previous",
            SavePosition::FixedPoint => "fixed_point",
            SavePosition::FloorNudge => "floor_nudge",
        },
        "jump.cancel" => match physics.jump.cancel {
            JumpCancel::Edge => "edge",
            JumpCancel::PollAlarm => "poll_alarm",
        },
        "jump.air" => match physics.jump.air {
            AirJump::Djump => "djump",
            AirJump::CurJumps => "cur_jumps",
            AirJump::DjumpThenTjump => "djump_then_tjump",
        },
        "jump.water_air" => match physics.jump.water_air {
            WaterAir::None => "none",
            WaterAir::Water2WithoutFlag => "water2_without_flag",
            WaterAir::WaterUnlimited => "water_unlimited",
            WaterAir::Water3Keeps => "water3_keeps",
        },
        "fall_cap.rule" => match physics.fall_cap.rule {
            FallCapRule::ClampAbs => "clamp_abs",
            FallCapRule::ClampAgainstGravity => "clamp_against_gravity",
            FallCapRule::ClampDownOnly => "clamp_down_only",
            FallCapRule::ShiftThenClamp => "shift_then_clamp",
        },
        "flip.model" => match physics.flip.model {
            FlipKind::None => "none",
            FlipKind::SwapObject => "swap_object",
            FlipKind::Yflag => "yflag",
            FlipKind::GravSign => "grav_sign",
        },
        "platform.take" => match physics.platform.take {
            PlatformTake::Vspeed => "vspeed",
            PlatformTake::DeltaY => "delta_y",
            PlatformTake::YspeedFlipped => "yspeed_flipped",
        },
        "ice.model" => match physics.ice.model {
            IceModel::None => "none",
            IceModel::Slip => "slip",
        },
        "input.walk" => match physics.input.walk {
            WalkRead::DirectLatch => "direct_latch",
            WalkRead::Sampled => "sampled",
        },
        "input.vine_keys" => match physics.input.vine_keys {
            iwbtg_sim::def::physics::VineKeys::Edges => "edges",
            iwbtg_sim::def::physics::VineKeys::LatchedFlags => "latched_flags",
        },
        "vine.model" => match physics.vine.model {
            VineModel::None => "none",
            VineModel::DistanceAsym => "distance_asym",
            VineModel::DistanceSymEarly => "distance_sym_early",
        },
        "gravity.reapply" => match physics.gravity.reapply {
            GravityReapply::Step => "step",
            GravityReapply::EndStepSlope => "end_step_slope",
            GravityReapply::CreateOnly => "create_only",
        },
        "slope.mirror_contact" => match physics.slope.mirror_contact {
            MirrorContact::Normal => "normal",
            MirrorContact::Left => "left",
            MirrorContact::Right => "right",
        },
        "slope.downhill" => match physics.slope.downhill {
            Downhill::None => "none",
            Downhill::Por => "por",
            Downhill::Rainbow => "rainbow",
            Downhill::FairyTale => "fairy_tale",
        },
        _ => return None,
    })
}

/// The resolved ruleset, profile and deviations a package plays under (decision 0005). Printed
/// by `pkg check`; the values are also what `gameplay_hash` normalizes to in format 0.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Resolved {
    /// `gm8@<engine_rev>` in format 0.
    pub ruleset: String,
    /// The profile bundle id (`gm8/a1@1`).
    pub profile: String,
    /// Declared deviations as `id=value`, in written order.
    pub deviations: Vec<String>,
    /// True when the package wrote an explicit `ruleset` or `[compat]`.
    pub explicit: bool,
}

impl Resolved {
    /// The `pkg check` lines.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut out = vec![
            format!("ruleset: {}", self.ruleset),
            format!("profile: {}", self.profile),
        ];
        for deviation in &self.deviations {
            out.push(format!("deviation: {deviation}"));
        }
        out
    }
}

/// The ruleset, profile and deviations `def` plays under. Absent fields resolve to `gm8` at the
/// package's revision and to the physics preset's family bundle (decision 0005).
#[must_use]
pub fn resolve(manifest: &crate::manifest::Manifest, def: &iwbtg_sim::def::GameDef) -> Resolved {
    let explicit = manifest.ruleset.is_some() || manifest.compat.is_some();
    let ruleset = manifest
        .ruleset
        .as_ref()
        .and_then(|r| Ruleset::parse(r.get_ref()).ok())
        .map_or_else(|| format!("{GM8}@{}", def.engine_rev), |r| r.name_rev());
    let profile = manifest
        .compat
        .as_ref()
        .and_then(|c| c.get_ref().profile.as_ref())
        .and_then(|p| Profile::parse(p.get_ref()).ok())
        .map_or_else(
            || format!("{GM8}/{}@{PROFILE_REV}", family_name(def.physics.preset)),
            |p| p.id(),
        );
    let deviations = manifest.compat.as_ref().map_or_else(Vec::new, |c| {
        c.get_ref()
            .deviation
            .iter()
            .map(|d| format!("{}={}", d.id.get_ref(), d.value.get_ref().show()))
            .collect()
    });
    Resolved {
        ruleset,
        profile,
        deviations,
        explicit,
    }
}
