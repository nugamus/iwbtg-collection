//! What the sim reports for the player to read: the sign text box (`BuiltinOutput::TextBox`) and
//! the `time_limit` countdown (read from the timer's built-in state).

use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::TextBounds;
use iwbtg_sim::builtins::{BuiltinOutput, BuiltinState};
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::world::KindState;

use super::order::Hud;
use super::{Game, RoomSize};

/// How long the sign box stays up after the last tick that raised it, in sim ticks. The original's
/// message box is a GM8 `show_message`, which is modal: it stays until the player dismisses it.
/// The research spec says the FT sign shows a box but not how it closes or how long it stays, and
/// freezing the sim to wait for a click is not something this client does (#459). The box
/// therefore stays for a declared client-side duration: 150 ticks is 3 s at the 50 Hz the IWBTG
/// games tick at, long enough to read a line or two and short enough to clear itself as the player
/// moves on. Counting sim ticks rather than wall time keeps a paused game's box up and makes two
/// screenshot builds draw the same frame.
const SIGN_BOX_TICKS: u32 = 150;

/// What the sign box shows. A tick that raises `BuiltinOutput::TextBox` starts (or restarts) the
/// latch, so an `on_up_press` sign stays readable after its single Up tick; a tick without one
/// counts the latch down. A `while_overlapping` sign raises on every tick it is touched, so it
/// keeps the box up while the kid stands on it and lets it fade out after.
#[derive(Resource, Default)]
pub struct SignText {
    /// The text the box shows, if any.
    text: Option<String>,
    /// Sim ticks left before the box hides.
    left: u32,
}

impl SignText {
    /// Shows `text` and restarts the latch.
    pub(super) fn raise(&mut self, text: String) {
        self.text = Some(text);
        self.left = SIGN_BOX_TICKS;
    }

    /// Hides the box at once (a retry).
    pub(super) fn clear(&mut self) {
        self.text = None;
        self.left = 0;
    }

    /// The text to draw, if the box is up.
    fn text(&self) -> Option<&String> {
        self.text.as_ref()
    }

    /// One sim tick without a raise: counts the latch down, hiding the box when it ends.
    fn tick(&mut self) {
        self.left = self.left.saturating_sub(1);
        if self.left == 0 {
            self.text = None;
        }
    }
}

#[derive(Component)]
pub struct SignBox;

#[derive(Component)]
pub struct SignLabel;

#[derive(Component)]
pub struct Countdown;

/// The message box is a full-width bottom overlay. Its height, margin and text size keep its top
/// edge below the Kid at the showcase signs' row (y 512, the Kid's 32 px frame ends at 521): the
/// white frame's top is at 525 on a 608 px room. The box may still cover world art at the very
/// bottom (the sign itself, the save stone); it never draws under a sprite (z 996, `Hud::Sign`).
const BOX_HEIGHT: f32 = 72.0;
const MARGIN: f32 = 8.0;
const BORDER: f32 = 3.0;
/// Horizontal padding from the inner panel's edge to the text.
const PAD_X: f32 = 12.0;
/// Vertical padding above and below the text.
const PAD_Y: f32 = 6.0;
/// The sign text's size in room pixels. The long showcase message (252 characters) wraps to
/// three lines at the box's text width, which fit in `BOX_HEIGHT - 2 * PAD_Y`.
const TEXT_SIZE: f32 = 16.0;

/// Takes the sign text a tick raised, if any, into the latch ([`SignText`]); a tick without one
/// counts the latch down.
pub fn note_tick(sign: &mut SignText, output: &[OutputEvent]) {
    match output.iter().rev().find_map(|e| match e {
        OutputEvent::Builtin(BuiltinOutput::TextBox { text }) => Some(text),
        _ => None,
    }) {
        Some(text) => sign.raise(text.clone()),
        None => sign.tick(),
    }
}

/// `H:MM:SS`. The seconds read below 0 for one Step before the timer borrows; show 0.
pub fn format_time(hours: i64, minutes: i64, seconds: i64) -> String {
    format!("{}:{:02}:{:02}", hours.max(0), minutes.max(0), seconds.max(0))
}

