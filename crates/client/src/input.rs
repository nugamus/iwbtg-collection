//! Turns key presses into one `InputFrame` per tick. The collector has no Bevy logic, so it is tested
//! like any other code (docs/architecture/replay.md, Actions and input frames).

use iwbtg_sim::input::{Action, InputFrame, vk};

use crate::bindings::{Bindings, ClientKey, PadInput, oem};

/// Collects key and controller events between two ticks. Edges are per input, as the sim's input
/// docs require: a press and a release inside one tick both show up, and a second input bound to
/// the same action still sets its own edge while the action stays held.
#[derive(bevy::prelude::Resource)]
pub struct Collector {
    bindings: Bindings,
    /// Keys that are down, by virtual-key code.
    down: [bool; 256],
    /// Controller inputs that are down.
    pad_down: [bool; PadInput::ALL.len()],
    pressed: u32,
    released: u32,
    /// The frame the last tick took, for the overlay.
    last: InputFrame,
    /// A quick-retry input went down since `take_quick_retry`. A client key: never part of an
    /// `InputFrame`.
    quick_retry: bool,
    /// A skip-cutscene input went down since `take_skip` (skip.rs). A client key as well.
    skip: bool,
    /// Frame-advance presses since `take_advance` (practice.rs). A client key as well.
    advance: u32,
}

impl Collector {
    /// A collector with the given bindings.
    pub fn new(bindings: Bindings) -> Collector {
        Collector {
            bindings,
            down: [false; 256],
            pad_down: [false; PadInput::ALL.len()],
            pressed: 0,
            released: 0,
            last: InputFrame::EMPTY,
            quick_retry: false,
            skip: false,
            advance: 0,
        }
    }

    /// Whether a quick-retry key or controller input went down since the last call.
    pub fn take_quick_retry(&mut self) -> bool {
        std::mem::take(&mut self.quick_retry)
    }

    /// Whether a skip-cutscene key or controller input went down since the last call.
    pub fn take_skip(&mut self) -> bool {
        std::mem::take(&mut self.skip)
    }

    /// How many frame-advance keys or controller inputs went down since the last call.
    pub fn take_advance(&mut self) -> u32 {
        std::mem::take(&mut self.advance)
    }

    /// Notes a client key's input going down.
    fn client_press(&mut self, key: Option<ClientKey>) {
        match key {
            Some(ClientKey::QuickRetry) => self.quick_retry = true,
            Some(ClientKey::SkipCutscene) => self.skip = true,
            Some(ClientKey::FrameAdvance) => self.advance = self.advance.saturating_add(1),
            None => {}
        }
    }

    /// What the last tick saw, for the overlay: the actions held or pressed, then the controller
    /// inputs that are down.
    pub fn describe(&self) -> String {
        let actions: Vec<&str> = Action::ALL
            .into_iter()
            .filter(|a| self.last.is_held(*a) || self.last.was_pressed(*a))
            .map(Action::name)
            .collect();
        let pads: Vec<&str> = PadInput::ALL
            .into_iter()
            .filter(|p| self.pad_down[*p as usize])
            .map(PadInput::name)
            .collect();
        format!("input: {}  pad: {}", actions.join(" "), pads.join(" "))
    }

    /// The frame the last tick took (the HUD's input display).
    pub fn last(&self) -> InputFrame {
        self.last
    }

    /// The player's bindings.
    pub fn bindings(&self) -> &Bindings {
        &self.bindings
    }

    /// A key went down.
    pub fn press(&mut self, key: u8) {
        self.down[usize::from(key)] = true;
        self.client_press(self.bindings.client_key(key));
        if let Some(action) = self.bindings.key_action(key) {
            self.pressed |= action.bit();
        }
    }

    /// A press of `action` for the next frame, as if one of its keys went down and up again
    /// before the tick (the pause menu's Retry from save presses Restart).
    pub fn press_action(&mut self, action: Action) {
        self.pressed |= action.bit();
    }

    /// A key went up.
    pub fn release(&mut self, key: u8) {
        self.down[usize::from(key)] = false;
        if let Some(action) = self.bindings.key_action(key) {
            self.released |= action.bit();
        }
    }

