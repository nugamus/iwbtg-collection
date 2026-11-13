//! The settings file: one versioned TOML file that holds every player setting, kept through
//! `storage.rs` (a file in the user's config folder natively, browser storage on the web) (docs/architecture/client/settings.md). The client reads it at start and writes it
//! when a setting changes; the settings screens of M6 only edit it.
//!
//! Reading is forgiving: an unknown key, a wrong type or an out-of-range value is a warning on
//! stderr and that entry keeps its default; the rest of the file still counts. Nothing here
//! touches the sim.

use std::sync::Mutex;

use toml::{Table, Value};

use crate::storage::{self, Store};

/// The layout version this build writes.
pub const VERSION: i64 = 1;
/// The largest audio offset, in milliseconds.
pub const MAX_AUDIO_OFFSET_MS: u32 = crate::audio::MAX_OFFSET_MS;
/// The largest volume, in percent.
pub const MAX_VOLUME: u32 = 100;
/// The longest profile name, in characters.
pub const MAX_NAME_CHARS: usize = 32;
/// The largest backdrop blur, in percent.
pub const MAX_BACKDROP_BLUR: u32 = 100;
/// The backdrop blur a fresh file has, in percent (`ui::backdrop::DEFAULT_BLUR`).
pub const DEFAULT_BACKDROP_BLUR: u32 = 60;

/// Every player setting.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The local profile name for the player card (`name`); empty falls back to the system
    /// account's name (`card::profile_name`).
    pub name: String,
    /// Start fullscreen.
    pub fullscreen: bool,
    /// Fullscreen area shape (`None` is the monitor's own), as `screen::RATIOS`.
    pub ratio: Option<(u32, u32)>,
    /// Windowed size in physical pixels; `None` is the default size.
    pub window: Option<(u32, u32)>,
    /// Sound is held back by this many milliseconds, 0 to [`MAX_AUDIO_OFFSET_MS`].
    pub audio_offset_ms: u32,
    /// Overall loudness in percent, 0 to [`MAX_VOLUME`] (`[audio] master_volume`, `--volume`).
    pub master_volume: u32,
    /// Music loudness in percent, on top of the master volume (`[audio] music_volume`).
    pub music_volume: u32,
    /// Sound effect loudness in percent, on top of the master volume (`[audio] sound_volume`).
    pub sound_volume: u32,
    /// Scale the game by whole steps instead of filling its room (F7, `--integer-scale`).
    pub integer_scale: bool,
    /// The debug overlay (F3) is shown. Off by default.
    pub overlay: bool,
    /// HUD side panel scale in percent, 75 to 150 (`hud::layout::UI_SCALE_RANGE`).
    pub ui_scale: u32,
    /// Backdrop blur in percent, 0 to [`MAX_BACKDROP_BLUR`] (`[` and `]`, later the settings slider).
    pub backdrop_blur: u32,
    /// Which HUD elements are shown (`[hud.show]`).
    pub hud_show: HudShow,
    /// Start the run again after every death (`[retry] auto`; `--auto-retry` overrides it for
    /// one run).
    pub auto_retry: bool,
    /// The results page shows where you died in a stage (`[results] heatmap`, #654; off by
    /// default, `--results-heatmap` sets it).
    pub results_heatmap: bool,
    /// The `[bindings]` table, kept as written: the input layer (#27) owns its layout and checks
    /// it; this file only stores it.
    pub bindings: Table,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            name: String::new(),
            fullscreen: false,
            ratio: None,
            window: None,
            audio_offset_ms: 0,
            master_volume: MAX_VOLUME,
            music_volume: MAX_VOLUME,
            sound_volume: MAX_VOLUME,
            integer_scale: false,
            overlay: false,
            ui_scale: 100,
            backdrop_blur: DEFAULT_BACKDROP_BLUR,
            hud_show: HudShow::default(),
            auto_retry: false,
            results_heatmap: false,
            bindings: Table::new(),
        }
    }
}

