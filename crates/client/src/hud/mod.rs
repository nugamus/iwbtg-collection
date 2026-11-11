//! The in-run HUD: three columns, live leaderboard left, game centre, run info right
//! (docs/product.md, In-game display; docs/architecture/client.md, HUD).
//!
//! `layout` decides where the game and the columns go; `screen.rs` puts the game camera there and
//! this module places the panels. Keys (no settings screen yet, #53): Shift+Tab hides and shows the
//! HUD, F7 toggles integer scale, Ctrl+`=` and Ctrl+`-` change the UI scale by 5%. Integer scale
//! and UI scale are saved in the settings file; the HUD always starts shown.

pub mod layout;
mod panels;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;

use crate::settings;
use crate::ui::UiKitPlugin;
use crate::ui::backdrop::BackdropCamera;
use layout::{Density, Layout, Options, UI_SCALE_RANGE};

fn parse_ui_scale(s: &str) -> Result<u32, String> {
    s.trim_end_matches('%')
        .parse()
        .ok()
        .filter(|p| UI_SCALE_RANGE.contains(p))
        .ok_or_else(|| "the UI scale is a percentage from 75 to 150".to_string())
}

/// The HUD's command-line flags. Both are saved in the settings file.
#[derive(clap::Args, Debug)]
pub struct HudArgs {
    /// Scale the side panels (never the game), 75 to 150 percent (Ctrl+= and Ctrl+- step by 5).
    #[arg(long, value_name = "PERCENT", value_parser = parse_ui_scale)]
    pub ui_scale: Option<u32>,
    /// Scale the game by the largest whole step that fits instead of filling the room it has (F7).
    #[arg(long)]
    pub integer_scale: bool,
}

/// Called by `main` after `settings::load`: puts the flags into the settings.
pub fn configure(args: &HudArgs) {
    settings::update(|s| {
        if let Some(p) = args.ui_scale {
            s.ui_scale = p;
        }
        s.integer_scale |= args.integer_scale;
    });
}

/// The player's HUD choices (read by `screen.rs` for the layout).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Default)]
pub struct HudOptions(pub Options);

/// Where everything went this frame (`screen.rs` writes it).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Default)]
pub struct HudLayout(pub Layout);

/// The HUD's camera: draws only UI, over the game.
#[derive(Component)]
struct HudCamera;

/// A column of the HUD; `left` is the leaderboard's.
#[derive(Component)]
struct Column {
    left: bool,
}

/// The shadow around the game, drawn under it by the backdrop's camera.
#[derive(Component)]
struct GameShadow;

/// What the panels were built for, so they are rebuilt only when it changes.
#[derive(Component, PartialEq, Clone, Copy)]
struct Built(Density, u32);

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<UiKitPlugin>() {
            app.add_plugins(UiKitPlugin);
        }
        let s = settings::get();
        app.insert_resource(HudOptions(Options {
            shown: true,
            ui_scale: s.ui_scale as f32 / 100.0,
            integer: s.integer_scale,
        }))
        .init_resource::<HudLayout>()
        .add_systems(Startup, spawn)
        .add_systems(Update, (keys, shadow, place.after(crate::screen::apply)).chain());
    }
}

fn spawn(mut commands: Commands) {
    let camera = commands
        .spawn((
            Camera2d,
            HudCamera,
            Camera {
                order: 10,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            RenderLayers::none(),
            IsDefaultUiCamera,
        ))
        .id();
    for left in [true, false] {
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                ..default()
            },
            Column { left },
            UiTargetCamera(camera),
        ));
    }
}

/// Spawns the shadow around the game once the backdrop's camera is there. `Hud169`:
/// `box-shadow: 0 0 60px rgba(0,0,0,.6)`, drawn over the backdrop and under the game.
fn shadow(mut commands: Commands, backdrop: Query<Entity, With<BackdropCamera>>, mut done: Local<bool>) {
    if *done {
        return;
    }
    let Some(camera) = backdrop.iter().next() else {
        return;
    };
    *done = true;
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
        BoxShadow::new(
            Color::srgba(0.0, 0.0, 0.0, 0.6),
            Val::ZERO,
            Val::ZERO,
            Val::ZERO,
            Val::Px(60.0),
        ),
        GameShadow,
        UiTargetCamera(camera),
        GlobalZIndex(10),
    ));
}