    /// The current state of a controller input; a change is a press or a release.
    pub fn set_pad(&mut self, input: PadInput, down: bool) {
        let slot = &mut self.pad_down[input as usize];
        if *slot == down {
            return;
        }
        *slot = down;
        if down {
            self.client_press(self.bindings.client_pad(input));
        }
        if let Some(action) = self.bindings.pad_action(input) {
            if down {
                self.pressed |= action.bit();
            } else {
                self.released |= action.bit();
            }
        }
    }

    /// The frame for the next tick: the actions held now (any of their inputs down) and the edges
    /// since the last frame. Clears the edges.
    pub fn take_frame(&mut self) -> InputFrame {
        let mut held = 0;
        for key in 0..=255u8 {
            if self.down[usize::from(key)]
                && let Some(action) = self.bindings.key_action(key)
            {
                held |= action.bit();
            }
        }
        for input in PadInput::ALL {
            if self.pad_down[input as usize]
                && let Some(action) = self.bindings.pad_action(input)
            {
                held |= action.bit();
            }
        }
        let frame = InputFrame {
            held,
            pressed: self.pressed,
            released: self.released,
        };
        self.pressed = 0;
        self.released = 0;
        self.last = frame;
        frame
    }
}

/// Turns one stick axis into a press. A pad that rests away from the centre (an axis the OS maps
/// wrongly, or a motion-sensor device that shows up as a pad) must never count as held: the axis
/// is ignored until it has been seen near the centre, and a press needs a clear push, with a
/// lower bar to let go.
#[derive(Default)]
pub struct StickGate {
    centred: bool,
    down: bool,
}

impl StickGate {
    /// Pushed past this counts as a press.
    pub const PRESS: f32 = 0.6;
    /// Back inside this counts as a release (and as seen at the centre).
    pub const RELEASE: f32 = 0.3;

    /// The next reading of the axis in the direction this gate watches (negate for the other way);
    /// returns whether the direction is down.
    pub fn update(&mut self, value: f32) -> bool {
        if value < Self::RELEASE {
            self.centred = true;
            self.down = false;
        } else if self.centred && value > Self::PRESS {
            self.down = true;
        }
        self.down
    }
}

/// Reads the controllers as `PadInput`s. Only the active controller counts: the one that last had
/// a button pressed. A DualSense over Bluetooth can show up as a second device (motion sensors)
/// whose axes never rest at zero, and that must not walk the kid right. The stick goes through
/// `StickGate`, so an axis that never rests at the centre is ignored.
///
/// Native and web share this: on both, Bevy's gilrs backend fills the same `Gamepad` components
/// (on wasm32 from the browser's Gamepad API, docs/architecture/client/input.md).
#[derive(Default)]
pub struct PadReader {
    active: Option<bevy::prelude::Entity>,
    gates: [StickGate; 4],
}

impl PadReader {
    /// Which inputs are down this frame, indexed by `PadInput as usize`. All up when no controller
    /// has had a button pressed yet, or the active one went away.
    pub fn read(
        &mut self,
        pads: &[(bevy::prelude::Entity, &bevy::input::gamepad::Gamepad)],
    ) -> [bool; PadInput::ALL.len()] {
        use bevy::input::gamepad::{GamepadAxis as A, GamepadButton as B};
        let mut down = [false; PadInput::ALL.len()];
        if let Some((entity, _)) = pads.iter().find(|(_, g)| g.get_pressed().next().is_some())
            && self.active != Some(*entity)
        {
            self.active = Some(*entity);
            self.gates = Default::default();
        }
        let Some(g) = self
            .active
            .and_then(|e| pads.iter().find(|(p, _)| *p == e))
            .map(|(_, g)| g)
        else {
            return down;
        };
        let axis = |a| g.get(a).unwrap_or(0.0);
        // Bevy's stick Y points up on every backend (gilrs flips the browser's down-positive Y).
        let stick = [
            self.gates[0].update(axis(A::LeftStickY)),
            self.gates[1].update(-axis(A::LeftStickY)),
            self.gates[2].update(-axis(A::LeftStickX)),
            self.gates[3].update(axis(A::LeftStickX)),
        ];
        for input in PadInput::ALL {
            let button = |b| g.pressed(b);
            down[input as usize] = match input {
                PadInput::South => button(B::South),
                PadInput::East => button(B::East),
                PadInput::West => button(B::West),
                PadInput::North => button(B::North),
                PadInput::LeftBumper => button(B::LeftTrigger),
                PadInput::RightBumper => button(B::RightTrigger),
                PadInput::LeftTrigger => button(B::LeftTrigger2),
                PadInput::RightTrigger => button(B::RightTrigger2),
                PadInput::Select => button(B::Select),
                PadInput::Start => button(B::Start),
                PadInput::DPadUp => button(B::DPadUp),
                PadInput::DPadDown => button(B::DPadDown),
                PadInput::DPadLeft => button(B::DPadLeft),
                PadInput::DPadRight => button(B::DPadRight),
                PadInput::StickUp => stick[0],
                PadInput::StickDown => stick[1],
                PadInput::StickLeft => stick[2],
                PadInput::StickRight => stick[3],
            };
        }
        down
    }
}

