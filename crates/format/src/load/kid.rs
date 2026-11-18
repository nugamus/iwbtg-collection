//! The kid's physics profiles, `physics.toml` and the room profiles `physics/<id>.toml`
//! (docs/format/physics.md).

use super::*;

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
        p
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
