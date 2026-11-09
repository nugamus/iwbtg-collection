//! The global input bindings: which keys and controller inputs stand for which client action
//! (docs/content.md, Controls). One set for every game, defaulting to the fangame standard. The
//! player's changes live in a small TOML file; only the actions it names differ from the defaults.
//!
//! ```toml
//! [keys]
//! jump = ["Shift", "Space"]
//! [pad]
//! jump = ["south"]
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use iwbtg_sim::input::{Action, IWBTG_DEFAULT_KEYS, key_code, vk};
use serde::{Deserialize, Serialize};

/// A controller input as the collector sees it: a button, or a stick pushed past the dead zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PadInput {
    South,
    East,
    West,
    North,
    LeftBumper,
    RightBumper,
    LeftTrigger,
    RightTrigger,
    Select,
    Start,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    StickUp,
    StickDown,
    StickLeft,
    StickRight,
}

impl PadInput {
    pub const ALL: [PadInput; 18] = [
        PadInput::South,
        PadInput::East,
        PadInput::West,
        PadInput::North,
        PadInput::LeftBumper,
        PadInput::RightBumper,
        PadInput::LeftTrigger,
        PadInput::RightTrigger,
        PadInput::Select,
        PadInput::Start,
        PadInput::DPadUp,
        PadInput::DPadDown,
        PadInput::DPadLeft,
        PadInput::DPadRight,
        PadInput::StickUp,
        PadInput::StickDown,
        PadInput::StickLeft,
        PadInput::StickRight,
    ];

    /// The name used in the bindings file.
    pub const fn name(self) -> &'static str {
        match self {
            PadInput::South => "south",
            PadInput::East => "east",
            PadInput::West => "west",
            PadInput::North => "north",
            PadInput::LeftBumper => "left-bumper",
            PadInput::RightBumper => "right-bumper",
            PadInput::LeftTrigger => "left-trigger",
            PadInput::RightTrigger => "right-trigger",
            PadInput::Select => "select",
            PadInput::Start => "start",
            PadInput::DPadUp => "dpad-up",
            PadInput::DPadDown => "dpad-down",
            PadInput::DPadLeft => "dpad-left",
            PadInput::DPadRight => "dpad-right",
            PadInput::StickUp => "stick-up",
            PadInput::StickDown => "stick-down",
            PadInput::StickLeft => "stick-left",
            PadInput::StickRight => "stick-right",
        }
    }

    pub fn from_name(name: &str) -> Option<PadInput> {
        PadInput::ALL
            .into_iter()
            .find(|p| p.name().eq_ignore_ascii_case(name))
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// The controller defaults: D-pad or left stick to move, A (south) jumps, X (west) shoots,
/// Y (north) restarts, B (east) suicides, Start opens the menu.
const DEFAULT_PAD: [(PadInput, Action); 11] = [
    (PadInput::South, Action::Jump),
    (PadInput::West, Action::Shoot),
    (PadInput::North, Action::Restart),
    (PadInput::Start, Action::Menu),
    (PadInput::DPadLeft, Action::Left),
    (PadInput::DPadRight, Action::Right),
    (PadInput::DPadUp, Action::Up),
    (PadInput::DPadDown, Action::Down),
    (PadInput::StickLeft, Action::Left),
    (PadInput::StickRight, Action::Right),
    (PadInput::East, Action::Suicide),
];

/// What every key and controller input does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bindings {
    keys: BTreeMap<u8, Action>,
    pad: [Option<Action>; PadInput::ALL.len()],
}

impl Default for Bindings {
    fn default() -> Bindings {
        let mut b = Bindings {
            keys: IWBTG_DEFAULT_KEYS.into_iter().collect(),
            pad: [None; PadInput::ALL.len()],
        };
        for (input, action) in DEFAULT_PAD {
            b.pad[input.index()] = Some(action);
        }
        b
    }
}

