//! The pause menu (#479, #624; docs/product.md, Pause; m5 `D13`, `D14`, `D16`, `Pause.dc.html`):
//! a page over the darkened game, in the client's own language.
//!
//! Escape (or Start on a controller) opens it and closes it again: it is the run screen
//! `RunScreen::Paused` (shell, #839), entered from and left to `RunScreen::Playing`. While it is
//! open the sim does not tick (it ticks only in `Playing`), the music and sounds hold, the backdrop
//! keeps running and the HUD steps aside (`hud::Suspended`): the page has the window. In Time
//! Attack the run's timer does not stop: every fixed-clock tick spent paused while the map's timer
//! runs (started, not ended) is counted and added to the HUD's time (`hud::Paused::ticks`), and
//! the status pill says `Timer running`. Classic stops game time (`Game time stopped`).
//!
//! The items, by mode ([`Item::list`]): Continue; Retry from save (what R does: the game's own
//! restart, pressed on the next tick); Restart run (Time Attack: the quick-retry key, `retry.rs`);
//! Quit to game select, or Back to the shelf in Classic (leaves the run without a results screen,
//! `shell::leave`; the run is not cleared, so no clear goes into the stats). Once the run is over
//! a minute old, Restart run and Quit ask for a 0.6 s hold of the key, button or click that picked
//! them (the row fills red), so a stray press cannot throw a long run away. Up and Down (and the
//! keys bound to up and down) move the selector, Enter, Space or the keys bound to jump or accept
//! pick, the mouse hovers and clicks. On a controller the D-pad or left stick moves, A (south)
//! picks and B (east) or Start continues; the hints then show the pad's glyphs. While the menu is
//! open R and the quick-retry key (and their controller inputs) pick their item at once.
//!
//! The right side is the run card: the game, the mode or map and mods, three numbers (Time
//! Attack: run time, your PB on this board and deaths; Classic: this session, deaths and where
//! you are), this session's deaths by room (`stats::SessionLog`) and the game's controls with the
//! player's bindings. The selector is the delicious fruit: the package's sprite `cherry` (its
//! first two frames) when it has one, else the delicious fruit the client carries
//! (`assets/ui/cherry.png`). The game's own P-key pause is sim input and stays the game's
//! (docs/architecture/client.md, Pause menu).

mod page;

use bevy::audio::AudioSink;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;
use bevy::window::PrimaryWindow;
use iwbtg_format::files::PackageFiles;
use iwbtg_sim::input::Action;
use iwbtg_sim::world::SimStatus;

use crate::bindings::{Bindings, PadInput};
use crate::hud::{Paused, Suspended};
use crate::input::{Collector, key_code};
use crate::package::{SpriteImage, decode_png};
use crate::shell::RunScreen;
use crate::ui::fonts::Type;
use crate::ui::motion::{OffsetX, Tween};
use crate::ui::shape::Shape;
use crate::view::Game;
use crate::locale;

/// The key that opens and closes the menu. The game's `menu` action has no keyboard default
/// (`Bindings::default` drops Escape, #758), so Escape never reaches the sim unless the player
/// binds it there.
pub const PAUSE_KEY: KeyCode = KeyCode::Escape;
/// The controller button that opens and closes the menu (#624). The game's `menu` action has no
/// controller default, so Start never reaches the sim unless the player binds it there.
pub const PAUSE_PAD: PadInput = PadInput::Start;

/// The menu's items, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Item {
    #[default]
    Continue,
    RetryFromSave,
    RestartRun,
    Quit,
}

impl Item {
    pub const ALL: [Item; 4] = [Item::Continue, Item::RetryFromSave, Item::RestartRun, Item::Quit];
    /// Classic has no run to restart: its items.
    pub const CLASSIC: [Item; 3] = [Item::Continue, Item::RetryFromSave, Item::Quit];

    /// The items of a mode, top to bottom.
    pub fn list(classic: bool) -> &'static [Item] {
        if classic { &Item::CLASSIC } else { &Item::ALL }
    }

    pub fn label(self, classic: bool) -> &'static str {
        match self {
            Item::Continue => locale::text("pause_continue"),
            Item::RetryFromSave => locale::text("pause_retry_from_save"),
            Item::RestartRun => locale::text("pause_restart_run"),
            Item::Quit if classic => locale::text("pause_back_to_shelf"),
            Item::Quit => locale::text("pause_quit_to_select"),
        }
    }

    /// The line under the label.
    pub fn sub(self, classic: bool) -> &'static str {
        match (self, classic) {
            (Item::Continue, _) => "",
            (Item::RetryFromSave, true) => locale::text("pause_retry_sub_classic"),
            (Item::RetryFromSave, false) => locale::text("pause_retry_sub"),
            (Item::RestartRun, _) => locale::text("pause_restart_sub"),
            (Item::Quit, true) => locale::text("pause_back_to_shelf_sub"),
            (Item::Quit, false) => locale::text("pause_quit_sub"),
        }
    }

    /// The key or button that picks it, on the row's right.
    pub fn glyph(self, _classic: bool, pad: bool) -> String {
        match (self, pad) {
            (Item::Continue, false) => "Esc".to_owned(),
            (Item::Continue, true) => "B".to_owned(),
            (Item::RetryFromSave, false) => "R".to_owned(),
            (Item::RetryFromSave, true) => "Y".to_owned(),
            (Item::RestartRun, false) => locale::format("pause_hold_key", &[("key", &"`")]),
            (Item::RestartRun, true) => locale::format("pause_hold_key", &[("key", &"Back")]),
            (Item::Quit, _) => String::new(),
        }
    }

    /// Throws the run away: asks for a hold once the run is long.
    pub fn destructive(self) -> bool {
        matches!(self, Item::RestartRun | Item::Quit)
    }

    fn index(self, classic: bool) -> usize {
        Item::list(classic).iter().position(|i| *i == self).unwrap_or(0)
    }

    /// The next item of the mode down (`+1`) or up (`-1`), wrapping round.
    fn step(self, by: i32, classic: bool) -> Item {
        let list = Item::list(classic);
        let n = list.len() as i32;
        list[(self.index(classic) as i32 + by).rem_euclid(n) as usize]
    }
}

/// `continue`, `retry`, `restart` or `quit`.
fn parse_item(s: &str) -> Result<Item, String> {
    match s.to_ascii_lowercase().as_str() {
        "continue" => Ok(Item::Continue),
        "retry" => Ok(Item::RetryFromSave),
        "restart" => Ok(Item::RestartRun),
        "quit" => Ok(Item::Quit),
        _ => Err("an item is continue, retry, restart or quit".to_string()),
    }
}

