//! The report read from a halt, the card's words, and the keys.

use bevy::state::app::StatesPlugin;

use super::*;

fn script(message: &str) -> Report {
    Report::new(&SimError::Script {
        message: message.to_owned(),
    })
}

/// The script host's message splits into the file, the line and column, what went wrong and the
/// bracket of where in the game.
#[test]
fn a_script_error_names_its_file_and_line() {
    let r = script(
        "scripts/boss.rhai: Array index 3 out of bounds: only 2 elements in array (line 9, \
         position 43) [kind @boss, instance 100089, Alarm(0), tick 24]",
    );
    assert_eq!(r.heading, "Script error");
    assert_eq!(r.file.as_deref(), Some("scripts/boss.rhai"));
    assert_eq!((r.line, r.column), (Some(9), Some(43)));
    assert_eq!(r.message, "Array index 3 out of bounds: only 2 elements in array");
    assert_eq!(
        r.context.as_deref(),
        Some("kind @boss · instance 100089 · Alarm(0) · tick 24")
    );
    assert_eq!(
        r.place().as_deref(),
        Some("scripts/boss.rhai · line 9, column 43")
    );
    assert_eq!(Report::sample(), r);
}

/// A message without a file or position (the host's own errors) is shown whole.
#[test]
fn a_bare_script_error_is_shown_whole() {
    let r = script("scripts nest too deeply (spawn inside Create inside spawn ...)");
    assert_eq!(r.file, None);
    assert_eq!(r.line, None);
    assert_eq!(r.place(), None);
    assert_eq!(
        r.message,
        "scripts nest too deeply (spawn inside Create inside spawn ...)"
    );
}

/// A halt that is not a script's says the game stopped, with the sim's words.
#[test]
fn another_halt_says_the_game_stopped() {
    let r = Report::new(&SimError::unsupported("DestroySelf"));
    assert_eq!(r.heading, "The game stopped");
    assert_eq!(r.message, "unsupported: DestroySelf");
    assert_eq!(r.file, None);
}

/// The card shows the place and the message, and its Back button says where it goes.
#[test]
fn the_card_shows_the_place_and_the_way_back() {
    for (back, label) in [(true, "Back to game select"), (false, "Quit")] {
        let mut app = App::new();
        let look = page::Look {
            title: "Script error fixture".into(),
            creator: None,
            art: None,
            back,
        };
        app.world_mut().spawn(page::page(&Report::sample(), &look));
        let texts: Vec<String> = app
            .world_mut()
            .query::<&Text>()
            .iter(app.world())
            .map(|t| t.0.clone())
            .collect();
        for want in [
            "scripts/boss.rhai · line 9, column 43",
            "Array index 3 out of bounds: only 2 elements in array",
            "Script error fixture",
            label,
        ] {
            assert!(texts.iter().any(|t| t == want), "{want}: {texts:?}");
        }
        let actions: Vec<Action> = app
            .world_mut()
            .query::<&Action>()
            .iter(app.world())
            .copied()
            .collect();
        assert_eq!(actions, [Action::Retry, Action::Back]);
    }
}

/// Escape, and Enter with no button focused, pick Back.
#[test]
fn escape_and_enter_go_back() {
    for key in [KeyCode::Escape, KeyCode::Enter] {
        let mut app = App::new();
        app.add_plugins(StatesPlugin)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(Stopped {
                report: Some(Report::sample()),
                ..default()
            })
            .add_systems(Update, keys);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        app.update();
        assert_eq!(app.world().resource::<Stopped>().picked, Some(Action::Back), "{key:?}");
    }
}
