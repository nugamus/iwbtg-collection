//! The kid's physics profiles, `physics.toml` and the room profiles `physics/<id>.toml`
//! (docs/format/physics.md).

use super::*;
use iwbtg_sim::def::physics::KidScriptSlot;
use iwbtg_sim::kid::{KidAction, KidCollisionRun};
use iwbtg_sim::script::validate::{BOOL_PROPS, REAL_PROPS};

/// True when `name` is an instance property a script resolves before the custom-variable
/// indexer: the numeric and flag property tables of `script::validate` (which include every kid
/// field of decision 0004) plus the string getter `kind`. A `[script] vars` name here is
/// unreachable as `this.<name>`.
fn is_reserved_instance_field(name: &str) -> bool {
    name == "kind" || REAL_PROPS.contains(&name) || BOOL_PROPS.contains(&name)
}

impl<'a> Loader<'a> {
    /// Resolves the `[sprites]` names of a loaded profile to sprite ids.
    pub(super) fn finish_physics(&mut self, l: physics::PhysicsLoad, path: &str) -> PhysicsProfile {
        let mut p = l.profile;
        let text = read_text(self.files, path, &mut Vec::new()).unwrap_or("");
        let pick = |this: &mut Self, name: &Option<String>| -> Option<SpriteId> {
            let name = name.as_ref()?;
            let at = text.find(name.as_str()).unwrap_or(0);
            this.sprite_ref(name, path, text, at..at + name.len())
        };
        if l.legacy {
            // A legacy draft names no kid sprites: the converter's names, when the package has them.
            let find = |this: &Self, name: &str| this.sprite_ids.get(name).map(|e| e.id);
            p.sprites.idle = find(self, "player_idle");
            p.sprites.running = find(self, "player_running");
            p.sprites.jump = find(self, "player_jump");
            p.sprites.fall = find(self, "player_fall");
            p.sprites.sliding = find(self, "player_sliding");
            return p;
        }
        p.sprites.idle = pick(self, &l.sprites.idle);
        p.sprites.running = pick(self, &l.sprites.running);
        p.sprites.jump = pick(self, &l.sprites.jump);
        p.sprites.fall = pick(self, &l.sprites.fall);
        p.sprites.sliding = pick(self, &l.sprites.sliding);
        // `[sounds]`: names resolved against the package's sounds.
        let names = l.sounds;
        let snd = |this: &mut Self, name: &Option<String>| -> Option<iwbtg_sim::def::SoundId> {
            let name = name.as_ref()?;
            let found = this
                .sounds
                .iter()
                .position(|d| d.name == *name)
                .and_then(|i| u32::try_from(i).ok());
            if found.is_none() {
                let at = text.find(name.as_str()).unwrap_or(0);
                this.error(
                    path,
                    text,
                    at..at + name.len(),
                    crate::engine_pack::unresolved(this.files, name, "sound"),
                );
            }
            found.map(iwbtg_sim::def::SoundId)
        };
        p.sounds.jump = snd(self, &names.jump);
        p.sounds.double_jump = snd(self, &names.double_jump);
        p.sounds.shoot = snd(self, &names.shoot);
        p.sounds.vine_kick = snd(self, &names.vine_kick);
        p.sounds.death = snd(self, &names.death);
        // `[script]` (decision 0004): resolve the file name to a script id, intern the vars and
        // check every slot function against the file. The loader has the profile's text here, so
        // a bad name, a missing file and a missing function get a position.
        let script = l.script.unwrap_or_default();
        let mut vars: Vec<String> = Vec::new();
        let mut seen = BTreeSet::new();
        let at = |name: &str| {
            let start = text.find(name).unwrap_or(0);
            start..start + name.len()
        };
        for var in script.vars.unwrap_or_default() {
            if !seen.insert(var.clone()) {
                continue;
            }
            if var.is_empty() {
                self.error(path, text, 0..0, "a `[script] vars` name cannot be empty");
                continue;
            }
            if !rhai::is_valid_identifier(&var) {
                self.error(
                    path,
                    text,
                    at(&var),
                    format!(
                        "`[script] vars` name `{var}` is not a Rhai identifier; a script cannot write `this.{var}`"
                    ),
                );
                continue;
            }
            if is_reserved_instance_field(&var) {
                self.error(
                    path,
                    text,
                    at(&var),
                    format!(
                        "`[script] vars` name `{var}` is a reserved instance property; `this.{var}` reads the field, not a custom variable"
                    ),
                );
                continue;
            }
            vars.push(var);
        }
        self.kid_script_vars.extend(vars.iter().cloned());
        p.script.vars = vars;
        match script.file {
            Some(name) => {
                let script_path = format!("scripts/{name}.rhai");
                if let Err(e) = check_id(&name) {
                    self.error(path, text, at(&name), format!("script {e}"));
                } else {
                    match read_text(self.files, &script_path, &mut Vec::new()) {
                        Some(source) if source.len() > MAX_SCRIPT_BYTES => {
                            self.error(path, text, at(&name), format!("`{script_path}` is over 256 KB"))
                        }
                        Some(source) => {
                            let sid = iwbtg_sim::def::ScriptId(
                                u32::try_from(self.scripts.len()).unwrap_or(u32::MAX),
                            );
                            self.scripts.push(iwbtg_sim::def::ScriptDef {
                                name: script_path.clone(),
                                source: source.to_owned(),
                            });
                            p.script.file = Some(sid);
                            // A compile error is the script validator's report; only a file that
                            // compiles is checked for the slot functions.
                            if let Some(fns) = iwbtg_sim::script::validate::function_arities(source) {
                                self.check_kid_slots(&p, path, text, &script_path, &fns);
                            }
                        }
                        None => self.error(path, text, at(&name), format!("`{script_path}` does not exist")),
                    }
                }
            }
            None => {
                if let Some(slot) = first_kid_slot(&p) {
                    self.error(
                        path,
                        text,
                        at(&slot.function),
                        format!(
                            "kid slot `{}` runs but the profile names no `[script] file`",
                            slot.function
                        ),
                    );
                }
            }
        }
        p
    }