/// Each HUD element on or off (`[hud.show]`; docs/product.md, In-game display). All on by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HudShow {
    pub leaderboard: bool,
    pub timer: bool,
    pub deaths: bool,
    pub stage: bool,
    pub stage_deaths: bool,
    pub input: bool,
    pub mods: bool,
}

impl Default for HudShow {
    fn default() -> Self {
        Self {
            leaderboard: true,
            timer: true,
            deaths: true,
            stage: true,
            stage_deaths: true,
            input: true,
            mods: true,
        }
    }
}

impl HudShow {
    /// The elements' names, as the file and `--hud-show`/`--hud-hide` write them.
    pub const NAMES: [&'static str; 7] = [
        "leaderboard",
        "timer",
        "deaths",
        "stage",
        "stage_deaths",
        "input",
        "mods",
    ];

    /// The switch for the element called `name`.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut bool> {
        Some(match name {
            "leaderboard" => &mut self.leaderboard,
            "timer" => &mut self.timer,
            "deaths" => &mut self.deaths,
            "stage" => &mut self.stage,
            "stage_deaths" => &mut self.stage_deaths,
            "input" => &mut self.input,
            "mods" => &mut self.mods,
            _ => return None,
        })
    }

    fn to_table(mut self) -> Table {
        let mut t = Table::new();
        for name in Self::NAMES {
            let on = *self.get_mut(name).expect("a known name");
            t.insert(name.into(), on.into());
        }
        t
    }
}

fn fmt_ratio(r: Option<(u32, u32)>) -> String {
    r.map_or("native".to_string(), |(w, h)| format!("{w}:{h}"))
}

impl Settings {
    /// Reads settings text. Every problem is pushed to `warnings`; the entry keeps its default.
    pub fn parse(text: &str, warnings: &mut Vec<String>) -> Settings {
        let mut s = Settings::default();
        let table: Table = match text.parse() {
            Ok(t) => t,
            Err(e) => {
                warnings.push(format!("not valid TOML, using defaults: {e}"));
                return s;
            }
        };
        for (key, value) in &table {
            match (key.as_str(), value) {
                ("version", Value::Integer(v)) => {
                    if *v != VERSION {
                        warnings.push(format!(
                            "version {v}, this build reads version {VERSION}; reading what it understands"
                        ));
                    }
                }
                ("version", _) => warnings.push("`version` must be an integer".into()),
                ("name", Value::String(n)) => {
                    let n = n.trim();
                    if n.chars().count() > MAX_NAME_CHARS || n.chars().any(char::is_control) {
                        warnings.push(format!(
                            "`name` must be at most {MAX_NAME_CHARS} characters without control characters, using the default"
                        ));
                    } else {
                        s.name = n.to_owned();
                    }
                }
                ("name", _) => warnings.push("`name` must be a string".into()),
                ("display", Value::Table(t)) => s.read_display(t, warnings),
                ("audio", Value::Table(t)) => s.read_audio(t, warnings),
                ("hud", Value::Table(t)) => s.read_hud(t, warnings),
                ("retry", Value::Table(t)) => s.read_retry(t, warnings),
                ("results", Value::Table(t)) => s.read_results(t, warnings),
                ("bindings", Value::Table(t)) => s.bindings = t.clone(),
                ("display" | "audio" | "hud" | "retry" | "results" | "bindings", _) => {
                    warnings.push(format!("`[{key}]` must be a table, using defaults"));
                }
                _ => warnings.push(format!("unknown entry `{key}`, ignored")),
            }
        }
        s
    }

