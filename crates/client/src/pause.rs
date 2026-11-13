//! The pause menu (#479; docs/product.md, Pause; artboards `Pause169`, `Pause43`, `PauseBox`):
//! the game's own PAUSE box, drawn in game pixels over the frozen, darkened game.
//!
//! Escape (or Start on a controller) opens it and closes it again. While it is open the sim does not tick (`SimTick` is
//! gated, as Sudden Death gates it in `mods.rs`), the music and sounds hold, and the backdrop and
//! HUD keep running. The run's timer does not stop: every fixed-clock tick spent paused while the
//! map's timer runs (started, not ended) is counted and added to the HUD's time
//! (`hud::Paused::ticks`); the paused line `Paused · timer running` shows in run info.
//!
//! The items: Continue; Retry from save (what R does: the game's own restart, pressed on the
//! next tick); Restart run (the quick-retry key, `retry.rs`); Quit (closes the client without a
//! results screen; the run is not cleared, so no clear goes into the stats). Up and Down (and the
//! keys bound to up and down) move the selector, Enter, Space or the keys bound to jump or accept
//! pick, the mouse hovers and clicks. On a controller the D-pad or left stick moves, A (south)
//! picks and B (east) or Start continues. While the menu is open R and the quick-retry key (and
//! their controller inputs) pick their item at once.
//!
//! The selector is the delicious fruit: the package's sprite `cherry` (its first two frames) when
//! it has one, else a placeholder drawn here. The game's own P-key pause is sim input and stays
//! the game's (docs/architecture/client.md, Pause menu).

use bevy::audio::AudioSink;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};
use bevy::window::PrimaryWindow;
use iwbtg_format::files::PackageFiles;
use iwbtg_sim::input::Action;
use iwbtg_sim::world::SimStatus;

use crate::bindings::{Bindings, PadInput};
use crate::hud::{HudLayout, Paused};
use crate::input::{Collector, key_code};
use crate::package::{SpriteImage, decode_png};
use crate::ui::fonts::{Family, Type};
use crate::ui::motion::{Ease, OffsetX, Tween};
use crate::ui::tokens;
use crate::ui::widgets::text;
use crate::view::{Game, SimTick};

/// The key that opens and closes the menu.
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

    pub fn label(self) -> &'static str {
        match self {
            Item::Continue => "Continue",
            Item::RetryFromSave => "Retry from save",
            Item::RestartRun => "Restart run",
            Item::Quit => "Quit",
        }
    }

    fn index(self) -> usize {
        Item::ALL.iter().position(|i| *i == self).unwrap_or(0)
    }

    /// The next item down (`+1`) or up (`-1`), wrapping round.
    fn step(self, by: i32) -> Item {
        let n = Item::ALL.len() as i32;
        Item::ALL[(self.index() as i32 + by).rem_euclid(n) as usize]
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

/// The menu's state.
#[derive(Resource, Debug, Default)]
pub struct Menu {
    pub open: bool,
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
/// An item that is not selected: `#8d909e`.
const IDLE_INK: Color = Color::srgb_u8(0x8d, 0x90, 0x9e);
/// The selected item moves this far right (game px), OutQuint.
const SHIFT: f32 = 4.0;
const SHIFT_SECONDS: f32 = 0.3;
/// The dim over the game: `rgba(0,0,0,.5)`.
const DIM: f32 = 0.5;
/// The box's fill: `rgba(0,0,0,.94)`.
const BOX_FILL: f32 = 0.94;
/// `--pause-preview` opens the menu at this sim tick, once the timer has run a little.
const PREVIEW_TICK: u64 = 25;

pub struct PausePlugin {
    pub args: PauseArgs,
    pub files: PackageFiles,
}

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        let image = fruit_image(&self.files);
        let handle = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(to_bevy(&image));
        let preview = self.args.pause_preview;
        app.insert_resource(Fruit(handle))
            .insert_resource(Menu { preview, ..default() })
            .configure_sets(FixedUpdate, SimTick.run_if(|menu: Res<Menu>| !menu.open))
            .add_systems(FixedUpdate, count.run_if(|menu: Res<Menu>| menu.open))
            .add_observer(on_activate)
            .add_systems(
                Update,
                (
                    keys.run_if(crate::results::closed),
                    mouse,
                    act,
                    hold_audio,
                    publish,
                    draw.after(crate::screen::apply),
                    looks,
                )
                    .chain(),
            );
    }
}