/// The pause menu's command-line flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug, Clone, Copy, Default)]
pub struct PauseArgs {
    /// Screenshot checks: open the pause menu at once with this item selected (continue, retry,
    /// restart or quit).
    #[arg(long, hide = true, value_name = "ITEM", value_parser = parse_item)]
    pub pause_preview: Option<Item>,
}

/// The pause menu's place in the module list (main.rs): its flags and plugin.
pub struct Registration;

impl crate::registration::ClientModule for Registration {
    type Args = PauseArgs;

    fn add(
        app: &mut bevy::prelude::App,
        args: PauseArgs,
        _startup: &crate::registration::Startup,
    ) {
        app.add_plugins(PausePlugin { args });
    }
}

/// The menu's state; it is open in `RunScreen::Paused`.
#[derive(Resource, Debug, Default)]
pub struct Menu {
    pub selected: Item,
    /// An item picked this frame, done by [`act`].
    picked: Option<Item>,
    /// This run's pauses that counted on the timer: the sim tick each began at and the ticks it
    /// lasted (the open one last).
    pub pauses: Vec<(u64, u64)>,
    /// The voices this menu held, resumed on close.
    held: Vec<Entity>,
    /// `--pause-preview`: open at [`PREVIEW_TICK`] with this item selected.
    preview: Option<Item>,
    /// The run is Classic (set when the menu opens): its items, card and accent.
    pub classic: bool,
    /// The menu was last driven by a controller: the hints show its glyphs.
    pub pad: bool,
    /// A destructive item being held to confirm.
    hold: Option<Hold>,
    /// The run is older than [`HOLD_AFTER_SECONDS`]: destructive items need a hold.
    long: bool,
}

/// A destructive item waiting for its hold: what picked it and how long it has been held.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Hold {
    item: Item,
    source: HoldSource,
    seconds: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HoldSource {
    Key(KeyCode),
    Pad(PadInput),
    Mouse,
}

/// How long a destructive item is held, once the run is older than [`HOLD_AFTER_SECONDS`].
pub const HOLD_SECONDS: f32 = 0.6;
pub const HOLD_AFTER_SECONDS: f64 = 60.0;

/// Whether the run screen is the pause menu.
fn is_open(screen: Option<&State<RunScreen>>) -> bool {
    screen.is_some_and(|s| *s.get() == RunScreen::Paused)
}

impl Menu {
    /// Ticks spent paused while the timer ran, over the whole run.
    pub fn paused_ticks(&self) -> u64 {
        self.pauses.iter().map(|p| p.1).sum()
    }
}

/// The selector's two frames, `FRUIT` px each.
#[derive(Resource)]
pub(crate) struct Fruit(pub(crate) Handle<Image>);

/// The selector's frame size in game pixels (the delicious fruit is 21 x 24).
pub(crate) const FRUIT: UVec2 = UVec2::new(21, 24);
/// The selector shows each frame this long (`pb-cherry 640ms steps(2)`).
const FRUIT_FRAME_SECONDS: f32 = 0.32;
/// `--pause-preview` opens the menu at this sim tick, once the timer has run a little.
const PREVIEW_TICK: u64 = 25;

pub struct PausePlugin {
    pub args: PauseArgs,
}

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        let preview = self.args.pause_preview;
        // The selector is the loaded package's fruit, else the client's own.
        crate::shell::run_resource(app, |w| {
            let image = fruit_image(&w.resource::<crate::shell::LoadedPackage>().pkg.files);
            Fruit(w.resource_mut::<Assets<Image>>().add(to_bevy(&image)))
        });
        crate::shell::run_resource(app, move |_| Menu { preview, ..default() });
        app.add_systems(FixedUpdate, count.run_if(in_state(RunScreen::Paused)))
            .add_observer(on_activate)
            .add_systems(
                Update,
                (
                    // The pause key works on the game and in the menu, not on the loading card or
                    // the results.
                    keys.run_if(|s: Option<Res<State<RunScreen>>>| {
                        s.is_some_and(|s| matches!(s.get(), RunScreen::Playing | RunScreen::Paused))
                    }),
                    mouse,
                    hold,
                    act,
                    hold_audio,
                    publish,
                    draw.after(crate::screen::apply),
                    looks,
                )
                    .chain()
                    .in_set(crate::shell::InRun),
            );
    }
}

/// A retry (`retry.rs`) starts a fresh run: no pauses, menu closed.
pub(crate) fn reset(world: &mut World) {
    if let Some(mut menu) = world.get_resource_mut::<Menu>() {
        menu.picked = None;
        menu.pauses.clear();
    }
    if crate::shell::run_screen(world) == Some(RunScreen::Paused) {
        crate::shell::go(world, RunScreen::Playing);
    }
}

/// Each fixed-clock tick while the menu is open: one more tick on the timer when the map's timer
/// runs (the sim would have counted it), the sim is running and the run is Time Attack.
fn count(game: NonSend<Game>, mut menu: ResMut<Menu>) {
    let world = game.sim.world();
    let t = &world.timing;
    // Classic stops game time (m5 D14): only Time Attack's timer runs on.
    if game.sim.options().mode == iwbtg_sim::stripped::PlayMode::Original {
        return;
    }
    if world.status != SimStatus::Running || !t.started || t.ended {
        return;
    }
    let at = world.tick;
    match menu.pauses.last_mut() {
        Some(p) if p.0 == at => p.1 += 1,
        _ => menu.pauses.push((at, 1)),
    }
}

/// The action a key is bound to.
fn bound(collector: Option<&Collector>, key: KeyCode) -> Option<Action> {
    collector.and_then(|c| key_code(key).and_then(|k| c.bindings().key_action(k)))
}

/// The key is bound to quick-retry.
fn quick_retry(collector: Option<&Collector>, key: KeyCode) -> bool {
    collector.is_some_and(|c| key_code(key).is_some_and(|k| c.bindings().is_quick_retry_key(k)))
}

/// What a controller input does in the open menu: pick an item, pick the selected one, or move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PadCommand {
    Pick(Item),
    PickSelected,
    Step(i32),
}

