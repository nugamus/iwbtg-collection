//! What the sim reports for the player to read: the sign text box (`BuiltinOutput::TextBox`) and
//! the `time_limit` countdown (read from the timer's built-in state).

use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::TextBounds;
use iwbtg_sim::builtins::{BuiltinOutput, BuiltinState};
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::world::KindState;

use super::{Game, RoomSize};

/// The text a sign showed in the latest tick. The sim sends it on every tick the sign speaks, so
/// the box is up exactly while a tick keeps raising it.
#[derive(Resource, Default)]
pub struct SignText(pub Option<String>);

#[derive(Component)]
pub struct SignBox;

#[derive(Component)]
pub struct SignLabel;

#[derive(Component)]
pub struct Countdown;

const BOX_HEIGHT: f32 = 96.0;
const MARGIN: f32 = 16.0;
const BORDER: f32 = 3.0;

/// Remembers the sign text of the tick that just ran (none when no sign spoke).
pub fn note_tick(sign: &mut SignText, output: &[OutputEvent]) {
    sign.0 = output.iter().rev().find_map(|e| match e {
        OutputEvent::Builtin(BuiltinOutput::TextBox { text }) => Some(text.clone()),
        _ => None,
    });
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
        Transform::from_xyz(0.0, y, 900.0),
        Visibility::Hidden,
        SignBox,
        children![
            (
                Sprite::from_color(Color::BLACK, Vec2::new(width, BOX_HEIGHT)),
                Transform::from_xyz(0.0, 0.0, 1.0),
            ),
            (
                Text2d::new(""),
                TextFont {
                    font_size: bevy::text::FontSize::Px(18.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                TextBounds::new(width - 24.0, BOX_HEIGHT - 16.0),
                Anchor::TOP_LEFT,
                Transform::from_xyz(-width / 2.0 + 12.0, BOX_HEIGHT / 2.0 - 8.0, 2.0),
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
        Transform::from_xyz(0.0, 0.0, 950.0),
        Visibility::Hidden,
        Countdown,
    ));
}

/// Shows the sign box while a sign speaks.
pub fn show_sign(
    sign: Res<SignText>,
    mut panel: Single<&mut Visibility, With<SignBox>>,
    mut label: Single<&mut Text2d, With<SignLabel>>,
) {
    match &sign.0 {
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

    #[test]
    fn the_sign_text_lasts_only_while_a_tick_raises_it() {
        let mut sign = SignText::default();
        let said = [OutputEvent::Builtin(BuiltinOutput::TextBox { text: "hi".into() })];
        note_tick(&mut sign, &said);
        assert_eq!(sign.0.as_deref(), Some("hi"));
        note_tick(&mut sign, &[]);
        assert_eq!(sign.0, None);
    }
}
