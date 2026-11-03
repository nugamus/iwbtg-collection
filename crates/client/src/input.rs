//! Turns key presses into one `InputFrame` per tick. The collector has no Bevy logic, so it is tested
//! like any other code (docs/architecture.md, Actions and input frames).

use iwbtg_sim::input::{Action, InputFrame, KeyMap, vk};

/// Collects key events between two ticks. Edges are per key, as the sim's input docs require: a
/// press and a release inside one tick both show up, and a second key bound to the same action
/// still sets its own edge while the action stays held.
#[derive(bevy::prelude::Resource)]
pub struct Collector {
    map: KeyMap,
    /// Keys that are down, by virtual-key code.
    down: [bool; 256],
    pressed: u32,
    released: u32,
}

impl Collector {
    /// A collector with the IWBTG default keys.
    pub fn new() -> Collector {
        Collector {
            map: KeyMap::iwbtg(),
            down: [false; 256],
            pressed: 0,
            released: 0,
        }
    }

    /// A key went down.
    pub fn press(&mut self, key: u8) {
        self.down[usize::from(key)] = true;
        if let Some(action) = self.map.action(key) {
            self.pressed |= action.bit();
        }
    }

    /// A key went up.
    pub fn release(&mut self, key: u8) {
        self.down[usize::from(key)] = false;
        if let Some(action) = self.map.action(key) {
            self.released |= action.bit();
        }
    }

    /// The frame for the next tick: the actions held now (any of their keys down) and the edges
    /// since the last frame. Clears the edges.
    pub fn take_frame(&mut self) -> InputFrame {
        let mut held = 0;
        for key in 0..=255u8 {
            if self.down[usize::from(key)]
                && let Some(action) = self.map.action(key)
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
        frame
    }
}

/// The virtual-key code of a Bevy key, for the keys the default map knows.
pub fn key_code(key: bevy::input::keyboard::KeyCode) -> Option<u8> {
    use bevy::input::keyboard::KeyCode as K;
    Some(match key {
        K::ShiftLeft => vk::LEFT_SHIFT,
        K::ShiftRight => vk::RIGHT_SHIFT,
        K::ControlLeft => vk::LEFT_CTRL,
        K::ControlRight => vk::RIGHT_CTRL,
        K::ArrowLeft => vk::LEFT,
        K::ArrowRight => vk::RIGHT,
        K::ArrowUp => vk::UP,
        K::ArrowDown => vk::DOWN,
        K::Enter => vk::ENTER,
        K::Escape => vk::ESCAPE,
        K::F2 => vk::F2,
        K::KeyZ => vk::Z,
        K::KeyR => vk::R,
        K::KeyS => vk::S,
        K::KeyQ => vk::Q,
        K::KeyP => vk::P,
        K::KeyM => vk::M,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_key_is_held_and_pressed_once() {
        let mut c = Collector::new();
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
        let mut c = Collector::new();
        c.press(vk::Z);
        c.release(vk::Z);
        let f = c.take_frame();
        assert!(!f.is_held(Action::Shoot));
        assert!(f.was_pressed(Action::Shoot) && f.was_released(Action::Shoot));
    }

    #[test]
    fn either_shift_jumps_and_the_action_stays_held_with_one_of_them() {
        let mut c = Collector::new();
        c.press(vk::LEFT_SHIFT);
        c.press(vk::RIGHT_SHIFT);
        c.take_frame();
        c.release(vk::LEFT_SHIFT);
        let f = c.take_frame();
        assert!(f.is_held(Action::Jump) && f.was_released(Action::Jump));
    }

    #[test]
    fn unbound_keys_do_nothing() {
        let mut c = Collector::new();
        c.press(b'X');
        assert_eq!(c.take_frame(), InputFrame::EMPTY);
    }
}