/// The open menu's controller layout: Start and B (east) continue, A (south) picks, the D-pad and
/// the left stick move; then the inputs bound to quick-retry, `restart`, `jump`/`accept` and
/// `up`/`down` do what their keys do.
fn pad_command(bindings: &Bindings, input: PadInput) -> Option<PadCommand> {
    use PadCommand::{Pick, PickSelected, Step};
    match input {
        PAUSE_PAD | PadInput::East => Some(Pick(Item::Continue)),
        PadInput::South => Some(PickSelected),
        PadInput::DPadUp | PadInput::StickUp => Some(Step(-1)),
        PadInput::DPadDown | PadInput::StickDown => Some(Step(1)),
        _ if bindings.client_pad(input) == Some(crate::bindings::ClientKey::QuickRetry) => {
            Some(Pick(Item::RestartRun))
        }
        _ => match bindings.pad_action(input) {
            Some(Action::Restart) => Some(Pick(Item::RetryFromSave)),
            Some(Action::Jump | Action::Accept) => Some(PickSelected),
            Some(Action::Up) => Some(Step(-1)),
            Some(Action::Down) => Some(Step(1)),
            _ => None,
        },
    }
}

/// Escape or Start opens (`RunScreen::Paused`) and closes; while open the arrows (D-pad, stick)
/// move and the pick keys (A) pick.
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    collector: Option<Res<Collector>>,
    game: NonSend<Game>,
    mut menu: ResMut<Menu>,
    screen: Res<State<RunScreen>>,
    mut next: ResMut<NextState<RunScreen>>,
    mut pad_before: Local<[bool; PadInput::ALL.len()]>,
) {
    // The controller inputs that went down since the last frame (the collector holds the active
    // pad's state, read before the fixed clock).
    let mut pad_pressed = Vec::new();
    if let Some(c) = collector.as_deref() {
        for input in PadInput::ALL {
            let down = c.pad_down(input);
            if down && !pad_before[input as usize] {
                pad_pressed.push(input);
            }
            pad_before[input as usize] = down;
        }
    }
    let classic = game.sim.options().mode == iwbtg_sim::stripped::PlayMode::Original;
    let long = run_seconds(&game) > HOLD_AFTER_SECONDS;
    menu.long = long;
    if *screen.get() != RunScreen::Paused {
        let opened = if let Some(item) = menu.preview
            && game.sim.world().tick >= PREVIEW_TICK
        {
            menu.preview = None;
            Some(item)
        } else if keys.just_pressed(PAUSE_KEY) {
            menu.pad = false;
            Some(Item::Continue)
        } else if pad_pressed.contains(&PAUSE_PAD) {
            menu.pad = true;
            Some(Item::Continue)
        } else {
            None
        };
        if let Some(item) = opened {
            menu.classic = classic;
            menu.hold = None;
            menu.selected = if Item::list(classic).contains(&item) {
                item
            } else {
                Item::Continue
            };
            next.set(RunScreen::Paused);
        }
        return;
    }
    if menu.hold.is_some() {
        // The hold runs its course in `hold`; other input waits.
        return;
    }
    let collector = collector.as_deref();
    if let Some(c) = collector {
        for &input in &pad_pressed {
            menu.pad = true;
            let pick = match pad_command(c.bindings(), input) {
                Some(PadCommand::Pick(item)) => Some(item),
                Some(PadCommand::PickSelected) => Some(menu.selected),
                Some(PadCommand::Step(by)) => {
                    menu.selected = menu.selected.step(by, classic);
                    None
                }
                None => None,
            };
            if let Some(item) = pick
                && Item::list(classic).contains(&item)
            {
                menu.selected = item;
                choose(&mut menu, item, HoldSource::Pad(input), long);
                return;
            }
        }
    }
    for &key in keys.get_just_pressed() {
        menu.pad = false;
        let action = bound(collector, key);
        let pick = match key {
            PAUSE_KEY => Some(Item::Continue),
            _ if quick_retry(collector, key) => Some(Item::RestartRun),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => Some(menu.selected),
            KeyCode::ArrowUp => {
                menu.selected = menu.selected.step(-1, classic);
                None
            }
            KeyCode::ArrowDown => {
                menu.selected = menu.selected.step(1, classic);
                None
            }
            _ => match action {
                Some(Action::Restart) => Some(Item::RetryFromSave),
                Some(Action::Jump | Action::Accept) => Some(menu.selected),
                Some(Action::Up) => {
                    menu.selected = menu.selected.step(-1, classic);
                    None
                }
                Some(Action::Down) => {
                    menu.selected = menu.selected.step(1, classic);
                    None
                }
                _ => None,
            },
        };
        if let Some(item) = pick
            && Item::list(classic).contains(&item)
        {
            menu.selected = item;
            choose(&mut menu, item, HoldSource::Key(key), long);
            break;
        }
    }
}

/// The run's age in seconds of game time.
fn run_seconds(game: &Game) -> f64 {
    let rate = f64::from(game.sim.def().tick_rate.max(1));
    game.sim.world().tick as f64 / rate
}

/// Picks `item`, or starts its hold when it throws a long run away.
fn choose(menu: &mut Menu, item: Item, source: HoldSource, long: bool) {
    if item.destructive() && long {
        menu.hold = Some(Hold {
            item,
            source,
            seconds: 0.0,
        });
    } else {
        menu.picked = Some(item);
    }
}

/// Runs a hold: while what started it stays down the row fills; at [`HOLD_SECONDS`] the item is
/// picked, and letting go earlier lets it be.
fn hold(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    collector: Option<Res<Collector>>,
    mut menu: ResMut<Menu>,
) {
    let Some(mut h) = menu.hold else {
        return;
    };
    let down = match h.source {
        HoldSource::Key(k) => keys.pressed(k),
        HoldSource::Pad(p) => collector.as_deref().is_some_and(|c| c.pad_down(p)),
        HoldSource::Mouse => mouse.as_deref().is_some_and(|m| m.pressed(MouseButton::Left)),
    };
    if !down {
        menu.hold = None;
        return;
    }
    h.seconds += time.delta_secs();
    if h.seconds >= HOLD_SECONDS {
        menu.hold = None;
        menu.picked = Some(h.item);
    } else {
        menu.hold = Some(h);
    }
}

use page::Row;

/// The mouse hovers to select; a press on a long run's destructive item starts its hold.
fn mouse(
    rows: Query<(&Row, &Hovered)>,
    buttons: Option<Res<ButtonInput<MouseButton>>>,
    screen: Option<Res<State<RunScreen>>>,
    mut menu: ResMut<Menu>,
) {
    if !is_open(screen.as_deref()) {
        return;
    }
    let pressed = buttons
        .as_deref()
        .is_some_and(|b| b.just_pressed(MouseButton::Left));
    for (row, hovered) in &rows {
        if !hovered.0 {
            continue;
        }
        if menu.selected != row.0 {
            menu.selected = row.0;
        }
        if pressed && row.0.destructive() && menu.long && menu.hold.is_none() {
            menu.pad = false;
            menu.hold = Some(Hold {
                item: row.0,
                source: HoldSource::Mouse,
                seconds: 0.0,
            });
        }
    }
}

