//! The package manifest `game.toml` (docs/format/v0.md, Manifest).
//!
//! Field names, required fields and allowed values follow v0.md exactly. Every struct denies
//! unknown keys, so a typo or a field from a newer format fails with file, line and column.
//! Fields that later checks point at keep their span ([`Spanned`]).

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use serde::Deserialize;
use toml::Spanned;
use toml::value::Datetime;

use crate::diag::Diagnostic;
use crate::files::{PackageFiles, check_path};
use crate::ids::{KindRef, check_id, check_semver};
use crate::number::Number;

/// Path of the manifest inside a package.
pub const MANIFEST_PATH: &str = "game.toml";

/// Newest format major version this build reads. Format 0 is the v0 draft.
pub const FORMAT_VERSION: u32 = 0;

/// Default room speed when `tick_rate` is absent.
pub const DEFAULT_TICK_RATE: u32 = 50;

/// Default script budget per tick when `[budget].ops_per_tick` is absent
/// (docs/decisions/0001-script-language.md, Step budget).
pub const DEFAULT_OPS_PER_TICK: u64 = 1_000_000;

/// Hard cap on `[budget].ops_per_tick` (docs/decisions/0001-script-language.md, Step budget).
pub const MAX_OPS_PER_TICK: u64 = 2_000_000;

/// The parsed `game.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Format major version. The client reads every version up to [`FORMAT_VERSION`].
    pub format: Spanned<u32>,
    /// Package id, snake_case. Stable forever; leaderboards key on it.
    pub id: Spanned<String>,
    /// Display title.
    pub title: Spanned<String>,
    /// Semver of our port. For humans.
    pub version: Spanned<String>,
    /// Version of the original we ported.
    #[serde(default)]
    pub source_version: Option<Spanned<String>>,
    /// Engine pack id in `games/_engines/`.
    #[serde(default)]
    pub engine: Option<Spanned<String>>,
    /// Built-in behavior revision this package was verified against.
    pub engine_rev: Spanned<u16>,
    /// Original runner; selects event-order differences.
    pub runner: Spanned<Runner>,
    /// Path to the physics profile.
    pub physics: Spanned<String>,
    /// First room.
    pub start_room: Spanned<String>,
    /// Default room speed; [`DEFAULT_TICK_RATE`] if absent. Rooms may override.
    #[serde(default)]
    pub tick_rate: Option<Spanned<u32>>,
    /// Upper bound for a replay, in ticks. The server rejects longer replays.
    pub max_ticks: Spanned<u64>,
    /// Event and collision rank order of every kind used.
    pub object_order: Spanned<Vec<Spanned<String>>>,
    /// At least one creator.
    pub creators: Spanned<Vec<Creator>>,
    /// Where the original comes from and its permission status.
    pub source: Spanned<Source>,
    /// Script budget.
    #[serde(default)]
    pub budget: Option<Budget>,
    /// Run timing: start and end triggers and stages.
    pub timing: Spanned<Timing>,
    /// Switches for tricks that are engine behaviors.
    #[serde(default)]
    pub tricks: BTreeMap<Spanned<String>, Spanned<TrickSetting>>,
    /// One entry per file that is not the creator's own work or ours.
    #[serde(default)]
    pub third_party: Vec<ThirdParty>,
    /// Optional per-track settings, keyed by music id.
    #[serde(default)]
    pub music: BTreeMap<Spanned<String>, MusicSettings>,
}

/// The original GameMaker runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runner {
    /// GameMaker 8.0.
    Gm80,
    /// GameMaker 8.1.
    Gm81,
}

/// One `[[creators]]` entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Creator {
    /// Name as the creator wants it shown.
    pub name: Spanned<String>,
    /// What they did.
    pub role: CreatorRole,
}

/// A creator's role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CreatorRole {
    /// Made the game.
    Author,
    /// Made the game with others.
    CoAuthor,
    /// Made the music.
    Music,
    /// Made the art.
    Art,
    /// Tested the game.
    Tester,
    /// Anything else.
    Other,
}

