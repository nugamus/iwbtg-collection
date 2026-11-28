//! The settings overlay (m5 `C1_Settings`, `C2_SettingsAudio`, `C4_SettingsSearch`;
//! `Settings.dc.html`, #53): one place for the player's settings, over any screen outside a run.
//! The main menu's Settings and the toolbar's gear open it; Escape or a click on the dimmed screen
//! closes it.
//!
//! Left: one icon per section; a click jumps to it. The panel: the title, the search (typing
//! filters the rows at once; Backspace deletes) and the rows by section. Every change applies and
//! saves at once (`settings::update`); there is no Apply button. Keys: Up and Down walk the rows,
//! Left and Right change the value (or move along a row of choices), Enter or Space toggles or
//! presses, Tab jumps a section. The mouse clicks a control; the wheel scrolls.
//!
//! General ends with the updater's two rows (#1606, #1675; native only, the browser has no
//! updater): the channel, and Updates with the running build, the last check and a button that
//! checks now, or shows the update dialog when a build is ready.
//!
//! What a row changes takes effect at once where the client can (display mode, ratio, window
//! size, integer scale, backdrop blur, UI scale, the HUD elements); the language, the volumes and
//! the run's own settings (auto-retry, slow motion, death map, shake and flashes) apply to the next
//! start or the next run, and the row says so. Sections whose screens are not built yet (Online,
//! Streaming, the key bindings and the audio calibration) are left out or say where they are
//! tracked.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};
use bevy::window::PrimaryWindow;

use crate::locale;
use crate::settings::{self, Language, UpdateChannel};
use crate::ui::fonts::Type;
use crate::ui::icons::{Icon, icon};
use crate::ui::shape::Shape;
use crate::ui::tokens;
use crate::ui::widgets::text;

/// The sections, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    General,
    Graphics,
    Audio,
    Input,
    Gameplay,
}

impl Section {
    pub const ALL: [Section; 5] = [
        Section::General,
        Section::Graphics,
        Section::Audio,
        Section::Input,
        Section::Gameplay,
    ];

    fn title(self) -> &'static str {
        locale::text(match self {
            Section::General => "set_general",
            Section::Graphics => "set_graphics",
            Section::Audio => "set_audio",
            Section::Input => "set_input",
            Section::Gameplay => "set_gameplay",
        })
    }

    fn icon(self) -> Icon {
        match self {
            Section::General => Icon::SetGeneral,
            Section::Graphics => Icon::SetGraphics,
            Section::Audio => Icon::SetAudio,
            Section::Input => Icon::SetInput,
            Section::Gameplay => Icon::SetGameplay,
        }
    }
}

/// Every setting the overlay shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Id {
    Language,
    ProfileName,
    UpdateChannel,
    Updates,
    Display,
    Ratio,
    WindowSize,
    IntegerScale,
    BackdropBlur,
    UiScale,
    Master,
    Music,
    Sounds,
    AudioOffset,
    KeyBindings,
    HudElements,
    AutoRetry,
    SlowSpeed,
    DeathMap,
    Shake,
    Flashes,
}

/// How a row is changed.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Toggle(bool),
    /// One of the options is picked (`Some`), or several are on (`on`).
    Chips {
        options: Vec<String>,
        picked: Option<usize>,
        on: Vec<bool>,
    },
    /// A value as a share 0..1 and its text.
    Slider(f32, String),
    /// The current value, stepped with Left and Right.
    Select(String),
    /// A value and a button that is not built yet.
    Info(String),
    /// A status and a button that does something now (Updates: Check now).
    Action { value: String, button: String },
}

/// One row: what it is, its section, its texts and how it reads now.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: Id,
    pub section: Section,
    pub label: String,
    pub desc: String,
    pub kind: Kind,
}

impl Id {
    pub const ALL: [Id; 21] = [
        Id::Language,
        Id::ProfileName,
        Id::UpdateChannel,
        Id::Updates,
        Id::Display,
        Id::Ratio,
        Id::WindowSize,
        Id::IntegerScale,
        Id::BackdropBlur,
        Id::UiScale,
        Id::Master,
        Id::Music,
        Id::Sounds,
        Id::AudioOffset,
        Id::KeyBindings,
        Id::HudElements,
        Id::AutoRetry,
        Id::SlowSpeed,
        Id::DeathMap,
        Id::Shake,
        Id::Flashes,
    ];

    fn section(self) -> Section {
        match self {
            Id::Language | Id::ProfileName | Id::UpdateChannel | Id::Updates => Section::General,
            Id::Display
            | Id::Ratio
            | Id::WindowSize
            | Id::IntegerScale
            | Id::BackdropBlur
            | Id::UiScale => Section::Graphics,
            Id::Master | Id::Music | Id::Sounds | Id::AudioOffset => Section::Audio,
            Id::KeyBindings => Section::Input,
            Id::HudElements
            | Id::AutoRetry
            | Id::SlowSpeed
            | Id::DeathMap
            | Id::Shake
            | Id::Flashes => Section::Gameplay,
        }
    }

    /// The row's label and description keys.
    fn keys(self) -> (&'static str, &'static str) {
        match self {
            Id::Language => ("set_language", "set_language_desc"),
            Id::ProfileName => ("set_profile_name", "set_profile_name_desc"),
            Id::UpdateChannel => ("set_update_channel", "set_update_channel_desc"),
            Id::Updates => ("set_updates", ""),
            Id::Display => ("set_display", ""),
            Id::Ratio => ("set_ratio", "set_ratio_desc"),
            Id::WindowSize => ("set_window_size", "set_window_size_desc"),
            Id::IntegerScale => ("set_integer_scale", "set_integer_scale_desc"),
            Id::BackdropBlur => ("set_backdrop_blur", "set_backdrop_blur_desc"),
            Id::UiScale => ("set_ui_scale", "set_ui_scale_desc"),
            Id::Master => ("set_master", "set_volume_desc"),
            Id::Music => ("set_music", "set_volume_desc"),
            Id::Sounds => ("set_sounds", "set_volume_desc"),
            Id::AudioOffset => ("set_audio_offset", "set_audio_offset_desc"),
            Id::KeyBindings => ("set_key_bindings", "set_key_bindings_desc"),
            Id::HudElements => ("set_hud_elements", "set_hud_elements_desc"),
            Id::AutoRetry => ("set_auto_retry", "set_auto_retry_desc"),
            Id::SlowSpeed => ("set_slow_speed", "set_slow_speed_desc"),
            Id::DeathMap => ("set_death_map", "set_death_map_desc"),
            Id::Shake => ("set_shake", "set_shake_desc"),
            Id::Flashes => ("set_flashes", ""),
        }
    }