    fn read_display(&mut self, t: &Table, w: &mut Vec<String>) {
        for (key, value) in t {
            match (key.as_str(), value) {
                ("fullscreen", Value::Boolean(b)) => self.fullscreen = *b,
                ("ratio", Value::String(r)) => match crate::screen::parse_ratio(r) {
                    Ok(r) => self.ratio = r,
                    Err(e) => w.push(format!("display.ratio `{r}`: {e}")),
                },
                ("window", Value::String(r)) => match crate::screen::parse_size(r) {
                    Ok(r) => self.window = Some(r),
                    Err(e) => w.push(format!("display.window `{r}`: {e}")),
                },
                ("integer_scale", Value::Boolean(b)) => self.integer_scale = *b,
                ("backdrop_blur", Value::Integer(v)) => match u32::try_from(*v) {
                    Ok(v) if v <= MAX_BACKDROP_BLUR => self.backdrop_blur = v,
                    _ => w.push(format!(
                        "display.backdrop_blur {v} is outside 0 to {MAX_BACKDROP_BLUR}, using the default"
                    )),
                },
                ("fullscreen" | "ratio" | "window" | "integer_scale" | "backdrop_blur", _) => {
                    w.push(format!("display.{key} has the wrong type, using the default"));
                }
                _ => w.push(format!("unknown entry `display.{key}`, ignored")),
            }
        }
    }

    fn read_audio(&mut self, t: &Table, w: &mut Vec<String>) {
        for (key, value) in t {
            match (key.as_str(), value) {
                ("offset_ms", Value::Integer(v)) => match u32::try_from(*v) {
                    Ok(v) if v <= MAX_AUDIO_OFFSET_MS => self.audio_offset_ms = v,
                    _ => w.push(format!(
                        "audio.offset_ms {v} is outside 0 to {MAX_AUDIO_OFFSET_MS}, using the default"
                    )),
                },
                ("offset_ms", _) => w.push("audio.offset_ms must be an integer".into()),
                ("master_volume" | "music_volume" | "sound_volume", Value::Integer(v)) => {
                    match u32::try_from(*v) {
                        Ok(v) if v <= MAX_VOLUME => *self.volume_mut(key) = v,
                        _ => w.push(format!(
                            "audio.{key} {v} is outside 0 to {MAX_VOLUME}, using the default"
                        )),
                    }
                }
                ("master_volume" | "music_volume" | "sound_volume", _) => {
                    w.push(format!("audio.{key} must be an integer"));
                }
                _ => w.push(format!("unknown entry `audio.{key}`, ignored")),
            }
        }
    }

    /// The volume setting stored under `key` in `[audio]`.
    fn volume_mut(&mut self, key: &str) -> &mut u32 {
        match key {
            "master_volume" => &mut self.master_volume,
            "music_volume" => &mut self.music_volume,
            _ => &mut self.sound_volume,
        }
    }

    fn read_retry(&mut self, t: &Table, w: &mut Vec<String>) {
        for (key, value) in t {
            match (key.as_str(), value) {
                ("auto", Value::Boolean(b)) => self.auto_retry = *b,
                ("auto", _) => w.push("retry.auto must be true or false".into()),
                _ => w.push(format!("unknown entry `retry.{key}`, ignored")),
            }
        }
    }

    fn read_results(&mut self, t: &Table, w: &mut Vec<String>) {
        for (key, value) in t {
            match (key.as_str(), value) {
                ("heatmap", Value::Boolean(b)) => self.results_heatmap = *b,
                ("heatmap", _) => w.push("results.heatmap must be true or false".into()),
                _ => w.push(format!("unknown entry `results.{key}`, ignored")),
            }
        }
    }