/// The `[source]` table.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// Original game page.
    pub url: Spanned<String>,
    /// Permission status.
    pub permission: Spanned<Permission>,
    /// When the status was last set (a local date).
    #[serde(default)]
    pub permission_date: Option<Spanned<Datetime>>,
    /// Where the permission came from, or why it is pending.
    #[serde(default)]
    pub permission_note: Option<String>,
}

/// Permission to port the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Permission {
    /// The creator said yes.
    Granted,
    /// Covered by the community's standing assumption.
    CommunityAssumed,
    /// Asked, no answer yet.
    Pending,
    /// The creator said no. A revoked package is never loaded, built or served.
    Revoked,
}

/// The `[budget]` table.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    /// Script budget units per tick; [`DEFAULT_OPS_PER_TICK`] if absent, at most
    /// [`MAX_OPS_PER_TICK`].
    #[serde(default)]
    pub ops_per_tick: Option<Spanned<u64>>,
}

/// The `[timing]` table.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timing {
    /// The run starts at this trigger.
    pub start: TriggerRef,
    /// The run ends at this trigger.
    pub end: TriggerRef,
    /// Stages in order; each ends at its trigger. May be absent for a single-stage game.
    #[serde(default)]
    pub stages: Vec<Stage>,
}

/// `{ trigger = "name" }`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerRef {
    /// A trigger name, global to the package.
    pub trigger: Spanned<String>,
}

/// One split stage.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    /// Stage id, unique within the package.
    pub id: Spanned<String>,
    /// Display name.
    pub name: Spanned<String>,
    /// The stage ends at this trigger.
    pub trigger: Spanned<String>,
}

/// A trick switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrickSetting {
    /// The trick works.
    On,
    /// The trick is disabled.
    Off,
    /// The trick works and the game needs it.
    Required,
}

/// One `[[third_party]]` entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThirdParty {
    /// Package path of the file.
    pub path: Spanned<String>,
    /// What it is.
    pub what: Spanned<String>,
    /// Rights holder.
    pub owner: Spanned<String>,
    /// SPDX id, or `unknown`.
    pub license: Spanned<String>,
    /// Clearance status.
    pub status: ThirdPartyStatus,
}

/// Clearance status of a third-party file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThirdPartyStatus {
    /// Cleared for public release.
    Cleared,
    /// Not cleared yet.
    ToClear,
    /// Never shipped in public builds.
    ExcludedFromPublic,
}

/// One `[music.<id>]` table.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MusicSettings {
    /// Playback volume, 1.0 is unchanged.
    #[serde(default)]
    pub volume: Option<Spanned<Number>>,
}

/// Only the `format` key, read before the full parse so a newer package gets a clear
/// "update the client" error instead of an unknown-key error.
#[derive(Deserialize)]
struct FormatProbe {
    format: Option<Spanned<i64>>,
}

impl Manifest {
    /// Parses `game.toml` text. Syntax errors, missing fields, unknown keys and wrong value
    /// types come back as one diagnostic with line and column.
    pub fn parse(text: &str) -> Result<Self, Diagnostic> {
        if let Ok(FormatProbe { format: Some(format) }) = toml::from_str::<FormatProbe>(text)
            && *format.get_ref() > i64::from(FORMAT_VERSION)
        {
            return Err(Diagnostic::error_at(
                MANIFEST_PATH,
                text,
                format.span().start,
                format!(
                    "this package uses format {}, but this client reads formats up to {FORMAT_VERSION}; \
                     update the client",
                    format.get_ref()
                ),
            ));
        }
        toml::from_str::<Manifest>(text).map_err(|e| toml_error(MANIFEST_PATH, text, &e))
    }

    /// Default room speed.
    pub fn tick_rate(&self) -> u32 {
        self.tick_rate
            .as_ref()
            .map_or(DEFAULT_TICK_RATE, |t| *t.get_ref())
    }

    /// Script budget units per tick.
    pub fn ops_per_tick(&self) -> u64 {
        self.budget
            .as_ref()
            .and_then(|b| b.ops_per_tick.as_ref())
            .map_or(DEFAULT_OPS_PER_TICK, |o| *o.get_ref())
    }

