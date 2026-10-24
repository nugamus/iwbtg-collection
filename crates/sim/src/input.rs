//! Client actions and per-tick input frames, plus the `.inputs` text format for tests and traces.
//!
//! See `docs/architecture.md`, "Actions and input frames" and "Text inputs for tests". Games read
//! actions, never raw keys. Keys only appear here so the research recorder's `.inputs` files
//! (which name original keys like `Shift`) compile to the same frames as our action-named files.
//!
//! Edges are per input, not per action. Each key (or, in an action-named file, each action name)
//! keeps its own down/up state, and its press and release set the action's `pressed` and
//! `released` bits even while another key bound to the same action keeps the action held. This is
//! what GM8 games see: a game that checks `keyboard_check_pressed` on each of its keys, and OpenGMK
//! itself for `vk_shift` (true when either Left or Right Shift was pressed, whatever the other one
//! is doing). The client's input collector must build frames the same way, so that a replay and an
//! `.inputs` file agree for the same physical input. `held` is the OR of every input's state.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One client action. Each is one bit of a `u16`; the top four bits are reserved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum Action {
    Left = 1 << 0,
    Right = 1 << 1,
    Up = 1 << 2,
    Down = 1 << 3,
    Jump = 1 << 4,
    Shoot = 1 << 5,
    Restart = 1 << 6,
    Suicide = 1 << 7,
    Skip = 1 << 8,
    Accept = 1 << 9,
    Back = 1 << 10,
    Menu = 1 << 11,
}

impl Action {
    /// Every action, in bit order.
    pub const ALL: [Action; 12] = [
        Action::Left,
        Action::Right,
        Action::Up,
        Action::Down,
        Action::Jump,
        Action::Shoot,
        Action::Restart,
        Action::Suicide,
        Action::Skip,
        Action::Accept,
        Action::Back,
        Action::Menu,
    ];

    /// Bits used by actions. Anything outside is reserved for future actions.
    pub const MASK: u16 = 0x0fff;

    /// The action's bit in `InputFrame` fields.
    pub const fn bit(self) -> u16 {
        self as u16
    }

    /// Lowercase name as written in `.inputs` files and scripts (`jump`).
    pub const fn name(self) -> &'static str {
        match self {
            Action::Left => "left",
            Action::Right => "right",
            Action::Up => "up",
            Action::Down => "down",
            Action::Jump => "jump",
            Action::Shoot => "shoot",
            Action::Restart => "restart",
            Action::Suicide => "suicide",
            Action::Skip => "skip",
            Action::Accept => "accept",
            Action::Back => "back",
            Action::Menu => "menu",
        }
    }

    /// Looks up an action by name, ignoring ASCII case.
    pub fn from_name(name: &str) -> Option<Action> {
        Action::ALL
            .into_iter()
            .find(|a| a.name().eq_ignore_ascii_case(name))
    }
}

/// The input for one tick. Edge bits keep sub-tick taps: a press and a release inside one tick
/// both show up, which GM8 treats differently from holding for one tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InputFrame {
    /// Actions down at the end of the tick's input (any of their inputs down).
    pub held: u16,
    /// Actions one of whose inputs went down during the tick. Can be set while the action was
    /// already held through another input (see the module docs).
    pub pressed: u16,
    /// Actions one of whose inputs went up during the tick. Can be set while the action stays
    /// held through another input (see the module docs).
    pub released: u16,
}

impl InputFrame {
    /// No action held, no edges.
    pub const EMPTY: InputFrame = InputFrame {
        held: 0,
        pressed: 0,
        released: 0,
    };

    /// Frame from the held bits of the previous and the current tick, with at most one edge per
    /// action. Sub-tick taps need the event-based path (`parse_inputs` or the client's collector).
    pub const fn from_held(previous: u16, held: u16) -> InputFrame {
        InputFrame {
            held,
            pressed: held & !previous,
            released: previous & !held,
        }
    }

    /// True while the action is down.
    pub const fn is_held(self, action: Action) -> bool {
        self.held & action.bit() != 0
    }

    /// True on the tick the action went down.
    pub const fn was_pressed(self, action: Action) -> bool {
        self.pressed & action.bit() != 0
    }

    /// True on the tick the action went up.
    pub const fn was_released(self, action: Action) -> bool {
        self.released & action.bit() != 0
    }
}

