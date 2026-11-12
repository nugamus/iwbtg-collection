//! The global input bindings: which keys and controller inputs stand for which client action
//! (docs/content.md, Controls). One set for every game, defaulting to the fangame standard. The
//! player's changes live in the `[bindings]` table of the settings file (settings.rs); only the
//! actions it names differ from the defaults.
//!
//! ```toml
//! [bindings.keys]
//! jump = ["Shift", "Space"]
//! [bindings.pad]
//! jump = ["south"]
//! quick-retry = ["select"]
//! skip-cutscene = ["right-bumper"]
//! ```
//!
//! `quick-retry` (retry.rs) and `skip-cutscene` (skip.rs) sit beside the sim actions in the same
//! tables, but they are client keys ([`ClientKey`]), not `Action`s: they never enter an
//! `InputFrame` or a replay. Quick-retry defaults to the key left of `1` (Backquote),
//! skip-cutscene to Backspace; neither has a controller default.

use std::collections::{BTreeMap, BTreeSet};

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

/// The name of the quick-retry binding in the file and in `--bind`.
pub const QUICK_RETRY: &str = "quick-retry";

/// A key the client handles itself, never an `Action`: it does not enter an `InputFrame`, so a
/// replay never sees it. Bound beside the actions, and never on an input an action has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientKey {
    /// Starts the run again (retry.rs).
    QuickRetry,
    /// Skips the cutscene or dialogue region that holds in a ranked run (skip.rs, #573). Not the
    /// game's own `skip` action (S), which the sim reads.
    SkipCutscene,
}

impl ClientKey {
    pub const ALL: [ClientKey; 2] = [ClientKey::QuickRetry, ClientKey::SkipCutscene];

    /// The name in the bindings file and in `--bind`.
    pub const fn name(self) -> &'static str {
        match self {
            ClientKey::QuickRetry => QUICK_RETRY,
            ClientKey::SkipCutscene => "skip-cutscene",
        }
    }

    /// The client key with this file name, any case.
    pub fn from_name(name: &str) -> Option<ClientKey> {
        ClientKey::ALL
            .into_iter()
            .find(|k| k.name().eq_ignore_ascii_case(name))
    }

    const fn index(self) -> usize {
        self as usize
    }

    /// The default key: Backquote (left of `1`) to quick-retry, Backspace to skip.
    const fn default_key(self) -> u8 {
        match self {
            ClientKey::QuickRetry => oem::BACKQUOTE,
            ClientKey::SkipCutscene => vk::BACKSPACE,
        }
    }
}

/// The keys and controller inputs of one client key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ClientInputs {
    keys: BTreeSet<u8>,
    pad: BTreeSet<PadInput>,
}

/// Virtual-key codes of the punctuation keys the sim has no names for (Windows `VK_OEM_*`).
pub mod oem {
    pub const SEMICOLON: u8 = 0xBA;
    pub const EQUAL: u8 = 0xBB;
    pub const COMMA: u8 = 0xBC;
    pub const MINUS: u8 = 0xBD;
    pub const PERIOD: u8 = 0xBE;
    pub const SLASH: u8 = 0xBF;
    /// The key left of `1` on a US layout.
    pub const BACKQUOTE: u8 = 0xC0;
    pub const BRACKET_LEFT: u8 = 0xDB;
    pub const BACKSLASH: u8 = 0xDC;
    pub const BRACKET_RIGHT: u8 = 0xDD;
    pub const QUOTE: u8 = 0xDE;
}

/// The punctuation keys by file name.
const OEM_NAMES: [(&str, u8); 11] = [
    ("Semicolon", oem::SEMICOLON),
    ("Equal", oem::EQUAL),
    ("Comma", oem::COMMA),
    ("Minus", oem::MINUS),
    ("Period", oem::PERIOD),
    ("Slash", oem::SLASH),
    ("Backquote", oem::BACKQUOTE),
    ("BracketLeft", oem::BRACKET_LEFT),
    ("Backslash", oem::BACKSLASH),
    ("BracketRight", oem::BRACKET_RIGHT),
    ("Quote", oem::QUOTE),
];

/// What every key and controller input does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bindings {
    keys: BTreeMap<u8, Action>,
    pad: [Option<Action>; PadInput::ALL.len()],
    /// The keys and controller inputs of each client key, by `ClientKey::index`.
    client: [ClientInputs; ClientKey::ALL.len()],
}