impl Bindings {
    /// The action a key stands for. A left or right Shift, Ctrl or Alt that is not bound itself
    /// takes the generic key's binding, as in the sim's key map.
    pub fn key_action(&self, key: u8) -> Option<Action> {
        self.keys.get(&key).copied().or_else(|| {
            let generic = match key {
                vk::LEFT_SHIFT | vk::RIGHT_SHIFT => vk::SHIFT,
                vk::LEFT_CTRL | vk::RIGHT_CTRL => vk::CTRL,
                vk::LEFT_ALT | vk::RIGHT_ALT => vk::ALT,
                _ => return None,
            };
            self.keys.get(&generic).copied()
        })
    }

    pub fn pad_action(&self, input: PadInput) -> Option<Action> {
        self.pad[input.index()]
    }

    /// The defaults with the file's changes applied. Each action the file names loses all its
    /// default keys (or all its default controller inputs) and gets the listed ones.
    pub fn from_config(config: &Config) -> Result<Bindings, String> {
        let mut b = Bindings::default();
        for (name, keys) in &config.keys {
            let action = parse_action(name)?;
            b.keys.retain(|_, a| *a != action);
            for key in keys {
                let code = key_from_name(key).ok_or_else(|| format!("unknown key `{key}`"))?;
                if let Some(other) = b.keys.get(&code)
                    && *other != action
                {
                    return Err(format!(
                        "key `{key}` is bound to both {} and {}",
                        other.name(),
                        action.name()
                    ));
                }
                b.keys.insert(code, action);
            }
        }
        for (name, inputs) in &config.pad {
            let action = parse_action(name)?;
            for slot in &mut b.pad {
                if *slot == Some(action) {
                    *slot = None;
                }
            }
            for input in inputs {
                let p = PadInput::from_name(input)
                    .ok_or_else(|| format!("unknown controller input `{input}`"))?;
                if let Some(other) = b.pad[p.index()]
                    && other != action
                {
                    return Err(format!(
                        "controller input `{input}` is bound to both {} and {}",
                        other.name(),
                        action.name()
                    ));
                }
                b.pad[p.index()] = Some(action);
            }
        }
        Ok(b)
    }

    /// Every action with its keys and controller inputs, for printing.
    pub fn describe(&self) -> String {
        let mut out = String::new();
        for action in Action::ALL {
            let keys: Vec<String> = self
                .keys
                .iter()
                .filter(|(_, a)| **a == action)
                .map(|(k, _)| key_name(*k))
                .collect();
            let pads: Vec<&str> = PadInput::ALL
                .into_iter()
                .filter(|p| self.pad_action(*p) == Some(action))
                .map(PadInput::name)
                .collect();
            let or_dash = |v: Vec<String>| {
                if v.is_empty() {
                    "-".to_string()
                } else {
                    v.join(", ")
                }
            };
            out.push_str(&format!(
                "{:<8} keys: {:<18} pad: {}\n",
                action.name(),
                or_dash(keys),
                or_dash(pads.into_iter().map(String::from).collect()),
            ));
        }
        out
    }
}

fn parse_action(name: &str) -> Result<Action, String> {
    Action::from_name(name).ok_or_else(|| {
        let all: Vec<&str> = Action::ALL.iter().map(|a| a.name()).collect();
        format!("unknown action `{name}` (actions: {})", all.join(", "))
    })
}

/// A key by its file name: the sim's names (`Shift`, `Z`, `F5`, `vk:20`) plus `LShift`, `RShift`,
/// `LCtrl`, `RCtrl`, `LAlt`, `RAlt` for one hand only.
pub fn key_from_name(name: &str) -> Option<u8> {
    match name.to_ascii_lowercase().as_str() {
        "lshift" => Some(vk::LEFT_SHIFT),
        "rshift" => Some(vk::RIGHT_SHIFT),
        "lctrl" => Some(vk::LEFT_CTRL),
        "rctrl" => Some(vk::RIGHT_CTRL),
        "lalt" => Some(vk::LEFT_ALT),
        "ralt" => Some(vk::RIGHT_ALT),
        _ => key_code(name),
    }
}