    fn read_hud(&mut self, t: &Table, w: &mut Vec<String>) {
        for (key, value) in t {
            match (key.as_str(), value) {
                ("overlay", Value::Boolean(b)) => self.overlay = *b,
                ("overlay", _) => w.push("hud.overlay must be true or false".into()),
                ("ui_scale", Value::Integer(v)) => match u32::try_from(*v) {
                    Ok(v) if crate::hud::layout::UI_SCALE_RANGE.contains(&v) => self.ui_scale = v,
                    _ => w.push(format!(
                        "hud.ui_scale {v} is outside 75 to 150, using the default"
                    )),
                },
                ("ui_scale", _) => w.push("hud.ui_scale must be an integer".into()),
                ("show", Value::Table(t)) => {
                    for (name, value) in t {
                        match (self.hud_show.get_mut(name), value) {
                            (Some(slot), Value::Boolean(b)) => *slot = *b,
                            (Some(_), _) => w.push(format!("hud.show.{name} must be true or false")),
                            (None, _) => w.push(format!("unknown entry `hud.show.{name}`, ignored")),
                        }
                    }
                }
                ("show", _) => w.push("`[hud.show]` must be a table, using defaults".into()),
                _ => w.push(format!("unknown entry `hud.{key}`, ignored")),
            }
        }
    }

    /// The file's text.
    pub fn to_toml(&self) -> String {
        let mut display = Table::new();
        display.insert("fullscreen".into(), self.fullscreen.into());
        display.insert("ratio".into(), fmt_ratio(self.ratio).into());
        if let Some((w, h)) = self.window {
            display.insert("window".into(), format!("{w}x{h}").into());
        }
        display.insert("integer_scale".into(), self.integer_scale.into());
        display.insert("backdrop_blur".into(), i64::from(self.backdrop_blur).into());
        let mut audio = Table::new();
        audio.insert("offset_ms".into(), i64::from(self.audio_offset_ms).into());
        audio.insert("master_volume".into(), i64::from(self.master_volume).into());
        audio.insert("music_volume".into(), i64::from(self.music_volume).into());
        audio.insert("sound_volume".into(), i64::from(self.sound_volume).into());
        let mut hud = Table::new();
        hud.insert("overlay".into(), self.overlay.into());
        hud.insert("ui_scale".into(), i64::from(self.ui_scale).into());
        hud.insert("show".into(), self.hud_show.to_table().into());
        let mut retry = Table::new();
        retry.insert("auto".into(), self.auto_retry.into());
        let mut results = Table::new();
        results.insert("heatmap".into(), self.results_heatmap.into());
        let mut root = Table::new();
        root.insert("version".into(), VERSION.into());
        if !self.name.is_empty() {
            root.insert("name".into(), self.name.clone().into());
        }
        root.insert("display".into(), display.into());
        root.insert("audio".into(), audio.into());
        root.insert("hud".into(), hud.into());
        root.insert("retry".into(), retry.into());
        root.insert("results".into(), results.into());
        root.insert("bindings".into(), self.bindings.clone().into());
        toml::to_string(&root).unwrap_or_default()
    }
}

/// The file's name in the config area of [`storage`] (native: `iwbtg-collection/settings.toml`
/// in the config folder; web: a `localStorage` entry).
pub const FILE: &str = "settings.toml";

/// Reads the settings from `store`. Nothing stored is the defaults, silently; every problem is a
/// warning and the entry keeps its default.
pub fn read_from(store: &Store) -> Settings {
    match store.read_text(FILE) {
        Ok(Some(text)) => {
            let mut warnings = Vec::new();
            let s = Settings::parse(&text, &mut warnings);
            for w in warnings {
                storage::warn(&format!("settings {}: {w}", store.describe(FILE)));
            }
            s
        }
        Ok(None) => Settings::default(),
        Err(e) => {
            storage::warn(&format!("settings {e}, using defaults"));
            Settings::default()
        }
    }
}

/// Writes the settings to `store` (natively beside the file, then renamed).
pub fn write_to(store: &Store, s: &Settings) -> Result<(), String> {
    store.write_text(FILE, &s.to_toml())
}

struct Slot {
    store: Option<Store>,
    current: Settings,
}

static STORE: Mutex<Option<Slot>> = Mutex::new(None);

fn store() -> std::sync::MutexGuard<'static, Option<Slot>> {
    STORE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Reads the file once at start and keeps the result for [`get`] and [`update`]. `isolated`