    /// The row as the settings read now.
    pub fn row(self, s: &settings::Settings) -> Row {
        let pct = |v: u32, max: u32| (v as f32 / max.max(1) as f32, format!("{v}%"));
        let names = |keys: &[&str]| keys.iter().map(|k| locale::text(k).to_owned()).collect::<Vec<_>>();
        let kind = match self {
            Id::Language => Kind::Select(
                match s.language {
                    Language::En => "English",
                    Language::Ja => "日本語",
                }
                .to_owned(),
            ),
            Id::ProfileName => Kind::Info(crate::card::profile_name()),
            Id::UpdateChannel => Kind::Chips {
                options: names(&["set_update_main", "set_update_releases"]),
                picked: UpdateChannel::ALL.iter().position(|c| *c == s.update_channel),
                on: Vec::new(),
            },
            // The status and the button follow the updater; `draw` fills them in.
            Id::Updates => Kind::Action {
                value: String::new(),
                button: locale::text("set_check_now").to_owned(),
            },
            Id::Display => Kind::Chips {
                options: names(&["set_windowed", "set_fullscreen"]),
                picked: Some(usize::from(s.fullscreen)),
                on: Vec::new(),
            },
            Id::Ratio => Kind::Chips {
                options: crate::screen::RATIOS
                    .iter()
                    .map(|r| r.map_or_else(|| locale::text("set_native").to_owned(), |(w, h)| format!("{w}:{h}")))
                    .collect(),
                picked: crate::screen::RATIOS.iter().position(|r| *r == s.ratio),
                on: Vec::new(),
            },
            Id::WindowSize => Kind::Select(s.window.map_or_else(
                || locale::text("set_default").to_owned(),
                |(w, h)| format!("{w} × {h}"),
            )),
            Id::IntegerScale => Kind::Toggle(s.integer_scale),
            Id::BackdropBlur => {
                let (f, t) = pct(s.backdrop_blur, settings::MAX_BACKDROP_BLUR);
                Kind::Slider(f, t)
            }
            Id::UiScale => {
                let r = crate::hud::layout::UI_SCALE_RANGE;
                let f = (s.ui_scale.saturating_sub(*r.start())) as f32 / (r.end() - r.start()) as f32;
                Kind::Slider(f, format!("{}%", s.ui_scale))
            }
            Id::Master => {
                let (f, t) = pct(s.master_volume, settings::MAX_VOLUME);
                Kind::Slider(f, t)
            }
            Id::Music => {
                let (f, t) = pct(s.music_volume, settings::MAX_VOLUME);
                Kind::Slider(f, t)
            }
            Id::Sounds => {
                let (f, t) = pct(s.sound_volume, settings::MAX_VOLUME);
                Kind::Slider(f, t)
            }
            Id::AudioOffset => Kind::Info(format!("+{} ms", s.audio_offset_ms)),
            Id::KeyBindings => Kind::Info(String::new()),
            Id::HudElements => Kind::Chips {
                options: names(&[
                    "set_hud_leaderboard",
                    "set_hud_timer",
                    "set_hud_deaths",
                    "set_hud_stage",
                    "set_hud_stage_deaths",
                    "set_hud_input",
                    "set_hud_mods",
                ]),
                picked: None,
                on: HUD_NAMES
                    .iter()
                    .map(|n| {
                        let mut show = s.hud_show;
                        show.get_mut(n).is_some_and(|b| *b)
                    })
                    .collect(),
            },
            Id::AutoRetry => Kind::Toggle(s.auto_retry),
            Id::SlowSpeed => Kind::Chips {
                options: crate::practice::SLOW_SPEEDS.iter().map(|v| format!("{v}%")).collect(),
                picked: crate::practice::SLOW_SPEEDS.iter().position(|v| *v == s.slow_speed),
                on: Vec::new(),
            },
            Id::DeathMap => Kind::Toggle(s.results_heatmap),
            Id::Shake => Kind::Toggle(s.shake),
            Id::Flashes => Kind::Toggle(s.flashes),
        };
        let (label, desc) = self.keys();
        Row {
            id: self,
            section: self.section(),
            label: locale::text(label).to_owned(),
            desc: if self == Id::Updates {
                crate::updater::running_line()
            } else if desc.is_empty() {
                String::new()
            } else {
                locale::text(desc).to_owned()
            },
            kind,
        }
    }

    /// Whether this build shows the row: the browser has no updater.
    fn available(self) -> bool {
        !(cfg!(target_arch = "wasm32") && matches!(self, Id::UpdateChannel | Id::Updates))
    }
}

/// The HUD elements the chips row switches, in its order.
const HUD_NAMES: [&str; 7] = [
    "leaderboard",
    "timer",
    "deaths",
    "stage",
    "stage_deaths",
    "input",
    "mods",
];

/// The rows that match `query` (every word in the label or the description; all with none).
pub fn rows(s: &settings::Settings, query: &str) -> Vec<Row> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    Id::ALL
        .iter()
        .filter(|id| id.available())
        .map(|id| id.row(s))
        .filter(|r| {
            let hay = format!("{} {} {}", r.label, r.desc, r.section.title()).to_lowercase();
            words.iter().all(|w| hay.contains(w))
        })
        .collect()
}