    /// Checks everything the parser cannot: id syntax, ranges, references to package files,
    /// duplicates and the permission status. `text` is the manifest source, for positions.
    pub fn validate(&self, text: &str, files: &PackageFiles, out: &mut Vec<Diagnostic>) {
        let mut v = Validator { text, files, out };

        if *self.format.get_ref() > FORMAT_VERSION {
            v.error(
                self.format.span(),
                format!("format must be at most {FORMAT_VERSION}"),
            );
        }
        v.id(&self.id);
        v.non_empty(&self.title, "title");
        if let Err(e) = check_semver(self.version.get_ref()) {
            v.error(self.version.span(), e);
        }
        if let Some(sv) = &self.source_version {
            v.non_empty(sv, "source_version");
        }
        if let Some(engine) = &self.engine {
            // TODO(OPEN: engine packs bundled or downloaded once, docs/format/v0.md): check that
            // the engine pack exists once packages can see engine packs.
            v.id(engine);
        }
        // TODO(OPEN: runner differences between GM 8.0 and 8.1, from the CLONE_SPEC): nothing to
        // check yet; the sim will read `runner` once the differences are known.
        if v.path(&self.physics, "physics") && !v.files.contains(self.physics.get_ref()) {
            v.error(
                self.physics.span(),
                format!(
                    "physics profile `{}` is not in the package",
                    self.physics.get_ref()
                ),
            );
        }
        if v.id(&self.start_room) {
            let room = format!("rooms/{}.toml", self.start_room.get_ref());
            if !v.files.contains(&room) {
                v.error(
                    self.start_room.span(),
                    format!("start_room `{}` has no `{room}`", self.start_room.get_ref()),
                );
            }
        }
        if let Some(rate) = &self.tick_rate
            && *rate.get_ref() == 0
        {
            v.error(rate.span(), "tick_rate must be at least 1");
        }
        if *self.max_ticks.get_ref() == 0 {
            v.error(self.max_ticks.span(), "max_ticks must be at least 1");
        }
        self.validate_object_order(&mut v);
        self.validate_creators(&mut v);
        self.validate_source(&mut v);
        if let Some(ops) = self.budget.as_ref().and_then(|b| b.ops_per_tick.as_ref())
            && !(1..=MAX_OPS_PER_TICK).contains(ops.get_ref())
        {
            v.error(
                ops.span(),
                format!("ops_per_tick must be between 1 and {MAX_OPS_PER_TICK}"),
            );
        }
        self.validate_timing(&mut v);
        for key in self.tricks.keys() {
            // TODO(OPEN: trick ids, from the research trick inventory): check `key` against the
            // known trick ids.
            v.id(key);
        }
        self.validate_third_party(&mut v);
        for (track, settings) in &self.music {
            if v.id(track) {
                let file = format!("music/{}.ogg", track.get_ref());
                if !v.files.contains(&file) {
                    v.error(
                        track.span(),
                        format!("music `{}` has no `{file}`", track.get_ref()),
                    );
                }
            }
            if let Some(volume) = &settings.volume {
                let value = volume.get_ref().0;
                if !value.is_finite() || value < 0.0 {
                    v.error(volume.span(), "volume must be a finite number of at least 0");
                }
            }
        }
    }

    fn validate_object_order(&self, v: &mut Validator<'_>) {
        let mut seen = BTreeSet::new();
        for entry in self.object_order.get_ref() {
            match KindRef::parse(entry.get_ref()) {
                Err(e) => v.error(entry.span(), e),
                Ok(kind) => {
                    if !seen.insert(entry.get_ref().as_str()) {
                        v.error(
                            entry.span(),
                            format!("`{}` is listed twice in object_order", entry.get_ref()),
                        );
                    }
                    match kind {
                        // TODO(OPEN: built-in kind list, from research/specs/mechanics.md): check
                        // bare kinds against the built-in list.
                        KindRef::BuiltIn(_) => {}
                        KindRef::Package(id) => {
                            let file = format!("objects/{id}.toml");
                            if !v.files.contains(&file) {
                                v.error(entry.span(), format!("`@{id}` has no `{file}`"));
                            }
                        }
                    }
                }
            }
        }
    }

