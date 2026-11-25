//! The controls panel (#52; docs/architecture/client/controls.md): a searchable list of the
//! games in a folder, the selected game's actions with its `[controls]` labels for `extra1` to
//! `extra4` (docs/format/game.md), and rebinding of the global bindings by keyboard.
//!
//! `iwbtg-client --controls <folder> [--screenshot <png>]` opens the panel alone, for checks and
//! screenshots until a settings screen or the pause menu hosts it. Rebinding goes through
//! [`bindings::Config`]: the same validation and the same settings table as `--bind`, so the
//! panel cannot create a double binding or an unknown input the command line would refuse.
//!
//! A game's `[controls]` labels are what its extra actions mean to players. The panel shows them
//! in the game's rows and in [`extra_actions_notice`], the line a notice before play carries; the
//! HUD's input display draws the same labels (hud/run_info.rs, #831).

use std::path::{Path, PathBuf};

use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy::ui::IsDefaultUiCamera;
use bevy::ui_widgets::Activate;
use iwbtg_format::manifest::Manifest;
use iwbtg_sim::input::Action;

use crate::bindings::{self, Bindings, ClientKey, Config};
use crate::ui::fonts::Type;
use crate::ui::tokens;
use crate::ui::widgets::{self, ButtonKind, SearchField, button_with, search_field, text};

/// The size the panel lays out at; [`fit_to_window`] scales it to the window, as the gallery does.
const DESIGN: Vec2 = Vec2::new(1400.0, 900.0);

/// A game row's height and the gap under it, in design px.
const ROW: f32 = 56.0;
const ROW_GAP: f32 = 6.0;

/// An action row's height and the gap under it.
const ACTION_ROW: f32 = 34.0;
const ACTION_GAP: f32 = 4.0;

/// The engine actions every game reads, in the panel's order; the four extras follow a game's
/// `[controls]` labels instead.
const GAME_ACTIONS: [Action; 15] = [
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
    Action::Pause,
    Action::Mute,
    Action::Reset,
];

/// The four extra actions, in `[controls]` order.
const EXTRA_ACTIONS: [Action; 4] = [
    Action::Extra1,
    Action::Extra2,
    Action::Extra3,
    Action::Extra4,
];

/// The panel's command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug, Default)]
pub struct ControlsArgs {
    /// Open the controls panel for a folder of packages instead of a game.
    #[arg(long, value_name = "FOLDER")]
    pub controls: Option<PathBuf>,
    /// Screenshot checks: pick this row of the list at start (`0` is the first title).
    #[arg(long, hide = true, value_name = "N", requires = "controls")]
    pub controls_pick: Option<usize>,
    /// Screenshot checks: start with this text in the search field.
    #[arg(long, hide = true, value_name = "TEXT", requires = "controls")]
    pub controls_search: Option<String>,
}

/// The panel's place in the module list (main.rs).
pub struct Registration;

impl crate::registration::ClientModule for Registration {
    type Args = ControlsArgs;

    fn prepare(
        args: &mut ControlsArgs,
        _startup: &crate::registration::Startup,
    ) -> Result<(), String> {
        if let Some(folder) = &args.controls
            && !folder.is_dir()
        {
            return Err(format!("--controls: {} is not a folder", folder.display()));
        }
        Ok(())
    }

    fn standalone(
        args: &ControlsArgs,
        startup: &crate::registration::Startup,
    ) -> Option<Result<(), String>> {
        args.controls.as_ref().map(|folder| {
            run(
                folder,
                args.controls_pick,
                args.controls_search.as_deref(),
                startup.screenshot.clone(),
                startup.window,
                startup.ratio,
            )
        })
    }
}

/// One game of the folder, as the list shows it.
#[derive(Clone, Debug, PartialEq)]
struct GameRow {
    title: String,
    creator: String,
    /// The `[controls]` labels of `extra1` to `extra4`; `None` where the game names no meaning.
    labels: [Option<String>; 4],
}