/// (a `--screenshot` check) neither reads nor writes the file: it starts from the defaults, so the
/// picture does not depend on the machine, and its flags are not saved.
pub fn load(isolated: bool) -> Settings {
    let backing = if isolated { None } else { Store::config() };
    let current = backing.as_ref().map(read_from).unwrap_or_default();
    *store() = Some(Slot {
        store: backing,
        current: current.clone(),
    });
    current
}

/// The current settings (the defaults before [`load`]).
pub fn get() -> Settings {
    store().as_ref().map(|s| s.current.clone()).unwrap_or_default()
}

/// This run neither reads nor writes the file (a `--screenshot` check, see [`load`]).
pub fn isolated() -> bool {
    store().as_ref().is_some_and(|s| s.store.is_none())
}

/// Changes the settings and writes the file when something changed.
pub fn update(change: impl FnOnce(&mut Settings)) {
    let mut guard = store();
    let Some(st) = guard.as_mut() else { return };
    let mut next = st.current.clone();
    change(&mut next);
    if next == st.current {
        return;
    }
    if let Some(store) = &st.store
        && let Err(e) = write_to(store, &next)
    {
        // Keep the change for this session; the next change tries again.
        storage::warn(&format!("settings {e}"));
    }
    st.current = next;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> (Settings, Vec<String>) {
        let mut w = Vec::new();
        (Settings::parse(text, &mut w), w)
    }

    #[test]
    fn empty_file_is_the_defaults() {
        let (s, w) = parse("");
        assert_eq!(s, Settings::default());
        assert!(w.is_empty());
        assert!(!s.overlay, "the F3 overlay is off by default (#598)");
    }

    #[test]
    fn round_trips() {
        let mut s = Settings {
            name: "Kid".into(),
            fullscreen: true,
            ratio: Some((16, 9)),
            window: Some((1280, 720)),
            audio_offset_ms: 120,
            master_volume: 80,
            music_volume: 0,
            sound_volume: 55,
            integer_scale: true,
            overlay: true,
            ui_scale: 125,
            backdrop_blur: 30,
            auto_retry: true,
            results_heatmap: true,
            hud_show: HudShow {
                input: false,
                leaderboard: false,
                ..HudShow::default()
            },
            ..Settings::default()
        };
        s.bindings = "[keys]\njump = [\"ShiftLeft\"]\n".parse().unwrap();
        let (back, w) = parse(&s.to_toml());
        assert_eq!(back, s);
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn auto_retry_is_read_and_checked() {
        let (s, w) = parse(
            "[retry]
auto = true
",
        );
        assert!(s.auto_retry && w.is_empty(), "{w:?}");
        let (s, w) = parse(
            "[retry]
auto = 3
foo = 1
",
        );
        assert!(!s.auto_retry);
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(!parse("").0.auto_retry);
    }

    #[test]
    fn the_results_heatmap_is_off_unless_the_file_turns_it_on() {
        assert!(!parse("").0.results_heatmap);
        let (s, w) = parse(
            "[results]
heatmap = true
",
        );
        assert!(s.results_heatmap && w.is_empty(), "{w:?}");
        let (s, w) = parse(
            "[results]
heatmap = \"yes\"
",
        );
        assert!(!s.results_heatmap);
        assert_eq!(w.len(), 1, "{w:?}");
    }

    #[test]
    fn volumes_are_full_by_default_and_read_and_checked() {
        let d = Settings::default();
        assert_eq!((d.master_volume, d.music_volume, d.sound_volume), (100, 100, 100));
        let (s, w) = parse(
            "[audio]
master_volume = 40
music_volume = 0
sound_volume = 100
",
        );
        assert_eq!((s.master_volume, s.music_volume, s.sound_volume), (40, 0, 100));
        assert!(w.is_empty(), "{w:?}");
        let (s, w) = parse(
            "[audio]
master_volume = 101
music_volume = -1
sound_volume = \"loud\"
",
        );
        assert_eq!((s.master_volume, s.music_volume, s.sound_volume), (100, 100, 100));
        assert_eq!(w.len(), 3, "{w:?}");
    }

    #[test]
    fn bad_entries_fall_back_one_by_one() {
        let (s, w) = parse(
            "version = 1\n\
             mystery = 1\n\
             [display]\nfullscreen = true\nratio = \"5:4\"\nwindow = 7\nextra = 1\nbackdrop_blur = 101\n\
             [audio]\noffset_ms = 9000\n\
             [hud]\noverlay = true\nui_scale = 200\n\
             [hud.show]\ninput = false\ntimer = 1\nsplits = false\n\
             [bindings]\njump = \"Space\"\n",
        );
        assert!(s.fullscreen, "the good entry beside bad ones still counts");
        assert_eq!(s.ratio, None);
        assert_eq!(s.window, None);
        assert_eq!(s.audio_offset_ms, 0);
        assert!(s.overlay, "an old file's `overlay = true` keeps the overlay on");
        assert_eq!(s.ui_scale, 100);
        assert_eq!(s.backdrop_blur, 60);
        assert!(!s.hud_show.input && s.hud_show.timer);
        assert_eq!(s.bindings.get("jump").and_then(Value::as_str), Some("Space"));
        assert_eq!(w.len(), 9, "{w:?}");
    }

    #[test]
    fn backdrop_blur_reads_the_whole_range() {
        for v in [0, 60, 100] {
            let (s, w) = parse(&format!(
                "[display]
backdrop_blur = {v}
"
            ));
            assert_eq!(s.backdrop_blur, v);
            assert!(w.is_empty(), "{w:?}");
        }
        let (s, w) = parse(
            "[display]
backdrop_blur = -5
",
        );
        assert_eq!((s.backdrop_blur, w.len()), (60, 1));
        let (s, w) = parse(
            "[display]
backdrop_blur = \"x\"
",
        );
        assert_eq!((s.backdrop_blur, w.len()), (60, 1));
    }

    #[test]
    fn name_is_trimmed_and_checked() {
        let (s, w) = parse(
            "name = \"  Kid \"
",
        );
        assert_eq!(s.name, "Kid");
        assert!(w.is_empty(), "{w:?}");
        let (s, w) = parse(&format!(
            "name = \"{}\"
",
            "x".repeat(33)
        ));
        assert_eq!(s.name, "");
        assert_eq!(w.len(), 1);
        let (s, w) = parse(
            "name = 5
",
        );
        assert_eq!(s.name, "");
        assert_eq!(w.len(), 1);
        assert!(!Settings::default().to_toml().contains("name"));
    }

    #[test]
    fn broken_toml_is_the_defaults_with_a_warning() {
        let (s, w) = parse("[display\nfullscreen = ");
        assert_eq!(s, Settings::default());
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn a_newer_version_warns_and_still_reads() {
        let (s, w) = parse("version = 2\n[audio]\noffset_ms = 40\n");
        assert_eq!(s.audio_offset_ms, 40);
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn file_write_and_read() {
        let dir = std::env::temp_dir().join(format!("iwbtg-settings-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::at(dir.join("nested"));
        assert_eq!(
            read_from(&store),
            Settings::default(),
            "nothing stored is the defaults"
        );
        let s = Settings {
            audio_offset_ms: 75,
            master_volume: 30,
            music_volume: 20,
            sound_volume: 10,
            ..Settings::default()
        };
        write_to(&store, &s).unwrap();
        let back = read_from(&store);
        assert_eq!(back.audio_offset_ms, 75);
        assert_eq!(
            (back.master_volume, back.music_volume, back.sound_volume),
            (30, 20, 10),
            "the volumes are kept between starts"
        );
        assert!(dir.join("nested").join("settings.toml").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
