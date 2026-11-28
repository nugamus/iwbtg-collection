//! The mod select sheet (#497, `ModSelect.dc.html`): the Mods button in game select's footer
//! opens it. It slides up (OutQuint, 400 ms) over the dimmed select screen; its columns group
//! `iwbtg_sim::mods::MODS` by kind, and its header says whether a run with the picked set
//! reaches a board and which one (docs/product.md, Mods).
//!
//! The picked set is `mods::RunMods`, so the list's PBs and the loading card follow it. Back (or
//! Escape) puts back the set the sheet opened with; Done keeps the picks. The footer's
//! Auto-retry switch is the settings file's `[retry] auto` (#773).

use bevy::input_focus::{FocusCause, InputFocus};
use bevy::picking::events::PointerClick;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;
use iwbtg_sim::mods::{MODS, Mod, ModDef, ModKind, ModSet};

use super::AppScreen;
use super::library::{Library, file_name};
use super::list::Selection;
use crate::loading::{RunKind, Standing, standing};
use crate::mods::{BUILT, RunMods};
use crate::practice::{SLOW_SPEEDS, SlowSpeed};
use crate::ui::fonts::Type;
use crate::ui::motion::{Ease, OffsetY, Opacity, Tween};
use crate::ui::shape::{Shape, UiOpacity, Veil};
use crate::ui::tokens;
use crate::ui::widgets::{
    ButtonKind, Chip, ModCard, ModOption, SearchField, Switch, button_with, chip, mod_card, mod_card_off, pill, rich,
    selected_mod_pill, switch, text,
};

/// Where the sheet's top edge sits on the 1080 px page, and so how far it slides.
const SHEET_TOP: f32 = 300.0;
const SLIDE: f32 = 1080.0 - SHEET_TOP;

/// Whether the sheet is open, and the set it opened with (Back puts it back).
#[derive(Resource, Debug, Default)]
pub struct Sheet {
    pub open: bool,
    before: ModSet,
}

/// The whole overlay: hidden while the sheet is closed and slid away.
#[derive(Component)]
pub(super) struct Overlay;
/// The dim over game select.
#[derive(Component)]
pub(super) struct Dim;
/// The sheet itself, which slides.
#[derive(Component)]
pub(super) struct Panel;
/// The header's right side: the Ranked or Unranked pill and the board line.
#[derive(Component)]
pub(super) struct StandingBox;
/// The footer's selected mod pills.
#[derive(Component)]
pub(super) struct Chosen;
/// A card's mod.
#[derive(Component, Clone, Copy)]
pub(super) struct Card(pub(super) Mod);

impl Sheet {
    /// Opens the sheet over `mods` (`--mod-select-preview`).
    pub fn open_with(&mut self, mods: ModSet) {
        set_open(self, mods, true);
    }
}

/// The sheet's and the footer's buttons.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum SheetButton {
    /// Game select's Mods button: opens the sheet.
    Mods,
    Back,
    Deselect,
    Done,
}

/// The Mods button's value (`none`, `SD+NB`).
#[derive(Component)]
pub(super) struct ModsValue;

/// The Auto-retry switch.
#[derive(Component)]
pub(super) struct AutoRetry;

/// The column colour of a kind.
pub fn kind_color(kind: ModKind) -> Color {
    match kind {
        ModKind::Challenge => tokens::MOD_CHALLENGE,
        ModKind::Visual => tokens::MOD_VISUAL,
        ModKind::Practice => tokens::MOD_PRACTICE,
    }
}

fn kind_name(kind: ModKind) -> &'static str {
    match kind {
        ModKind::Challenge => "Challenge",
        ModKind::Visual => "Visual",
        ModKind::Practice => "Practice",
    }
}

/// The small capitals of the column heads and the footer's `Selected`: 13 px, 700, 0.14 em.
const CAPS: Type = Type {
    tracking: 0.14,
    caps: true,
    ..Type::barlow(13.0, 700)
};