/// A click on an item picks it.
fn on_activate(
    activate: On<Activate>,
    rows: Query<&Row>,
    screen: Option<Res<State<RunScreen>>>,
    // Run-only: absent on game select, whose clicks this global observer also sees (#1467).
    menu: Option<ResMut<Menu>>,
) {
    if let Some(mut menu) = menu
        && let Ok(row) = rows.get(activate.entity)
        && is_open(screen.as_deref())
        && menu.hold.is_none()
    {
        menu.selected = row.0;
        menu.pad = false;
        // A long run's destructive item is held, not clicked (`mouse` starts that hold).
        if !(row.0.destructive() && menu.long) {
            menu.picked = Some(row.0);
        }
    }
}

/// Does what was picked. Every item closes the menu (back to `RunScreen::Playing`); the key edges
/// collected while it was open are dropped, so a key pressed in the menu never reaches the game.
fn act(world: &mut World) {
    let Some(item) = world.resource_mut::<Menu>().picked.take() else {
        return;
    };
    crate::shell::go(world, RunScreen::Playing);
    if let Some(mut c) = world.get_resource_mut::<Collector>() {
        c.take_frame();
    }
    match item {
        Item::Continue => {}
        Item::RetryFromSave => {
            if let Some(mut c) = world.get_resource_mut::<Collector>() {
                c.press_action(Action::Restart);
            }
        }
        Item::RestartRun => crate::retry::request(world),
        // Classic shows what the session did first (m5 A8); its Back to the shelf leaves.
        Item::Quit if world.resource::<Menu>().classic => crate::session::open(world, false),
        Item::Quit => {
            eprintln!("pause: quit (the run is discarded)");
            crate::shell::leave(world);
        }
    }
}

/// Holds every playing voice while the menu is open and resumes those on close; a voice the game
/// itself paused stays paused.
fn hold_audio(
    screen: Option<Res<State<RunScreen>>>,
    mut menu: ResMut<Menu>,
    mut sinks: Query<(Entity, &mut AudioSink)>,
) {
    if is_open(screen.as_deref()) {
        for (entity, sink) in &mut sinks {
            if !sink.is_paused() {
                sink.pause();
                menu.held.push(entity);
            }
        }
    } else if !menu.held.is_empty() {
        for entity in std::mem::take(&mut menu.held) {
            if let Ok((_, sink)) = sinks.get_mut(entity) {
                sink.play();
            }
        }
    }
}

/// Tells the HUD whether the menu is open and how long the run has been paused; the HUD steps
/// aside while it is (the page sets the UI scale).
fn publish(
    screen: Option<Res<State<RunScreen>>>,
    menu: Res<Menu>,
    mut paused: ResMut<Paused>,
    suspended: Option<ResMut<Suspended>>,
    mut was_open: Local<bool>,
) {
    let open = is_open(screen.as_deref());
    let want = Paused {
        open,
        ticks: menu.paused_ticks(),
    };
    if *paused != want {
        *paused = want;
    }
    if open != *was_open {
        *was_open = open;
        if let Some(mut s) = suspended {
            s.0 = open;
        }
    }
}

/// The page's root: the board it was built for and whether it is the pad's.
#[derive(Component)]
struct Root {
    board: page::Board,
    pad: bool,
    selected_list: bool,
}