/// A retry (`retry.rs`) starts a fresh run: no pauses, menu closed.
pub(crate) fn reset(world: &mut World) {
    if let Some(mut menu) = world.get_resource_mut::<Menu>() {
        menu.open = false;
        menu.picked = None;
        menu.pauses.clear();
    }
}

/// Each fixed-clock tick while the menu is open: one more tick on the timer when the map's timer
/// runs (the sim would have counted it) and the sim is running.
fn count(game: NonSend<Game>, mut menu: ResMut<Menu>) {
    let world = game.sim.world();
    let t = &world.timing;
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

/// Escape or Start opens and closes; while open the arrows (D-pad, stick) move and the pick keys
/// (A) pick.
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    collector: Option<Res<Collector>>,
    game: NonSend<Game>,
    mut menu: ResMut<Menu>,
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
    if let Some(item) = menu.preview
        && game.sim.world().tick >= PREVIEW_TICK
    {
        menu.preview = None;
        menu.open = true;
        menu.selected = item;
        return;
    }
    if !menu.open {
        if keys.just_pressed(PAUSE_KEY) || pad_pressed.contains(&PAUSE_PAD) {
            menu.open = true;
            menu.selected = Item::Continue;
        }
        return;
    }
    let collector = collector.as_deref();
    if let Some(c) = collector {
        for &input in &pad_pressed {
            let pick = match pad_command(c.bindings(), input) {
                Some(PadCommand::Pick(item)) => Some(item),
                Some(PadCommand::PickSelected) => Some(menu.selected),
                Some(PadCommand::Step(by)) => {
                    menu.selected = menu.selected.step(by);
                    None
                }
                None => None,
            };
            if let Some(item) = pick {
                menu.selected = item;
                menu.picked = Some(item);
                return;
            }
        }
    }
    for &key in keys.get_just_pressed() {
        let action = bound(collector, key);
        let pick = match key {
            PAUSE_KEY => Some(Item::Continue),
            _ if quick_retry(collector, key) => Some(Item::RestartRun),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => Some(menu.selected),
            KeyCode::ArrowUp => {
                menu.selected = menu.selected.step(-1);
                None
            }
            KeyCode::ArrowDown => {
                menu.selected = menu.selected.step(1);
                None
            }
            _ => match action {
                Some(Action::Restart) => Some(Item::RetryFromSave),
                Some(Action::Jump | Action::Accept) => Some(menu.selected),
                Some(Action::Up) => {
                    menu.selected = menu.selected.step(-1);
                    None
                }
                Some(Action::Down) => {
                    menu.selected = menu.selected.step(1);
                    None
                }
                _ => None,
            },
        };
        if let Some(item) = pick {
            menu.selected = item;
            menu.picked = Some(item);
            break;
        }
    }
}

/// An item row; the mouse hovers to select and clicks to pick.
#[derive(Component)]
struct Row(Item);

fn mouse(rows: Query<(&Row, &Hovered), Changed<Hovered>>, mut menu: ResMut<Menu>) {
    if !menu.open {
        return;
    }
    for (row, hovered) in &rows {
        if hovered.0 {
            menu.selected = row.0;
        }
    }
}

/// A click on an item picks it.
fn on_activate(activate: On<Activate>, rows: Query<&Row>, mut menu: ResMut<Menu>) {
    if let Ok(row) = rows.get(activate.entity)
        && menu.open
    {
        menu.selected = row.0;
        menu.picked = Some(row.0);
    }
}

/// Does what was picked. Every item closes the menu; the key edges collected while it was open
/// are dropped, so a key pressed in the menu never reaches the game.
fn act(world: &mut World) {
    let Some(item) = world.resource_mut::<Menu>().picked.take() else {
        return;
    };
    world.resource_mut::<Menu>().open = false;
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
        Item::Quit => {
            eprintln!("pause: quit (the run is discarded)");
            world.write_message(AppExit::Success);
        }
    }
}

