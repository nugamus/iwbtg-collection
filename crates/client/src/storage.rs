//! Where the client keeps what it remembers between sessions: the settings file (`settings.rs`,
//! with the bindings), the save slots (`savefile.rs`) and the local stats (`stats/`). One small interface, two backends
//! (docs/architecture/client/storage.md):
//!
//! - native: files under the user's config or data folder, written beside the file and renamed
//!   so a crash never leaves half a file;
//! - web (wasm32-unknown-unknown): the browser's `localStorage`, one entry per file, keyed
//!   `iwbtg-collection/<area>/<name>`. Text is stored as is; bytes as lowercase hex.
//!
//! Every failure is a `String` the caller turns into a [`warn`]ing; nothing here stops the game.
//! The rest of the client never uses `cfg` for storage: it asks [`Store::config`] or
//! [`Store::data`] and calls the methods below.

/// The two places the client keeps things: settings (`Config`) and save slots (`Data`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    Config,
    Data,
}

/// One area of storage. `name`s are relative, `/`-separated (`saves/<game id>.save`).
#[derive(Debug, Clone)]
pub struct Store {
    inner: backend::Backend,
}

impl Store {
    /// The settings area, or `None` when this machine or browser has none (no config folder,
    /// storage turned off).
    pub fn config() -> Option<Store> {
        Self::open(Area::Config)
    }

    /// The data area (save slots, stats), or `None` when there is none.
    pub fn data() -> Option<Store> {
        Self::open(Area::Data)
    }

    fn open(area: Area) -> Option<Store> {
        backend::Backend::open(area).map(|inner| Store { inner })
    }

    /// A store in a folder of the caller's choice (tests).
    #[cfg(all(test, not(all(target_arch = "wasm32", target_os = "unknown"))))]
    pub fn at(root: std::path::PathBuf) -> Store {
        Store {
            inner: backend::Backend { root },
        }
    }

    /// Where `name` lives, for messages: a path, or the `localStorage` key.
    pub fn describe(&self, name: &str) -> String {
        self.inner.describe(name)
    }

    /// Reads bytes. `Ok(None)` is nothing stored under `name`.
    pub fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        self.inner.read(name)
    }

    /// Reads text. `Ok(None)` is nothing stored under `name`.
    pub fn read_text(&self, name: &str) -> Result<Option<String>, String> {
        self.inner.read_text(name)
    }

    /// Stores bytes, replacing what was there.
    pub fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        self.inner.write(name, bytes)
    }

    /// Stores text, replacing what was there.
    pub fn write_text(&self, name: &str, text: &str) -> Result<(), String> {
        self.inner.write_text(name, text)
    }

    /// Removes `name`; removing nothing is fine.
    pub fn remove(&self, name: &str) -> Result<(), String> {
        self.inner.remove(name)
    }
}

/// A warning the player can find: stderr natively, the browser console on the web, and once per
/// session a notice on screen (`ui::notice`, #566).
pub fn warn(message: &str) {
    console_warn(message);
    crate::ui::notice::post(crate::ui::notice::Kind::Storage, message);
}