/// What the card shows, from the run (read once when the menu opens).
fn card(world: &mut World) -> page::Card {
    let classic = world
        .get_resource::<Menu>()
        .is_some_and(|m| m.classic);
    let game = world.non_send::<Game>();
    let sim = &game.sim;
    let def = sim.def();
    let rate = def.tick_rate.max(1);
    let world_now = sim.world();
    let paused = world.get_resource::<Menu>().map_or(0, Menu::paused_ticks);
    let manifest = world
        .get_resource::<crate::shell::LoadedPackage>()
        .and_then(|p| p.pkg.files.get("game.toml"))
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|t| iwbtg_format::manifest::Manifest::parse(t).ok());
    let title = manifest
        .as_ref()
        .map(|m| m.title.get_ref().clone())
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| def.id.clone());
    let extras = manifest
        .as_ref()
        .and_then(|m| m.controls.as_ref())
        .map(|c| c.labels().map(|l| l.map(str::to_owned)))
        .unwrap_or_default();
    let map_name = manifest
        .as_ref()
        .and_then(|m| m.maps.iter().find(|x| x.id.get_ref() == sim.map_id()))
        .map(|m| m.name.get_ref().clone());
    let room = def
        .rooms
        .get(world_now.room.0 as usize)
        .map_or_else(String::new, |r| r.name.clone());
    let ranked_now = sim.ranked_ticks().saturating_add(paused);
    let run_deaths = u64::from(world_now.engine.run.deaths);
    let mods = world
        .get_resource::<crate::mods::RunMods>()
        .map_or(iwbtg_sim::mods::ModSet::NONE, |m| m.0);
    let stats = world.get_resource::<crate::stats::Stats>();
    let session = stats.and_then(|s| s.open_session());
    let game_mode = stats.map(|s| s.book.game_mode(&def.id, !classic)).unwrap_or_default();
    let rooms: Vec<(String, u32)> = session
        .map(|s| {
            s.deaths_by_room()
                .into_iter()
                .take(4)
                .map(|(r, n)| (r.to_owned(), n))
                .collect()
        })
        .unwrap_or_default();
    let big = if classic {
        let (seconds, deaths) = session.map_or((0.0, 0), |s| (s.seconds, s.deaths));
        vec![
            (
                locale::text("pause_this_session").to_owned(),
                crate::shell::shelf::clock(seconds),
                locale::text("pause_game_time_stopped").to_owned(),
            ),
            (
                locale::text("pause_deaths").to_owned(),
                crate::card::grouped(deaths),
                locale::format(
                    "pause_in_this_game",
                    &[("n", &crate::card::grouped(game_mode.deaths))],
                ),
            ),
            (
                locale::text("pause_where").to_owned(),
                room.clone(),
                String::new(),
            ),
        ]
    } else {
        let key = crate::stats::map_key(&def.id, sim.map_id());
        let pb = stats
            .and_then(|s| s.book.board(&key, true, &mods.codes()))
            .and_then(|b| b.pb().map(|r| r.ticks));
        vec![
            (
                locale::text("pause_run_time").to_owned(),
                crate::stats::fmt_ticks(ranked_now, rate),
                locale::text("pause_still_running").to_owned(),
            ),
            (
                locale::text("pause_your_pb").to_owned(),
                pb.map_or_else(|| "—".to_owned(), |t| crate::stats::fmt_ticks(t, rate)),
                if pb.is_some() {
                    String::new()
                } else {
                    locale::text("pause_no_clear_yet").to_owned()
                },
            ),
            (
                locale::text("pause_deaths").to_owned(),
                crate::card::grouped(run_deaths),
                locale::format(
                    "pause_in_this_game",
                    &[("n", &crate::card::grouped(game_mode.deaths))],
                ),
            ),
        ]
    };
    let bindings = world
        .get_resource::<Collector>()
        .map(|c| c.bindings().clone())
        .unwrap_or_default();
    let pad = world.get_resource::<Menu>().is_some_and(|m| m.pad);
    let glyph = |action: Action| -> Option<String> {
        if pad {
            bindings.pads_of(action).first().map(|p| pad_glyph(*p).to_owned())
        } else {
            bindings
                .keys_of(action)
                .first()
                .map(|k| crate::bindings::key_name(*k))
        }
    };
    let mut controls = Vec::new();
    let mv = if pad {
        Some("D-pad".to_owned())
    } else {
        match (glyph(Action::Left), glyph(Action::Right)) {
            (Some(l), Some(r)) => Some(format!("{} {}", arrow(&l), arrow(&r))),
            _ => None,
        }
    };
    if let Some(m) = mv {
        controls.push((m, locale::text("pause_control_move").to_owned(), false));
    }
    for (action, key) in [
        (Action::Jump, "pause_control_jump"),
        (Action::Shoot, "pause_control_shoot"),
        (Action::Restart, "pause_retry_from_save"),
    ] {
        if let Some(g) = glyph(action) {
            controls.push((g, locale::text(key).to_owned(), false));
        }
    }
    for (i, label) in extras.iter().enumerate() {
        let Some(label) = label else {
            continue;
        };
        let action = [Action::Extra1, Action::Extra2, Action::Extra3, Action::Extra4][i];
        let g = glyph(action).unwrap_or_else(|| "?".to_owned());
        controls.push((
            g,
            locale::format("pause_control_extra", &[("what", label), ("n", &(i + 1))]),
            true,
        ));
    }
    page::Card {
        classic,
        title,
        map_line: if classic {
            locale::text("mode_classic").to_owned()
        } else {
            map_name.unwrap_or_else(|| locale::text("mode_time_attack").to_owned())
        },
        mods: mods.iter().map(|d| d.code).collect(),
        big,
        live_note: !classic,
        rooms_key: if classic {
            locale::text("pause_rooms_classic").to_owned()
        } else {
            locale::text("pause_rooms").to_owned()
        },
        rooms,
        controls,
    }
}

/// `Left` as an arrow, else the key's name.
fn arrow(name: &str) -> String {
    match name {
        "Left" => "←".to_owned(),
        "Right" => "→".to_owned(),
        other => other.to_owned(),
    }
}

/// A controller input as the hints draw it (A, B, X, Y, LB, …).
pub(crate) fn pad_glyph(p: PadInput) -> &'static str {
    match p {
        PadInput::South => "A",
        PadInput::East => "B",
        PadInput::West => "X",
        PadInput::North => "Y",
        PadInput::LeftBumper => "LB",
        PadInput::RightBumper => "RB",
        PadInput::LeftTrigger => "LT",
        PadInput::RightTrigger => "RT",
        PadInput::Select => "Back",
        PadInput::Start => "Start",
        PadInput::DPadUp | PadInput::DPadDown | PadInput::DPadLeft | PadInput::DPadRight => "D-pad",
        _ => "Stick",
    }
}

/// The status pill's text: `Game time stopped`, or `Timer running · 38:09.02`.
fn status(world: &World) -> String {
    let menu = world.resource::<Menu>();
    if menu.classic {
        return locale::text("pause_game_time_stopped_status").to_owned();
    }
    let game = world.non_send::<Game>();
    let rate = game.sim.def().tick_rate.max(1);
    let t = game.sim.ranked_ticks().saturating_add(menu.paused_ticks());
    locale::format(
        "pause_timer_running",
        &[("time", &crate::stats::fmt_ticks(t, rate))],
    )
}

/// Spawns the page when the menu opens (again when the window's board or the device changes),
/// sets the UI scale for it, and removes it when the menu closes.
fn draw(world: &mut World) {
    let open = is_open(world.get_resource::<State<RunScreen>>());
    let window = world
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .iter(world)
        .next()
        .map(|w| w.size());
    let roots: Vec<(Entity, page::Board, bool, bool)> = world
        .query::<(Entity, &Root)>()
        .iter(world)
        .map(|(e, r)| (e, r.board, r.pad, r.selected_list))
        .collect();
    let Some(win) = window.filter(|w| w.x >= 1.0 && w.y >= 1.0) else {
        return;
    };
    if !open {
        for (e, ..) in roots {
            world.entity_mut(e).despawn();
        }
        return;
    }
    let board = page::Board::for_window(win);
    let scale = board.scale(win);
    if (world.resource::<UiScale>().0 - scale).abs() > 1e-4 {
        world.resource_mut::<UiScale>().0 = scale;
    }
    let (pad, classic) = {
        let m = world.resource::<Menu>();
        (m.pad, m.classic)
    };
    let mut current = false;
    for (e, b, p, c) in roots {
        if b == board && p == pad && c == classic {
            current = true;
        } else {
            world.entity_mut(e).despawn();
        }
    }
    if current {
        return;
    }
    let card = card(world);
    let status = status(world);
    let fruit = world.resource::<Fruit>().0.clone();
    let menu = world.resource::<Menu>();
    let page = page::page(board, menu, &card, status, fruit, pad);
    // Centred in the window at the board's scale.
    let (w, h) = (win.x / scale, win.y / scale);
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Px(w),
            height: Val::Px(h),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        // The game at half brightness behind the page's gradient.
        BackgroundColor(crate::ui::tokens::black(0.5)),
        GlobalZIndex(20),
        Root {
            board,
            pad,
            selected_list: classic,
        },
        crate::shell::RUN_ONLY,
        children![page],
    ));
}