/// Windows virtual-key codes, the numbering GM8's `vk_*` constants and the recorder use.
pub mod vk {
    pub const BACKSPACE: u8 = 8;
    pub const TAB: u8 = 9;
    pub const ENTER: u8 = 13;
    pub const SHIFT: u8 = 16;
    pub const CTRL: u8 = 17;
    pub const ALT: u8 = 18;
    pub const LEFT_SHIFT: u8 = 160;
    pub const RIGHT_SHIFT: u8 = 161;
    pub const LEFT_CTRL: u8 = 162;
    pub const RIGHT_CTRL: u8 = 163;
    pub const LEFT_ALT: u8 = 164;
    pub const RIGHT_ALT: u8 = 165;
    pub const ESCAPE: u8 = 27;
    pub const SPACE: u8 = 32;
    pub const PAGE_UP: u8 = 33;
    pub const PAGE_DOWN: u8 = 34;
    pub const END: u8 = 35;
    pub const HOME: u8 = 36;
    pub const LEFT: u8 = 37;
    pub const UP: u8 = 38;
    pub const RIGHT: u8 = 39;
    pub const DOWN: u8 = 40;
    pub const INSERT: u8 = 45;
    pub const DELETE: u8 = 46;
    /// F1; F2..F12 follow.
    pub const F1: u8 = 112;
    pub const Q: u8 = b'Q';
    pub const R: u8 = b'R';
    pub const S: u8 = b'S';
    pub const Z: u8 = b'Z';
}

/// Maps a recorder key name to its virtual-key code, ignoring ASCII case. Accepts the recorder's
/// set: `Shift Ctrl Alt Left Right Up Down Enter Space Escape Backspace Tab PageUp PageDown Home
/// End Insert Delete F1..F12 A..Z 0..9` and `vk:NN`.
pub fn key_code(name: &str) -> Option<u8> {
    let n = name.to_ascii_lowercase();
    let code = match n.as_str() {
        "shift" => vk::SHIFT,
        "ctrl" | "control" => vk::CTRL,
        "alt" => vk::ALT,
        "left" => vk::LEFT,
        "up" => vk::UP,
        "right" => vk::RIGHT,
        "down" => vk::DOWN,
        "enter" | "return" => vk::ENTER,
        "space" => vk::SPACE,
        "escape" | "esc" => vk::ESCAPE,
        "backspace" => vk::BACKSPACE,
        "tab" => vk::TAB,
        "pageup" => vk::PAGE_UP,
        "pagedown" => vk::PAGE_DOWN,
        "end" => vk::END,
        "home" => vk::HOME,
        "insert" => vk::INSERT,
        "delete" => vk::DELETE,
        _ => {
            if let Some(v) = n.strip_prefix("vk:") {
                return v.parse::<u8>().ok();
            }
            if let Some(f) = n.strip_prefix('f')
                && let Ok(k) = f.parse::<u8>()
                && (1..=12).contains(&k)
            {
                return Some(vk::F1 + k - 1);
            }
            return match n.as_bytes() {
                [b] if b.is_ascii_lowercase() || b.is_ascii_digit() => Some(b.to_ascii_uppercase()),
                _ => None,
            };
        }
    };
    Some(code)
}

/// The key the recorder actually presses or releases for a code: OpenGMK's `button_press` and
/// `button_release` turn generic Shift, Ctrl and Alt (16, 17, 18) into the left-hand keys (160,
/// 162, 164). So `press Shift` and `release vk:160` act on the same key.
pub const fn recorder_key(code: u8) -> u8 {
    match code {
        vk::SHIFT => vk::LEFT_SHIFT,
        vk::CTRL => vk::LEFT_CTRL,
        vk::ALT => vk::LEFT_ALT,
        other => other,
    }
}

/// The generic modifier a left- or right-hand modifier counts as, as GM8's `keyboard_check` on
/// `vk_shift`, `vk_control` and `vk_alt` reads either hand.
const fn generic_modifier(code: u8) -> Option<u8> {
    match code {
        vk::LEFT_SHIFT | vk::RIGHT_SHIFT => Some(vk::SHIFT),
        vk::LEFT_CTRL | vk::RIGHT_CTRL => Some(vk::CTRL),
        vk::LEFT_ALT | vk::RIGHT_ALT => Some(vk::ALT),
        _ => None,
    }
}