/// What a change does to a setting: `step` -1 or +1 (Left, Right), 0 to toggle or press; `chip`
/// the choice a click or Enter picked. Returns the new settings.
pub fn change(mut s: settings::Settings, id: Id, step: i32, chip: Option<usize>) -> settings::Settings {
    let nudge = |v: u32, by: i32, max: u32| v.saturating_add_signed(by).min(max);
    match id {
        Id::Language => {
            s.language = match s.language {
                Language::En => Language::Ja,
                Language::Ja => Language::En,
            }
        }
        Id::UpdateChannel => {
            let all = UpdateChannel::ALL;
            let at = all.iter().position(|c| *c == s.update_channel).unwrap_or(0);
            let i = chip.unwrap_or((at + 1) % all.len());
            s.update_channel = all[i.min(all.len() - 1)];
        }
        Id::Display => {
            s.fullscreen = match (chip, step) {
                (Some(c), _) => c == 1,
                (None, 0) => !s.fullscreen,
                (None, d) => d > 0,
            }
        }
        Id::Ratio => {
            let n = crate::screen::RATIOS.len();
            let at = crate::screen::RATIOS.iter().position(|r| *r == s.ratio).unwrap_or(0);
            let i = chip.unwrap_or_else(|| (at as i32 + step.signum().max(if step == 0 { 1 } else { -1 })).rem_euclid(n as i32) as usize);
            s.ratio = crate::screen::RATIOS[i.min(n - 1)];
        }
        Id::WindowSize => {
            let w = crate::screen::WINDOWS;
            let at = s.window.and_then(|x| w.iter().position(|p| *p == x));
            let next = match at {
                None => 0,
                Some(i) => (i as i32 + if step < 0 { -1 } else { 1 }).rem_euclid(w.len() as i32) as usize,
            };
            s.window = Some(w[next]);
        }
        Id::IntegerScale => s.integer_scale = !s.integer_scale,
        Id::BackdropBlur => {
            s.backdrop_blur = nudge(s.backdrop_blur, step * 10, settings::MAX_BACKDROP_BLUR);
        }
        Id::UiScale => {
            let r = crate::hud::layout::UI_SCALE_RANGE;
            s.ui_scale = s.ui_scale.saturating_add_signed(step * 5).clamp(*r.start(), *r.end());
        }
        Id::Master => s.master_volume = nudge(s.master_volume, step * 10, settings::MAX_VOLUME),
        Id::Music => s.music_volume = nudge(s.music_volume, step * 10, settings::MAX_VOLUME),
        Id::Sounds => s.sound_volume = nudge(s.sound_volume, step * 10, settings::MAX_VOLUME),
        Id::HudElements => {
            if let Some(c) = chip
                && let Some(name) = HUD_NAMES.get(c)
                && let Some(b) = s.hud_show.get_mut(name)
            {
                *b = !*b;
            }
        }
        Id::AutoRetry => s.auto_retry = !s.auto_retry,
        Id::SlowSpeed => {
            let speeds = crate::practice::SLOW_SPEEDS;
            let at = speeds.iter().position(|v| *v == s.slow_speed).unwrap_or(0);
            let i = chip.unwrap_or((at + 1) % speeds.len());
            s.slow_speed = speeds[i.min(speeds.len() - 1)];
        }
        Id::DeathMap => s.results_heatmap = !s.results_heatmap,
        Id::Shake => s.shake = !s.shake,
        Id::Flashes => s.flashes = !s.flashes,
        Id::ProfileName | Id::AudioOffset | Id::KeyBindings | Id::Updates => {}
    }
    s
}

/// The overlay's state.
#[derive(Resource, Debug, Default)]
pub struct Overlay {
    pub open: bool,
    /// The focused row among those shown.
    pub focus: usize,
    /// The choice the focus is on in a row of choices.
    pub chip: usize,
    pub query: String,
    /// Bumped by [`Overlay::refresh`]: the rows are read again.
    refreshes: u64,
    /// A change picked this frame: the row, the step and the choice.
    pending: Option<(Id, i32, Option<usize>)>,
    /// A section to jump to.
    jump: Option<Section>,
}

impl Overlay {
    /// Reads the rows again (the updater's state changed under the Updates row): only the rows
    /// whose content changed are rebuilt.
    pub fn refresh(&mut self) {
        self.refreshes = self.refreshes.wrapping_add(1);
    }

    /// Opens it (the main menu's Settings, the toolbar's gear).
    pub fn show(&mut self) {
        self.open = true;
        self.focus = 0;
        self.chip = 0;
        self.query.clear();
    }
}

/// Whether the overlay is closed (other screens' keys wait while it is open).
pub fn closed(o: Option<Res<Overlay>>) -> bool {
    !o.is_some_and(|o| o.open)
}

/// The toolbar's gear opens it.
fn on_gear(
    activate: On<Activate>,
    buttons: Query<&crate::ui::toolbar::ToolbarButton>,
    overlay: Option<ResMut<Overlay>>,
) {
    if let (Ok(crate::ui::toolbar::ToolbarButton::Settings), Some(mut o)) = (buttons.get(activate.entity), overlay)
        && !o.open
    {
        o.show();
    }
}

/// The overlay's flags (main.rs flattens each module's group, #530).
#[derive(clap::Args, Debug, Clone, Default)]
pub struct SettingsScreenArgs {
    /// Screenshot checks: open the settings overlay at once, searching for this text (`""` for
    /// no search).
    #[arg(long, hide = true, value_name = "QUERY")]
    pub settings_preview: Option<String>,
    /// Screenshot checks: draw this run in `en` or `ja`, not the settings file's language
    /// (not saved).
    #[arg(long, hide = true, value_name = "CODE")]
    pub language: Option<String>,
}

/// The module's place in the module list (main.rs).
pub struct Registration;

impl crate::registration::ClientModule for Registration {
    type Args = SettingsScreenArgs;

    fn prepare(args: &mut SettingsScreenArgs, _startup: &crate::registration::Startup) -> Result<(), String> {
        let Some(code) = &args.language else {
            return Ok(());
        };
        let language = crate::settings::Language::parse(code)
            .ok_or_else(|| format!("--language: `{code}` is not en or ja"))?;
        if !crate::locale::set_for_run(language) {
            return Err("--language: a string was read before the flag".to_owned());
        }
        Ok(())
    }

    fn add(app: &mut App, args: SettingsScreenArgs, _startup: &crate::registration::Startup) {
        let mut overlay = Overlay::default();
        if let Some(q) = args.settings_preview {
            overlay.show();
            overlay.query = q;
        }
        app.insert_resource(overlay)
            .add_observer(on_activate)
            .add_observer(on_gear)
            .add_systems(
                Update,
                (keys, wheel, act, draw)
                    .chain()
                    .run_if(|o: Res<Overlay>| o.open || o.is_changed()),
            );
    }
}