/// The virtual-key code of a Bevy key. Keys with no code (media keys, IME keys) are ignored.
pub fn key_code(key: bevy::input::keyboard::KeyCode) -> Option<u8> {
    use bevy::input::keyboard::KeyCode as K;
    const LETTERS: [K; 26] = [
        K::KeyA,
        K::KeyB,
        K::KeyC,
        K::KeyD,
        K::KeyE,
        K::KeyF,
        K::KeyG,
        K::KeyH,
        K::KeyI,
        K::KeyJ,
        K::KeyK,
        K::KeyL,
        K::KeyM,
        K::KeyN,
        K::KeyO,
        K::KeyP,
        K::KeyQ,
        K::KeyR,
        K::KeyS,
        K::KeyT,
        K::KeyU,
        K::KeyV,
        K::KeyW,
        K::KeyX,
        K::KeyY,
        K::KeyZ,
    ];
    const DIGITS: [K; 10] = [
        K::Digit0,
        K::Digit1,
        K::Digit2,
        K::Digit3,
        K::Digit4,
        K::Digit5,
        K::Digit6,
        K::Digit7,
        K::Digit8,
        K::Digit9,
    ];
    const FUNCTION: [K; 12] = [
        K::F1,
        K::F2,
        K::F3,
        K::F4,
        K::F5,
        K::F6,
        K::F7,
        K::F8,
        K::F9,
        K::F10,
        K::F11,
        K::F12,
    ];
    if let Some(i) = LETTERS.iter().position(|k| *k == key) {
        return Some(b'A' + i as u8);
    }
    if let Some(i) = DIGITS.iter().position(|k| *k == key) {
        return Some(b'0' + i as u8);
    }
    if let Some(i) = FUNCTION.iter().position(|k| *k == key) {
        return Some(vk::F1 + i as u8);
    }
    Some(match key {
        K::ShiftLeft => vk::LEFT_SHIFT,
        K::ShiftRight => vk::RIGHT_SHIFT,
        K::ControlLeft => vk::LEFT_CTRL,
        K::ControlRight => vk::RIGHT_CTRL,
        K::AltLeft => vk::LEFT_ALT,
        K::AltRight => vk::RIGHT_ALT,
        K::ArrowLeft => vk::LEFT,
        K::ArrowRight => vk::RIGHT,
        K::ArrowUp => vk::UP,
        K::ArrowDown => vk::DOWN,
        K::Enter => vk::ENTER,
        K::Escape => vk::ESCAPE,
        K::Space => vk::SPACE,
        K::Tab => vk::TAB,
        K::Backspace => vk::BACKSPACE,
        K::PageUp => vk::PAGE_UP,
        K::PageDown => vk::PAGE_DOWN,
        K::Home => vk::HOME,
        K::End => vk::END,
        K::Insert => vk::INSERT,
        K::Delete => vk::DELETE,
        K::Semicolon => oem::SEMICOLON,
        K::Equal => oem::EQUAL,
        K::Comma => oem::COMMA,
        K::Minus => oem::MINUS,
        K::Period => oem::PERIOD,
        K::Slash => oem::SLASH,
        K::Backquote => oem::BACKQUOTE,
        K::BracketLeft => oem::BRACKET_LEFT,
        K::Backslash => oem::BACKSLASH,
        K::BracketRight => oem::BRACKET_RIGHT,
        K::Quote => oem::QUOTE,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use iwbtg_sim::input::Action;

    #[test]
    fn a_stick_must_be_seen_at_the_centre_before_it_counts() {
        let mut g = StickGate::default();
        // An axis stuck at full deflection never presses.
        assert!((0..10).all(|_| !g.update(1.0)));
        assert!(!g.update(0.0));
        assert!(g.update(0.9));
        // Hysteresis: a wobble between the bars keeps the state.
        assert!(g.update(0.4));
        assert!(!g.update(0.2));
        assert!(!g.update(0.4));
        // A small drift never presses.
        assert!(!g.update(0.5));
    }

    use bevy::input::gamepad::{Gamepad, GamepadAxis as A, GamepadButton as B};
    use bevy::prelude::Entity;

    fn pad_down(down: &[bool; PadInput::ALL.len()]) -> Vec<&'static str> {
        PadInput::ALL
            .into_iter()
            .filter(|p| down[*p as usize])
            .map(PadInput::name)
            .collect()
    }

    /// The default controller bindings through the shared reader, as both native and the web build
    /// see a pad: the D-pad and the stick move, south jumps, west shoots, north restarts.
    #[test]
    fn a_pad_drives_move_jump_shoot_and_restart() {
        let mut reader = PadReader::default();
        let mut c = Collector::new(Bindings::default());
        let mut g = Gamepad::default();
        let e = Entity::from_raw_u32(1).unwrap();
        g.digital_mut().press(B::South);
        g.digital_mut().press(B::West);
        g.digital_mut().press(B::DPadLeft);
        let down = reader.read(&[(e, &g)]);
        assert_eq!(pad_down(&down), ["south", "west", "dpad-left"]);
        for input in PadInput::ALL {
            c.set_pad(input, down[input as usize]);
        }
        let f = c.take_frame();
        assert!(f.was_pressed(Action::Jump) && f.was_pressed(Action::Shoot) && f.is_held(Action::Left));

        let mut g = Gamepad::default();
        g.digital_mut().press(B::North);
        g.analog_mut().set(A::LeftStickX, 1.0);
        let down = reader.read(&[(e, &g)]);
        for input in PadInput::ALL {
            c.set_pad(input, down[input as usize]);
        }
        let f = c.take_frame();
        assert!(f.was_pressed(Action::Restart) && f.was_released(Action::Jump));
        assert!(
            f.is_held(Action::Right),
            "the stick rested at the centre before, so it counts"
        );
    }

    /// Only the pad that last had a button pressed counts, and a stick resting off centre never
    /// presses (a DualSense's motion-sensor device).
    #[test]
    fn only_the_active_pad_counts_and_a_stuck_axis_never_presses() {
        let mut reader = PadReader::default();
        let (a, b) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        let mut sensors = Gamepad::default();
        sensors.analog_mut().set(A::LeftStickX, 1.0);
        let idle = Gamepad::default();
        // No button pressed yet: nothing is active.
        assert!(pad_down(&reader.read(&[(a, &sensors), (b, &idle)])).is_empty());
        let mut pressed = Gamepad::default();
        pressed.digital_mut().press(B::South);
        assert_eq!(pad_down(&reader.read(&[(a, &sensors), (b, &pressed)])), ["south"]);
        // A button on the sensor device makes it active, but its stuck stick still does nothing.
        let mut sensors_button = Gamepad::default();
        sensors_button.analog_mut().set(A::LeftStickX, 1.0);
        sensors_button.digital_mut().press(B::Start);
        assert_eq!(
            pad_down(&reader.read(&[(a, &sensors_button), (b, &idle)])),
            ["start"]
        );
        // The active pad unplugged: all up.
        assert!(pad_down(&reader.read(&[(b, &idle)])).is_empty());
    }

    /// Bevy's stick Y points up on every backend, the browser's included.
    #[test]
    fn stick_up_is_positive_y() {
        let mut reader = PadReader::default();
        let e = Entity::from_raw_u32(1).unwrap();
        let mut g = Gamepad::default();
        g.digital_mut().press(B::South);
        reader.read(&[(e, &g)]);
        g.analog_mut().set(A::LeftStickY, 0.9);
        assert_eq!(pad_down(&reader.read(&[(e, &g)])), ["south", "stick-up"]);
        g.analog_mut().set(A::LeftStickY, -0.9);
        assert_eq!(pad_down(&reader.read(&[(e, &g)])), ["south", "stick-down"]);
    }

    #[test]
    fn the_overlay_text_names_what_the_last_tick_saw() {
        let mut c = Collector::new(Bindings::default());
        c.press(vk::Z);
        c.set_pad(PadInput::South, true);
        c.take_frame();
        assert_eq!(c.describe(), "input: jump shoot  pad: south");
    }

    #[test]
    fn a_held_key_is_held_and_pressed_once() {
        let mut c = Collector::new(Bindings::default());
        c.press(vk::LEFT);
        let f = c.take_frame();
        assert!(f.is_held(Action::Left) && f.was_pressed(Action::Left));
        let f = c.take_frame();
        assert!(f.is_held(Action::Left) && !f.was_pressed(Action::Left));
        c.release(vk::LEFT);
        let f = c.take_frame();
        assert!(!f.is_held(Action::Left) && f.was_released(Action::Left));
    }

    #[test]
    fn a_tap_inside_one_tick_keeps_both_edges() {
        let mut c = Collector::new(Bindings::default());
        c.press(vk::Z);
        c.release(vk::Z);
        let f = c.take_frame();
        assert!(!f.is_held(Action::Shoot));
        assert!(f.was_pressed(Action::Shoot) && f.was_released(Action::Shoot));
    }

    #[test]
    fn either_shift_jumps_and_the_action_stays_held_with_one_of_them() {
        let mut c = Collector::new(Bindings::default());
        c.press(vk::LEFT_SHIFT);
        c.press(vk::RIGHT_SHIFT);
        c.take_frame();
        c.release(vk::LEFT_SHIFT);
        let f = c.take_frame();
        assert!(f.is_held(Action::Jump) && f.was_released(Action::Jump));
    }

    /// The quick-retry key (and a bound controller input) asks for a retry but leaves the frame
    /// empty: it is a client key and never reaches the sim or a replay.
    #[test]
    fn the_quick_retry_key_never_reaches_an_input_frame() {
        let mut config = crate::bindings::Config::default();
        config.set("quick-retry=pad:select").unwrap();
        let mut c = Collector::new(Bindings::from_config(&config).unwrap());
        assert!(!c.take_quick_retry());
        c.press(oem::BACKQUOTE);
        c.set_pad(PadInput::Select, true);
        assert_eq!(c.take_frame(), InputFrame::EMPTY);
        c.release(oem::BACKQUOTE);
        c.set_pad(PadInput::Select, false);
        assert_eq!(c.take_frame(), InputFrame::EMPTY);
        assert!(c.take_quick_retry());
        assert!(!c.take_quick_retry(), "one request per press");
        // A pad press alone counts too, and the sim's keys do not.
        c.set_pad(PadInput::Select, true);
        assert!(c.take_quick_retry());
        c.press(vk::Z);
        assert!(!c.take_quick_retry());
    }

    /// The frame-advance key counts its presses and, like quick-retry, never reaches a frame.
    #[test]
    fn the_frame_advance_key_counts_presses_and_never_reaches_an_input_frame() {
        let mut c = Collector::new(Bindings::default());
        assert_eq!(c.take_advance(), 0);
        c.press(b'F');
        c.release(b'F');
        c.press(b'F');
        assert_eq!(c.take_frame(), InputFrame::EMPTY);
        assert_eq!(c.take_advance(), 2, "one step per press");
        assert_eq!(c.take_advance(), 0);
        assert!(!c.take_skip() && !c.take_quick_retry());
    }

    /// The skip-cutscene key asks for a skip and, like quick-retry, never reaches a frame.
    #[test]
    fn the_skip_cutscene_key_never_reaches_an_input_frame() {
        let mut c = Collector::new(Bindings::default());
        assert!(!c.take_skip());
        c.press(vk::BACKSPACE);
        assert_eq!(c.take_frame(), InputFrame::EMPTY);
        assert!(c.take_skip());
        assert!(!c.take_skip(), "one request per press");
        assert!(!c.take_quick_retry(), "not a quick-retry");
        assert_eq!(
            key_code(bevy::input::keyboard::KeyCode::Backspace),
            Some(vk::BACKSPACE)
        );
    }

    #[test]
    fn the_quick_retry_key_has_a_virtual_key_code() {
        use bevy::input::keyboard::KeyCode as K;
        assert_eq!(key_code(K::Backquote), Some(oem::BACKQUOTE));
    }

    #[test]
    fn unbound_keys_do_nothing() {
        let mut c = Collector::new(Bindings::default());
        c.press(b'X');
        assert_eq!(c.take_frame(), InputFrame::EMPTY);
    }
}