/// Shift+Tab, F7, Ctrl+= and Ctrl+-.
fn keys(keys: Res<ButtonInput<KeyCode>>, mut hud: ResMut<HudOptions>) {
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    if shift && keys.just_pressed(KeyCode::Tab) {
        hud.0.shown = !hud.0.shown;
    }
    if keys.just_pressed(KeyCode::F7) {
        hud.0.integer = !hud.0.integer;
        let on = hud.0.integer;
        settings::update(|s| s.integer_scale = on);
    }
    let step = if ctrl && keys.any_just_pressed([KeyCode::Equal, KeyCode::NumpadAdd]) {
        5
    } else if ctrl && keys.any_just_pressed([KeyCode::Minus, KeyCode::NumpadSubtract]) {
        -5
    } else {
        0
    };
    if step != 0 {
        let now = (hud.0.ui_scale * 100.0).round() as i32 + step;
        let p = now.clamp(*UI_SCALE_RANGE.start() as i32, *UI_SCALE_RANGE.end() as i32) as u32;
        hud.0.ui_scale = p as f32 / 100.0;
        settings::update(|s| s.ui_scale = p);
    }
}

type ColumnParts = (
    Entity,
    &'static Column,
    &'static mut Node,
    &'static mut Visibility,
    Option<&'static Built>,
);

/// Places the columns and the game's shadow from `HudLayout`, and rebuilds a panel when its
/// density or width changed.
fn place(
    mut commands: Commands,
    hud: Res<HudOptions>,
    layout: Res<HudLayout>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    mut ui_scale: ResMut<UiScale>,
    mut columns: Query<ColumnParts, Without<GameShadow>>,
    mut shadow: Query<&mut Node, With<GameShadow>>,
) {
    let l = layout.0;
    // UI px to physical px.
    let px = window.resolution.scale_factor() * l.content;
    if px <= 0.0 {
        return;
    }
    if (ui_scale.0 - l.content).abs() > 1e-4 {
        ui_scale.0 = l.content;
    }
    let set = |node: &mut Node, r: Rect| {
        let want = (
            Val::Px(r.min.x / px),
            Val::Px(r.min.y / px),
            Val::Px(r.width() / px),
            Val::Px(r.height() / px),
        );
        if (node.left, node.top, node.width, node.height) != want {
            (node.left, node.top, node.width, node.height) = want;
        }
    };
    for mut node in &mut shadow {
        set(&mut node, l.game);
    }
    for (entity, column, mut node, mut visibility, built) in &mut columns {
        let shown = if hud.0.shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != shown {
            *visibility = shown;
        }
        let r = if column.left { l.left } else { l.right };
        set(&mut node, r);
        if !hud.0.shown {
            continue;
        }
        // The artboards' proportions: the leaderboard takes 92% of its column and sits 7% from
        // the game; run info takes 84% and sits 9% from it. Small windows use the whole column.
        let col = r.width() / px;
        let small = l.density == Density::Small;
        let (width, gap) = match (column.left, small) {
            (true, true) => (col, 0.0),
            (true, false) => (col * 0.92, col * 0.07),
            (false, _) => (col * 0.84, col * 0.09),
        };
        let want = Built(l.density, width.round() as u32);
        if built == Some(&want) {
            continue;
        }
        let mut e = commands.entity(entity);
        e.despawn_related::<Children>().insert(want);
        let margin = if column.left {
            UiRect::left(Val::Px(col - width - gap))
        } else {
            UiRect::left(Val::Px(gap))
        };
        let wrap = Node { margin, ..default() };
        if column.left {
            e.with_child((wrap, children![panels::leaderboard(width, l.density)]));
        } else {
            e.with_child((wrap, children![panels::run_info(width, l.density)]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(app: &mut App, held: &[KeyCode], key: KeyCode) {
        {
            let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            input.reset_all();
            for k in held {
                input.press(*k);
            }
            input.press(key);
        }
        app.update();
        let mut input = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.release_all();
        input.clear();
    }

    #[test]
    fn shift_tab_toggles_the_hud_and_ctrl_steps_the_ui_scale_within_its_range() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<HudOptions>()
            .add_systems(Update, keys);
        let hud = |app: &App| app.world().resource::<HudOptions>().0;
        press(&mut app, &[], KeyCode::Tab);
        assert!(hud(&app).shown, "Tab alone does nothing");
        press(&mut app, &[KeyCode::ShiftLeft], KeyCode::Tab);
        assert!(!hud(&app).shown);
        press(&mut app, &[KeyCode::ShiftRight], KeyCode::Tab);
        assert!(hud(&app).shown);
        press(&mut app, &[KeyCode::ControlLeft], KeyCode::Equal);
        assert!((hud(&app).ui_scale - 1.05).abs() < 1e-6);
        for _ in 0..20 {
            press(&mut app, &[KeyCode::ControlLeft], KeyCode::Minus);
        }
        assert!((hud(&app).ui_scale - 0.75).abs() < 1e-6, "stops at 75%");
        press(&mut app, &[], KeyCode::F7);
        assert!(hud(&app).integer);
    }

    #[test]
    fn the_ui_scale_flag_takes_75_to_150_percent() {
        assert_eq!(parse_ui_scale("125"), Ok(125));
        assert_eq!(parse_ui_scale("80%"), Ok(80));
        assert!(parse_ui_scale("200").is_err());
        assert!(parse_ui_scale("big").is_err());
    }
}