/// A clickable part: a row's control (with its choice), a section icon, or the dim to close.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Row(usize, Option<usize>),
    Section(Section),
    Close,
}

fn on_activate(activate: On<Activate>, hits: Query<&Hit>, overlay: Option<ResMut<Overlay>>) {
    let (Some(mut overlay), Ok(hit)) = (overlay, hits.get(activate.entity)) else {
        return;
    };
    if !overlay.open {
        return;
    }
    match *hit {
        Hit::Row(i, chip) => {
            overlay.focus = i;
            if let Some(c) = chip {
                overlay.chip = c;
            }
            let rows = rows(&settings::get(), &overlay.query);
            if let Some(r) = rows.get(i) {
                overlay.pending = Some((r.id, 0, chip));
            }
        }
        Hit::Section(s) => overlay.jump = Some(s),
        Hit::Close => overlay.open = false,
    }
}

/// Keys while it is open; typing goes to the search.
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut typed: MessageReader<KeyboardInput>,
    mut overlay: ResMut<Overlay>,
) {
    if !overlay.open {
        typed.clear();
        return;
    }
    let shown = rows(&settings::get(), &overlay.query);
    let n = shown.len();
    if keys.just_pressed(KeyCode::Escape) {
        if overlay.query.is_empty() {
            overlay.open = false;
        } else {
            overlay.query.clear();
            overlay.focus = 0;
        }
        typed.clear();
        return;
    }
    if keys.just_pressed(KeyCode::ArrowDown) && n > 0 {
        overlay.focus = (overlay.focus + 1) % n;
        overlay.chip = 0;
    }
    if keys.just_pressed(KeyCode::ArrowUp) && n > 0 {
        overlay.focus = (overlay.focus + n - 1) % n;
        overlay.chip = 0;
    }
    if keys.just_pressed(KeyCode::Tab) && n > 0 {
        let here = shown.get(overlay.focus).map(|r| r.section);
        if let Some(i) = shown
            .iter()
            .enumerate()
            .skip(overlay.focus + 1)
            .find(|(_, r)| Some(r.section) != here)
            .map(|(i, _)| i)
        {
            overlay.focus = i;
        } else {
            overlay.focus = 0;
        }
        overlay.chip = 0;
    }
    if let Some(row) = shown.get(overlay.focus.min(n.saturating_sub(1))) {
        let step = i32::from(keys.just_pressed(KeyCode::ArrowRight)) - i32::from(keys.just_pressed(KeyCode::ArrowLeft));
        let pick = keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]);
        match &row.kind {
            // A row of switches: Left and Right move along it, Enter switches the one picked.
            Kind::Chips { on, .. } if !on.is_empty() => {
                if step != 0 {
                    overlay.chip = (overlay.chip as i32 + step).rem_euclid(on.len() as i32) as usize;
                }
                if pick {
                    overlay.pending = Some((row.id, 0, Some(overlay.chip)));
                }
            }
            Kind::Chips { options, picked, .. } => {
                if step != 0 {
                    let at = picked.unwrap_or(0) as i32 + step;
                    let c = at.clamp(0, options.len() as i32 - 1) as usize;
                    overlay.pending = Some((row.id, step, Some(c)));
                }
            }
            Kind::Toggle(_) | Kind::Select(_) => {
                if step != 0 || pick {
                    overlay.pending = Some((row.id, if step == 0 { 1 } else { step }, None));
                }
            }
            Kind::Action { .. } => {
                if pick {
                    overlay.pending = Some((row.id, 0, None));
                }
            }
            Kind::Slider(..) => {
                if step != 0 {
                    overlay.pending = Some((row.id, step, None));
                }
            }
            Kind::Info(_) => {}
        }
    }
    // Typing filters; Space toggles a focused switch when nothing is typed.
    let mut query = overlay.query.clone();
    for ev in typed.read() {
        if !ev.state.is_pressed() {
            continue;
        }
        match &ev.logical_key {
            Key::Backspace => {
                query.pop();
            }
            Key::Space if query.is_empty() => {
                if let Some(row) = shown.get(overlay.focus)
                    && matches!(row.kind, Kind::Toggle(_))
                {
                    overlay.pending = Some((row.id, 1, None));
                }
            }
            Key::Space => query.push(' '),
            Key::Character(c) if !c.chars().any(char::is_control) => query.push_str(c),
            _ => {}
        }
    }
    if query != overlay.query {
        overlay.query = query;
        overlay.focus = 0;
        overlay.chip = 0;
    }
}

/// The wheel scrolls the rows.
fn wheel(
    mut wheel: MessageReader<MouseWheel>,
    overlay: Res<Overlay>,
    mut lists: Query<&mut ScrollPosition, With<List>>,
) {
    let dy: f32 = wheel.read().map(|w| w.y).sum();
    if !overlay.open || dy == 0.0 {
        return;
    }
    for mut pos in &mut lists {
        pos.y = (pos.y - dy * 60.0).max(0.0);
    }
}

/// Applies a change: saves it and sets what can change at once.
fn act(world: &mut World) {
    // Read first: taking from the overlay marks it changed, which makes `draw` read the rows.
    let quiet = {
        let o = world.resource::<Overlay>();
        o.pending.is_none() && o.jump.is_none()
    };
    if quiet {
        return;
    }
    let (pending, jump) = {
        let mut o = world.resource_mut::<Overlay>();
        (o.pending.take(), o.jump.take())
    };
    if let Some(section) = jump {
        let shown = rows(&settings::get(), &world.resource::<Overlay>().query);
        if let Some(i) = shown.iter().position(|r| r.section == section) {
            let mut o = world.resource_mut::<Overlay>();
            o.focus = i;
            o.chip = 0;
        }
    }
    let Some((id, step, chip)) = pending else {
        return;
    };
    let new = change(settings::get(), id, step, chip);
    settings::update(|s| *s = new.clone());
    apply_live(world, &new, id);
}