/// IWBTG engine default keys for each action, used to read the research `.inputs` files.
/// `back` has no default key. OPEN: confirm in CLONE_SPEC (none of these is confirmed yet; Q for
/// `suicide` is a guess from common fangame engines).
pub const IWBTG_DEFAULT_KEYS: [(u8, Action); 11] = [
    (vk::SHIFT, Action::Jump),
    (vk::Z, Action::Shoot),
    (vk::R, Action::Restart),
    (vk::LEFT, Action::Left),
    (vk::RIGHT, Action::Right),
    (vk::UP, Action::Up),
    (vk::DOWN, Action::Down),
    (vk::ENTER, Action::Accept),
    (vk::ESCAPE, Action::Menu),
    (vk::S, Action::Skip),
    (vk::Q, Action::Suicide),
];

/// Which action each original key stands for. Several keys may share an action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyMap {
    keys: BTreeMap<u8, Action>,
}

impl KeyMap {
    /// A map with no keys bound.
    pub fn empty() -> KeyMap {
        KeyMap {
            keys: BTreeMap::new(),
        }
    }

    /// The IWBTG engine default map (`IWBTG_DEFAULT_KEYS`).
    pub fn iwbtg() -> KeyMap {
        let mut map = KeyMap::empty();
        for (key, action) in IWBTG_DEFAULT_KEYS {
            map.bind(key, action);
        }
        map
    }

    /// Binds a virtual-key code to an action, replacing any earlier binding of that key.
    pub fn bind(&mut self, key: u8, action: Action) {
        self.keys.insert(key, action);
    }

    /// The action a key stands for, if any. A left- or right-hand Shift, Ctrl or Alt that is not
    /// bound itself takes the binding of the generic key (`vk:161` acts as `Shift`), as GM8 reads
    /// `vk_shift` from either hand.
    pub fn action(&self, key: u8) -> Option<Action> {
        self.keys
            .get(&key)
            .or_else(|| generic_modifier(key).and_then(|g| self.keys.get(&g)))
            .copied()
    }
}

impl Default for KeyMap {
    fn default() -> KeyMap {
        KeyMap::iwbtg()
    }
}

/// Whether a press of a key that is already down sets its `pressed` bit again (for example two
/// overlapping `hold` ranges of one key). OPEN: confirm in CLONE_SPEC how GM8 and OpenGMK's TAS
/// input treat a repeated press.
pub const PRESS_WHILE_HELD_SETS_PRESSED: bool = true;

/// Whether a release of a key that is already up sets its `released` bit. OPEN: confirm in
/// CLONE_SPEC how GM8 and OpenGMK's TAS input treat a release of a key that is up.
pub const RELEASE_WHILE_UP_SETS_RELEASED: bool = true;

/// Highest frame number an `.inputs` file may name. Our own limit, not a GM8 fact: it bounds
/// memory on bad input (about 11.6 hours at 50 ticks per second).
pub const MAX_INPUT_FRAME: u32 = (1 << 21) - 1;

/// A compiled `.inputs` file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedInputs {
    /// One frame per tick; the index is the frame number, from 0 through `end`.
    pub frames: Vec<InputFrame>,
    /// The last frame run (`frame N: end`).
    pub end: u32,
}

/// Why an `.inputs` file was rejected.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum InputsError {
    /// A line is malformed. `line` counts from 1.
    #[error("inputs line {line}: {kind}")]
    Line { line: u32, kind: LineError },
    /// No `frame N: end` line, so the run length is unknown.
    #[error("inputs: no 'frame N: end' line")]
    MissingEnd,
}

/// What is wrong with one `.inputs` line.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LineError {
    #[error("bad frame number '{0}'")]
    BadFrame(String),
    #[error("frame {0} is past the limit {MAX_INPUT_FRAME}")]
    FrameTooLarge(u32),
    #[error("frame range {0}-{1} goes backwards")]
    Backwards(u32, u32),
    #[error("missing press/release/hold/end")]
    MissingCommand,
    #[error("unknown command '{0}'")]
    UnknownCommand(String),
    #[error("'{0}' takes a single frame; use 'hold'")]
    SingleFrame(String),
    #[error("'end' takes a single frame and no names")]
    BadEnd,
    #[error("no keys or actions given")]
    NoNames,
    /// Not a recorder key name, in a key-named file (`parse_key_inputs`).
    #[error("unknown key '{0}' (this file takes key names, not action names)")]
    UnknownKey(String),
    /// Not an action name, in an action-named file (`parse_action_inputs`).
    #[error("unknown action '{0}' (this file takes action names, not key names)")]
    UnknownAction(String),
    #[error("key '{0}' has no action in the key map")]
    UnmappedKey(String),
}