/// Holds every playing voice while the menu is open and resumes those on close; a voice the game
/// itself paused stays paused.
fn hold_audio(mut menu: ResMut<Menu>, mut sinks: Query<(Entity, &mut AudioSink)>) {
    if menu.open {
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

/// Tells the HUD whether the menu is open and how long the run has been paused.
fn publish(menu: Res<Menu>, mut paused: ResMut<Paused>) {
    let want = Paused {
        open: menu.open,
        ticks: menu.paused_ticks(),
    };
    if *paused != want {
        *paused = want;
    }
}

/// The dim over the game; the box is its child, centred.
#[derive(Component)]
struct Root {
    /// Logical UI px per game px the box was built for.
    k: f32,
}

/// The selector of an item.
#[derive(Component)]
struct Selector(Item);

/// The label of an item.
#[derive(Component)]
struct Label(Item);

/// Spawns the menu when it opens (and again when the game's size on screen changes), places it on
/// the game and removes it when it closes.
#[allow(clippy::type_complexity, reason = "one Bevy system")]
fn draw(
    mut commands: Commands,
    menu: Res<Menu>,
    layout: Res<HudLayout>,
    fruit: Res<Fruit>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut roots: Query<(Entity, &Root, &mut Node)>,
) {
    let l = layout.0;
    let px = window.resolution.scale_factor() * l.content;
    if !menu.open || px <= 0.0 || l.game.width() <= 0.0 {
        for (entity, ..) in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }
    // Game px to the UI's px (`UiScale` is the HUD's panel scale, `hud::place`).
    let k = l.scale / px;
    let rect = (
        Val::Px(l.game.min.x / px),
        Val::Px(l.game.min.y / px),
        Val::Px(l.game.width() / px),
        Val::Px(l.game.height() / px),
    );
    let mut found = false;
    for (entity, root, mut node) in &mut roots {
        if (root.k - k).abs() > 1e-4 {
            commands.entity(entity).despawn();
            continue;
        }
        found = true;
        if (node.left, node.top, node.width, node.height) != rect {
            (node.left, node.top, node.width, node.height) = rect;
        }
    }
    if found {
        return;
    }
    let mut root = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: rect.0,
            top: rect.1,
            width: rect.2,
            height: rect.3,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        // The game area darkens to 50%.
        BackgroundColor(tokens::black(DIM)),
        GlobalZIndex(20),
        Root { k },
    ));
    root.with_child(pause_box(k, menu.selected, fruit.0.clone()));
}

/// The PAUSE box at `k` UI px per game px (`PauseBox.dc.html`): 300 px wide, padding 22/26/24,
/// black at 94%, a 2 px white border and a 2 px white outline 3 px outside it, PAUSE in
/// Pixelify Sans 34 px 700 tracked 0.18 em, then the items 32 px tall and 2 px apart.
fn pause_box(k: f32, selected: Item, fruit: Handle<Image>) -> impl Bundle {
    let px = |v: f32| Val::Px(v * k);
    let heading = Type {
        family: Family::Pixel,
        tracking: 0.18,
        ..Type::barlow(34.0 * k, 700)
    };
    (
        Node {
            width: px(300.0),
            flex_direction: FlexDirection::Column,
            row_gap: px(18.0),
            padding: UiRect {
                left: px(26.0),
                right: px(26.0),
                top: px(22.0),
                bottom: px(24.0),
            },
            border: UiRect::all(px(2.0)),
            ..default()
        },
        BackgroundColor(tokens::black(BOX_FILL)),
        BorderColor::all(Color::WHITE),
        Outline::new(px(2.0), px(3.0), Color::WHITE),
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            let px = |v: f32| Val::Px(v * k);
            c.spawn((
                Node {
                    justify_content: JustifyContent::Center,
                    // The tracking after the last letter, balanced on the left (`padding-left: .18em`).
                    padding: UiRect::left(px(34.0 * 0.18)),
                    ..default()
                },
                // `line-height: 1`.
                children![(
                    Text::new("PAUSE"),
                    heading.font(),
                    heading.spacing(),
                    bevy::text::LineHeight::RelativeToFont(1.0),
                    TextColor(Color::WHITE),
                    crate::ui::shape::Ink(Color::WHITE),
                )],
            ));
            c.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(2.0),
                ..default()
            })
            .with_children(|list| {
                for item in Item::ALL {
                    list.spawn(row(k, item, item == selected, fruit.clone()));
                }
            });
        })),
    )
}

/// The item's type: Pixelify Sans 18 px, 600 selected and 500 otherwise, tracked 0.04 em.
fn item_type(k: f32, selected: bool) -> Type {
    Type {
        family: Family::Pixel,
        tracking: 0.04,
        ..Type::barlow(18.0 * k, if selected { 600 } else { 500 })
    }
}