/// Sets what a setting changes in the running client.
fn apply_live(world: &mut World, s: &settings::Settings, id: Id) {
    match id {
        Id::Display | Id::Ratio | Id::WindowSize => {
            if let Some(mut screen) = world.get_resource_mut::<crate::screen::Screen>() {
                screen.set(s);
            }
            if id == Id::WindowSize
                && let Some((w, h)) = s.window
                && let Some(mut window) = world
                    .query_filtered::<&mut Window, With<PrimaryWindow>>()
                    .iter_mut(world)
                    .next()
            {
                window.resolution.set_physical_resolution(w, h);
            }
        }
        Id::IntegerScale | Id::UiScale => {
            if let Some(mut hud) = world.get_resource_mut::<crate::hud::HudOptions>() {
                hud.0.integer = s.integer_scale;
                hud.0.ui_scale = s.ui_scale as f32 / 100.0;
            }
        }
        Id::BackdropBlur => {
            if let Some(mut b) = world.get_resource_mut::<crate::ui::backdrop::Backdrop>() {
                b.blur = s.backdrop_blur as f32 / 100.0;
            }
        }
        Id::HudElements => {
            if let Some(mut shown) = world.get_resource_mut::<crate::hud::Shown>() {
                shown.0 = s.hud_show;
            }
        }
        // A new channel checks at once; the Updates button checks or shows the ready build.
        Id::UpdateChannel => crate::updater::check_now(world),
        Id::Updates => crate::updater::press(world),
        _ => {}
    }
}

/// What the overlay's tree is built from. Only a change here builds it again (a search, the
/// language, the mode's accent); a focus move or a value change updates the tree in place
/// (`ui::focus`, `ui::live`, #1681).
#[derive(Clone, Debug, PartialEq)]
struct Frame {
    query: String,
    ids: Vec<Id>,
    language: Language,
    accent: Color,
}

/// The overlay's root and its scrolling list.
#[derive(Component)]
struct Root(Frame);
#[derive(Component)]
struct List;
/// A row's node, for scrolling it into view and rebuilding its content.
#[derive(Component)]
struct RowNode(usize);

/// Where the keyboard focus shows: a row (or one choice in a row of switches), or a section's
/// icon in the strip.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum Spot {
    Row(usize, Option<usize>),
    Section(Section),
}

/// The panel's width and the strip's.
const STRIP: f32 = 72.0;
const PANEL: f32 = 660.0;
/// `#08090d`, `#10121a`.
const STRIP_FILL: Color = Color::srgb_u8(0x08, 0x09, 0x0d);
const PANEL_FILL: Color = Color::srgb_u8(0x10, 0x12, 0x1a);
/// The section icon at rest.
const STRIP_ICON: Color = Color::srgb_u8(0x8d, 0x93, 0xa8);

/// The rows as shown now: the settings, and the updater under the Updates row.
fn shown_rows(world: &World, query: &str) -> Vec<Row> {
    let mut shown = rows(&settings::get(), query);
    // The Updates row reads the updater, which `rows` cannot see.
    if let Some(updater) = world.get_resource::<crate::updater::Updater>() {
        let ready = matches!(updater.phase, crate::updater::Phase::Ready(_));
        for row in shown.iter_mut().filter(|r| r.id == Id::Updates) {
            row.kind = Kind::Action {
                value: crate::updater::status_text(updater),
                button: locale::text(if ready { "set_update_show" } else { "set_check_now" }).to_owned(),
            };
        }
    }
    shown
}

/// Keeps the overlay's tree in step with the overlay: builds it when it opens or its frame
/// changes, rebuilds a row's content when that row changed, moves the focus looks, and takes it
/// down when it closes.
fn draw(world: &mut World, mut seen: Local<Option<settings::Settings>>) {
    let open = world.resource::<Overlay>().open;
    let roots: Vec<(Entity, Frame)> = world.query::<(Entity, &Root)>().iter(world).map(|(e, r)| (e, r.0.clone())).collect();
    if !open {
        for (e, _) in roots {
            world.entity_mut(e).despawn();
        }
        return;
    }
    // A setting can also change while the overlay is open from outside it (F11, F9, `[` and
    // `]` in `screen.rs`): the rows are read again then too.
    let now = settings::get();
    let settings_changed = seen.as_ref() != Some(&now);
    *seen = Some(now);
    if !roots.is_empty() && !settings_changed && !world.is_resource_changed::<Overlay>() {
        scroll_into_view(world);
        return;
    }
    let (focus, chip, query) = {
        let o = world.resource::<Overlay>();
        (o.focus, o.chip, o.query.clone())
    };
    let shown = shown_rows(world, &query);
    let accent = crate::ui::mode::accent(world.get_resource::<crate::ui::mode::Mode>());
    let frame = Frame {
        query: query.clone(),
        ids: shown.iter().map(|r| r.id).collect(),
        language: settings::get().language,
        accent,
    };
    let current = roots.iter().any(|(_, f)| *f == frame);
    if !current {
        let keep_scroll = world
            .query_filtered::<&ScrollPosition, With<List>>()
            .iter(world)
            .next()
            .map(|p| p.y);
        for (e, _) in roots {
            world.entity_mut(e).despawn();
        }
        build(world, frame, &shown, keep_scroll);
    }
    // Each row's content follows its data; an unchanged row is left alone.
    let row_nodes: Vec<(Entity, usize)> = world.query::<(Entity, &RowNode)>().iter(world).map(|(e, r)| (e, r.0)).collect();
    for (e, i) in row_nodes {
        if let Some(row) = shown.get(i) {
            crate::ui::live::refresh(world, e, row, |c, row| row_content(c, i, row, accent));
        }
    }
    // The focus looks: a few component writes, never a respawn.
    let active = shown.get(focus).map(|r| r.section);
    let mut spots = world.query::<(&Spot, &mut crate::ui::focus::Focused)>();
    for (spot, mut focused) in spots.iter_mut(world) {
        let on = match *spot {
            Spot::Row(i, None) => i == focus,
            Spot::Row(i, Some(j)) => i == focus && j == chip,
            Spot::Section(s) => active == Some(s),
        };
        crate::ui::focus::set(&mut focused, on);
    }
    scroll_into_view(world);
}