/// Which names an `.inputs` file uses. The caller picks: the words `left`, `right`, `up` and
/// `down` are both key names and action names, and under a key map that binds, say, Up to `jump`
/// they mean different actions, so the parser never guesses.
#[derive(Clone, Copy, Debug)]
enum Names<'a> {
    /// Action names only (`jump`), as in our own test files.
    Actions,
    /// Recorder key names only (`Shift`, `vk:NN`), mapped to actions through the key map, as in
    /// the research trace files.
    Keys(&'a KeyMap),
}

/// What a name in a command refers to. Each source keeps its own down/up state, as each key does
/// in the recorder, so two keys bound to one action behave like two keys (see the module docs).
/// A file uses only one kind, so a key and an action name never share an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    /// A key as the recorder holds it (after `recorder_key`).
    Key(u8),
    Action(Action),
}

#[derive(Clone, Copy, Debug)]
struct Event {
    down: bool,
    source: Source,
    action: Action,
}

/// Parses an action-named `.inputs` file (`hold jump`) into one `InputFrame` per tick. Key names
/// are an error; use `parse_key_inputs` for recorder files.
///
/// Grammar (the research trace recorder's): one command per line, `#` starts a comment, the words
/// `frame`/`frames` and the colon are optional, frame numbers count from 0.
/// `frame N: press NAME...`, `frame N: release NAME...`, `frames A-B: hold NAME...` (down on A,
/// up on B+1), `frame N: end` (N is the last frame run; the last `end` line wins). Events on one
/// frame apply in line order.
pub fn parse_action_inputs(text: &str) -> Result<ParsedInputs, InputsError> {
    parse(text, Names::Actions)
}

/// Parses a key-named `.inputs` file, as the research trace recorder writes them (`hold Shift`),
/// mapping each key to its action through `keys`. Same grammar as `parse_action_inputs`; action
/// names are an error, so `left` is always the Left arrow key. Generic Shift, Ctrl and Alt are the
/// left-hand keys, as in the recorder (`recorder_key`).
pub fn parse_key_inputs(text: &str, keys: &KeyMap) -> Result<ParsedInputs, InputsError> {
    parse(text, Names::Keys(keys))
}

fn parse(text: &str, names: Names) -> Result<ParsedInputs, InputsError> {
    let mut events: BTreeMap<u32, Vec<Event>> = BTreeMap::new();
    let mut end = None;
    for (index, raw) in text.lines().enumerate() {
        let line = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let err = |kind| InputsError::Line { line, kind };
        let content = raw.split('#').next().unwrap_or("").trim();
        if content.is_empty() {
            continue;
        }
        let rest = strip_frame_keyword(content);
        let (frames, rest) = match rest.find(|c: char| c == ':' || c.is_whitespace()) {
            Some(i) => (
                &rest[..i],
                rest[i..].trim_start_matches(|c: char| c == ':' || c.is_whitespace()),
            ),
            None => (rest, ""),
        };
        let (a, b) = parse_frames(frames).map_err(err)?;
        let mut words = rest.split_whitespace();
        let command = words
            .next()
            .ok_or_else(|| err(LineError::MissingCommand))?
            .to_ascii_lowercase();
        if command == "end" {
            if a != b || words.next().is_some() {
                return Err(err(LineError::BadEnd));
            }
            end = Some(a);
            continue;
        }
        if !matches!(command.as_str(), "press" | "release" | "hold") {
            return Err(err(LineError::UnknownCommand(command)));
        }
        if command != "hold" && a != b {
            return Err(err(LineError::SingleFrame(command)));
        }
        let names = words
            .map(|w| resolve(w, names).map_err(err))
            .collect::<Result<Vec<_>, _>>()?;
        if names.is_empty() {
            return Err(err(LineError::NoNames));
        }
        let mut push = |frame: u32, down: bool| {
            let list = events.entry(frame).or_default();
            list.extend(
                names
                    .iter()
                    .map(|&(source, action)| Event { down, source, action }),
            );
        };
        match command.as_str() {
            "press" => push(a, true),
            "release" => push(a, false),
            _ => {
                push(a, true);
                // b <= MAX_INPUT_FRAME, so this cannot overflow.
                push(b + 1, false);
            }
        }
    }
    let end = end.ok_or(InputsError::MissingEnd)?;
    Ok(ParsedInputs {
        frames: compile(&events, end),
        end,
    })
}