impl GameRow {
    /// How many extra actions the game labels.
    fn extras(&self) -> usize {
        self.labels.iter().flatten().count()
    }

    /// The labels as [`extra_actions_notice`] takes them.
    fn label_refs(&self) -> [Option<&str>; 4] {
        self.labels
            .each_ref()
            .map(|label| label.as_ref().map(String::as_str))
    }

    /// The tag after a game's name in the list (`2 extra actions`), empty when it has none.
    fn extras_tag(&self) -> String {
        match self.extras() {
            0 => String::new(),
            1 => "1 extra action".to_string(),
            n => format!("{n} extra actions"),
        }
    }
}

/// What a rebind row binds: a sim action or one of the client's own keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Action(Action),
    Client(ClientKey),
}

impl Target {
    /// The name the settings' `[bindings]` table and `--bind` use.
    fn name(self) -> &'static str {
        match self {
            Target::Action(a) => a.name(),
            Target::Client(k) => k.name(),
        }
    }

    /// The name the panel shows.
    fn label(self) -> &'static str {
        match self {
            Target::Action(a) => action_label(a),
            Target::Client(ClientKey::QuickRetry) => "Quick retry",
            Target::Client(ClientKey::SkipCutscene) => "Skip cutscene",
            Target::Client(ClientKey::FrameAdvance) => "Frame advance",
        }
    }

    /// The keys and controller inputs bound now, by name.
    fn inputs(self, bindings: &Bindings) -> (Vec<String>, Vec<&'static str>) {
        let (keys, pads) = match self {
            Target::Action(a) => (bindings.keys_of(a), bindings.pads_of(a)),
            Target::Client(k) => (bindings.client_keys_of(k), bindings.client_pads_of(k)),
        };
        (
            keys.into_iter().map(bindings::key_name).collect(),
            pads.into_iter().map(|p| p.name()).collect(),
        )
    }

    /// Whether the binding is still the default.
    fn is_default(self, bindings: &Bindings) -> bool {
        match self {
            Target::Action(a) => bindings.is_default_action(a),
            Target::Client(k) => bindings.is_default_client(k),
        }
    }
}

/// The panel's name for an action.
fn action_label(action: Action) -> &'static str {
    match action {
        Action::Left => "Left",
        Action::Right => "Right",
        Action::Up => "Up",
        Action::Down => "Down",
        Action::Jump => "Jump",
        Action::Shoot => "Shoot",
        Action::Restart => "Restart",
        Action::Suicide => "Suicide",
        Action::Skip => "Skip",
        Action::Accept => "Accept",
        Action::Back => "Back",
        Action::Menu => "Menu",
        Action::Pause => "Pause",
        Action::Mute => "Mute",
        Action::Reset => "Reset",
        Action::Extra1 => "Extra 1",
        Action::Extra2 => "Extra 2",
        Action::Extra3 => "Extra 3",
        Action::Extra4 => "Extra 4",
    }
}

/// A panel waiting for the next key press for `target`.
#[derive(Clone, Copy, Debug)]
struct Listen {
    target: Target,
    /// The frame the row was activated in: its own key press (Enter, Space) must not bind itself.
    fresh: bool,
}

/// The panel's state. Every change rebuilds the list and the action columns (`rebuild`).
#[derive(Resource)]
struct Panel {
    games: Vec<GameRow>,
    /// The games the search keeps, as indices into `games`.
    shown: Vec<usize>,
    /// The picked row, as an index into `shown`.
    selected: usize,
    query: String,
    /// The list's scroll offset in design px.
    scroll: f32,
    config: Config,
    bindings: Bindings,
    listening: Option<Listen>,
    /// Why the last rebind was refused (a double binding, say), shown above the columns.
    error: Option<String>,
    /// Whether a successful rebind is written to the settings (`false` in a `--screenshot` run).
    save: bool,
}

impl Panel {
    /// The picked game.
    fn current(&self) -> Option<&GameRow> {
        self.shown.get(self.selected).map(|i| &self.games[*i])
    }