/// The Mods button's value: `none` or the codes.
pub(super) fn button_value(set: ModSet) -> String {
    if set == ModSet::NONE {
        "none".to_owned()
    } else {
        set.codes()
    }
}

/// The header's pill and line for `set` on the picked map (`map`): Ranked and the board the run
/// counts for, or Unranked and why (the loading card's reason, `loading::standing`).
pub fn header(set: ModSet, slow_speed: u8, map: &str) -> (bool, String) {
    let run = RunKind {
        original: false,
        mods: set,
        slow_speed,
        practice: false,
    };
    match standing(run) {
        Standing::Unranked(why) => (false, why),
        Standing::Ranked(_) => {
            let mut board = map.to_owned();
            if set == ModSet::NONE {
                board.push_str(" · no mods");
            }
            for d in set.iter() {
                board.push_str(" + ");
                board.push_str(d.code);
            }
            (true, board)
        }
    }
}

/// Spawns the sheet, closed.
pub(super) fn spawn(commands: &mut Commands, camera: Entity, set: ModSet, slow_speed: u8, auto_retry: bool) {
    let flat = |fill: Color| Shape {
        shear: 0.0,
        radius: 0.0,
        ..Shape::fill(fill)
    };
    let glass = flat(Color::NONE)
        .with_veil(Veil {
            stops: [
                tokens::SHEET_TOP,
                tokens::SHEET_TOP.mix(&tokens::SHEET_BOTTOM, 0.5),
                tokens::SHEET_BOTTOM,
            ],
            mid: 0.5,
            vertical: true,
        })
        .with_shadow(-30.0, 80.0, tokens::black(0.5));
    let accent_line = flat(Color::NONE).with_veil(Veil::across(
        tokens::ACCENT,
        tokens::ACCENT.with_alpha(0.0),
        tokens::ACCENT.with_alpha(0.0),
        0.6,
    ));
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            GlobalZIndex(20),
            Visibility::Hidden,
            Overlay,
            UiTargetCamera(camera),
            DespawnOnExit(AppScreen::Select),
        ))
        .with_children(|o| {
            o.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                flat(tokens::SHEET_DIM),
                UiOpacity(0.0),
                Tween::<Opacity>::at_rest(0.0, tokens::motion::PANEL, Ease::OutQuint),
                Dim,
            ));
            o.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    right: Val::Px(0.0),
                    top: Val::Px(SHEET_TOP),
                    bottom: Val::Px(0.0),
                    ..default()
                },
                glass,
                UiTransform::from_translation(Val2::px(0.0, SLIDE)),
                Tween::<OffsetY>::at_rest(SLIDE, tokens::motion::PANEL, Ease::OutQuint),
                Panel,
            ))
            .with_children(|p| {
                p.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        right: Val::Px(0.0),
                        top: Val::Px(0.0),
                        height: Val::Px(3.0),
                        ..default()
                    },
                    accent_line,
                ));
                spawn_header(p);
                spawn_columns(p, set, slow_speed);
                spawn_footer(p, auto_retry);
            });
        });
}

fn spawn_header(p: &mut ChildSpawnerCommands) {
    p.spawn(Node {
        position_type: PositionType::Absolute,
        left: Val::Px(64.0),
        right: Val::Px(64.0),
        top: Val::Px(34.0),
        align_items: AlignItems::FlexEnd,
        justify_content: JustifyContent::SpaceBetween,
        ..default()
    })
    .with_children(|h| {
        h.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            },
            children![
                text("Mods", Type::condensed(54.0, 700).italic(), tokens::INK),
                text(
                    "Mods change how a run plays. Every combination has its own leaderboard.",
                    Type::barlow(16.0, 400),
                    tokens::SOFT
                ),
            ],
        ));
        h.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                row_gap: Val::Px(6.0),
                ..default()
            },
            StandingBox,
        ));
    });
}