    fn validate_creators(&self, v: &mut Validator<'_>) {
        if self.creators.get_ref().is_empty() {
            v.error(self.creators.span(), "creators needs at least one entry");
        }
        for creator in self.creators.get_ref() {
            v.non_empty(&creator.name, "creator name");
        }
    }

    fn validate_source(&self, v: &mut Validator<'_>) {
        let source = self.source.get_ref();
        let url = source.url.get_ref();
        let host = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or("");
        if host.is_empty() || host.starts_with('/') || url.chars().any(char::is_whitespace) {
            v.error(
                source.url.span(),
                format!("source url `{url}` must be an http:// or https:// address"),
            );
        }
        if *source.permission.get_ref() == Permission::Revoked {
            v.error(
                source.permission.span(),
                "permission is `revoked`: a revoked package is never loaded, built or served",
            );
        }
        if let Some(date) = &source.permission_date {
            let d = date.get_ref();
            if d.date.is_none() || d.time.is_some() || d.offset.is_some() {
                v.error(
                    date.span(),
                    "permission_date must be a date without a time, for example 2026-10-24",
                );
            }
        }
    }

    fn validate_timing(&self, v: &mut Validator<'_>) {
        let timing = self.timing.get_ref();
        v.non_empty(&timing.start.trigger, "trigger name");
        v.non_empty(&timing.end.trigger, "trigger name");
        let mut ids = BTreeSet::new();
        for stage in &timing.stages {
            if v.id(&stage.id) && !ids.insert(stage.id.get_ref().as_str()) {
                v.error(
                    stage.id.span(),
                    format!("stage id `{}` is used twice", stage.id.get_ref()),
                );
            }
            v.non_empty(&stage.name, "stage name");
            v.non_empty(&stage.trigger, "trigger name");
        }
    }

    fn validate_third_party(&self, v: &mut Validator<'_>) {
        let mut paths = BTreeSet::new();
        for entry in &self.third_party {
            let path = entry.path.get_ref();
            if path.starts_with("engine:") {
                // TODO(OPEN: engine packs bundled or downloaded once, docs/format/v0.md): resolve
                // engine pack assets once packages can see engine packs.
            } else if v.path(&entry.path, "third_party path") && !v.files.contains(path) {
                v.error(
                    entry.path.span(),
                    format!("third_party path `{path}` is not in the package"),
                );
            }
            if !paths.insert(path.as_str()) {
                v.error(
                    entry.path.span(),
                    format!("third_party path `{path}` is listed twice"),
                );
            }
            v.non_empty(&entry.what, "what");
            v.non_empty(&entry.owner, "owner");
            v.non_empty(&entry.license, "license");
        }
    }
}

/// Collects diagnostics for one manifest.
struct Validator<'a> {
    text: &'a str,
    files: &'a PackageFiles,
    out: &'a mut Vec<Diagnostic>,
}

impl Validator<'_> {
    fn error(&mut self, span: Range<usize>, message: impl Into<String>) {
        self.out.push(Diagnostic::error_at(
            MANIFEST_PATH,
            self.text,
            span.start,
            message,
        ));
    }

    /// Checks id syntax; true when valid.
    fn id(&mut self, id: &Spanned<String>) -> bool {
        match check_id(id.get_ref()) {
            Ok(()) => true,
            Err(e) => {
                self.error(id.span(), e);
                false
            }
        }
    }

    /// Checks package path syntax; true when valid.
    fn path(&mut self, path: &Spanned<String>, what: &str) -> bool {
        match check_path(path.get_ref()) {
            Ok(()) => true,
            Err(e) => {
                self.error(path.span(), format!("{what} `{}`: {e}", path.get_ref()));
                false
            }
        }
    }

    fn non_empty(&mut self, value: &Spanned<String>, what: &str) {
        if value.get_ref().trim().is_empty() {
            self.error(value.span(), format!("{what} must not be empty"));
        }
    }
}

/// Turns a `toml` parse or deserialize error into a diagnostic at its span.
pub(crate) fn toml_error(file: &str, text: &str, error: &toml::de::Error) -> Diagnostic {
    let offset = error.span().map_or(0, |s| s.start);
    Diagnostic::error_at(file, text, offset, error.message().trim_end())
}