    /// Recomputes `shown` from the search and keeps `selected` in range.
    fn refilter(&mut self) {
        let query = self.query.to_lowercase();
        let words: Vec<&str> = query.split_whitespace().collect();
        self.shown = self
            .games
            .iter()
            .enumerate()
            .filter(|(_, g)| {
                let title = g.title.to_lowercase();
                let creator = g.creator.to_lowercase();
                words
                    .iter()
                    .all(|w| title.contains(w) || creator.contains(w))
            })
            .map(|(i, _)| i)
            .collect();
        if self.selected >= self.shown.len() {
            self.selected = 0;
        }
        self.scroll = 0.0;
    }

    /// Moves the pick by `step` rows, kept inside the list.
    fn step(&mut self, step: isize) {
        if self.shown.is_empty() {
            return;
        }
        let last = self.shown.len() - 1;
        self.selected = (self.selected as isize + step).clamp(0, last as isize) as usize;
    }

    /// Starts listening for a key for `target`.
    fn listen(&mut self, target: Target) {
        self.listening = Some(Listen {
            target,
            fresh: true,
        });
        self.error = None;
    }

    /// Binds `vk` to the row being listened to; a refused binding keeps the old one and says why.
    fn bind(&mut self, vk: u8) {
        let Some(listen) = self.listening.take() else {
            return;
        };
        let spec = format!("{}={}", listen.target.name(), bindings::key_name(vk));
        let mut config = self.config.clone();
        match config.set(&spec) {
            Ok(()) => match Bindings::from_config(&config) {
                Ok(inputs) => {
                    self.config = config;
                    self.bindings = inputs;
                    self.error = None;
                    if self.save {
                        bindings::save_config(&self.config);
                    }
                }
                Err(e) => self.error = Some(e),
            },
            Err(e) => self.error = Some(e),
        }
    }
}

/// The line a notice before play carries when a game labels extra actions (#52): each label with
/// the key bound to it now, or `unbound`. `None` when the game uses no extra action.
///
/// The panel shows the same line under the picked game's name.
pub fn extra_actions_notice(labels: [Option<&str>; 4], bindings: &Bindings) -> Option<String> {
    let parts: Vec<String> = labels
        .into_iter()
        .zip(EXTRA_ACTIONS)
        .filter_map(|(label, action)| {
            let label = label?;
            let keys: Vec<String> = bindings
                .keys_of(action)
                .into_iter()
                .map(bindings::key_name)
                .collect();
            Some(if keys.is_empty() {
                format!("{label} (unbound)")
            } else {
                format!("{label} ({})", keys.join(" / "))
            })
        })
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(format!("Extra actions: {}.", parts.join(", ")))
    }
}

/// Reads the packages in `folder` into the list, in title order: the set `--select` lists, read
/// for its `game.toml` only.
fn load(folder: &Path) -> Vec<GameRow> {
    let mut games: Vec<GameRow> = crate::shell::select::packages(folder)
        .iter()
        .filter_map(|path| read_row(path))
        .collect();
    games.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then_with(|| a.title.cmp(&b.title))
    });
    games
}

/// One package's row; `None` when its `game.toml` does not read (the client cannot play it, so
/// there is nothing to bind for it).
fn read_row(path: &Path) -> Option<GameRow> {
    let text = if crate::package::is_iwbz(path) {
        let files = crate::package::read_iwbz(path).ok()?;
        std::str::from_utf8(files.get("game.toml")?).ok()?.to_owned()
    } else {
        std::fs::read_to_string(path.join("game.toml")).ok()?
    };
    let manifest = Manifest::parse(&text).ok()?;
    let labels = manifest
        .controls
        .as_ref()
        .map(|c| c.labels().map(|l| l.map(str::to_owned)))
        .unwrap_or([None, None, None, None]);
    let title = manifest.title.get_ref().trim();
    Some(GameRow {
        title: if title.is_empty() {
            crate::shell::select::library::file_name(path)
        } else {
            title.to_owned()
        },
        creator: crate::loading::authors(&manifest).unwrap_or_default(),
        labels,
    })
}