/// The name `key_from_name` reads back for a virtual-key code.
pub fn key_name(code: u8) -> String {
    match code {
        vk::LEFT_SHIFT => "LShift".into(),
        vk::RIGHT_SHIFT => "RShift".into(),
        vk::LEFT_CTRL => "LCtrl".into(),
        vk::RIGHT_CTRL => "RCtrl".into(),
        vk::LEFT_ALT => "LAlt".into(),
        vk::RIGHT_ALT => "RAlt".into(),
        vk::SHIFT => "Shift".into(),
        vk::CTRL => "Ctrl".into(),
        vk::ALT => "Alt".into(),
        vk::LEFT => "Left".into(),
        vk::RIGHT => "Right".into(),
        vk::UP => "Up".into(),
        vk::DOWN => "Down".into(),
        vk::ENTER => "Enter".into(),
        vk::SPACE => "Space".into(),
        vk::ESCAPE => "Escape".into(),
        vk::BACKSPACE => "Backspace".into(),
        vk::TAB => "Tab".into(),
        vk::PAGE_UP => "PageUp".into(),
        vk::PAGE_DOWN => "PageDown".into(),
        vk::END => "End".into(),
        vk::HOME => "Home".into(),
        vk::INSERT => "Insert".into(),
        vk::DELETE => "Delete".into(),
        c @ vk::F1..=123 => format!("F{}", c - vk::F1 + 1),
        c if c.is_ascii_uppercase() || c.is_ascii_digit() => char::from(c).to_string(),
        c => format!("vk:{c}"),
    }
}

/// The player's changes, as stored in the bindings file: action name to input names.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pad: BTreeMap<String, Vec<String>>,
}

impl Config {
    /// Applies `action=input,input,...` where an input is a key name or `pad:<name>`. The keys
    /// (or controller inputs) listed replace what the file had for that action; the other kind
    /// stays as it was. The result is checked, so a bad name or a double binding is an error.
    pub fn set(&mut self, spec: &str) -> Result<(), String> {
        let (action, inputs) = spec
            .split_once('=')
            .ok_or_else(|| format!("`{spec}`: expected ACTION=INPUT[,INPUT...]"))?;
        let action = parse_action(action.trim())?.name().to_string();
        let (mut keys, mut pads) = (Vec::new(), Vec::new());
        for input in inputs.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            match input.strip_prefix("pad:") {
                Some(p) => pads.push(p.to_string()),
                None => keys.push(input.to_string()),
            }
        }
        if keys.is_empty() && pads.is_empty() {
            return Err(format!("`{spec}`: no input named"));
        }
        let mut next = self.clone();
        if !keys.is_empty() {
            next.keys.insert(action.clone(), keys);
        }
        if !pads.is_empty() {
            next.pad.insert(action, pads);
        }
        Bindings::from_config(&next)?;
        *self = next;
        Ok(())
    }
}

/// Where the global bindings file lives: the user's config folder, `iwbtg-collection/bindings.toml`.
pub fn default_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.map(|b| b.join("iwbtg-collection").join("bindings.toml"))
}

/// Reads the bindings file. A missing file is the defaults; a broken one is an error.
pub fn load_config(path: &Path) -> Result<Config, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