    /// Checks the functions a profile's script slots name against the functions its script
    /// defines (decision 0004): the name must exist, and its argument count must match the slot
    /// (0 in a list event, 1 in a collision row).
    fn check_kid_slots(
        &mut self,
        p: &PhysicsProfile,
        path: &str,
        text: &str,
        script_path: &str,
        fns: &BTreeSet<(String, usize)>,
    ) {
        for (slot, arity) in kid_slot_uses(p) {
            let name = &slot.function;
            let start = text.find(name.as_str()).unwrap_or(0);
            let span = start..start + name.len();
            let defined: Vec<usize> = fns.iter().filter(|(n, _)| n == name).map(|(_, a)| *a).collect();
            if defined.is_empty() {
                self.error(
                    path,
                    text,
                    span,
                    format!("`{script_path}` has no function `{name}`"),
                );
            } else if !defined.contains(&arity) {
                let have: Vec<String> = defined.iter().map(ToString::to_string).collect();
                self.error(
                    path,
                    text,
                    span,
                    format!(
                        "the kid slot function `{name}` takes {} argument{}, not {arity}",
                        have.join(" or "),
                        if defined == [1] { "" } else { "s" }
                    ),
                );
            }
        }
    }

    /// Reads every room profile, `physics/<id>.toml` (#33), in path order. The kid kind's events
    /// are fixed at load from the game's profile, so a room profile must handle the same events
    /// (the same `[events]` rows' triggers: `kid::event_set`); its actions and values may differ.
    /// The game's own profile file, when it lies in `physics/`, is not loaded twice: rooms that
    /// name it use the game's profile.
    pub(super) fn load_room_physics(&mut self, game: &PhysicsProfile) {
        let game_events = iwbtg_sim::kid::event_set(game);
        let paths: Vec<String> = self
            .files
            .iter()
            .map(|(p, _)| p)
            .filter(|p| {
                p.strip_prefix("physics/")
                    .and_then(|r| r.strip_suffix(".toml"))
                    .is_some_and(|id| !id.is_empty() && !id.contains('/'))
            })
            .map(str::to_owned)
            .collect();
        for path in paths {
            let id = path["physics/".len()..path.len() - ".toml".len()].to_owned();
            if path == *self.m.physics.get_ref() {
                self.room_physics_ids.insert(id, None);
                continue;
            }
            let mut d = Vec::new();
            let loaded = physics::load(self.files, &path, self.trace_mode(), &mut d);
            self.diags.append(&mut d);
            let Some(l) = loaded else { continue };
            let profile = self.finish_physics(l, &path);
            if iwbtg_sim::kid::event_set(&profile) != game_events {
                self.diags.push(Diagnostic::file_error(
                    &path,
                    format!(
                        "room profile `{id}` handles other kid events than the game's profile `{}`; \
                         a room profile may change actions and values, not which events the kid has",
                        self.m.physics.get_ref()
                    ),
                ));
                continue;
            }
            let Ok(index) = u16::try_from(self.room_physics.len()) else {
                self.diags
                    .push(Diagnostic::file_error(&path, "more than 65535 room profiles"));
                continue;
            };
            self.room_physics.push(profile);
            self.room_physics_ids.insert(id, Some(index));
        }
    }