fn spawn_columns(p: &mut ChildSpawnerCommands, set: ModSet, slow_speed: u8) {
    p.spawn(Node {
        position_type: PositionType::Absolute,
        left: Val::Px(54.0),
        right: Val::Px(64.0),
        top: Val::Px(150.0),
        display: Display::Grid,
        grid_template_columns: RepeatedGridTrack::flex(3, 1.0),
        column_gap: Val::Px(40.0),
        ..default()
    })
    .with_children(|g| {
        for kind in [ModKind::Challenge, ModKind::Visual, ModKind::Practice] {
            let color = kind_color(kind);
            g.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                // The artboard's 10 px card margin, for the head too.
                padding: UiRect::left(Val::Px(10.0)),
                min_width: Val::Px(0.0),
                ..default()
            })
            .with_children(|c| {
                c.spawn(Node {
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(10.0),
                    padding: UiRect::bottom(Val::Px(6.0)),
                    ..default()
                })
                .with_children(|head| {
                    head.spawn((
                        Node {
                            width: Val::Px(18.0),
                            height: Val::Px(4.0),
                            ..default()
                        },
                        Shape {
                            shear: 0.0,
                            radius: 2.0,
                            ..Shape::fill(color)
                        },
                    ));
                    head.spawn(text(kind_name(kind), CAPS, tokens::INK));
                    if kind == ModKind::Practice {
                        head.spawn(text("· never ranked", Type::barlow(13.0, 600), tokens::AMBER));
                    }
                });
                for d in MODS.iter().filter(|d| d.kind == kind) {
                    spawn_card(c, d, color, set.contains(d.id), slow_speed);
                }
                // Mirror shows faded until it is built (#1574).
                if kind == ModKind::Challenge {
                    c.spawn(mod_card_off("MR", "Mirror", "The rooms flipped left to right.", Some("Coming later")));
                }
            });
        }
    });
}

fn spawn_card(c: &mut ChildSpawnerCommands, d: &ModDef, color: Color, on: bool, slow_speed: u8) {
    if !BUILT.contains(&d.id) {
        c.spawn(mod_card_off(d.code, d.name, "Not built yet.", None));
        return;
    }
    let options: Vec<(String, bool)> = if d.id == Mod::SlowMotion {
        SLOW_SPEEDS
            .iter()
            .map(|s| (format!("{s}%"), *s == slow_speed))
            .collect()
    } else {
        Vec::new()
    };
    // Flashlight is new with the m5 sheet (D9).
    let tag = (d.id == Mod::Flashlight).then_some("New");
    c.spawn((
        mod_card(d.code, d.name, d.description, color, on, &options, tag),
        Card(d.id),
    ));
}

fn spawn_footer(p: &mut ChildSpawnerCommands, auto_retry: bool) {
    // The note over the footer (m5 D9).
    p.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(64.0),
            right: Val::Px(64.0),
            bottom: Val::Px(96.0),
            ..default()
        },
        children![text(
            "Mirror comes later (#1574): it needs sim work and a check of every room that reads the kid's x. Flashlight and the ghost are drawn by the client, so a replay plays the same with or without them.",
            Type::barlow(14.0, 400),
            tokens::MUTED
        )],
    ));
    p.spawn(Node {
        position_type: PositionType::Absolute,
        left: Val::Px(64.0),
        right: Val::Px(64.0),
        bottom: Val::Px(26.0),
        align_items: AlignItems::Center,
        column_gap: Val::Px(14.0),
        ..default()
    })
    .with_children(|f| {
        f.spawn((
            button_with(
                ButtonKind::Secondary,
                52.0,
                26.0,
                text("Back", Type::BUTTON, tokens::INK),
            ),
            SheetButton::Back,
        ));
        f.spawn((
            switch("Auto-retry", "Restart from the start after a death", auto_retry),
            AutoRetry,
        ));
        ghost_switch(f, crate::settings::get().ghost);
        f.spawn(Node {
            flex_grow: 1.0,
            ..default()
        });
        // `Selected` and the pills, 14 px apart; the artboard's 10 px margin before Deselect all.
        f.spawn((
            Node {
                align_items: AlignItems::Center,
                column_gap: Val::Px(14.0),
                margin: UiRect::right(Val::Px(10.0)),
                ..default()
            },
            children![text("Selected", CAPS, tokens::MUTED)],
            Chosen,
        ));
        f.spawn((
            button_with(
                ButtonKind::Secondary,
                52.0,
                22.0,
                text("Deselect all", Type::BUTTON, tokens::INK),
            ),
            SheetButton::Deselect,
        ));
        f.spawn((
            button_with(
                ButtonKind::Primary,
                52.0,
                30.0,
                text("Done", Type::PRIMARY, tokens::INK),
            ),
            SheetButton::Done,
        ));
    });
}