pub fn save_config(path: &Path, config: &Config) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let text = toml::to_string(config).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_fangame_standard() {
        let b = Bindings::default();
        assert_eq!(b.key_action(vk::RIGHT_SHIFT), Some(Action::Jump));
        assert_eq!(b.key_action(vk::Z), Some(Action::Shoot));
        assert_eq!(b.key_action(vk::R), Some(Action::Restart));
        assert_eq!(b.pad_action(PadInput::South), Some(Action::Jump));
        assert_eq!(b.pad_action(PadInput::StickLeft), Some(Action::Left));
    }

    #[test]
    fn a_named_action_loses_its_default_keys() {
        let mut c = Config::default();
        c.set("jump=Space").unwrap();
        let b = Bindings::from_config(&c).unwrap();
        assert_eq!(b.key_action(vk::SPACE), Some(Action::Jump));
        assert_eq!(b.key_action(vk::LEFT_SHIFT), None);
        assert_eq!(b.pad_action(PadInput::South), Some(Action::Jump));
    }

    #[test]
    fn pad_bindings_replace_only_the_pad() {
        let mut c = Config::default();
        c.set("jump=pad:north").unwrap_err();
        c.set("restart=pad:east").unwrap_err();
        c.set("suicide=pad:north").unwrap_err();
        c.set("restart=pad:select").unwrap();
        let b = Bindings::from_config(&c).unwrap();
        assert_eq!(b.pad_action(PadInput::Select), Some(Action::Restart));
        assert_eq!(b.pad_action(PadInput::North), None);
        assert_eq!(b.key_action(vk::R), Some(Action::Restart));
    }

    #[test]
    fn a_double_binding_is_refused() {
        let mut c = Config::default();
        assert!(c.set("jump=Z").is_err());
        assert_eq!(c, Config::default());
        assert!(c.set("fly=Space").is_err());
        assert!(c.set("jump=Banana").is_err());
    }

    #[test]
    fn the_file_round_trips() {
        let mut c = Config::default();
        c.set("jump=LShift,Space").unwrap();
        let text = toml::to_string(&c).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back, c);
        let b = Bindings::from_config(&back).unwrap();
        assert_eq!(b.key_action(vk::LEFT_SHIFT), Some(Action::Jump));
        assert_eq!(b.key_action(vk::RIGHT_SHIFT), None);
    }

    #[test]
    fn key_names_read_back() {
        for code in [vk::LEFT_SHIFT, vk::SPACE, vk::F1 + 4, b'Z', b'7', 20, vk::PAGE_UP] {
            assert_eq!(key_from_name(&key_name(code)), Some(code));
        }
    }
}

/// The bindings' command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug)]
pub struct BindingsArgs {
    /// Change a global binding and save it: `ACTION=INPUT[,INPUT...]`, an input being a key name
    /// (`Space`, `LShift`, `F5`) or `pad:<button>` (`pad:south`, `pad:dpad-left`, `pad:stick-up`).
    /// Repeatable. Replaces that action's keys (or controller inputs). With a package, the game
    /// then starts; without one, the bindings are only saved.
    #[arg(long, value_name = "ACTION=INPUT")]
    pub bind: Vec<String>,
    /// Print the global bindings and exit.
    #[arg(long)]
    pub show_bindings: bool,
    /// Bindings file (default: the user's config folder, iwbtg-collection/bindings.toml).
    #[arg(long, value_name = "FILE")]
    pub bindings_file: Option<PathBuf>,
}

/// Loads the bindings file, applying and saving any `--bind` changes.
pub fn load(args: &BindingsArgs) -> Result<Bindings, String> {
    let path = match &args.bindings_file {
        Some(p) => p.clone(),
        None => default_path().ok_or("no config folder found; pass --bindings-file")?,
    };
    let mut config = load_config(&path)?;
    if !args.bind.is_empty() {
        for spec in &args.bind {
            config.set(spec)?;
        }
        save_config(&path, &config)?;
        eprintln!("bindings saved to {}", path.display());
    }
    Bindings::from_config(&config).map_err(|e| format!("{}: {e}", path.display()))
}

/// Puts the input collector, built from the player's bindings, into the app.
pub struct BindingsPlugin(pub Bindings);

impl Plugin for BindingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(crate::input::Collector::new(self.0.clone()));
    }
}