fn row(k: f32, item: Item, selected: bool, fruit: Handle<Image>) -> impl Bundle {
    let px = |v: f32| Val::Px(v * k);
    let shift = if selected { SHIFT * k } else { 0.0 };
    (
        Button,
        Hovered::default(),
        Row(item),
        Node {
            height: px(32.0),
            align_items: AlignItems::Center,
            column_gap: px(10.0),
            ..default()
        },
        children![
            (
                Node {
                    width: px(FRUIT.x as f32),
                    height: px(FRUIT.y as f32),
                    flex_shrink: 0.0,
                    ..default()
                },
                ImageNode {
                    image: fruit,
                    rect: Some(Rect::new(0.0, 0.0, FRUIT.x as f32, FRUIT.y as f32)),
                    color: Color::WHITE.with_alpha(if selected { 1.0 } else { 0.0 }),
                    ..default()
                },
                Selector(item),
            ),
            (
                text(
                    item.label(),
                    item_type(k, selected),
                    if selected { Color::WHITE } else { IDLE_INK }
                ),
                UiTransform::from_translation(Val2::px(shift, 0.0)),
                Tween::<OffsetX>::at_rest(shift, SHIFT_SECONDS, Ease::OutQuint),
                Label(item),
            ),
        ],
    )
}

/// Follows the selection: the selected item is white, bolder and shifted right with the fruit
/// beside it; the fruit steps between its two frames.
#[allow(clippy::type_complexity, reason = "one pass over the box's parts")]
fn looks(
    time: Res<Time>,
    menu: Res<Menu>,
    roots: Query<&Root>,
    mut selectors: Query<(&Selector, &mut ImageNode)>,
    mut labels: Query<(
        &Label,
        &mut TextColor,
        &mut crate::ui::shape::Ink,
        &mut TextFont,
        &mut Tween<OffsetX>,
    )>,
    mut clock: Local<f32>,
) {
    let Some(root) = roots.iter().next() else {
        *clock = 0.0;
        return;
    };
    *clock += time.delta_secs();
    let frame = (*clock / FRUIT_FRAME_SECONDS) as u32 % 2;
    let x = (frame * FRUIT.x) as f32;
    for (Selector(item), mut image) in &mut selectors {
        let alpha = if *item == menu.selected { 1.0 } else { 0.0 };
        if (image.color.alpha() - alpha).abs() > 1e-3 {
            image.color = Color::WHITE.with_alpha(alpha);
        }
        let want = Some(Rect::new(x, 0.0, x + FRUIT.x as f32, FRUIT.y as f32));
        if image.rect != want {
            image.rect = want;
        }
    }
    for (Label(item), mut color, mut ink, mut font, mut tween) in &mut labels {
        let on = *item == menu.selected;
        let ink_want = if on { Color::WHITE } else { IDLE_INK };
        if color.0 != ink_want {
            color.0 = ink_want;
            ink.0 = ink_want;
        }
        let weight = item_type(root.k, on).font().weight;
        if font.weight != weight {
            font.weight = weight;
        }
        tween.retarget(if on { SHIFT * root.k } else { 0.0 });
    }
}

/// The package's `cherry` sprite (its first two frames), else the placeholder.
fn fruit_image(files: &PackageFiles) -> SpriteImage {
    let path = iwbtg_format::sprites::sprite_pngs(files)
        .into_iter()
        .find(|(key, _)| key == "cherry" || key.ends_with("/cherry"))
        .map(|(_, path)| path);
    path.and_then(|p| files.get(&p).and_then(decode_png))
        .filter(|s| s.size.0 >= 2 * FRUIT.x && s.size.1 >= FRUIT.y)
        .map(|s| crop(&s, 2 * FRUIT.x, FRUIT.y))
        .unwrap_or_else(placeholder)
}

/// The top-left `w` x `h` of `s`.
fn crop(s: &SpriteImage, w: u32, h: u32) -> SpriteImage {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let start = ((y * s.size.0) * 4) as usize;
        rgba.extend_from_slice(&s.rgba[start..start + (w * 4) as usize]);
    }
    SpriteImage { size: (w, h), rgba }
}