/// Opens the panel window and runs until it closes. `window` and `ratio` as for the gallery; a
/// `--screenshot` run uses the default bindings and writes nothing.
pub fn run(
    folder: &Path,
    pick: Option<usize>,
    search: Option<&str>,
    screenshot: Option<std::path::PathBuf>,
    window: Option<(u32, u32)>,
    ratio: Option<(u32, u32)>,
) -> Result<(), String> {
    let size = window.or_else(|| ratio.map(|(w, h)| (1600, 1600 * h / w)));
    let resolution = match size {
        Some((w, h)) => bevy::window::WindowResolution::new(w, h).with_scale_factor_override(1.0),
        None => (1600, 1000).into(),
    };
    let isolated = screenshot.is_some();
    let config = if isolated {
        Config::default()
    } else {
        bindings::config_from_settings()
    };
    let inputs = Bindings::from_config(&config).map_err(|e| e.to_string())?;
    let mut panel = Panel {
        games: load(folder),
        shown: Vec::new(),
        selected: 0,
        query: search.unwrap_or_default().to_string(),
        scroll: 0.0,
        config,
        bindings: inputs,
        listening: None,
        error: None,
        save: !isolated,
    };
    panel.refilter();
    if let Some(pick) = pick {
        panel.selected = pick.min(panel.shown.len().saturating_sub(1));
    }

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(crate::logging::log_plugin())
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "IWBTG Collection: controls".to_string(),
                    resolution,
                    ..default()
                }),
                ..default()
            }),
    )
    .add_plugins((crate::render_fix::plugin, crate::ui::UiKitPlugin))
    .insert_resource(ClearColor(tokens::GROUND))
    .insert_resource(panel)
    .insert_resource(InitialSearch(search.unwrap_or_default().to_string()))
    .add_systems(Startup, setup)
    .add_observer(on_activate)
    .add_systems(
        Update,
        (
            fit_to_window,
            read_search,
            move_selection,
            scroll_list,
            capture_key,
        ),
    )
    // After the systems above, so one frame's changes cost one rebuild.
    .add_systems(PostUpdate, rebuild);
    if let Some(path) = screenshot {
        let size = size.map(|(w, h)| UVec2::new(w, h));
        crate::view::add_screenshot(&mut app, path, size);
    }
    crate::view::shot_result(app.run())
}

/// The list's viewport: its rows scroll inside it and are clipped by it.
#[derive(Component)]
struct GameList;

/// The action columns.
#[derive(Component)]
struct Detail;

/// A row that picks game `shown[i]`.
#[derive(Component)]
struct PickGame(usize);

/// A row that waits for a key for this target.
#[derive(Component)]
struct Rebind(Target);

/// The search text a `--controls-search` check starts with; the field shows it too.
#[derive(Resource)]
struct InitialSearch(String);

fn setup(mut commands: Commands, search: Res<InitialSearch>) {
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
            padding: UiRect::all(Val::Px(32.0)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(18.0),
            ..default()
        },
        children![
            (
                Node {
                    align_items: AlignItems::Baseline,
                    column_gap: Val::Px(16.0),
                    ..default()
                },
                children![
                    text("CONTROLS", Type::LABEL, tokens::MUTED),
                    text(
                        "Pick a game, then one of its rows to rebind",
                        Type::barlow(14.0, 400),
                        tokens::MUTED
                    ),
                ],
            ),
            (
                Node {
                    flex_grow: 1.0,
                    min_height: Val::Px(0.0),
                    column_gap: Val::Px(28.0),
                    ..default()
                },
                children![
                    (
                        Node {
                            width: Val::Px(420.0),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(12.0),
                            min_height: Val::Px(0.0),
                            ..default()
                        },
                        children![
                            (
                                Node {
                                    width: Val::Percent(100.0),
                                    height: Val::Px(widgets::SEARCH_HEIGHT),
                                    ..default()
                                },
                                children![search_field(
                                    "Search games: title, creator",
                                    &search.0
                                )],
                            ),
                            (
                                Node {
                                    flex_grow: 1.0,
                                    min_height: Val::Px(0.0),
                                    overflow: Overflow::clip_y(),
                                    ..default()
                                },
                                GameList,
                            ),
                        ],
                    ),
                    (
                        Node {
                            flex_grow: 1.0,
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(14.0),
                            min_height: Val::Px(0.0),
                            ..default()
                        },
                        Detail,
                    ),
                ],
            ),
        ],
    ));
}