/// Follows the selection and the hold: the selected row is lit, ringed and shifted right with the
/// fruit beside it; the fruit steps between its two frames; the status keeps the time; the held
/// row fills.
#[allow(clippy::type_complexity, reason = "one pass over the page's parts")]
#[allow(clippy::too_many_arguments, reason = "one pass over the page's parts")]
fn looks(
    time: Res<Time>,
    menu: Res<Menu>,
    game: NonSend<Game>,
    roots: Query<&Root>,
    mut selectors: Query<(&page::Selector, &mut ImageNode)>,
    mut plates: Query<(&page::Plate, &mut Shape, &mut Tween<OffsetX>)>,
    mut labels: Query<(&page::Label, &mut TextColor, &mut TextFont), Without<page::Sub>>,
    mut subs: Query<(&page::Sub, &mut TextColor), Without<page::Label>>,
    mut fills: Query<(&page::HoldFill, &mut Node)>,
    mut statuses: Query<&mut Text, With<page::Status>>,
    mut live: Query<&mut Text, (With<page::LiveTime>, Without<page::Status>)>,
    mut dots: Query<&mut BackgroundColor, With<page::StatusDot>>,
    mut clock: Local<f32>,
) {
    if roots.is_empty() {
        *clock = 0.0;
        return;
    }
    *clock += time.delta_secs();
    let frame = (*clock / FRUIT_FRAME_SECONDS) as u32 % 2;
    let x = (frame * FRUIT.x) as f32;
    for (page::Selector(item), mut image) in &mut selectors {
        let alpha = if *item == menu.selected { 1.0 } else { 0.0 };
        if (image.color.alpha() - alpha).abs() > 1e-3 {
            image.color = Color::WHITE.with_alpha(alpha);
        }
        let want = Some(Rect::new(x, 0.0, x + FRUIT.x as f32, FRUIT.y as f32));
        if image.rect != want {
            image.rect = want;
        }
    }
    for (page::Plate(item), mut shape, mut tween) in &mut plates {
        let on = *item == menu.selected;
        let fill = if on { page::plate_on() } else { page::PLATE_OFF };
        let ring = on.then_some((2.0, page::RING));
        if shape.fill != fill || shape.ring != ring {
            shape.fill = fill;
            shape.ring = ring;
        }
        tween.retarget(if on { page::SHIFT } else { 0.0 });
    }
    for (page::Label(item), mut color, mut font) in &mut labels {
        let on = *item == menu.selected;
        let ink = if on { Color::WHITE } else { page::IDLE_INK };
        if color.0 != ink {
            color.0 = ink;
        }
        let weight = Type::barlow(20.0, if on { 700 } else { 600 }).font().weight;
        if font.weight != weight {
            font.weight = weight;
        }
    }
    for (page::Sub(item), mut color) in &mut subs {
        let ink = if *item == menu.selected {
            page::IDLE_INK
        } else {
            crate::ui::tokens::MUTED
        };
        if color.0 != ink {
            color.0 = ink;
        }
    }
    for (page::HoldFill(item), mut node) in &mut fills {
        let pct = match menu.hold {
            Some(h) if h.item == *item => 100.0 * (h.seconds / HOLD_SECONDS).min(1.0),
            _ => 0.0,
        };
        let want = Val::Percent(pct);
        if node.width != want {
            node.width = want;
        }
    }
    if !menu.classic {
        let rate = game.sim.def().tick_rate.max(1);
        let t = crate::stats::fmt_ticks(
            game.sim.ranked_ticks().saturating_add(menu.paused_ticks()),
            rate,
        );
        let line = locale::format("pause_timer_running", &[("time", &t)]);
        for mut text in &mut statuses {
            if text.0 != line {
                text.0 = line.clone();
            }
        }
        for mut text in &mut live {
            if text.0 != t {
                text.0 = t.clone();
            }
        }
        // The dot pulses once a second (`ps-live 1s ease-in-out`), held with reduced motion.
        let a = 0.675 + 0.325 * (std::f32::consts::TAU * *clock).cos();
        for mut bg in &mut dots {
            bg.0 = crate::ui::tokens::ACCENT.with_alpha(a);
        }
    }
}

/// The package's `cherry` sprite (its first two frames), else the client's own ([`bundled_fruit`]).
pub(crate) fn fruit_image(files: &PackageFiles) -> SpriteImage {
    let path = iwbtg_format::sprites::sprite_pngs(files)
        .into_iter()
        .find(|(key, _)| key == "cherry" || key.ends_with("/cherry"))
        .map(|(_, path)| path);
    path.and_then(|p| files.get(&p).and_then(decode_png))
        .filter(|s| s.size.0 >= 2 * FRUIT.x && s.size.1 >= FRUIT.y)
        .map(|s| crop(&s, 2 * FRUIT.x, FRUIT.y))
        .unwrap_or_else(bundled_fruit)
}

/// The top-left `w` x `h` of `s`.
pub(crate) fn crop(s: &SpriteImage, w: u32, h: u32) -> SpriteImage {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let start = ((y * s.size.0) * 4) as usize;
        rgba.extend_from_slice(&s.rgba[start..start + (w * 4) as usize]);
    }
    SpriteImage { size: (w, h), rgba }
}

/// The delicious fruit the client carries (`assets/ui/cherry.png`, credited in
/// `assets/ui/CREDITS.md`): the engine's cherry, two frames of 21 x 24. The main menu draws it,
/// and it stands in for a game that has no `cherry` sprite (#1608).
pub(crate) fn bundled_fruit() -> SpriteImage {
    decode_png(include_bytes!("../assets/ui/cherry.png"))
        .filter(|s| s.size.0 >= 2 * FRUIT.x && s.size.1 >= FRUIT.y)
        .map(|s| crop(&s, 2 * FRUIT.x, FRUIT.y))
        .expect("the bundled cherry is a 42 x 24 PNG")
}