/// A delicious fruit of our own (no game's art): a red cherry with a green stem, two frames
/// whose shine moves, 21 x 24 each.
fn placeholder() -> SpriteImage {
    let (w, h) = (2 * FRUIT.x, FRUIT.y);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let mut put = |x: i32, y: i32, c: [u8; 4]| {
        if (0..w as i32).contains(&x) && (0..h as i32).contains(&y) {
            let i = ((y as u32 * w + x as u32) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&c);
        }
    };
    const OUTLINE: [u8; 4] = [0x5a, 0x06, 0x12, 255];
    const RED: [u8; 4] = [0xe0, 0x1a, 0x2c, 255];
    const DEEP: [u8; 4] = [0xa8, 0x0c, 0x1e, 255];
    const SHINE: [u8; 4] = [0xff, 0xff, 0xff, 255];
    const STEM: [u8; 4] = [0x2f, 0x8a, 0x2a, 255];
    const LEAF: [u8; 4] = [0x5c, 0xc8, 0x3c, 255];
    for frame in 0..2 {
        let ox = frame * FRUIT.x as i32;
        let (cx, cy, r) = (10.0f32, 15.0f32, 7.5f32);
        for y in 0..FRUIT.y as i32 {
            for x in 0..FRUIT.x as i32 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                if d <= r - 1.0 {
                    // The lower right is in shadow.
                    let shade = (x as f32 - cx) + (y as f32 - cy) > 4.0;
                    put(ox + x, y, if shade { DEEP } else { RED });
                } else if d <= r {
                    put(ox + x, y, OUTLINE);
                }
            }
        }
        // The stem from the top of the fruit up and to the right, and a leaf.
        for (x, y) in [(10, 8), (11, 7), (11, 6), (12, 5), (12, 4), (13, 3), (13, 2)] {
            put(ox + x, y, STEM);
        }
        for (x, y) in [
            (14, 3),
            (15, 3),
            (15, 2),
            (16, 2),
            (14, 4),
            (15, 4),
            (16, 3),
            (17, 2),
        ] {
            put(ox + x, y, LEAF);
        }
        // The shine: two pixels top left, one row lower in the second frame.
        let sy = 11 + frame;
        put(ox + 7, sy, SHINE);
        put(ox + 7, sy + 1, SHINE);
        put(ox + 8, sy, SHINE);
    }
    SpriteImage { size: (w, h), rgba }
}

fn to_bevy(s: &SpriteImage) -> Image {
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

    #[test]
    fn the_selection_wraps_and_items_parse() {
        assert_eq!(Item::Continue.step(-1), Item::Quit);
        assert_eq!(Item::Quit.step(1), Item::Continue);
        assert_eq!(Item::RetryFromSave.step(1), Item::RestartRun);
        assert_eq!(parse_item("Restart"), Ok(Item::RestartRun));
        assert!(parse_item("menu").is_err());
    }

    #[test]
    fn the_placeholder_fruit_has_two_frames_that_differ() {
        let s = placeholder();
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
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo-clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut world = crate::view::test_world(&pkg);
        world.insert_resource(Menu {
            open: true,
            ..default()
        });
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

    /// A controller opens the menu with Start, moves with the D-pad and the stick, picks with A
    /// and continues with B or Start; Start never reaches the game (#624).
    #[test]
    fn a_controller_opens_navigates_picks_and_closes_the_menu() {
        use bevy::ecs::system::RunSystemOnce;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo-clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut world = crate::view::test_world(&pkg);
        world.init_resource::<ButtonInput<KeyCode>>();
        world.init_resource::<Menu>();
        let mut system = IntoSystem::into_system(keys);
        system.initialize(&mut world);
        // One frame: set the pad as the collector would see it, then run the menu's keys.
        let mut frame = |world: &mut World, down: &[PadInput]| {
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
        assert!(world.resource::<Menu>().open, "Start opens");
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
        assert!(!world.resource::<Menu>().open);
        let f = world.resource_mut::<Collector>().take_frame();
        assert_eq!(f.pressed, Action::Restart.bit(), "only Retry from save's restart");

        frame(&mut world, &[]);
        frame(&mut world, &[PAUSE_PAD]);
        assert!(world.resource::<Menu>().open);
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
        assert!(!world.resource::<Menu>().open);
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
    /// next tick; Quit asks the app to exit.
    #[test]
    fn picks_close_the_menu_and_do_their_item() {
        use bevy::ecs::system::RunSystemOnce;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo-clear.pkg");
        let pkg = crate::package::open(&dir, false).unwrap();
        let mut world = crate::view::test_world(&pkg);
        world.init_resource::<Messages<AppExit>>();
        world.insert_resource(Menu {
            open: true,
            picked: Some(Item::RetryFromSave),
            ..default()
        });
        // Down was pressed to reach the item: it must not reach the game.
        world
            .resource_mut::<Collector>()
            .press(iwbtg_sim::input::vk::DOWN);
        world.run_system_once(act).unwrap();
        assert!(!world.resource::<Menu>().open);
        let frame = world.resource_mut::<Collector>().take_frame();
        assert_eq!(frame.pressed, Action::Restart.bit());
        world.resource_mut::<Menu>().picked = Some(Item::Quit);
        world.run_system_once(act).unwrap();
        assert!(!world.resource::<Messages<AppExit>>().is_empty());
    }
}