/// Scales the panel to the window, as the gallery does.
fn fit_to_window(window: Single<&Window, Changed<Window>>, mut scale: ResMut<UiScale>) {
    let size = window.size();
    let fit = (size.x / DESIGN.x).min(size.y / DESIGN.y);
    if fit > 0.0 && (scale.0 - fit).abs() > 1e-4 {
        scale.0 = fit;
    }
}

/// The search field's text filters the list.
fn read_search(fields: Query<&EditableText, With<SearchField>>, mut panel: ResMut<Panel>) {
    let Ok(field) = fields.single() else {
        return;
    };
    let query = field.value().to_string();
    if panel.query == query {
        return;
    }
    panel.query = query;
    panel.refilter();
}

/// Up and Down walk the list.
fn move_selection(
    keys: Res<ButtonInput<KeyCode>>,
    list: Query<&ComputedNode, With<GameList>>,
    mut panel: ResMut<Panel>,
) {
    if panel.listening.is_some() {
        return;
    }
    let step = if keys.just_pressed(KeyCode::ArrowDown) {
        1
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        -1
    } else {
        return;
    };
    panel.step(step);
    keep_visible(&mut panel, &list);
}

/// The mouse wheel scrolls the list.
fn scroll_list(
    mut wheel: MessageReader<MouseWheel>,
    list: Query<&ComputedNode, With<GameList>>,
    mut panel: ResMut<Panel>,
) {
    let dy: f32 = wheel.read().map(|w| w.y).sum();
    if dy == 0.0 || panel.listening.is_some() {
        return;
    }
    panel.scroll -= dy * 40.0;
    keep_visible(&mut panel, &list);
}

/// Clamps the scroll to the list's content and keeps the picked row inside the viewport.
fn keep_visible(panel: &mut Panel, list: &Query<&ComputedNode, With<GameList>>) {
    let visible = list
        .single()
        .map(|n| n.content_box().height())
        .unwrap_or(0.0);
    let content = panel.shown.len() as f32 * (ROW + ROW_GAP) - ROW_GAP;
    if visible > 0.0 {
        let top = panel.selected as f32 * (ROW + ROW_GAP);
        if top < panel.scroll {
            panel.scroll = top;
        }
        let bottom = top + ROW;
        if bottom > panel.scroll + visible {
            panel.scroll = bottom - visible;
        }
    }
    let max = (content - visible).max(0.0);
    panel.scroll = panel.scroll.clamp(0.0, max);
}

/// A click, Enter or Space on a row: pick the game, or start listening for a rebind.
fn on_activate(
    activate: On<Activate>,
    picks: Query<&PickGame>,
    rebinds: Query<&Rebind>,
    mut panel: ResMut<Panel>,
) {
    if let Ok(pick) = picks.get(activate.entity) {
        if pick.0 < panel.shown.len() {
            panel.selected = pick.0;
            panel.error = None;
        }
    } else if let Ok(rebind) = rebinds.get(activate.entity) {
        panel.listen(rebind.0);
    }
}

/// While a row listens, the next key press binds it; Escape cancels.
fn capture_key(keys: Res<ButtonInput<KeyCode>>, mut panel: ResMut<Panel>) {
    let Some(mut listen) = panel.listening else {
        return;
    };
    // The activation itself (Enter, Space) is still "just pressed" this frame.
    if listen.fresh {
        listen.fresh = false;
        panel.listening = Some(listen);
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        panel.listening = None;
        panel.error = None;
        return;
    }
    if let Some(vk) = keys
        .get_just_pressed()
        .filter_map(|code| crate::input::key_code(*code))
        .next()
    {
        panel.bind(vk);
    }
}