/// Drops a leading `frames` or `frame`, ignoring ASCII case.
fn strip_frame_keyword(s: &str) -> &str {
    for kw in ["frames", "frame"] {
        if let Some(head) = s.get(..kw.len())
            && head.eq_ignore_ascii_case(kw)
        {
            return s[kw.len()..].trim_start();
        }
    }
    s
}

/// Parses `N` or `A-B`.
fn parse_frames(s: &str) -> Result<(u32, u32), LineError> {
    let one = |x: &str| {
        let x = x.trim();
        let n = x.parse::<u32>().map_err(|_| LineError::BadFrame(x.to_string()))?;
        if n > MAX_INPUT_FRAME {
            return Err(LineError::FrameTooLarge(n));
        }
        Ok(n)
    };
    match s.split_once('-') {
        Some((a, b)) => {
            let (a, b) = (one(a)?, one(b)?);
            if b < a {
                return Err(LineError::Backwards(a, b));
            }
            Ok((a, b))
        }
        None => {
            let a = one(s)?;
            Ok((a, a))
        }
    }
}

/// Resolves a name in the file's name space.
fn resolve(word: &str, names: Names) -> Result<(Source, Action), LineError> {
    match names {
        Names::Actions => match Action::from_name(word) {
            Some(action) => Ok((Source::Action(action), action)),
            None => Err(LineError::UnknownAction(word.to_string())),
        },
        Names::Keys(keys) => {
            let code = key_code(word).ok_or_else(|| LineError::UnknownKey(word.to_string()))?;
            let code = recorder_key(code);
            match keys.action(code) {
                Some(action) => Ok((Source::Key(code), action)),
                None => Err(LineError::UnmappedKey(word.to_string())),
            }
        }
    }
}