/// A warning for the console only (stderr natively, the browser console on the web), for a
/// caller that posts its own notice (a dropped file that does not open, `import.rs`).
pub fn console_warn(message: &str) {
    backend::warn(message);
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod backend {
    use std::path::{Path, PathBuf};

    use super::Area;

    #[derive(Debug, Clone)]
    pub struct Backend {
        pub root: PathBuf,
    }

    impl Backend {
        /// Config: `$IWBTG_CONFIG_DIR`, else `%APPDATA%` on Windows, else `$XDG_CONFIG_HOME` or
        /// `~/.config`, then `iwbtg-collection`. Data: `%LOCALAPPDATA%` on Windows, else
        /// `$XDG_DATA_HOME` or `~/.local/share`, then `iwbtg-collection`.
        pub fn open(area: Area) -> Option<Backend> {
            let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
            let root = match area {
                Area::Config => {
                    if let Some(dir) = env("IWBTG_CONFIG_DIR") {
                        return Some(Backend { root: dir });
                    }
                    if cfg!(windows) {
                        env("APPDATA")
                    } else {
                        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))
                    }
                }
                Area::Data => {
                    if cfg!(windows) {
                        env("LOCALAPPDATA")
                    } else {
                        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local/share")))
                    }
                }
            }?;
            Some(Backend {
                root: root.join("iwbtg-collection"),
            })
        }

        fn path(&self, name: &str) -> PathBuf {
            name.split('/').fold(self.root.clone(), |p, part| p.join(part))
        }

        pub fn describe(&self, name: &str) -> String {
            self.path(name).display().to_string()
        }

        pub fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
            let path = self.path(name);
            match std::fs::read(&path) {
                Ok(b) => Ok(Some(b)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(format!("{}: cannot read ({e})", path.display())),
            }
        }

        pub fn read_text(&self, name: &str) -> Result<Option<String>, String> {
            match self.read(name)? {
                None => Ok(None),
                Some(b) => String::from_utf8(b)
                    .map(Some)
                    .map_err(|_| format!("{}: not UTF-8 text", self.describe(name))),
            }
        }

        /// Writes beside the file and renames it, so a crash never leaves half a file.
        pub fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
            let path = self.path(name);
            let err = |p: &Path, e: std::io::Error| format!("{}: cannot write ({e})", p.display());
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| err(dir, e))?;
            }
            let mut tmp = path.clone().into_os_string();
            tmp.push(".tmp");
            let tmp = PathBuf::from(tmp);
            std::fs::write(&tmp, bytes).map_err(|e| err(&tmp, e))?;
            std::fs::rename(&tmp, &path).map_err(|e| err(&path, e))
        }

        pub fn write_text(&self, name: &str, text: &str) -> Result<(), String> {
            self.write(name, text.as_bytes())
        }

        pub fn remove(&self, name: &str) -> Result<(), String> {
            let path = self.path(name);
            match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    Err(format!("{}: cannot remove ({e})", path.display()))
                }
                _ => Ok(()),
            }
        }
    }

    pub fn warn(message: &str) {
        eprintln!("warning: {message}");
    }
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod backend {
    use super::Area;
    use super::hex::{hex, unhex};

    #[derive(Debug, Clone)]
    pub struct Backend {
        prefix: &'static str,
    }

    fn storage() -> Result<web_sys::Storage, String> {
        let window = web_sys::window().ok_or("no browser window")?;
        // Private browsing or a blocked site throws here or returns nothing.
        match window.local_storage() {
            Ok(Some(s)) => Ok(s),
            Ok(None) => Err("browser storage is not available".into()),
            Err(e) => Err(format!("browser storage is blocked ({})", js_text(&e))),
        }
    }

    fn js_text(v: &web_sys::wasm_bindgen::JsValue) -> String {
        v.as_string()
            .or_else(|| {
                web_sys::js_sys::Reflect::get(v, &"message".into())
                    .ok()?
                    .as_string()
            })
            .unwrap_or_else(|| format!("{v:?}"))
    }

    impl Backend {
        pub fn open(area: Area) -> Option<Backend> {
            if let Err(e) = storage() {
                super::warn(&format!("{e}; nothing will be kept after this page closes"));
                return None;
            }
            let prefix = match area {
                Area::Config => "iwbtg-collection/config/",
                Area::Data => "iwbtg-collection/data/",
            };
            Some(Backend { prefix })
        }

        fn key(&self, name: &str) -> String {
            format!("{}{name}", self.prefix)
        }

        pub fn describe(&self, name: &str) -> String {
            format!("localStorage[{}]", self.key(name))
        }

        pub fn read_text(&self, name: &str) -> Result<Option<String>, String> {
            storage()?
                .get_item(&self.key(name))
                .map_err(|e| format!("{}: cannot read ({})", self.describe(name), js_text(&e)))
        }

        pub fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
            match self.read_text(name)? {
                None => Ok(None),
                Some(t) => unhex(&t)
                    .map(Some)
                    .ok_or_else(|| format!("{}: not hex bytes", self.describe(name))),
            }
        }

        pub fn write_text(&self, name: &str, text: &str) -> Result<(), String> {
            // A full quota or private mode throws here.
            storage()?
                .set_item(&self.key(name), text)
                .map_err(|e| format!("{}: cannot write ({})", self.describe(name), js_text(&e)))
        }

        pub fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
            self.write_text(name, &hex(bytes))
        }

        pub fn remove(&self, name: &str) -> Result<(), String> {
            storage()?
                .remove_item(&self.key(name))
                .map_err(|e| format!("{}: cannot remove ({})", self.describe(name), js_text(&e)))
        }
    }

    pub fn warn(message: &str) {
        web_sys::console::warn_1(&format!("warning: {message}").into());
    }
}

/// Bytes in a `localStorage` string (web only; compiled natively for its tests).
#[cfg(any(test, all(target_arch = "wasm32", target_os = "unknown")))]
mod hex {
    pub fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn unhex(text: &str) -> Option<Vec<u8>> {
        if !text.len().is_multiple_of(2) {
            return None;
        }
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
            .collect()
    }
}

#[cfg(all(test, not(all(target_arch = "wasm32", target_os = "unknown"))))]
mod tests {
    use super::*;

    fn temp(name: &str) -> Store {
        let d = std::env::temp_dir().join(format!("iwbtg-storage-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        Store::at(d)
    }

    #[test]
    fn bytes_and_text_round_trip_in_nested_names() {
        let s = temp("round");
        assert_eq!(s.read("saves/a.save").unwrap(), None);
        s.write("saves/a.save", &[0, 1, 255]).unwrap();
        assert_eq!(s.read("saves/a.save").unwrap(), Some(vec![0, 1, 255]));
        s.write_text("settings.toml", "a = 1\n").unwrap();
        assert_eq!(s.read_text("settings.toml").unwrap().as_deref(), Some("a = 1\n"));
        assert!(s.describe("saves/a.save").ends_with("a.save"));
    }

    #[test]
    fn remove_is_quiet_about_nothing() {
        let s = temp("remove");
        s.remove("saves/none.save").unwrap();
        s.write("saves/b.save", b"x").unwrap();
        s.remove("saves/b.save").unwrap();
        assert_eq!(s.read("saves/b.save").unwrap(), None);
    }

    #[test]
    fn web_bytes_round_trip_through_hex() {
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(hex::unhex(&hex::hex(&bytes)), Some(bytes));
        assert_eq!(hex::hex(&[0x0a, 0xff]), "0aff");
        assert_eq!(hex::unhex("abc"), None, "odd length");
        assert_eq!(hex::unhex("zz"), None, "not hex");
        assert_eq!(hex::unhex("aéb"), None, "not ASCII");
    }

    #[test]
    fn bad_text_is_an_error_not_a_panic() {
        let s = temp("utf8");
        s.write("settings.toml", &[0xff, 0xfe]).unwrap();
        assert!(s.read_text("settings.toml").unwrap_err().contains("UTF-8"));
    }
}