/// Builds the overlay's tree; the rows' content is filled in by `draw`.
fn build(world: &mut World, frame: Frame, shown: &[Row], keep_scroll: Option<f32>) {
    let (query, accent) = (frame.query.clone(), frame.accent);
    let sections: Vec<(usize, Row)> = shown.iter().cloned().enumerate().collect();
    let found = shown.len();
    world.spawn((
        Root(frame),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        GlobalZIndex(60),
        Children::spawn(SpawnWith(move |c: &mut ChildSpawner| {
            // The screen behind, dimmed: a click on it closes.
            c.spawn((
                Button,
                Hit::Close,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(tokens::dark(Color::srgb_u8(5, 6, 10), 0.55)),
            ));
            strip(c, accent);
            panel(c, &sections, found, &query, accent, keep_scroll);
        })),
    ));
}

fn strip(c: &mut ChildSpawner, accent: Color) {
    use crate::ui::focus::{FocusFill, FocusInk, Focused};
    c.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            width: Val::Px(STRIP),
            flex_direction: FlexDirection::Column,
            padding: UiRect::top(Val::Px(18.0)),
            row_gap: Val::Px(4.0),
            ..default()
        },
        BackgroundColor(STRIP_FILL),
    ))
    .with_children(|c| {
        for s in Section::ALL {
            c.spawn((
                Button,
                Hit::Section(s),
                Spot::Section(s),
                Focused(false),
                FocusFill {
                    rest: Color::NONE,
                    focus: tokens::white(0.06),
                },
                Node {
                    height: Val::Px(60.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|c| {
                c.spawn((
                    Spot::Section(s),
                    Focused(false),
                    FocusFill {
                        rest: Color::NONE,
                        focus: accent,
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(10.0),
                        bottom: Val::Px(10.0),
                        width: Val::Px(4.0),
                        border_radius: BorderRadius::right(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ));
                c.spawn((
                    icon(s.icon(), 26.0, STRIP_ICON),
                    Spot::Section(s),
                    Focused(false),
                    FocusInk {
                        rest: STRIP_ICON,
                        focus: Color::WHITE,
                    },
                ));
            });
        }
    });
}

fn panel(c: &mut ChildSpawner, shown: &[(usize, Row)], found: usize, query: &str, accent: Color, keep_scroll: Option<f32>) {
    c.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(STRIP),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            width: Val::Px(PANEL),
            flex_direction: FlexDirection::Column,
            ..default()
        },
        BackgroundColor(PANEL_FILL),
        BoxShadow::new(tokens::black(0.45), Val::Px(20.0), Val::ZERO, Val::ZERO, Val::Px(60.0)),
    ))
    .with_children(|c| {
        c.spawn(Node {
            padding: UiRect::new(Val::Px(36.0), Val::Px(36.0), Val::Px(34.0), Val::Px(0.0)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|c| {
            c.spawn(text(locale::text("set_title"), Type::barlow(30.0, 700), tokens::INK));
            c.spawn(text(locale::text("set_sub"), Type::barlow(15.0, 400), tokens::MUTED));
        });
        // The search.
        c.spawn(Node {
            padding: UiRect::new(Val::Px(36.0), Val::Px(36.0), Val::Px(18.0), Val::Px(8.0)),
            ..default()
        })
        .with_child((
            Node {
                flex_grow: 1.0,
                height: Val::Px(46.0),
                align_items: AlignItems::Center,
                column_gap: Val::Px(12.0),
                padding: UiRect::horizontal(Val::Px(18.0)),
                ..default()
            },
            if query.is_empty() {
                Shape::fill(tokens::white(0.07))
            } else {
                Shape::fill(tokens::white(0.07)).with_ring(2.0, tokens::white(0.6))
            },
            Children::spawn(SpawnWith({
                let query = query.to_owned();
                move |c: &mut ChildSpawner| {
                    c.spawn(icon(Icon::Search, 18.0, tokens::MUTED));
                    let (t, ink) = if query.is_empty() {
                        (locale::text("set_search").to_owned(), tokens::MUTED)
                    } else {
                        (format!("{query}|"), tokens::INK)
                    };
                    c.spawn((
                        Node {
                            flex_grow: 1.0,
                            ..default()
                        },
                        children![text(&t, Type::barlow(16.0, 400), ink)],
                    ));
                    if !query.is_empty() {
                        c.spawn(text(
                            &locale::format("set_found", &[("n", &found)]),
                            Type::barlow(13.0, 400),
                            tokens::MUTED,
                        ));
                    }
                }
            })),
        ));
        // The rows, by section.
        let mut list = c.spawn((
            List,
            Node {
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                padding: UiRect::new(Val::Px(36.0), Val::Px(36.0), Val::Px(8.0), Val::Px(40.0)),
                overflow: Overflow::scroll_y(),
                min_height: Val::Px(0.0),
                ..default()
            },
            ScrollPosition(Vec2::new(0.0, keep_scroll.unwrap_or(0.0))),
        ));
        list.with_children(|c| {
            let mut last = None;
            for (i, row) in shown {
                if last != Some(row.section) {
                    last = Some(row.section);
                    c.spawn(Node {
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(10.0),
                        padding: UiRect::new(Val::Px(0.0), Val::Px(0.0), Val::Px(18.0), Val::Px(8.0)),
                        flex_shrink: 0.0,
                        ..default()
                    })
                    .with_children(|c| {
                        c.spawn((
                            Node {
                                width: Val::Px(4.0),
                                height: Val::Px(20.0),
                                border_radius: BorderRadius::all(Val::Px(2.0)),
                                ..default()
                            },
                            BackgroundColor(accent),
                        ));
                        c.spawn(text(row.section.title(), Type::barlow(20.0, 700), tokens::INK));
                    });
                }
                row_node(c, *i, row, !query.is_empty(), accent);
            }
            if shown.is_empty() {
                c.spawn(text(locale::text("set_nothing"), Type::barlow(15.0, 400), tokens::MUTED));
            }
        });
    });
}

/// Whether a row lays its choices out under its label (more than three choices).
fn wide(row: &Row) -> bool {
    matches!(&row.kind, Kind::Chips { options, .. } if options.len() > 3)
}

/// A row's box: its size never depends on the focus. The focus fills it and rings it from
/// outside (an outline, not a border), so no row moves when the focus does.
fn row_node(c: &mut ChildSpawner, i: usize, row: &Row, hit: bool, accent: Color) {
    let wide = wide(row);
    let rest = if hit { accent.with_alpha(0.08) } else { Color::NONE };
    c.spawn((
        RowNode(i),
        Button,
        Hit::Row(i, None),
        Spot::Row(i, None),
        crate::ui::focus::FocusFill {
            rest,
            focus: tokens::white(0.06),
        },
        crate::ui::focus::ring(2.0, tokens::white(0.5)),
        Node {
            min_height: Val::Px(58.0),
            padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
            margin: UiRect::horizontal(Val::Px(-14.0)),
            border_radius: BorderRadius::all(Val::Px(10.0)),
            flex_direction: if wide { FlexDirection::Column } else { FlexDirection::Row },
            align_items: if wide { AlignItems::Stretch } else { AlignItems::Center },
            column_gap: Val::Px(18.0),
            row_gap: Val::Px(10.0),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(rest),
    ));
}

/// A row's content: its label, its description and its control, built from the row's data.
fn row_content(c: &mut ChildSpawner, i: usize, row: &Row, accent: Color) {
    c.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(3.0),
        flex_grow: 1.0,
        flex_shrink: 1.0,
        min_width: Val::Px(0.0),
        ..default()
    })
    .with_children(|c| {
        c.spawn(text(&row.label, Type::barlow(16.0, 600), tokens::INK));
        if !row.desc.is_empty() {
            c.spawn(text(&row.desc, Type::barlow(13.0, 400), tokens::MUTED));
        }
    });
    control(c, i, row, accent, wide(row));
}

fn control(c: &mut ChildSpawner, i: usize, row: &Row, accent: Color, wide: bool) {
    let mut box_ = c.spawn(Node {
        align_items: AlignItems::Center,
        justify_content: if wide { JustifyContent::FlexStart } else { JustifyContent::FlexEnd },
        column_gap: Val::Px(6.0),
        row_gap: Val::Px(6.0),
        flex_wrap: FlexWrap::Wrap,
        flex_shrink: 0.0,
        ..default()
    });
    box_.with_children(|c| match &row.kind {
        Kind::Toggle(on) => {
            c.spawn((
                Node {
                    width: Val::Px(42.0),
                    height: Val::Px(24.0),
                    border_radius: BorderRadius::all(Val::Px(12.0)),
                    ..default()
                },
                BackgroundColor(if *on { accent } else { tokens::white(0.2) }),
            ))
            .with_child((
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(3.0),
                    left: Val::Px(if *on { 21.0 } else { 3.0 }),
                    width: Val::Px(18.0),
                    height: Val::Px(18.0),
                    border_radius: BorderRadius::all(Val::Px(9.0)),
                    ..default()
                },
                BackgroundColor(if *on { Color::WHITE } else { tokens::SOFT }),
            ));
        }
        Kind::Chips { options, picked, on } => {
            for (j, o) in options.iter().enumerate() {
                let lit = picked.map_or_else(|| on.get(j).copied().unwrap_or(false), |p| p == j);
                let shape = Shape::fill(if lit { tokens::over(accent, 0.26, PANEL_FILL) } else { tokens::white(0.07) });
                let mut chip = c.spawn((
                    Button,
                    Hit::Row(i, Some(j)),
                    crate::ui::widgets::sheared(34.0, 11.0, shape),
                    children![text(
                        o,
                        Type::barlow(14.0, if lit { 700 } else { 500 }),
                        if lit { Color::WHITE } else { tokens::MUTED }
                    )],
                ));
                // A row of switches has a cursor on one switch; a row of choices has none.
                if !on.is_empty() {
                    chip.insert((Spot::Row(i, Some(j)), crate::ui::focus::shape_ring(2.0, tokens::white(0.6))));
                }
            }
        }
        Kind::Slider(f, value) => {
            c.spawn((
                Node {
                    width: Val::Px(170.0),
                    height: Val::Px(8.0),
                    ..default()
                },
                Shape {
                    radius: 4.0,
                    ..Shape::fill(tokens::white(0.12))
                },
            ))
            .with_children(|c| {
                c.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        bottom: Val::Px(0.0),
                        width: Val::Percent(100.0 * f.clamp(0.0, 1.0)),
                        ..default()
                    },
                    Shape {
                        radius: 4.0,
                        ..Shape::fill(accent)
                    },
                ));
                c.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(-6.0),
                        left: Val::Percent(100.0 * f.clamp(0.0, 1.0)),
                        margin: UiRect::left(Val::Px(-6.0)),
                        width: Val::Px(12.0),
                        height: Val::Px(20.0),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::WHITE),
                ));
            });
            c.spawn((
                Node {
                    width: Val::Px(62.0),
                    justify_content: JustifyContent::FlexEnd,
                    ..default()
                },
                children![text(value, Type::barlow(15.0, 600), tokens::INK)],
            ));
        }
        Kind::Select(value) => {
            c.spawn((
                crate::ui::widgets::sheared(38.0, 16.0, Shape::fill(tokens::white(0.09))),
                children![
                    text(value, Type::barlow(15.0, 600), tokens::INK),
                    text("‹ ›", Type::barlow(15.0, 600), tokens::MUTED),
                ],
            ));
        }
        Kind::Info(value) => {
            if !value.is_empty() {
                c.spawn(text(value, Type::barlow(15.0, 600), tokens::INK));
            }
        }
        Kind::Action { value, button } => {
            if !value.is_empty() {
                c.spawn((
                    Node {
                        max_width: Val::Px(230.0),
                        ..default()
                    },
                    children![text(value, Type::barlow(14.0, 500), tokens::SOFT)],
                ));
            }
            c.spawn((
                Button,
                Hit::Row(i, Some(0)),
                Spot::Row(i, None),
                crate::ui::focus::shape_ring(2.0, tokens::white(0.6)),
                crate::ui::widgets::sheared(36.0, 16.0, Shape::fill(tokens::white(0.12))),
                children![text(button, Type::barlow(15.0, 700), tokens::INK)],
            ));
        }
    });
}

