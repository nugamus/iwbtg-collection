//! Turns key presses into one `InputFrame` per tick. The collector has no Bevy logic, so it is tested
//! like any other code (docs/architecture/replay.md, Actions and input frames).

use iwbtg_sim::input::{Action, InputFrame, vk};

use crate::bindings::{Bindings, PadInput};

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
        if let Some(action) = self.bindings.key_action(key) {
            self.pressed |= action.bit();
        }
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

    #[test]
    fn unbound_keys_do_nothing() {
        let mut c = Collector::new(Bindings::default());
        c.press(b'X');
        assert_eq!(c.take_frame(), InputFrame::EMPTY);
    }
}