/// Rebuilds the list and the action columns from [`Panel`] whenever it changes.
fn rebuild(
    mut commands: Commands,
    panel: Res<Panel>,
    list: Single<Entity, With<GameList>>,
    detail: Single<Entity, With<Detail>>,
) {
    if !panel.is_changed() {
        return;
    }
    commands.entity(*list).despawn_related::<Children>();
    commands.entity(*list).with_children(|parent| {
        parent
            .spawn(Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(-panel.scroll),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(ROW_GAP),
                ..default()
            })
            .with_children(|column| {
                for (pos, index) in panel.shown.iter().enumerate() {
                    let game = &panel.games[*index];
                    column.spawn((game_row(game, pos == panel.selected), PickGame(pos)));
                }
            });
    });

    commands.entity(*detail).despawn_related::<Children>();
    let picked = panel.current().cloned();
    commands.entity(*detail).with_children(|parent| {
        let Some(game) = &picked else {
            parent.spawn(text(
                "No game matches the search",
                Type::barlow(18.0, 600),
                tokens::MUTED,
            ));
            return;
        };
        parent.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            },
            children![
                text(&game.title, Type::barlow(26.0, 700), tokens::INK),
                text(
                    &if game.creator.is_empty() {
                        "No creator listed".to_string()
                    } else {
                        game.creator.clone()
                    },
                    Type::barlow(13.0, 400),
                    tokens::MUTED,
                ),
            ],
        ));
        let notice = extra_actions_notice(game.label_refs(), &panel.bindings).unwrap_or_else(|| {
            "No extra actions in this game; the standard set covers it.".to_string()
        });
        parent.spawn(text(
            &notice,
            Type::barlow(13.0, 500),
            if game.extras() > 0 {
                tokens::AMBER
            } else {
                tokens::MUTED
            },
        ));
        if let Some(error) = &panel.error {
            parent.spawn(text(error, Type::barlow(13.0, 500), tokens::BEHIND));
        } else if panel.listening.is_some() {
            parent.spawn(text(
                "Press a key, or Escape to cancel",
                Type::barlow(13.0, 500),
                tokens::ACCENT,
            ));
        }
        parent
            .spawn(Node {
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                column_gap: Val::Px(24.0),
                ..default()
            })
            .with_children(|columns| {
                columns
                    .spawn(Node {
                        flex_grow: 1.0,
                        min_height: Val::Px(0.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(ACTION_GAP),
                        ..default()
                    })
                    .with_children(|column| {
                        column.spawn(text("GAME ACTIONS", Type::LABEL, tokens::MUTED));
                        for action in GAME_ACTIONS {
                            column.spawn(action_row(
                                Target::Action(action),
                                action_label(action),
                                None,
                                &panel,
                            ));
                        }
                        for (label, action) in game.labels.iter().zip(EXTRA_ACTIONS) {
                            if let Some(label) = label {
                                column.spawn(action_row(
                                    Target::Action(action),
                                    label,
                                    Some("EXTRA"),
                                    &panel,
                                ));
                            }
                        }
                    });
                columns
                    .spawn(Node {
                        width: Val::Px(330.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(ACTION_GAP),
                        ..default()
                    })
                    .with_children(|column| {
                        column.spawn(text("CLIENT KEYS", Type::LABEL, tokens::MUTED));
                        for key in ClientKey::ALL {
                            column.spawn(action_row(
                                Target::Client(key),
                                Target::Client(key).label(),
                                None,
                                &panel,
                            ));
                        }
                    });
            });
    });
}

