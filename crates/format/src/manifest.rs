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
use crate::files::{CONTENT_DIRS, PackageFiles, TESTS_DIR, check_path};
use crate::ids::{EngineRef, KindRef, check_id, check_semver};
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
    ///
    /// A missing field is reported at the start of the table that lacks it, because that is
    /// the span `toml` gives. For a missing top-level field (`id`, `format`, `[timing]`, ...)
    /// the table is the whole document, so the position is always 1:1, on the `#:schema`
    /// line, and the message names the field. This is on purpose: a root key can go anywhere
    /// before the first `[table]` header, so no single line is more correct than another.
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
        self.validate_physics(&mut v);
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

    fn validate_physics(&self, v: &mut Validator<'_>) {
        if !v.path(&self.physics, "physics") {
            return;
        }
        let path = self.physics.get_ref();
        if !path.ends_with(".toml")
            || path == MANIFEST_PATH
            || CONTENT_DIRS.iter().any(|dir| path.starts_with(dir))
        {
            v.error(
                self.physics.span(),
                format!(
                    "physics profile `{path}` must be a `.toml` file other than `{MANIFEST_PATH}`, \
                     outside {} (usually `physics.toml`)",
                    CONTENT_DIRS.join(", ")
                ),
            );
        } else if !v.files.contains(path) {
            v.error(
                self.physics.span(),
                format!("physics profile `{path}` is not in the package"),
            );
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
        let start = timing.start.trigger.get_ref();
        let mut ids = BTreeSet::new();
        let mut triggers = BTreeSet::new();
        for stage in &timing.stages {
            if v.id(&stage.id) && !ids.insert(stage.id.get_ref().as_str()) {
                v.error(
                    stage.id.span(),
                    format!("stage id `{}` is used twice", stage.id.get_ref()),
                );
            }
            v.non_empty(&stage.name, "stage name");
            if !v.non_empty(&stage.trigger, "trigger name") {
                continue;
            }
            // Each stage ends at its trigger, so a shared trigger would close two stages on one
            // tick, and the start trigger would close a stage as the run begins. The last stage
            // may end at the run's `end` trigger.
            let trigger = stage.trigger.get_ref();
            if trigger == start {
                v.error(
                    stage.trigger.span(),
                    format!(
                        "stage `{}` ends at trigger `{trigger}`, which is the run's start trigger",
                        stage.id.get_ref()
                    ),
                );
            } else if !triggers.insert(trigger.as_str()) {
                v.error(
                    stage.trigger.span(),
                    format!("stage trigger `{trigger}` is used by two stages"),
                );
            }
        }
    }

    fn validate_third_party(&self, v: &mut Validator<'_>) {
        let mut paths = BTreeSet::new();
        for entry in &self.third_party {
            let path = entry.path.get_ref();
            if let Some(engine_ref) = EngineRef::parse(path) {
                // TODO(OPEN: engine packs bundled or downloaded once, docs/format/v0.md): resolve
                // engine pack assets once packages can see engine packs.
                if let Err(e) = engine_ref {
                    v.error(entry.path.span(), format!("third_party path: {e}"));
                }
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

    /// Checks package path syntax and that the file is shipped (not under `tests/`); true when
    /// valid.
    fn path(&mut self, path: &Spanned<String>, what: &str) -> bool {
        let text = path.get_ref();
        let result = check_path(text).and_then(|()| {
            if text.starts_with(TESTS_DIR) {
                Err("files under `tests/` are not shipped with the package")
            } else {
                Ok(())
            }
        });
        match result {
            Ok(()) => true,
            Err(e) => {
                self.error(path.span(), format!("{what} `{text}`: {e}"));
                false
            }
        }
    }

    /// Checks that a text field is not blank; true when it is not.
    fn non_empty(&mut self, value: &Spanned<String>, what: &str) -> bool {
        let ok = !value.get_ref().trim().is_empty();
        if !ok {
            self.error(value.span(), format!("{what} must not be empty"));
        }
        ok
    }
}

/// Turns a `toml` parse or deserialize error into a diagnostic at its span.
pub(crate) fn toml_error(file: &str, text: &str, error: &toml::de::Error) -> Diagnostic {
    let offset = error.span().map_or(0, |s| s.start);
    Diagnostic::error_at(file, text, offset, error.message().trim_end())
}

#[cfg(test)]
mod tests {
    use crate::{Diagnostic, PackageFiles, check};

    /// The `tests/valid/minimal` manifest without `[timing]`, with `extra` appended at line 20.
    fn manifest(extra: &str) -> String {
        format!(
            r#"#:schema ../game.schema.json
format = 0
id = "minimal"
title = "Minimal"
version = "0.1.0"
engine_rev = 1
runner = "gm80"
physics = "physics.toml"
start_room = "title"
max_ticks = 180_000
object_order = ["kid", "block"]

[[creators]]
name = "IWBTG Collection"
role = "author"

[source]
url = "https://example.com/minimal"
permission = "granted"
{extra}"#
        )
    }

    /// Lines 20 to 23.
    const TIMING: &str = r#"
[timing]
start = { trigger = "run_start" }
end = { trigger = "run_end" }
"#;

    /// Checks a package of `game_toml`, `physics.toml`, `rooms/title.toml` and the `extra` files
    /// (empty; the manifest checks only look at which files exist).
    fn run(game_toml: &str, extra: &[&str]) -> Vec<Diagnostic> {
        let mut files = PackageFiles::new();
        files.insert("game.toml", game_toml.as_bytes().to_vec()).unwrap();
        for path in ["physics.toml", "rooms/title.toml"].iter().chain(extra) {
            files.insert(*path, Vec::new()).unwrap();
        }
        check(&files)
    }

    /// Asserts exactly one error, at `line`, whose message contains `fragment`.
    fn assert_one(diags: &[Diagnostic], line: u32, fragment: &str) {
        match diags {
            [d] if d.is_error() && d.line == line && d.message.contains(fragment) => {}
            _ => panic!("want one error at line {line} containing {fragment:?}, got {diags:#?}"),
        }
    }

    fn with_physics(path: &str) -> String {
        manifest(TIMING).replace("\"physics.toml\"", &format!("\"{path}\""))
    }

    #[test]
    fn base_manifest_is_valid() {
        let diags = run(&manifest(TIMING), &[]);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn physics_under_tests_is_rejected() {
        assert_one(
            &run(&with_physics("tests/p.toml"), &["tests/p.toml"]),
            8,
            "physics `tests/p.toml`: files under `tests/` are not shipped",
        );
    }

    #[test]
    fn physics_must_be_a_profile_file() {
        for bad in [
            "game.toml",
            "rooms/title.toml",
            "music/stage1.toml",
            "physics.png",
        ] {
            assert_one(
                &run(&with_physics(bad), &["music/stage1.toml", "physics.png"]),
                8,
                &format!("physics profile `{bad}` must be a `.toml` file"),
            );
        }
        let diags = run(&with_physics("profiles/needle.toml"), &["profiles/needle.toml"]);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    /// A manifest with one `[[third_party]]` entry; its `path` is on line 26.
    fn third_party(path: &str) -> String {
        manifest(&format!(
            r#"{TIMING}
[[third_party]]
path = "{path}"
what = "Kid sprite"
owner = "Kayin"
license = "unknown"
status = "to-clear"
"#
        ))
    }

    #[test]
    fn third_party_under_tests_is_rejected() {
        assert_one(
            &run(&third_party("tests/x.ogg"), &["tests/x.ogg"]),
            26,
            "third_party path `tests/x.ogg`: files under `tests/` are not shipped",
        );
    }

    #[test]
    fn engine_third_party_paths_are_checked() {
        let diags = run(&third_party("engine:yuuutu/kid_mask"), &[]);
        assert!(diags.is_empty(), "{diags:#?}");
        for bad in [
            "engine:",
            "engine:../../etc",
            "engine:yuuutu",
            "engine:yuuutu/Kid",
        ] {
            assert_one(
                &run(&third_party(bad), &[]),
                26,
                &format!("third_party path: engine reference `{bad}`"),
            );
        }
    }

    /// A manifest whose `[timing]` has `stages = [<stages>]` on line 24.
    fn stages(stages: &str) -> String {
        manifest(&format!(
            r#"
[timing]
start = {{ trigger = "run_start" }}
end = {{ trigger = "run_end" }}
stages = [{stages}]
"#
        ))
    }

    #[test]
    fn stage_triggers_are_unique() {
        let text =
            stages(r#"{ id = "a", name = "A", trigger = "t" }, { id = "b", name = "B", trigger = "t" }"#);
        let diags = run(&text, &[]);
        assert_one(&diags, 24, "stage trigger `t` is used by two stages");
        // Points at the second stage's trigger.
        assert_eq!(
            diags[0].column,
            1 + text.lines().nth(23).unwrap().rfind("\"t\"").unwrap() as u32
        );
    }

    #[test]
    fn stage_trigger_is_not_the_start_trigger() {
        let text = stages(r#"{ id = "a", name = "A", trigger = "run_start" }"#);
        assert_one(&run(&text, &[]), 24, "which is the run's start trigger");
    }

    #[test]
    fn last_stage_may_end_at_the_end_trigger() {
        let text = stages(
            r#"{ id = "a", name = "A", trigger = "a_clear" }, { id = "b", name = "B", trigger = "run_end" }"#,
        );
        let diags = run(&text, &[]);
        assert!(diags.is_empty(), "{diags:#?}");
    }
}