pub(crate) fn to_bevy(s: &SpriteImage) -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let mut image = Image::new(
        Extent3d {
            width: s.size.0,
            height: s.size.1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        s.rgba.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = bevy::image::ImageSampler::nearest();
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The run screen is `screen`, with nothing asked for.
    fn on_screen(world: &mut World, screen: RunScreen) {
        world.insert_resource(State::new(screen));
        world.init_resource::<NextState<RunScreen>>();
    }

    /// What the state transition between two frames does: the screen asked for becomes the one.
    fn apply_screen(world: &mut World) {
        if let NextState::Pending(s) = std::mem::take(&mut *world.resource_mut::<NextState<RunScreen>>()) {
            world.insert_resource(State::new(s));
        }
    }

    fn screen(world: &World) -> RunScreen {
        *world.resource::<State<RunScreen>>().get()
    }

    #[test]
    fn the_selection_wraps_and_items_parse() {
        assert_eq!(Item::Continue.step(-1, false), Item::Quit);
        assert_eq!(Item::Quit.step(1, false), Item::Continue);
        assert_eq!(Item::RetryFromSave.step(1, false), Item::RestartRun);
        assert_eq!(Item::RetryFromSave.step(1, true), Item::Quit, "Classic has no Restart run");
        assert_eq!(parse_item("Restart"), Ok(Item::RestartRun));
        assert!(parse_item("menu").is_err());
    }

    #[test]
    fn the_bundled_fruit_has_two_frames_that_differ() {
        let s = bundled_fruit();
        assert_eq!(s.size, (42, 24));
        let frame = |f: u32| {
            crop(
                &SpriteImage {
                    size: s.size,
                    rgba: s.rgba.clone(),
                },
                42,
                24,
            )
            .rgba
            .chunks(42 * 4)
            .flat_map(|row| row[(f * 21 * 4) as usize..((f + 1) * 21 * 4) as usize].to_vec())
            .collect::<Vec<u8>>()
        };
        assert_ne!(frame(0), frame(1));
        assert!(frame(0).chunks(4).any(|p| p[3] == 255), "something is drawn");
    }

    /// Paused ticks count only while the map's timer runs; the sim does not tick meanwhile, and
    /// the pause is kept with the tick it began at.
    #[test]
    fn paused_ticks_count_while_the_timer_runs() {
        use bevy::ecs::system::RunSystemOnce;
        use iwbtg_sim::input::InputFrame;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut world = crate::view::test_world_in(&pkg, iwbtg_sim::stripped::PlayMode::Stripped);
        world.init_resource::<Menu>();
        world.run_system_once(count).unwrap();
        let started = world.non_send::<Game>().sim.world().timing.started;
        if !started {
            assert_eq!(
                world.resource::<Menu>().paused_ticks(),
                0,
                "the timer has not started"
            );
            for _ in 0..200 {
                if world.non_send::<Game>().sim.world().timing.started {
                    break;
                }
                world.non_send_mut::<Game>().sim.step(InputFrame::EMPTY);
            }
            assert!(world.non_send::<Game>().sim.world().timing.started);
        }
        let at = world.non_send::<Game>().sim.world().tick;
        for _ in 0..250 {
            world.run_system_once(count).unwrap();
        }
        assert_eq!(world.resource::<Menu>().pauses.last(), Some(&(at, 250)));
        world.non_send_mut::<Game>().sim.step(InputFrame::EMPTY);
        world.run_system_once(count).unwrap();
        assert_eq!(
            world.resource::<Menu>().pauses.len(),
            2,
            "a later pause is its own"
        );
        assert_eq!(world.resource::<Menu>().paused_ticks(), 251);
        reset(&mut world);
        assert_eq!(world.resource::<Menu>().paused_ticks(), 0);
    }

    /// Classic stops game time (m5 D14): a pause adds nothing to the run's time.
    #[test]
    fn classic_pauses_do_not_count() {
        use bevy::ecs::system::RunSystemOnce;
        use iwbtg_sim::input::InputFrame;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut world = crate::view::test_world(&pkg);
        world.init_resource::<Menu>();
        for _ in 0..200 {
            if world.non_send::<Game>().sim.world().timing.started {
                break;
            }
            world.non_send_mut::<Game>().sim.step(InputFrame::EMPTY);
        }
        for _ in 0..50 {
            world.run_system_once(count).unwrap();
        }
        assert_eq!(world.resource::<Menu>().paused_ticks(), 0);
    }

    /// Once the run is over a minute old, Restart run and Quit wait for a 0.6 s hold of the key
    /// that picked them; letting go earlier picks nothing.
    #[test]
    fn a_long_runs_destructive_items_need_a_hold() {
        let mut menu = Menu::default();
        choose(&mut menu, Item::Quit, HoldSource::Key(KeyCode::Enter), false);
        assert_eq!(menu.picked.take(), Some(Item::Quit), "a short run picks at once");
        choose(&mut menu, Item::Continue, HoldSource::Key(KeyCode::Enter), true);
        assert_eq!(menu.picked.take(), Some(Item::Continue), "only destructive items wait");
        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        app.init_resource::<ButtonInput<KeyCode>>();
        app.add_systems(Update, hold);
        let mut menu = Menu::default();
        choose(&mut menu, Item::RestartRun, HoldSource::Key(KeyCode::Enter), true);
        assert!(menu.picked.is_none() && menu.hold.is_some());
        app.insert_resource(menu);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Enter);
        let step = |app: &mut App, secs: f32| {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs_f32(secs));
            app.update();
        };
        step(&mut app, 0.3);
        assert!(app.world().resource::<Menu>().picked.is_none(), "not yet");
        step(&mut app, 0.35);
        assert_eq!(app.world().resource::<Menu>().picked, Some(Item::RestartRun));
        let mut menu = Menu::default();
        choose(&mut menu, Item::Quit, HoldSource::Key(KeyCode::Enter), true);
        app.insert_resource(menu);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::Enter);
        step(&mut app, 0.7);
        let menu = app.world().resource::<Menu>();
        assert!(menu.picked.is_none() && menu.hold.is_none(), "let go: nothing");
    }

    /// A controller opens the menu with Start, moves with the D-pad and the stick, picks with A
    /// and continues with B or Start; Start never reaches the game (#624).
    #[test]
    fn a_controller_opens_navigates_picks_and_closes_the_menu() {
        use bevy::ecs::system::RunSystemOnce;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut world = crate::view::test_world_in(&pkg, iwbtg_sim::stripped::PlayMode::Stripped);
        world.init_resource::<ButtonInput<KeyCode>>();
        world.init_resource::<Menu>();
        on_screen(&mut world, RunScreen::Playing);
        let mut system = IntoSystem::into_system(keys);
        system.initialize(&mut world);
        // One frame: the screen asked for last frame, the pad as the collector would see it, then
        // the menu's keys.
        let mut frame = |world: &mut World, down: &[PadInput]| {
            apply_screen(world);
            let mut c = world.resource_mut::<Collector>();
            for input in PadInput::ALL {
                c.set_pad(input, down.contains(&input));
            }
            system.run((), world).unwrap();
        };
        assert_eq!(
            world.resource::<Collector>().bindings().pad_action(PAUSE_PAD),
            None,
            "Start is not the game's"
        );
        frame(&mut world, &[PAUSE_PAD]);
        apply_screen(&mut world);
        assert_eq!(screen(&world), RunScreen::Paused, "Start opens");
        frame(&mut world, &[PAUSE_PAD]);
        assert!(
            world.resource::<Menu>().picked.is_none(),
            "held Start does nothing more"
        );
        frame(&mut world, &[PadInput::DPadDown]);
        frame(&mut world, &[]);
        frame(&mut world, &[PadInput::StickDown]);
        assert_eq!(world.resource::<Menu>().selected, Item::RestartRun);
        frame(&mut world, &[PadInput::DPadUp]);
        frame(&mut world, &[PadInput::South]);
        assert_eq!(
            world.resource::<Menu>().picked,
            Some(Item::RetryFromSave),
            "A picks"
        );
        world.run_system_once(act).unwrap();
        apply_screen(&mut world);
        assert_eq!(screen(&world), RunScreen::Playing);
        let f = world.resource_mut::<Collector>().take_frame();
        assert_eq!(f.pressed, Action::Restart.bit(), "only Retry from save's restart");

        frame(&mut world, &[]);
        frame(&mut world, &[PAUSE_PAD]);
        apply_screen(&mut world);
        assert_eq!(screen(&world), RunScreen::Paused);
        frame(&mut world, &[PadInput::East]);
        assert_eq!(
            world.resource::<Menu>().picked,
            Some(Item::Continue),
            "B continues"
        );
        world.run_system_once(act).unwrap();
        frame(&mut world, &[]);
        frame(&mut world, &[PAUSE_PAD]);
        frame(&mut world, &[]);
        frame(&mut world, &[PAUSE_PAD]);
        assert_eq!(
            world.resource::<Menu>().picked,
            Some(Item::Continue),
            "Start closes"
        );
        world.run_system_once(act).unwrap();
        apply_screen(&mut world);
        assert_eq!(screen(&world), RunScreen::Playing);
        assert_eq!(world.resource_mut::<Collector>().take_frame().pressed, 0);
    }

    /// Y (the default `restart` input) picks Retry from save, as R does; a pad input bound to
    /// quick-retry picks Restart run.
    #[test]
    fn bound_controller_inputs_pick_their_items() {
        let mut config = crate::bindings::Config::default();
        config.set("quick-retry=pad:select").unwrap();
        let b = Bindings::from_config(&config).unwrap();
        assert_eq!(
            pad_command(&b, PadInput::North),
            Some(PadCommand::Pick(Item::RetryFromSave))
        );
        assert_eq!(
            pad_command(&b, PadInput::Select),
            Some(PadCommand::Pick(Item::RestartRun))
        );
        assert_eq!(pad_command(&b, PadInput::West), None, "shoot does nothing");
    }

    /// Retry from save closes the menu, drops what was pressed in it and presses Restart for the
    /// next tick; Quit leaves the run (without `--select`, the app exits).
    #[test]
    fn picks_close_the_menu_and_do_their_item() {
        use bevy::ecs::system::RunSystemOnce;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut world = crate::view::test_world(&pkg);
        world.init_resource::<Messages<AppExit>>();
        on_screen(&mut world, RunScreen::Paused);
        world.insert_resource(Menu {
            picked: Some(Item::RetryFromSave),
            ..default()
        });
        // Down was pressed to reach the item: it must not reach the game.
        world
            .resource_mut::<Collector>()
            .press(iwbtg_sim::input::vk::DOWN);
        world.run_system_once(act).unwrap();
        apply_screen(&mut world);
        assert_eq!(screen(&world), RunScreen::Playing);
        let frame = world.resource_mut::<Collector>().take_frame();
        assert_eq!(frame.pressed, Action::Restart.bit());
        world.resource_mut::<Menu>().picked = Some(Item::Quit);
        world.run_system_once(act).unwrap();
        assert!(!world.resource::<Messages<AppExit>>().is_empty());
    }

    /// Escape is the pause key only (#758): opening and closing the menu gives the game the same
    /// (empty) input whether a tick runs on the frame Escape goes down, before the menu opens, or
    /// not. `collect_input` runs before the fixed clock and `keys` in Update, so both happen.
    #[test]
    fn escape_sends_the_game_nothing_on_any_frame_timing() {
        use bevy::ecs::system::RunSystemOnce;
        use iwbtg_sim::input::{InputFrame, vk};
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        assert_eq!(
            Bindings::default().key_action(vk::ESCAPE),
            None,
            "`menu` has no keyboard default"
        );
        let mut runs = Vec::new();
        for tick_on_open in [true, false] {
            let mut world = crate::view::test_world(&pkg);
            world.init_resource::<ButtonInput<KeyCode>>();
            world.init_resource::<Menu>();
            on_screen(&mut world, RunScreen::Playing);
            let mut system = IntoSystem::into_system(keys);
            system.initialize(&mut world);
            let mut ticks = Vec::new();
            // One frame: the screen asked for last frame, the key edge as `collect_input` feeds
            // it, maybe a tick (only while playing), then the menu.
            let mut frame = |world: &mut World, down: bool, tick: bool| {
                apply_screen(world);
                let mut input = world.resource_mut::<ButtonInput<KeyCode>>();
                input.clear();
                if down {
                    input.press(PAUSE_KEY);
                } else {
                    input.release(PAUSE_KEY);
                }
                let mut c = world.resource_mut::<Collector>();
                if down {
                    c.press(vk::ESCAPE);
                } else {
                    c.release(vk::ESCAPE);
                }
                if tick {
                    ticks.push(c.take_frame());
                }
                system.run((), world).unwrap();
                world.run_system_once(act).unwrap();
            };
            frame(&mut world, true, tick_on_open);
            apply_screen(&mut world);
            assert_eq!(screen(&world), RunScreen::Paused, "Escape opens");
            frame(&mut world, false, false);
            frame(&mut world, true, false);
            apply_screen(&mut world);
            assert_eq!(screen(&world), RunScreen::Playing, "Escape closes");
            frame(&mut world, false, true);
            frame(&mut world, false, true);
            assert!(
                ticks.iter().all(|f| *f == InputFrame::EMPTY),
                "tick on open {tick_on_open}: {ticks:?}"
            );
            runs.push(ticks.split_off(ticks.len() - 2));
        }
        assert_eq!(runs[0], runs[1], "the ticks after the menu are the same");
    }
}