    /// A room's `physics = "<id>"`: the index of `physics/<id>.toml` in the room profiles, or
    /// `None` (the game's profile) when it is the game's own file or has an error.
    pub(super) fn room_physics_ref(&mut self, name: &Spanned<String>, path: &str, text: &str) -> Option<u16> {
        let id = name.get_ref();
        match self.room_physics_ids.get(id) {
            Some(index) => *index,
            None => {
                let file = format!("physics/{id}.toml");
                if !self.files.contains(&file) {
                    self.error(
                        path,
                        text,
                        name.span(),
                        format!("physics profile `{id}` has no `{file}`"),
                    );
                }
                None
            }
        }
    }

    /// Trace mode: the kid's mask from the legacy physics.toml `mask = { sprite = ... }` (CR8).
    pub(super) fn legacy_kid_mask(&mut self) {
        let Some(&kid) = self.kind_ids.get(Builtin::Kid.name()) else {
            return;
        };
        if self
            .kinds
            .get(kid.index().unwrap_or(usize::MAX))
            .is_some_and(|k| k.mask.is_some())
        {
            return;
        }
        let Some(text) = read_text(self.files, self.m.physics.get_ref(), &mut Vec::new()) else {
            return;
        };
        let Ok(table) = toml::from_str::<toml::Table>(text) else {
            return;
        };
        let Some(sprite) = table
            .get("mask")
            .and_then(|m| m.get("sprite"))
            .and_then(toml::Value::as_str)
        else {
            return;
        };
        let Some(sid) = self.sprite_ids.get(sprite).map(|e| e.id) else {
            return;
        };
        let mask = self.mask_of(sid);
        if let Some(k) = self.kind_mut(kid) {
            k.mask = mask;
        }
        self.note("CR8", format!("kid mask `{sprite}`"));
    }
}

/// Every script slot a profile's event lists name, with the argument count the slot calls it
/// with (0 in a list event, 1 in a collision row; decision 0004).
fn kid_slot_uses(profile: &PhysicsProfile) -> Vec<(&KidScriptSlot, usize)> {
    fn walk<'a>(list: &'a [KidAction], out: &mut Vec<(&'a KidScriptSlot, usize)>) {
        for action in list {
            if let KidAction::Script(slot) = action {
                out.push((slot, 0));
            }
        }
    }
    let e = &profile.events;
    let mut out = Vec::new();
    for list in [&e.create, &e.room_start, &e.step, &e.end_step] {
        walk(list, &mut out);
    }
    for row in &e.alarms {
        walk(&row.run, &mut out);
    }
    for row in &e.keys {
        walk(&row.run, &mut out);
    }
    for row in &e.collisions {
        if let KidCollisionRun::Script(slot) = &row.run {
            out.push((slot, 1));
        }
    }
    out
}

/// The first script slot of a profile, for the "no `[script] file`" error.
fn first_kid_slot(profile: &PhysicsProfile) -> Option<&KidScriptSlot> {
    kid_slot_uses(profile).first().map(|(slot, _)| *slot)
}