/// A game row: its title over its creator, a tag for its extra actions, highlighted when picked.
fn game_row(game: &GameRow, picked: bool) -> impl Bundle {
    let kind = if picked {
        ButtonKind::ControlOn
    } else {
        ButtonKind::Control
    };
    let extras = game.extras_tag();
    (
        button_with(
            kind,
            ROW,
            16.0,
            (
                Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(10.0),
                    ..default()
                },
                children![
                    (
                        Node {
                            flex_direction: FlexDirection::Column,
                            ..default()
                        },
                        children![
                            text(&game.title, Type::barlow(15.0, 600), tokens::INK),
                            text(
                                &if game.creator.is_empty() {
                                    "No creator listed".to_string()
                                } else {
                                    game.creator.clone()
                                },
                                Type::barlow(12.0, 400),
                                tokens::MUTED
                            ),
                        ],
                    ),
                    text(&extras, Type::barlow(11.0, 700), tokens::AMBER),
                ],
            ),
        ),
    )
}

/// One action row: its label and tag left, its keys (or `press a key…`) right; a click starts a
/// rebind for it.
fn action_row(target: Target, label: &str, tag: Option<&str>, panel: &Panel) -> impl Bundle {
    let listening = panel.listening.is_some_and(|l| l.target == target);
    let changed = !target.is_default(&panel.bindings);
    let (keys, pads) = target.inputs(&panel.bindings);
    let kind = if listening {
        ButtonKind::ControlOn
    } else {
        ButtonKind::Control
    };
    let tag = if changed {
        ("CHANGED", tokens::AMBER)
    } else if let Some(tag) = tag {
        (tag, tokens::ACCENT)
    } else {
        ("", tokens::MUTED)
    };
    let (right, note): (Vec<String>, bool) = if listening {
        (vec!["press a key…".to_string()], true)
    } else if keys.is_empty() && pads.is_empty() {
        (vec!["unbound".to_string()], true)
    } else {
        (
            keys.into_iter()
                .chain(pads.into_iter().map(|p| format!("pad {p}")))
                .collect(),
            false,
        )
    };
    (
        button_with(
            kind,
            ACTION_ROW,
            14.0,
            (
                Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(10.0),
                    ..default()
                },
                children![
                    (
                        Node {
                            column_gap: Val::Px(8.0),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        children![
                            text(label, Type::barlow(14.0, 600), tokens::INK),
                            text(tag.0, Type::barlow(10.0, 700), tag.1),
                        ],
                    ),
                    (
                        Node {
                            column_gap: Val::Px(6.0),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        Children::spawn(SpawnIter(right.into_iter().map(move |name| {
                            widgets::pill(
                                &name,
                                Type::barlow(12.0, 600),
                                if note {
                                    Color::NONE
                                } else {
                                    tokens::KEY_IDLE
                                },
                                if listening {
                                    tokens::ACCENT
                                } else if note {
                                    tokens::MUTED
                                } else {
                                    tokens::INK
                                },
                                22.0,
                                8.0,
                            )
                        }))),
                    ),
                ],
            ),
        ),
        Rebind(target),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use iwbtg_sim::input::vk;

    /// A panel over `games` with the default bindings and every game shown.
    fn panel(games: Vec<GameRow>) -> Panel {
        let mut panel = Panel {
            games,
            shown: Vec::new(),
            selected: 0,
            query: String::new(),
            scroll: 0.0,
            config: Config::default(),
            bindings: Bindings::default(),
            listening: None,
            error: None,
            save: false,
        };
        panel.refilter();
        panel
    }

    fn game(title: &str, creator: &str, labels: [Option<&str>; 4]) -> GameRow {
        GameRow {
            title: title.to_string(),
            creator: creator.to_string(),
            labels: labels.map(|l| l.map(str::to_owned)),
        }
    }

    /// #52: typing keeps the games whose title or creator holds every word.
    #[test]
    fn the_search_keeps_games_that_hold_every_word() {
        let mut p = panel(vec![
            game("Kaenbyou", "reo", [None, None, None, None]),
            game("Boshy", "Solgryn", [None, None, None, None]),
            game("Kaenbyou Hard", "solgryn", [None, None, None, None]),
        ]);
        p.query = "kaen".into();
        p.refilter();
        assert_eq!(p.shown, [0, 2]);
        p.query = "kaen solgryn".into();
        p.refilter();
        assert_eq!(p.shown, [2], "both words, in title or creator");
        p.query = "nothing".into();
        p.refilter();
        assert!(p.shown.is_empty());
        assert_eq!(p.selected, 0, "the pick stays in range");
        assert_eq!(p.current(), None);
    }

    /// #52: the list counts the game's `[controls]` labels.
    #[test]
    fn extras_are_counted_from_the_labels() {
        assert_eq!(game("a", "", [None, None, None, None]).extras(), 0);
        let one = game("a", "", [Some("Teleport"), None, None, None]);
        assert_eq!(one.extras(), 1);
        assert_eq!(one.extras_tag(), "1 extra action");
        let two = game("a", "", [Some("Teleport"), None, Some("Slow"), None]);
        assert_eq!(two.extras(), 2);
        assert_eq!(two.extras_tag(), "2 extra actions");
        assert_eq!(two.label_refs(), [Some("Teleport"), None, Some("Slow"), None]);
    }

    /// #52: the notice names each label with the key bound now, or `unbound`.
    #[test]
    fn the_notice_names_labels_and_their_keys() {
        let mut config = Config::default();
        config.set("extra1=E").unwrap();
        let bindings = Bindings::from_config(&config).unwrap();
        let line = extra_actions_notice([Some("Teleport"), None, Some("Slow"), None], &bindings)
            .expect("the game labels extras");
        assert_eq!(
            line,
            "Extra actions: Teleport (E), Slow (unbound)."
        );
        assert_eq!(extra_actions_notice([None; 4], &bindings), None);
    }

    /// #52: a fixture package's labels are read from its `game.toml`.
    #[test]
    fn a_fixture_reads_its_labels() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/input.pkg");
        let row = read_row(&path).expect("the fixture reads");
        assert_eq!(row.title, "Demo room");
        assert_eq!(row.labels[0].as_deref(), Some("Teleport"));
        assert_eq!(row.labels[1], None);
        assert_eq!(row.labels[2].as_deref(), Some("Slow"));
        assert!(row.creator.contains("IWBTG Collection"));
        assert_eq!(row.extras(), 2);
    }

    /// #52: rebinding uses `Config`'s checks; a conflict keeps the old binding and says why.
    #[test]
    fn a_refused_rebind_keeps_the_old_binding() {
        let mut p = panel(Vec::new());
        p.listen(Target::Action(Action::Jump));
        p.bind(vk::Z); // Z shoots
        assert!(p.error.as_deref().is_some_and(|e| e.contains('Z')));
        assert_eq!(p.config, Config::default());
        assert_eq!(p.bindings.key_action(vk::SHIFT), Some(Action::Jump));

        p.listen(Target::Action(Action::Jump));
        p.bind(vk::SPACE);
        assert_eq!(p.error, None);
        assert_eq!(p.bindings.key_action(vk::SPACE), Some(Action::Jump));
        assert_eq!(
            p.bindings.key_action(vk::LEFT_SHIFT),
            None,
            "Space replaced it"
        );
        assert!(!p.bindings.is_default_action(Action::Jump));
    }

    /// #52: a client key is rebound through the same table, by its own name.
    #[test]
    fn a_client_key_rebinds_by_its_name() {
        let mut p = panel(Vec::new());
        assert!(p.bindings.is_default_client(ClientKey::QuickRetry));
        p.listen(Target::Client(ClientKey::QuickRetry));
        p.bind(vk::F1 + 4);
        assert_eq!(p.error, None);
        assert_eq!(p.config.keys["quick-retry"], vec!["F5".to_string()]);
        assert_eq!(
            p.bindings.client_key(vk::F1 + 4),
            Some(ClientKey::QuickRetry)
        );
        assert!(!p.bindings.is_default_client(ClientKey::QuickRetry));
    }
}