pub fn setup(mut commands: Commands, room: Res<RoomSize>) {
    let width = room.0.x - 2.0 * MARGIN;
    let y = -room.0.y / 2.0 + MARGIN + BOX_HEIGHT / 2.0;
    // White frame, black panel on top, white text: the plain IWBTG message box.
    commands.spawn((
        Sprite::from_color(
            Color::WHITE,
            Vec2::new(width + 2.0 * BORDER, BOX_HEIGHT + 2.0 * BORDER),
        ),
        Transform::from_xyz(0.0, y, Hud::Sign.z()),
        Visibility::Hidden,
        SignBox,
        crate::shell::RUN_ONLY,
        children![
            (
                Sprite::from_color(Color::BLACK, Vec2::new(width, BOX_HEIGHT)),
                Transform::from_xyz(0.0, 0.0, 1.0),
            ),
            (
                Text2d::new(""),
                TextFont {
                    font_size: bevy::text::FontSize::Px(TEXT_SIZE),
                    ..default()
                },
                TextColor(Color::WHITE),
                TextBounds::new(width - 2.0 * PAD_X, BOX_HEIGHT - 2.0 * PAD_Y),
                Anchor::TOP_LEFT,
                Transform::from_xyz(-width / 2.0 + PAD_X, BOX_HEIGHT / 2.0 - PAD_Y, 2.0),
                SignLabel,
            ),
        ],
    ));
    commands.spawn((
        Text2d::new(""),
        TextFont {
            font_size: bevy::text::FontSize::Px(24.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Anchor::TOP_LEFT,
        Transform::from_xyz(0.0, 0.0, Hud::Countdown.z()),
        Visibility::Hidden,
        Countdown,
        crate::shell::RUN_ONLY,
    ));
}

/// Shows the sign box while a sign speaks.
pub fn show_sign(
    sign: Res<SignText>,
    mut panel: Single<&mut Visibility, With<SignBox>>,
    mut label: Single<&mut Text2d, With<SignLabel>>,
) {
    match sign.text() {
        Some(text) => {
            **panel = Visibility::Inherited;
            if label.0 != *text {
                label.0.clone_from(text);
            }
        }
        None => **panel = Visibility::Hidden,
    }
}

/// Draws the running `time_limit` timer as `H:MM:SS` at the timer's own place in the room.
pub fn show_countdown(
    game: NonSend<Game>,
    room: Res<RoomSize>,
    mut text: Single<(&mut Text2d, &mut Transform, &mut Visibility), With<Countdown>>,
) {
    let timer = game
        .sim
        .world()
        .instances()
        .iter()
        .find_map(|i| match (&i.state, i.alive) {
            (KindState::Builtin(BuiltinState::TimeLimit(t)), true) if !t.start_zone => Some((
                i.m.x().get() as f32,
                i.m.y().get() as f32,
                t.hours,
                t.minutes,
                t.seconds,
            )),
            _ => None,
        });
    let (label, transform, visibility) = &mut *text;
    match timer {
        Some((x, y, h, m, s)) => {
            label.0 = format_time(h, m, s);
            transform.translation.x = x - room.0.x / 2.0;
            transform.translation.y = room.0.y / 2.0 - y;
            **visibility = Visibility::Inherited;
        }
        None => **visibility = Visibility::Hidden,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_reads_h_mm_ss() {
        assert_eq!(format_time(0, 2, 56), "0:02:56");
        assert_eq!(format_time(1, 0, 5), "1:00:05");
        assert_eq!(format_time(0, 0, -1), "0:00:00");
    }

    /// The box's white frame top edge in GM coordinates (y down from the room's top).
    fn box_top(room_h: f32) -> f32 {
        room_h - MARGIN - BOX_HEIGHT - BORDER
    }

    #[test]
    fn the_sign_box_sits_below_the_kid_at_the_showcase_signs() {
        // The Kid's 32x32 frame with origin y 23 at the showcase signs' row (512) ends at 521.
        let kid_bottom = 512.0 + 32.0 - 23.0;
        assert!(
            box_top(608.0) > kid_bottom,
            "box top {} is not below the Kid's {kid_bottom}",
            box_top(608.0)
        );
    }

    #[test]
    fn the_long_showcase_message_fits_in_three_lines() {
        // The 252-character courtyard message wraps to three lines at the box's text width (the
        // screenshot records the wrap); 1.2 is the font's line height.
        let text_height = 3.0 * TEXT_SIZE * 1.2;
        assert!(
            text_height <= BOX_HEIGHT - 2.0 * PAD_Y,
            "three lines at {TEXT_SIZE} px ({text_height}) do not fit the text area"
        );
    }

    #[test]
    fn the_sign_box_lasts_the_latch_after_the_tick_that_raises_it() {
        let mut sign = SignText::default();
        let said = [OutputEvent::Builtin(BuiltinOutput::TextBox { text: "hi".into() })];
        note_tick(&mut sign, &said);
        assert_eq!(sign.text().map(String::as_str), Some("hi"));
        // The tick that raised it does not count down, then every silent tick does.
        for _ in 0..SIGN_BOX_TICKS - 1 {
            note_tick(&mut sign, &[]);
        }
        assert_eq!(
            sign.text().map(String::as_str),
            Some("hi"),
            "the box stays up for the declared duration"
        );
        note_tick(&mut sign, &[]);
        assert_eq!(sign.text(), None, "and hides when the latch ends");
    }

    #[test]
    fn a_new_raise_restarts_the_latch() {
        let mut sign = SignText::default();
        let said = [OutputEvent::Builtin(BuiltinOutput::TextBox { text: "hi".into() })];
        note_tick(&mut sign, &said);
        for _ in 0..SIGN_BOX_TICKS - 1 {
            note_tick(&mut sign, &[]);
        }
        note_tick(&mut sign, &said);
        assert_eq!(sign.text().map(String::as_str), Some("hi"));
        for _ in 0..SIGN_BOX_TICKS - 1 {
            note_tick(&mut sign, &[]);
        }
        assert_eq!(
            sign.text().map(String::as_str),
            Some("hi"),
            "a second speak gets the whole duration again"
        );
    }

    #[test]
    fn a_retry_clears_the_box_at_once() {
        let mut sign = SignText::default();
        note_tick(
            &mut sign,
            &[OutputEvent::Builtin(BuiltinOutput::TextBox {
                text: "hi".into(),
            })],
        );
        sign.clear();
        assert_eq!(sign.text(), None);
        note_tick(&mut sign, &[]);
        assert_eq!(sign.text(), None);
    }
}