impl Default for Bindings {
    fn default() -> Bindings {
        let mut b = Bindings {
            keys: IWBTG_DEFAULT_KEYS.into_iter().collect(),
            pad: [None; PadInput::ALL.len()],
            client: ClientKey::ALL.map(|k| ClientInputs {
                keys: BTreeSet::from([k.default_key()]),
                pad: BTreeSet::new(),
            }),
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

    /// The client key a key stands for.
    pub fn client_key(&self, key: u8) -> Option<ClientKey> {
        ClientKey::ALL
            .into_iter()
            .find(|k| self.client[k.index()].keys.contains(&key))
    }

    /// The client key a controller input stands for.
    pub fn client_pad(&self, input: PadInput) -> Option<ClientKey> {
        ClientKey::ALL
            .into_iter()
            .find(|k| self.client[k.index()].pad.contains(&input))
    }

    /// The key quick-retries the run.
    pub fn is_quick_retry_key(&self, key: u8) -> bool {
        self.client_key(key) == Some(ClientKey::QuickRetry)
    }

    /// The controller input quick-retries the run.
    #[cfg(test)]
    pub fn is_quick_retry_pad(&self, input: PadInput) -> bool {
        self.client_pad(input) == Some(ClientKey::QuickRetry)
    }

    /// Some key or controller input stands for `action`.
    pub fn binds(&self, action: Action) -> bool {
        self.keys.values().any(|a| *a == action) || self.pad.contains(&Some(action))
    }

    /// The defaults with the file's changes applied. Each action the file names loses all its
    /// default keys (or all its default controller inputs) and gets the listed ones.
    pub fn from_config(config: &Config) -> Result<Bindings, String> {
        let mut b = Bindings::default();
        // Client keys first: the sim actions' entries are checked against what they end up with.
        for ck in ClientKey::ALL {
            if let Some(keys) = config.keys.get(ck.name()) {
                let mut set = BTreeSet::new();
                for key in keys {
                    set.insert(key_from_name(key).ok_or_else(|| format!("unknown key `{key}`"))?);
                }
                b.client[ck.index()].keys = set;
            }
            if let Some(inputs) = config.pad.get(ck.name()) {
                let mut set = BTreeSet::new();
                for input in inputs {
                    set.insert(
                        PadInput::from_name(input)
                            .ok_or_else(|| format!("unknown controller input `{input}`"))?,
                    );
                }
                b.client[ck.index()].pad = set;
            }
        }
        // Two client keys never share an input.
        for (i, a) in ClientKey::ALL.into_iter().enumerate() {
            for other in &ClientKey::ALL[i + 1..] {
                let (x, y) = (&b.client[a.index()], &b.client[other.index()]);
                if let Some(key) = x.keys.intersection(&y.keys).next() {
                    return Err(format!(
                        "key `{}` is bound to both {} and {}",
                        key_name(*key),
                        a.name(),
                        other.name()
                    ));
                }
                if let Some(input) = x.pad.intersection(&y.pad).next() {
                    return Err(format!(
                        "controller input `{}` is bound to both {} and {}",
                        input.name(),
                        a.name(),
                        other.name()
                    ));
                }
            }
        }
        for (name, keys) in &config.keys {
            if ClientKey::from_name(name).is_some() {
                continue;
            }
            let action = parse_action(name)?;
            b.keys.retain(|_, a| *a != action);
            for key in keys {
                let code = key_from_name(key).ok_or_else(|| format!("unknown key `{key}`"))?;
                if let Some(ck) = b.client_key(code) {
                    return Err(format!(
                        "key `{key}` is bound to both {} and {}",
                        ck.name(),
                        action.name()
                    ));
                }
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
            if ClientKey::from_name(name).is_some() {
                continue;
            }
            let action = parse_action(name)?;
            for slot in &mut b.pad {
                if *slot == Some(action) {
                    *slot = None;
                }
            }
            for input in inputs {
                let p = PadInput::from_name(input)
                    .ok_or_else(|| format!("unknown controller input `{input}`"))?;
                if let Some(ck) = b.client_pad(p) {
                    return Err(format!(
                        "controller input `{input}` is bound to both {} and {}",
                        ck.name(),
                        action.name()
                    ));
                }
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
        // A client key's input the sim actions still hold (a default not moved off) clashes.
        for ck in ClientKey::ALL {
            for key in &b.client[ck.index()].keys {
                if let Some(other) = b.keys.get(key) {
                    return Err(format!(
                        "key `{}` is bound to both {} and {}",
                        key_name(*key),
                        ck.name(),
                        other.name()
                    ));
                }
            }
            for input in &b.client[ck.index()].pad {
                if let Some(other) = b.pad[input.index()] {
                    return Err(format!(
                        "controller input `{}` is bound to both {} and {}",
                        input.name(),
                        ck.name(),
                        other.name()
                    ));
                }
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
                "{:<13} keys: {:<18} pad: {}\n",
                action.name(),
                or_dash(keys),
                or_dash(pads.into_iter().map(String::from).collect()),
            ));
        }
        for ck in ClientKey::ALL {
            let inputs = &self.client[ck.index()];
            let keys: Vec<String> = inputs.keys.iter().map(|k| key_name(*k)).collect();
            let pads: Vec<String> = inputs.pad.iter().map(|p| p.name().to_string()).collect();
            let or_dash = |v: Vec<String>| {
                if v.is_empty() {
                    "-".to_string()
                } else {
                    v.join(", ")
                }
            };
            out.push_str(&format!(
                "{:<13} keys: {:<18} pad: {}\n",
                ck.name(),
                or_dash(keys),
                or_dash(pads)
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
        lower => OEM_NAMES
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(lower))
            .map(|(_, c)| *c)
            .or_else(|| key_code(name)),
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
        c if OEM_NAMES.iter().any(|(_, o)| *o == c) => OEM_NAMES
            .iter()
            .find(|(_, o)| *o == c)
            .map_or_else(String::new, |(n, _)| (*n).to_string()),
        c @ vk::F1..=123 => format!("F{}", c - vk::F1 + 1),
        c if c.is_ascii_uppercase() || c.is_ascii_digit() => char::from(c).to_string(),
        c => format!("vk:{c}"),
    }
}

/// The player's changes, as stored in the settings' `[bindings]` table: action name to input names.
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
        let action = action.trim();
        let action = if let Some(ck) = ClientKey::from_name(action) {
            ck.name().to_string()
        } else {
            parse_action(action)?.name().to_string()
        };
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

impl Config {
    /// Reads the settings file's `[bindings]` table. Each entry is checked on its own: a broken
    /// one (unknown action, key or input, a wrong type, a double binding) is pushed to `warnings`
    /// and that action keeps its defaults; the other entries still count.
    pub fn from_table(table: &toml::Table, warnings: &mut Vec<String>) -> Config {
        let mut config = Config::default();
        for (kind, value) in table {
            let is_keys = match kind.as_str() {
                "keys" => true,
                "pad" => false,
                _ => {
                    warnings.push(format!("unknown entry `bindings.{kind}`, ignored"));
                    continue;
                }
            };
            let Some(entries) = value.as_table() else {
                warnings.push(format!("`[bindings.{kind}]` must be a table, using defaults"));
                continue;
            };
            for (action, inputs) in entries {
                let list: Option<Vec<String>> = inputs
                    .as_array()
                    .and_then(|a| a.iter().map(|v| v.as_str().map(String::from)).collect());
                let Some(list) = list else {
                    warnings.push(format!(
                        "bindings.{kind}.{action} must be a list of names, using the default"
                    ));
                    continue;
                };
                let mut next = config.clone();
                let map = if is_keys { &mut next.keys } else { &mut next.pad };
                map.insert(action.clone(), list);
                match Bindings::from_config(&next) {
                    Ok(_) => config = next,
                    Err(e) => warnings.push(format!("bindings.{kind}.{action}: {e}, using the default")),
                }
            }
        }
        config
    }

    /// The `[bindings]` table for the settings file.
    pub fn to_table(&self) -> toml::Table {
        toml::Table::try_from(self).unwrap_or_default()
    }
}

/// The settings' bindings as a `Config`; problems are warnings (see `Config::from_table`).
pub fn config_from_settings() -> Config {
    let mut warnings = Vec::new();
    let config = Config::from_table(&crate::settings::get().bindings, &mut warnings);
    for w in warnings {
        crate::storage::warn(&format!("settings: {w}"));
    }
    config
}

/// Saves `config` as the settings' `[bindings]` table (one write of `settings.toml`, which is
/// browser storage on the web).
pub fn save_config(config: &Config) {
    let table = config.to_table();
    crate::settings::update(|s| s.bindings = table);
}

/// The file earlier builds kept the bindings in, beside `settings.toml` (#541).
const LEGACY_FILE: &str = "bindings.toml";

/// The table of an old `bindings.toml`, when the settings have no bindings yet and the file
/// reads. The caller removes the file once the table is saved.
fn legacy_table(store: &crate::storage::Store, current: &toml::Table) -> Option<toml::Table> {
    if !current.is_empty() {
        return None;
    }
    let text = match store.read_text(LEGACY_FILE) {
        Ok(Some(text)) => text,
        Ok(None) => return None,
        Err(e) => {
            crate::storage::warn(&format!("bindings {e}, not moved into settings"));
            return None;
        }
    };
    match toml::from_str::<Config>(&text) {
        Ok(c) => Some(c.to_table()),
        Err(e) => {
            crate::storage::warn(&format!(
                "bindings {}: {e}, not moved into settings",
                store.describe(LEGACY_FILE)
            ));
            None
        }
    }
}

/// Moves an old `bindings.toml` into the settings once, when `[bindings]` is empty.
fn migrate_legacy() {
    let Some(store) = crate::storage::Store::config() else {
        return;
    };
    let Some(table) = legacy_table(&store, &crate::settings::get().bindings) else {
        return;
    };
    crate::settings::update(|s| s.bindings = table);
    match store.remove(LEGACY_FILE) {
        Ok(()) => crate::storage::warn(&format!(
            "bindings moved from {} into settings",
            store.describe(LEGACY_FILE)
        )),
        Err(e) => crate::storage::warn(&format!("bindings {e}, old file left in place")),
    }
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
    fn the_settings_table_round_trips() {
        let mut c = Config::default();
        c.set("jump=LShift,Space").unwrap();
        c.set("restart=pad:select").unwrap();
        let table = c.to_table();
        assert!(table.contains_key("keys") && table.contains_key("pad"));
        let mut w = Vec::new();
        assert_eq!(Config::from_table(&table, &mut w), c);
        assert!(w.is_empty(), "{w:?}");
        // Through the whole settings file text as well.
        let s = crate::settings::Settings {
            bindings: table,
            ..Default::default()
        };
        let back = crate::settings::Settings::parse(&s.to_toml(), &mut w);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(Config::from_table(&back.bindings, &mut w), c);
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn a_broken_entry_warns_and_keeps_its_default() {
        let table: toml::Table = "[keys]\n\
             jump = [\"Space\"]\nshoot = [\"Banana\"]\nfly = [\"X\"]\nrestart = 5\n\
             [pad]\nsuicide = [\"nowhere\"]\n"
            .parse()
            .unwrap();
        let mut w = Vec::new();
        let c = Config::from_table(&table, &mut w);
        assert_eq!(w.len(), 4, "{w:?}");
        let b = Bindings::from_config(&c).unwrap();
        assert_eq!(
            b.key_action(vk::SPACE),
            Some(Action::Jump),
            "the good entry counts"
        );
        assert_eq!(
            b.key_action(vk::Z),
            Some(Action::Shoot),
            "a broken one is the default"
        );
        assert_eq!(b.key_action(vk::R), Some(Action::Restart));
        assert_eq!(b.pad_action(PadInput::East), Some(Action::Suicide));
        let table: toml::Table = "[extra]\na = 1\n[pad]\n".parse().unwrap();
        let mut w = Vec::new();
        assert_eq!(Config::from_table(&table, &mut w), Config::default());
        assert_eq!(w.len(), 1, "{w:?}");
    }

    #[test]
    fn a_double_binding_entry_is_dropped() {
        let table: toml::Table = "[keys]\njump = [\"Z\"]\n".parse().unwrap();
        let mut w = Vec::new();
        assert_eq!(Config::from_table(&table, &mut w), Config::default());
        assert_eq!(w.len(), 1, "{w:?}");
    }

    #[test]
    fn an_old_bindings_file_moves_into_empty_settings() {
        let dir = std::env::temp_dir().join(format!("iwbtg-bindings-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = crate::storage::Store::at(dir.clone());
        let none = toml::Table::new();
        assert_eq!(legacy_table(&store, &none), None, "no old file");
        store
            .write_text(LEGACY_FILE, "[keys]\njump = [\"Space\"]\n")
            .unwrap();
        let table = legacy_table(&store, &none).expect("moves");
        let mut w = Vec::new();
        let c = Config::from_table(&table, &mut w);
        assert_eq!(c.keys["jump"], vec!["Space".to_string()]);
        // Settings that already have bindings win; the old file is left alone.
        assert_eq!(legacy_table(&store, &table), None);
        // A broken old file is not moved.
        store.write_text(LEGACY_FILE, "[keys\n").unwrap();
        assert_eq!(legacy_table(&store, &none), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn quick_retry_defaults_to_backquote_and_is_not_an_action() {
        let b = Bindings::default();
        assert!(b.is_quick_retry_key(oem::BACKQUOTE));
        assert_eq!(b.key_action(oem::BACKQUOTE), None);
        assert!(!b.is_quick_retry_key(vk::R));
        assert!(PadInput::ALL.iter().all(|p| !b.is_quick_retry_pad(*p)));
    }

    #[test]
    fn quick_retry_is_rebound_with_the_other_bindings() {
        let mut c = Config::default();
        c.set("quick-retry=F5,pad:select").unwrap();
        let b = Bindings::from_config(&c).unwrap();
        assert!(b.is_quick_retry_key(vk::F1 + 4));
        assert!(!b.is_quick_retry_key(oem::BACKQUOTE), "the default is replaced");
        assert!(b.is_quick_retry_pad(PadInput::Select));
        let mut w = Vec::new();
        assert_eq!(Config::from_table(&c.to_table(), &mut w), c);
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn quick_retry_cannot_share_an_input_with_an_action() {
        let mut c = Config::default();
        assert!(c.set("quick-retry=Z").is_err(), "Z shoots");
        assert!(c.set("quick-retry=pad:south").is_err(), "south jumps");
        assert!(c.set("jump=Backquote").is_err(), "the default quick-retry key");
        assert_eq!(c, Config::default());
        // Moving quick-retry away frees the key for an action.
        c.set("quick-retry=F5").unwrap();
        c.set("jump=Backquote").unwrap();
        assert!(Bindings::from_config(&c).is_ok());
    }

    #[test]
    fn skip_cutscene_defaults_to_backspace_and_is_not_an_action() {
        let b = Bindings::default();
        assert_eq!(b.client_key(vk::BACKSPACE), Some(ClientKey::SkipCutscene));
        assert_eq!(b.key_action(vk::BACKSPACE), None);
        // The game's own skip action (S) is a different thing: it stays a sim action.
        assert_eq!(b.key_action(b'S'), Some(Action::Skip));
        assert_eq!(b.client_key(b'S'), None);
        assert!(PadInput::ALL.iter().all(|p| b.client_pad(*p).is_none()));
    }

    #[test]
    fn skip_cutscene_is_rebound_and_never_shares_an_input() {
        let mut c = Config::default();
        c.set("skip-cutscene=Tab,pad:select").unwrap();
        let b = Bindings::from_config(&c).unwrap();
        assert_eq!(b.client_key(vk::TAB), Some(ClientKey::SkipCutscene));
        assert_eq!(b.client_key(vk::BACKSPACE), None, "the default is replaced");
        assert_eq!(b.client_pad(PadInput::Select), Some(ClientKey::SkipCutscene));
        assert!(c.set("quick-retry=pad:select").is_err(), "two client keys");
        assert!(c.set("quick-retry=Tab").is_err());
        assert!(c.set("jump=Tab").is_err(), "a client key and an action");
        assert!(c.set("skip-cutscene=S").is_err(), "S is the game's skip action");
        let mut w = Vec::new();
        assert_eq!(Config::from_table(&c.to_table(), &mut w), c);
        assert!(w.is_empty(), "{w:?}");
        assert!(Bindings::default().describe().contains("skip-cutscene"));
    }

    #[test]
    fn key_names_read_back() {
        for code in [
            vk::LEFT_SHIFT,
            vk::SPACE,
            vk::F1 + 4,
            b'Z',
            b'7',
            20,
            vk::PAGE_UP,
            oem::BACKQUOTE,
            oem::QUOTE,
        ] {
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
}

/// Reads the bindings from the settings (`settings::load` has run), applying and saving any
/// `--bind` changes. `isolated` (a `--screenshot` check) also skips moving an old `bindings.toml`.
pub fn load(args: &BindingsArgs, isolated: bool) -> Result<Bindings, String> {
    if !isolated {
        migrate_legacy();
    }
    let mut config = config_from_settings();
    if !args.bind.is_empty() {
        for spec in &args.bind {
            config.set(spec)?;
        }
        save_config(&config);
        eprintln!("bindings saved in the settings file");
    }
    Bindings::from_config(&config)
}

/// Puts the input collector, built from the player's bindings, into the app.
pub struct BindingsPlugin(pub Bindings);

impl Plugin for BindingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(crate::input::Collector::new(self.0.clone()));
    }
}