/// One of the Ghost switch's choices.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GhostChoice(pub(super) crate::settings::Ghost);

/// The Ghost switch (m5 D9, #64): Off, Your PB, Board #1. A ghost is drawn only, never ranked.
fn ghost_switch(f: &mut ChildSpawnerCommands, picked: crate::settings::Ghost) {
    use crate::settings::Ghost;
    f.spawn((
        Node {
            height: Val::Px(52.0),
            align_items: AlignItems::Center,
            column_gap: Val::Px(14.0),
            padding: UiRect::new(Val::Px(18.0), Val::Px(10.0), Val::Px(0.0), Val::Px(0.0)),
            ..default()
        },
        Shape::fill(tokens::white(0.06)),
    ))
    .with_children(|g| {
        g.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(1.0),
                margin: UiRect::right(Val::Px(14.0)),
                ..default()
            },
            children![
                text("Ghost", Type::barlow(15.0, 700), tokens::INK),
                text("drawn only, never ranked", Type::barlow(12.0, 400), tokens::MUTED),
            ],
        ));
        for (ghost, label) in [(Ghost::Off, "Off"), (Ghost::Pb, "Your PB"), (Ghost::Top, "Board #1")] {
            g.spawn((chip(label, ghost == picked), GhostChoice(ghost)));
        }
    });
}

/// A click on a ghost choice picks it and writes `[ghost] pick`.
pub(super) fn on_ghost(activate: On<Activate>, mut choices: Query<(&GhostChoice, &mut Chip)>) {
    let Ok((&GhostChoice(picked), _)) = choices.get(activate.entity) else {
        return;
    };
    crate::settings::update(|s| s.ghost = picked);
    for (choice, mut chip) in &mut choices {
        let on = choice.0 == picked;
        if chip.on != on {
            chip.on = on;
        }
    }
}

/// `set` with `m` flipped.
fn toggle(set: ModSet, m: Mod) -> ModSet {
    let bit = ModSet::NONE.with(m).bits();
    ModSet::from_bits(set.bits() ^ bit).unwrap_or(set)
}

/// Opens the sheet (keeping the set to put back) or closes it.
fn set_open(sheet: &mut Sheet, mods: ModSet, open: bool) {
    if open && !sheet.open {
        sheet.before = mods;
    }
    sheet.open = open;
}

/// The Mods button opens the sheet; a card flips its mod; Back puts back the set the sheet
/// opened with and closes it, Done closes it, Deselect all clears the set.
pub(super) fn on_activate(
    activate: On<Activate>,
    buttons: Query<&SheetButton>,
    cards: Query<&Card>,
    sheet: Option<ResMut<Sheet>>,
    mods: Option<ResMut<RunMods>>,
) {
    let (Some(mut sheet), Some(mut mods)) = (sheet, mods) else {
        return;
    };
    if let Ok(card) = cards.get(activate.entity) {
        let set = mods.0;
        mods.0 = toggle(set, card.0);
        return;
    }
    match buttons.get(activate.entity) {
        Ok(SheetButton::Mods) => set_open(&mut sheet, mods.0, true),
        Ok(SheetButton::Back) => {
            mods.0 = sheet.before;
            set_open(&mut sheet, mods.0, false);
        }
        Ok(SheetButton::Done) => set_open(&mut sheet, mods.0, false),
        Ok(SheetButton::Deselect) => mods.0 = ModSet::NONE,
        Err(_) => {}
    }
}