/// Keeps the focused row in the list's view.
fn scroll_into_view(world: &mut World) {
    let focus = world.resource::<Overlay>().focus;
    let mut list = world.query_filtered::<(&ComputedNode, &UiGlobalTransform, &mut ScrollPosition), With<List>>();
    let Some((list_node, list_at, _)) = list.iter(world).next() else {
        return;
    };
    let (lh, ly, scale) = (list_node.size().y, list_at.translation.y, list_node.inverse_scale_factor());
    let rows: Vec<(f32, f32)> = world
        .query::<(&RowNode, &ComputedNode, &UiGlobalTransform)>()
        .iter(world)
        .filter(|(r, ..)| r.0 == focus)
        .map(|(_, n, t)| (t.translation.y, n.size().y))
        .collect();
    let Some(&(ry, rh)) = rows.first() else {
        return;
    };
    // Centres in physical px: the row's top and bottom against the list's.
    let (top, bottom) = (ry - rh / 2.0, ry + rh / 2.0);
    let (ltop, lbottom) = (ly - lh / 2.0 + 60.0, ly + lh / 2.0 - 60.0);
    let delta = if top < ltop {
        top - ltop
    } else if bottom > lbottom {
        bottom - lbottom
    } else {
        return;
    };
    if let Some((_, _, mut pos)) = list.iter_mut(world).next() {
        pos.y = (pos.y + delta * scale).max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_search_keeps_matching_rows() {
        let s = settings::Settings::default();
        assert_eq!(rows(&s, "").len(), Id::ALL.len());
        let scale = rows(&s, "scale");
        assert!(scale.iter().any(|r| r.id == Id::IntegerScale) && scale.iter().any(|r| r.id == Id::UiScale));
        assert!(scale.iter().all(|r| r.id != Id::Master));
        assert!(rows(&s, "zzzz").is_empty());
        let updates = rows(&s, "update");
        assert!(updates.iter().any(|r| r.id == Id::UpdateChannel) && updates.iter().any(|r| r.id == Id::Updates));
        assert!(updates.iter().all(|r| r.section == Section::General));
    }

    #[test]
    fn changes_step_and_clamp() {
        let s = settings::Settings::default();
        let s = change(s, Id::Master, 1, None);
        assert_eq!(s.master_volume, settings::MAX_VOLUME, "already at the top");
        let s = change(s, Id::Master, -1, None);
        assert_eq!(s.master_volume, settings::MAX_VOLUME - 10);
        let s = change(s, Id::IntegerScale, 0, None);
        assert!(s.integer_scale);
        let s = change(s, Id::Display, 0, Some(1));
        assert!(s.fullscreen);
        let s = change(s, Id::Ratio, 0, Some(2));
        assert_eq!(s.ratio, crate::screen::RATIOS[2]);
        let before = s.hud_show.timer;
        let s = change(s, Id::HudElements, 0, Some(1));
        assert_eq!(s.hud_show.timer, !before);
        let s = change(s, Id::UiScale, -1, None);
        assert_eq!(s.ui_scale, 95);
        let s = change(s, Id::SlowSpeed, 1, Some(1));
        assert_eq!(s.slow_speed, crate::practice::SLOW_SPEEDS[1]);
        let s = change(s, Id::UpdateChannel, 0, Some(1));
        assert_eq!(s.update_channel, UpdateChannel::Releases);
        let s = change(s, Id::UpdateChannel, 1, None);
        assert_eq!(s.update_channel, UpdateChannel::MainBuilds, "Enter steps round");
        assert_eq!(change(s.clone(), Id::Updates, 0, None), s, "the button changes no setting");
    }

    /// The overlay's systems in a bare app, opened.
    fn open_app() -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .add_message::<KeyboardInput>()
            .add_message::<MouseWheel>()
            .insert_resource(Overlay::default())
            .add_systems(Update, ((keys, wheel, act, draw).chain(), crate::ui::focus::look).chain());
        app.world_mut().resource_mut::<Overlay>().show();
        app.update();
        app
    }

    fn press(app: &mut App, key: KeyCode) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
        app.update();
    }

    /// Every row's entity and its layout, in order.
    fn row_boxes(app: &mut App) -> Vec<(usize, Entity, Node)> {
        let mut q = app.world_mut().query::<(Entity, &RowNode, &Node)>();
        let mut v: Vec<_> = q.iter(app.world()).map(|(e, r, n)| (r.0, e, n.clone())).collect();
        v.sort_by_key(|(i, ..)| *i);
        v
    }

    fn focused_rows(app: &mut App) -> Vec<usize> {
        let mut q = app.world_mut().query::<(&RowNode, &crate::ui::focus::Focused)>();
        let mut v: Vec<usize> = q.iter(app.world()).filter(|(_, f)| f.0).map(|(r, _)| r.0).collect();
        v.sort_unstable();
        v
    }

    #[test]
    fn moving_the_focus_rebuilds_nothing_and_moves_nothing() {
        let mut app = open_app();
        let before = row_boxes(&mut app);
        assert_eq!(before.len(), rows(&settings::get(), "").len());
        assert_eq!(focused_rows(&mut app), vec![0]);
        let root = app.world_mut().query_filtered::<Entity, With<Root>>().single(app.world()).unwrap();
        for (key, want) in [(KeyCode::ArrowDown, 1), (KeyCode::ArrowDown, 2), (KeyCode::ArrowUp, 1)] {
            press(&mut app, key);
            assert_eq!(focused_rows(&mut app), vec![want], "{key:?}");
            assert_eq!(row_boxes(&mut app), before, "the same rows, laid out the same");
        }
        assert!(app.world().get_entity(root).is_ok(), "the overlay was not rebuilt");
        let row = before[1].1;
        assert_eq!(app.world().get::<Outline>(row).unwrap().color, tokens::white(0.5), "the ring shows");
        assert_eq!(app.world().get::<Outline>(before[0].1).unwrap().color, Color::NONE);
    }

    #[test]
    fn a_search_builds_the_list_again_and_escape_restores_it() {
        let mut app = open_app();
        let all = row_boxes(&mut app).len();
        let mut typed = app.world_mut().resource_mut::<Messages<KeyboardInput>>();
        typed.write(KeyboardInput {
            key_code: KeyCode::KeyZ,
            logical_key: Key::Character("zzzz".into()),
            state: bevy::input::ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
        app.update();
        assert!(row_boxes(&mut app).is_empty(), "nothing matches");
        press(&mut app, KeyCode::Escape);
        assert_eq!(row_boxes(&mut app).len(), all);
        assert_eq!(app.world_mut().query::<&Root>().iter(app.world()).count(), 1, "one overlay");
    }
}