/// Replays the events in frame and line order into frames 0 through `end`. Events after `end`
/// never run, as in the recorder.
fn compile(events: &BTreeMap<u32, Vec<Event>>, end: u32) -> Vec<InputFrame> {
    let mut down: BTreeMap<Source, Action> = BTreeMap::new();
    let mut held = 0u16;
    let mut frames = Vec::new();
    for frame in 0..=end {
        let mut out = InputFrame {
            held,
            ..InputFrame::EMPTY
        };
        if let Some(list) = events.get(&frame) {
            for ev in list {
                let was_down = down.contains_key(&ev.source);
                if ev.down {
                    if PRESS_WHILE_HELD_SETS_PRESSED || !was_down {
                        out.pressed |= ev.action.bit();
                    }
                    down.insert(ev.source, ev.action);
                } else {
                    if RELEASE_WHILE_UP_SETS_RELEASED || was_down {
                        out.released |= ev.action.bit();
                    }
                    down.remove(&ev.source);
                }
            }
            held = down.values().fold(0, |bits, a| bits | a.bit());
            out.held = held;
        }
        frames.push(out);
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn parse(text: &str) -> ParsedInputs {
        parse_key_inputs(text, &KeyMap::iwbtg()).expect("valid inputs")
    }

    fn parse_actions(text: &str) -> ParsedInputs {
        parse_action_inputs(text).expect("valid inputs")
    }

    fn line_error(text: &str) -> (u32, LineError) {
        match parse_key_inputs(text, &KeyMap::iwbtg()) {
            Err(InputsError::Line { line, kind }) => (line, kind),
            other => panic!("expected a line error, got {other:?}"),
        }
    }

    const J: u16 = Action::Jump.bit();

    #[test]
    fn action_bits_are_distinct_and_named() {
        let mut all = 0u16;
        for a in Action::ALL {
            assert_eq!(a.bit().count_ones(), 1);
            assert_eq!(all & a.bit(), 0);
            all |= a.bit();
            assert_eq!(Action::from_name(a.name()), Some(a));
        }
        assert_eq!(all, Action::MASK);
        assert_eq!(Action::from_name("JUMP"), Some(Action::Jump));
        assert_eq!(Action::from_name("dash"), None);
    }

    #[test]
    fn from_held_derives_edges() {
        let l = Action::Left.bit();
        let f = InputFrame::from_held(J | l, J | Action::Right.bit());
        assert!(f.is_held(Action::Jump) && f.is_held(Action::Right));
        assert!(!f.is_held(Action::Left));
        assert!(f.was_pressed(Action::Right) && !f.was_pressed(Action::Jump));
        assert!(f.was_released(Action::Left) && !f.was_released(Action::Jump));
        assert_eq!(InputFrame::from_held(0, 0), InputFrame::EMPTY);
    }

    #[test]
    fn hold_range_presses_on_a_and_releases_after_b() {
        let p = parse("frames 2-4: hold Shift\nframe 6: end\n");
        assert_eq!(p.end, 6);
        assert_eq!(p.frames.len(), 7);
        let held: Vec<bool> = p.frames.iter().map(|f| f.is_held(Action::Jump)).collect();
        assert_eq!(held, [false, false, true, true, true, false, false]);
        assert_eq!(p.frames[2].pressed, J);
        assert_eq!(p.frames[5].released, J);
        let edges = p.frames.iter().filter(|f| f.pressed | f.released != 0).count();
        assert_eq!(edges, 2);
    }

    #[test]
    fn press_and_release_in_one_frame_sets_both_edges() {
        let p = parse("frame 3: press Shift\nframe 3: release Shift\nframe 5: end");
        assert_eq!(
            p.frames[3],
            InputFrame {
                held: 0,
                pressed: J,
                released: J
            }
        );
        assert_eq!(p.frames[4], InputFrame::EMPTY);

        // Release then press on one frame: both edges, and the key stays down.
        let p = parse("frames 1-2: hold Shift\nframe 3: press Shift\nframe 4: end");
        assert_eq!(
            p.frames[3],
            InputFrame {
                held: J,
                pressed: J,
                released: J
            }
        );
        assert!(p.frames[4].is_held(Action::Jump));
    }

    #[test]
    fn overlapping_holds_follow_recorder_events() {
        // Events: press 10, press 20, release 30, release 40. The first release lets the key up,
        // exactly as the recorder applies them.
        let p = parse("frames 10-29: hold Shift\nframes 20-39: hold Shift\nframe 45: end");
        assert!(p.frames[25].is_held(Action::Jump));
        assert_eq!(p.frames[20].pressed, J);
        assert_eq!(p.frames[30].released, J);
        assert!(!p.frames[30].is_held(Action::Jump));
        assert!(!p.frames[35].is_held(Action::Jump));
        assert_eq!(p.frames[40].released, J);
    }

    #[test]
    fn two_keys_for_one_action_give_per_key_edges() {
        // Decided semantics (module docs): each key sets the action's edges on its own, as GM8
        // games that check each key see them. The action stays held while either key is down.
        let mut map = KeyMap::iwbtg();
        map.bind(vk::SPACE, Action::Jump);
        let p = parse_key_inputs(
            "frames 1-5: hold Shift\nframes 3-7: hold Space\nframe 9: end",
            &map,
        )
        .expect("valid");
        assert!((1..=7).all(|f| p.frames[f].is_held(Action::Jump)));
        // Space goes down while Shift holds jump: a press edge.
        assert_eq!(
            p.frames[3],
            InputFrame {
                held: J,
                pressed: J,
                released: 0
            }
        );
        // Shift goes up while Space holds jump: a release edge, jump still held.
        assert_eq!(
            p.frames[6],
            InputFrame {
                held: J,
                pressed: 0,
                released: J
            }
        );
        assert_eq!(p.frames[8].released, J);
        assert!(!p.frames[8].is_held(Action::Jump));
    }

    #[test]
    fn modifier_keys_match_the_recorder() {
        // The recorder presses Left Shift for `Shift`, so `vk:160` releases it.
        let p = parse("frames 1-5: hold Shift\nframe 3: release vk:160\nframe 7: end");
        assert_eq!(p.frames[3].released, J);
        assert!(!p.frames[3].is_held(Action::Jump));
        assert_eq!(
            p,
            parse("frames 1-2: hold Shift\nframe 6: release Shift\nframe 7: end")
        );
        // Right Shift is jump too, with its own state and edges, as keyboard_check(vk_shift)
        // reads either hand.
        let p = parse("frames 1-5: hold Shift\nframes 3-7: hold vk:161\nframe 9: end");
        assert!((1..=7).all(|f| p.frames[f].is_held(Action::Jump)));
        assert_eq!(p.frames[3].pressed, J);
        assert_eq!(p.frames[6].released, J);
        assert!(!p.frames[8].is_held(Action::Jump));
        // Ctrl and Alt fold the same way.
        let mut map = KeyMap::empty();
        map.bind(vk::CTRL, Action::Shoot);
        map.bind(vk::ALT, Action::Restart);
        let p = parse_key_inputs(
            "frame 1: press Ctrl vk:165\nframe 2: release vk:162 Alt\nframe 3: end",
            &map,
        )
        .expect("valid");
        assert_eq!(p.frames[1].held, Action::Shoot.bit() | Action::Restart.bit());
        // `Alt` is Left Alt, not the Right Alt still down, so restart stays held; its release of
        // an up key still sets the edge (RELEASE_WHILE_UP_SETS_RELEASED).
        assert_eq!(p.frames[2].held, Action::Restart.bit());
        assert_eq!(p.frames[2].released, Action::Shoot.bit() | Action::Restart.bit());
        // A binding of the exact hand wins over the generic key.
        let mut map = KeyMap::iwbtg();
        map.bind(vk::RIGHT_SHIFT, Action::Shoot);
        assert_eq!(map.action(vk::RIGHT_SHIFT), Some(Action::Shoot));
        assert_eq!(map.action(vk::LEFT_SHIFT), Some(Action::Jump));
        assert_eq!(map.action(vk::RIGHT_CTRL), None);
    }

    #[test]
    fn key_and_action_names_are_separate_files() {
        // A key map that binds the Up arrow to jump: in a key file `up` is that key, in an
        // action file it is the `up` action. Neither file accepts the other kind of name.
        let mut map = KeyMap::empty();
        map.bind(vk::UP, Action::Jump);
        let keys = parse_key_inputs("frames 1-5: hold up\nframe 6: end", &map).expect("valid");
        assert!(keys.frames[1].is_held(Action::Jump));
        let actions = parse_actions("frames 1-5: hold up\nframe 6: end");
        assert!(actions.frames[1].is_held(Action::Up));
        assert!(!actions.frames[1].is_held(Action::Jump));
        assert_eq!(
            parse_key_inputs("frame 3: press jump\nframe 9: end", &KeyMap::iwbtg()),
            Err(InputsError::Line {
                line: 1,
                kind: LineError::UnknownKey("jump".into())
            })
        );
        assert_eq!(
            parse_action_inputs("frame 3: press jump\nframe 5: release Shift\nframe 9: end"),
            Err(InputsError::Line {
                line: 2,
                kind: LineError::UnknownAction("Shift".into())
            })
        );
    }

    #[test]
    fn grammar_variants_and_action_names() {
        let p = parse_actions(
            "# header\n\n10 press jump left\nFRAME 11 release JUMP # tap\nframes12-13:hold right\nframe 14: end",
        );
        assert_eq!(p.frames[10].pressed, J | Action::Left.bit());
        assert!(p.frames[11].was_released(Action::Jump));
        assert!(p.frames[11].is_held(Action::Left));
        assert!(p.frames[12].is_held(Action::Right));
        assert!(p.frames[14].was_released(Action::Right));
        // A key file and an action file for the same input compile to the same frames.
        assert_eq!(
            parse("frames 1-2: hold Shift\nframe 4: end"),
            parse_actions("frames 1-2: hold jump\nframe 4: end")
        );
        assert_eq!(
            parse("frame 1: press Left Shift\nframe 2: release Shift\nframe 3: end"),
            parse_actions("frame 1: press left jump\nframe 2: release jump\nframe 3: end")
        );
    }

    #[test]
    fn events_after_end_are_ignored_and_last_end_wins() {
        let p = parse("frame 3: end\nframes 2-9: hold Z\nframe 4: end");
        assert_eq!(p.end, 4);
        assert_eq!(p.frames.len(), 5);
        assert!(p.frames[4].is_held(Action::Shoot));
    }

    #[test]
    fn errors_carry_line_numbers() {
        assert_eq!(
            line_error("# c\nframe x: press Shift"),
            (2, LineError::BadFrame("x".into()))
        );
        assert_eq!(
            line_error("\n\nframes 5-3: hold Shift"),
            (3, LineError::Backwards(5, 3))
        );
        assert_eq!(line_error("frame 1:"), (1, LineError::MissingCommand));
        assert_eq!(
            line_error("frame 1: tap Shift"),
            (1, LineError::UnknownCommand("tap".into()))
        );
        assert_eq!(
            line_error("frames 1-2: press Shift"),
            (1, LineError::SingleFrame("press".into()))
        );
        assert_eq!(line_error("frame 1: press"), (1, LineError::NoNames));
        assert_eq!(
            line_error("frame 1: press Banana"),
            (1, LineError::UnknownKey("Banana".into()))
        );
        assert_eq!(
            parse_action_inputs("frame 1: end\nframe 1: press Banana"),
            Err(InputsError::Line {
                line: 2,
                kind: LineError::UnknownAction("Banana".into())
            })
        );
        assert_eq!(
            line_error("frame 1: press F2"),
            (1, LineError::UnmappedKey("F2".into()))
        );
        assert_eq!(line_error("frames 1-2: end"), (1, LineError::BadEnd));
        assert_eq!(line_error("frame 1: end Shift"), (1, LineError::BadEnd));
        assert_eq!(
            line_error("frame 99999999: end"),
            (1, LineError::FrameTooLarge(99_999_999))
        );
        assert_eq!(
            line_error("frame 99999999999: end"),
            (1, LineError::BadFrame("99999999999".into()))
        );
        assert_eq!(
            parse_key_inputs("frame 1: press Shift", &KeyMap::iwbtg()),
            Err(InputsError::MissingEnd)
        );
        // Non-ASCII text is an error, not a panic.
        assert!(parse_key_inputs("främe 1: end", &KeyMap::iwbtg()).is_err());
        assert!(parse_key_inputs("é", &KeyMap::iwbtg()).is_err());
        assert!(parse_action_inputs("frame 1: press é\nframe 2: end").is_err());
    }

    #[test]
    fn key_names_match_the_recorder() {
        assert_eq!(key_code("shift"), Some(16));
        assert_eq!(key_code("ESC"), Some(27));
        assert_eq!(key_code("f12"), Some(123));
        assert_eq!(key_code("F13"), None);
        assert_eq!(key_code("z"), Some(b'Z'));
        assert_eq!(key_code("7"), Some(b'7'));
        assert_eq!(key_code("vk:160"), Some(160));
        assert_eq!(key_code("vk:300"), None);
        assert_eq!(key_code("zz"), None);
    }

    /// Reads a research trace input file. `None` when IWBTG_RESEARCH is unset, so the test skips.
    fn research_inputs(game: &str, case: &str) -> Option<String> {
        // Test-only: the sim itself reads no environment or files.
        let root = std::env::var_os("IWBTG_RESEARCH")?;
        let path = std::path::Path::new(&root)
            .join("specs/engines/gm8/traces")
            .join(game)
            .join(format!("{case}.inputs"));
        Some(std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
    }

    #[test]
    fn real_djump_inputs_of_rely_the_needle() {
        let Some(text) = research_inputs("rely-the-needle-4-2", "djump") else {
            eprintln!("skipped: IWBTG_RESEARCH is not set");
            return;
        };
        let p = parse(&text);
        assert_eq!(p.end, 109);
        assert_eq!(p.frames.len(), 110);
        let held: BTreeSet<usize> = (0..p.frames.len())
            .filter(|&f| p.frames[f].is_held(Action::Jump))
            .collect();
        let expected: BTreeSet<usize> = (10..=29).chain(32..=41).collect();
        assert_eq!(held, expected);
        let pressed: Vec<usize> = (0..p.frames.len())
            .filter(|&f| p.frames[f].was_pressed(Action::Jump))
            .collect();
        let released: Vec<usize> = (0..p.frames.len())
            .filter(|&f| p.frames[f].was_released(Action::Jump))
            .collect();
        assert_eq!(pressed, [10, 32]);
        assert_eq!(released, [30, 42]);
        assert!(p.frames.iter().all(|f| f.held & !J == 0 && f.pressed & !J == 0));
    }
}
