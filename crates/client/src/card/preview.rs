//! `iwbtg-client --card-preview <footer|results|results-compact>`: the player card alone on the
//! backdrop, scaled to fit the window, until game select (#148) and results (#41) embed it.
//! It shows your own stats book; the results variants take your latest clear as "this run".
//! With `--screenshot` it shows the artboard's sample numbers instead, so the image is the same
//! on every machine.

use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;

use super::{COLLECTION_GAMES, Card, RunChange, Variant, card, profile_name};
use crate::stats::Book;
use crate::storage::Store;
use crate::ui::UiKitPlugin;
use crate::ui::backdrop::{Backdrop, dim};
use crate::ui::tokens;

/// The card preview's flag.
#[derive(clap::Args, Debug)]
pub struct CardArgs {
    /// Show the player card alone (a dev view until game select and results have it).
    #[arg(long, value_enum, hide = true, value_name = "VARIANT")]
    pub card_preview: Option<Variant>,
}

/// Room around the card, in the card's logical px.
const MARGIN: f32 = 40.0;

/// The artboard's sample (`ProfileCard.dc.html`, scenario `pb`).
pub(crate) fn sample() -> Card {
    Card {
        name: "nugamus".to_owned(),
        games_cleared: 13,
        games_total: 20,
        deaths: 48_213,
        play_seconds: (212 * 3600 + 14 * 60) as f64,
        run: Some(RunChange {
            deaths: 1284,
            play_seconds: (38 * 60 + 12) as f64,
            game: "Kaenbyou".to_owned(),
            entries_cleared: 4,
            entries_total: 5,
        }),
    }
}

/// Your card from the stats book, with your latest clear as the run.
fn from_stats(book: &Book) -> Card {
    let latest = book
        .maps
        .values()
        .flat_map(|m| {
            m.boards
                .values()
                .flat_map(move |b| b.runs.iter().map(move |r| (m, r)))
        })
        .max_by_key(|(_, r)| r.finished_at);
    let run = latest.map(|(m, r)| {
        // Without the package the preview knows only the entries you have played.
        let played = book.maps.values().filter(|x| x.game == m.game).count();
        RunChange::new(book, &m.game, &m.game, played, r, m.tick_rate)
    });
    Card {
        run,
        ..Card::from_book(book, &profile_name(), COLLECTION_GAMES)
    }
}

/// The data the preview shows.
#[derive(Resource)]
struct Preview(Variant, Card);

/// Opens the preview window and runs until it closes. `window` and `ratio` as for the gallery.
pub fn run(
    variant: Variant,
    screenshot: Option<std::path::PathBuf>,
    window: Option<(u32, u32)>,
    ratio: Option<(u32, u32)>,
) {
    let data = if screenshot.is_some() {
        sample()
    } else {
        let book = Store::data()
            .and_then(|s| s.read_text(crate::stats::FILE).ok().flatten())
            .and_then(|t| Book::parse(&t).ok())
            .unwrap_or_default();
        from_stats(&book)
    };
    let fit = variant.size() + Vec2::splat(2.0 * MARGIN);
    let size = window.or_else(|| ratio.map(|(w, h)| (1600, 1600 * h / w)));
    let resolution = match size {
        Some((w, h)) => bevy::window::WindowResolution::new(w, h).with_scale_factor_override(1.0),
        None => ((fit.x * 2.0) as u32, (fit.y * 2.0) as u32).into(),
    };
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "IWBTG Collection: player card".to_string(),
            resolution,
            ..default()
        }),
        ..default()
    }))
    .add_plugins(UiKitPlugin)
    .insert_resource(ClearColor(tokens::GROUND))
    .insert_resource(Backdrop {
        dim: if variant == Variant::Footer {
            dim::SELECT
        } else {
            dim::RESULTS
        },
        ..default()
    })
    .insert_resource(Preview(variant, data))
    .add_systems(Startup, setup)
    .add_systems(Update, fit_to_window);
    if let Some(path) = screenshot {
        app.insert_resource(crate::view::ScreenshotTo(path))
            .add_systems(Update, crate::view::take_screenshot);
    }
    app.run();
}

fn setup(mut commands: Commands, preview: Res<Preview>) {
    // The backdrop has its own camera; this one draws over it and must not clear.
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::None,
            ..default()
        },
        IsDefaultUiCamera,
    ));
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![card(&preview.1, preview.0)],
    ));
}

/// Scales the card and its margin to fit the window.
fn fit_to_window(
    window: Single<&Window, Changed<Window>>,
    preview: Res<Preview>,
    mut scale: ResMut<UiScale>,
) {
    let want = preview.0.size() + Vec2::splat(2.0 * MARGIN);
    let size = window.size();
    let fit = (size.x / want.x).min(size.y / want.y);
    if fit > 0.0 && (scale.0 - fit).abs() > 1e-4 {
        scale.0 = fit;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{Board, MapStats, Run};

    #[test]
    fn the_latest_clear_is_this_run() {
        let run = |ticks, finished_at| Run {
            ticks,
            deaths: 3,
            splits: vec![ticks],
            segment_deaths: vec![3],
            finished_at,
            replay: None,
            death_log: Default::default(),
        };
        let mut book = Book::default();
        for (key, game, at) in [("a/main", "a", 10), ("b/easy", "b", 20)] {
            let mut board = Board::default();
            board.add(run(500, at));
            let mut m = MapStats {
                game: game.into(),
                tick_rate: 50,
                clears: 1,
                ..MapStats::default()
            };
            m.boards.insert("ranked/NM".into(), board);
            book.maps.insert(key.into(), m);
        }
        book.maps.insert(
            "b/hard".into(),
            MapStats {
                game: "b".into(),
                ..MapStats::default()
            },
        );
        let c = from_stats(&book);
        let r = c.run.expect("a clear");
        assert_eq!(r.game, "b");
        assert_eq!((r.entries_cleared, r.entries_total), (1, 2));
        assert_eq!(r.play_seconds, 10.0);
        assert!(from_stats(&Book::default()).run.is_none());
    }
}
