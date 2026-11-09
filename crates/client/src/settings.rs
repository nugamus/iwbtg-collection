//! The settings file: one versioned TOML file in the user's config folder that holds every
//! player setting (docs/architecture/client.md, Settings). The client reads it at start and writes it
//! when a setting changes; the settings screens of M6 only edit it.
//!
//! Reading is forgiving: an unknown key, a wrong type or an out-of-range value is a warning on
//! stderr and that entry keeps its default; the rest of the file still counts. Nothing here
//! touches the sim.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use toml::{Table, Value};

/// The layout version this build writes.
pub const VERSION: i64 = 1;
/// The largest audio offset, in milliseconds.
pub const MAX_AUDIO_OFFSET_MS: u32 = crate::audio::MAX_OFFSET_MS;

/// Every player setting.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Start fullscreen.
    pub fullscreen: bool,
    /// Fullscreen area shape (`None` is the monitor's own), as `screen::RATIOS`.
    pub ratio: Option<(u32, u32)>,
    /// Windowed size in physical pixels; `None` is the default size.
    pub window: Option<(u32, u32)>,
    /// Sound is held back by this many milliseconds, 0 to [`MAX_AUDIO_OFFSET_MS`].
    pub audio_offset_ms: u32,
    /// The debug overlay (F3) is shown.
    pub overlay: bool,
    /// The `[bindings]` table, kept as written: the input layer (#27) owns its layout and checks
    /// it; this file only stores it.
    pub bindings: Table,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fullscreen: false,
            ratio: None,
            window: None,
            audio_offset_ms: 0,
            overlay: true,
            bindings: Table::new(),
        }
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
                ("display", Value::Table(t)) => s.read_display(t, warnings),
                ("audio", Value::Table(t)) => s.read_audio(t, warnings),
                ("hud", Value::Table(t)) => s.read_hud(t, warnings),
                ("bindings", Value::Table(t)) => s.bindings = t.clone(),
                ("display" | "audio" | "hud" | "bindings", _) => {
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
                ("fullscreen" | "ratio" | "window", _) => {
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
                _ => w.push(format!("unknown entry `audio.{key}`, ignored")),
            }
        }
    }

    fn read_hud(&mut self, t: &Table, w: &mut Vec<String>) {
        for (key, value) in t {
            match (key.as_str(), value) {
                ("overlay", Value::Boolean(b)) => self.overlay = *b,
                ("overlay", _) => w.push("hud.overlay must be true or false".into()),
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
        let mut audio = Table::new();
        audio.insert("offset_ms".into(), i64::from(self.audio_offset_ms).into());
        let mut hud = Table::new();
        hud.insert("overlay".into(), self.overlay.into());
        let mut root = Table::new();
        root.insert("version".into(), VERSION.into());
        root.insert("display".into(), display.into());
        root.insert("audio".into(), audio.into());
        root.insert("hud".into(), hud.into());
        root.insert("bindings".into(), self.bindings.clone().into());
        toml::to_string(&root).unwrap_or_default()
    }
}

/// Where the file lives: `$IWBTG_CONFIG_DIR`, else `%APPDATA%` on Windows, else
/// `$XDG_CONFIG_HOME` or `~/.config`, then `iwbtg-collection/settings.toml`.
pub fn default_path() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = env("IWBTG_CONFIG_DIR") {
        return Some(dir.join("settings.toml"));
    }
    let base = if cfg!(windows) {
        env("APPDATA")
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))
    }?;
    Some(base.join("iwbtg-collection").join("settings.toml"))
}

/// Reads the file at `path`. A missing file is the defaults, silently.
pub fn read_file(path: &Path) -> Settings {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let mut warnings = Vec::new();
            let s = Settings::parse(&text, &mut warnings);
            for w in warnings {
                eprintln!("settings {}: {w}", path.display());
            }
            s
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(e) => {
            eprintln!("settings {}: cannot read ({e}), using defaults", path.display());
            Settings::default()
        }
    }
}

/// Writes the file beside itself and renames it, so a crash never leaves half a file.
pub fn write_file(path: &Path, s: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, s.to_toml())?;
    std::fs::rename(&tmp, path)
}

struct Store {
    path: Option<PathBuf>,
    current: Settings,
}

static STORE: Mutex<Option<Store>> = Mutex::new(None);

fn store() -> std::sync::MutexGuard<'static, Option<Store>> {
    STORE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Reads the file once at start and keeps the result for [`get`] and [`update`]. `isolated`
/// (a `--screenshot` check) neither reads nor writes the file: it starts from the defaults, so the
/// picture does not depend on the machine, and its flags are not saved.
pub fn load(isolated: bool) -> Settings {
    let path = if isolated { None } else { default_path() };
    let current = path.as_deref().map(read_file).unwrap_or_default();
    *store() = Some(Store {
        path,
        current: current.clone(),
    });
    current
}

/// The current settings (the defaults before [`load`]).
pub fn get() -> Settings {
    store()
        .as_ref()
        .map(|s| s.current.clone())
        .unwrap_or_default()
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
    if let Some(path) = &st.path
        && let Err(e) = write_file(path, &next)
    {
        eprintln!("settings {}: cannot write ({e})", path.display());
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
    }

    #[test]
    fn round_trips() {
        let mut s = Settings {
            fullscreen: true,
            ratio: Some((16, 9)),
            window: Some((1280, 720)),
            audio_offset_ms: 120,
            overlay: false,
            ..Settings::default()
        };
        s.bindings = "[keys]\njump = [\"ShiftLeft\"]\n".parse().unwrap();
        let (back, w) = parse(&s.to_toml());
        assert_eq!(back, s);
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn bad_entries_fall_back_one_by_one() {
        let (s, w) = parse(
            "version = 1\n\
             mystery = 1\n\
             [display]\nfullscreen = true\nratio = \"5:4\"\nwindow = 7\nextra = 1\n\
             [audio]\noffset_ms = 9000\n\
             [hud]\noverlay = false\n\
             [bindings]\njump = \"Space\"\n",
        );
        assert!(s.fullscreen, "the good entry beside bad ones still counts");
        assert_eq!(s.ratio, None);
        assert_eq!(s.window, None);
        assert_eq!(s.audio_offset_ms, 0);
        assert!(!s.overlay);
        assert_eq!(s.bindings.get("jump").and_then(Value::as_str), Some("Space"));
        assert_eq!(w.len(), 5, "{w:?}");
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
        let path = dir.join("nested").join("settings.toml");
        let s = Settings {
            audio_offset_ms: 75,
            ..Settings::default()
        };
        write_file(&path, &s).unwrap();
        assert_eq!(read_file(&path).audio_offset_ms, 75);
        assert_eq!(read_file(&dir.join("missing.toml")), Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