/// A click on a slow motion speed picks it (and does not flip the card under it) and writes
/// `[practice] slow_speed`, so the next start keeps it (#904).
pub(super) fn on_option(
    mut click: On<PointerClick>,
    options: Query<&ModOption>,
    speed: Option<ResMut<SlowSpeed>>,
) {
    let Ok(option) = options.get(click.entity) else {
        return;
    };
    click.propagate(false);
    if let (Some(mut speed), Some(&s)) = (speed, SLOW_SPEEDS.get(option.index)) {
        speed.0 = s;
        crate::settings::update(|settings| settings.slow_speed = s);
    }
}

/// Escape on the open sheet is Back.
pub(super) fn keys(keys: Res<ButtonInput<KeyCode>>, mut sheet: ResMut<Sheet>, mut mods: ResMut<RunMods>) {
    if sheet.open && keys.just_pressed(KeyCode::Escape) {
        mods.0 = sheet.before;
        set_open(&mut sheet, mods.0, false);
    }
}

/// Whether the sheet is closed: game select's own keys run only then.
pub(super) fn closed(sheet: Option<Res<Sheet>>) -> bool {
    sheet.is_none_or(|s| !s.open)
}

/// Slides the sheet in or out and fades the dim; hides the overlay once it is out. Opening
/// takes the keyboard focus from the search field, closing gives it back.
#[allow(clippy::type_complexity, reason = "one query per part")]
pub(super) fn slide(
    sheet: Res<Sheet>,
    mut overlay: Query<&mut Visibility, With<Overlay>>,
    mut dim: Query<&mut Tween<Opacity>, With<Dim>>,
    mut panel: Query<&mut Tween<OffsetY>, With<Panel>>,
    search: Query<Entity, With<SearchField>>,
    mut focus: ResMut<InputFocus>,
) {
    if sheet.is_changed() {
        for mut d in &mut dim {
            d.retarget(if sheet.open { 1.0 } else { 0.0 });
        }
        for mut p in &mut panel {
            p.retarget(if sheet.open { 0.0 } else { SLIDE });
        }
        if sheet.open {
            focus.clear();
        } else if let Ok(field) = search.single() {
            focus.set(field, FocusCause::Navigated);
        }
    }
    let gone = panel.iter().all(|p| p.finished() && p.to == SLIDE);
    let want = if sheet.open || !gone {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in &mut overlay {
        v.set_if_neq(want);
    }
}

/// The cards, the slow motion speeds, the header, the selected pills and the Mods button follow
/// the picked set, the speed and the picked map.
#[allow(clippy::too_many_arguments, reason = "every part that shows the set")]
pub(super) fn follow(
    mods: Res<RunMods>,
    speed: Res<SlowSpeed>,
    sel: Res<Selection>,
    lib: Res<Library>,
    mut cards: Query<(&Card, &mut ModCard)>,
    mut options: Query<&mut ModOption>,
    standing_box: Query<Entity, With<StandingBox>>,
    chosen: Query<Entity, With<Chosen>>,
    mut value_text: Query<&mut Text, With<ModsValue>>,
    mut commands: Commands,
) {
    if !mods.is_changed() && !speed.is_changed() && !sel.is_changed() && !lib.is_changed() {
        return;
    }
    let set = mods.0;
    for (card, mut look) in &mut cards {
        let on = set.contains(card.0);
        if look.on != on {
            look.on = on;
        }
    }
    for mut o in &mut options {
        let on = SLOW_SPEEDS.get(o.index) == Some(&speed.0);
        if o.on != on {
            o.on = on;
        }
    }
    let map = lib
        .games
        .get(sel.game)
        .map_or_else(String::new, |g| match g.maps.get(sel.map) {
            Some(m) => m.name.clone(),
            None => file_name(&g.path),
        });
    let (ranked, line) = header(set, speed.0, &map);
    // Each part is rebuilt only when what it shows changed (`ui::live`, #1685): moving through
    // the list keeps them unless the map's name or the standing changes.
    for e in &standing_box {
        crate::ui::live::queue_refresh(&mut commands, e, (ranked, line.clone()), |s, (ranked, line)| {
            let pill_style = Type {
                size: 13.0,
                ..Type::PILL_CAPS
            };
            let soft = Type::barlow(14.0, 400);
            if *ranked {
                s.spawn(pill(
                    "Ranked",
                    pill_style,
                    tokens::RANKED,
                    tokens::ON_RANKED,
                    30.0,
                    16.0,
                ));
                s.spawn(rich(vec![
                    ("Counts for ".to_owned(), soft, tokens::SOFT),
                    (line.clone(), Type::barlow(14.0, 700), Color::WHITE),
                ]));
            } else {
                s.spawn(pill(
                    "Unranked",
                    Type {
                        tracking: 0.1,
                        ..pill_style
                    },
                    tokens::AMBER,
                    tokens::ON_AMBER,
                    30.0,
                    16.0,
                ));
                s.spawn(text(line, soft, tokens::SOFT));
            }
        });
    }
    for e in &chosen {
        crate::ui::live::queue_refresh(&mut commands, e, set, |c, set| {
            c.spawn(text("Selected", CAPS, tokens::MUTED));
            if *set == ModSet::NONE {
                c.spawn(text("None", Type::barlow(15.0, 600), tokens::MUTED));
            }
            for d in set.iter() {
                c.spawn(selected_mod_pill(d.code, kind_color(d.kind)));
            }
        });
    }
    for mut t in &mut value_text {
        t.0 = button_value(set);
    }
}

/// The Auto-retry switch writes `[retry] auto` (read by the next run, `retry.rs`).
pub(super) fn auto_retry(switches: Query<&Switch, (With<AutoRetry>, Changed<Switch>)>) {
    for s in &switches {
        let on = s.on;
        crate::settings::update(|settings| settings.auto_retry = on);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_follows_the_ranked_rule() {
        let set = |codes: &str| ModSet::parse(codes).unwrap();
        assert_eq!(header(ModSet::NONE, 75, "Hard"), (true, "Hard · no mods".into()));
        assert_eq!(header(set("SD,NB"), 75, "Hard"), (true, "Hard + SD + NB".into()));
        let (ranked, why) = header(set("HB"), 75, "Hard");
        assert!(!ranked && why.starts_with("Hitboxes"), "a practice aid (#554): {why}");
        assert_eq!(header(set("FL"), 75, "Hard"), (true, "Hard + FL".into()));
        let (ranked, why) = header(set("SD,SL"), 50, "Hard");
        assert!(!ranked);
        assert!(why.starts_with("Slow motion 50% is a practice assist"), "{why}");
        let (ranked, why) = header(set("FA"), 75, "Hard");
        assert!(
            !ranked && why.starts_with("Frame advance is a practice assist"),
            "{why}"
        );
    }

    #[test]
    fn the_button_shows_the_codes_or_none() {
        assert_eq!(button_value(ModSet::NONE), "none");
        assert_eq!(button_value(ModSet::parse("NB,SD").unwrap()), "SD+NB");
    }

    #[test]
    fn back_puts_back_the_set_the_sheet_opened_with() {
        let mut sheet = Sheet::default();
        let sd = ModSet::parse("SD").unwrap();
        set_open(&mut sheet, sd, true);
        set_open(&mut sheet, ModSet::NONE, true);
        assert_eq!(sheet.before, sd, "opening again keeps the first set");
        set_open(&mut sheet, ModSet::NONE, false);
        assert!(!sheet.open);
        assert_eq!(
            toggle(toggle(sd, Mod::NoBackground), Mod::SuddenDeath).codes(),
            "NB"
        );
    }
}
