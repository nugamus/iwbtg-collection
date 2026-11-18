//! `game.toml` parts that need the loaded package (docs/format/game.md): `[timing]`, maps,
//! reachable timing rooms and `[stripped]`.

use super::*;

impl<'a> Loader<'a> {
    pub(super) fn timing(&mut self) -> Option<TimingDef> {
        let t = self.m.timing.get_ref();
        let start = self.trigger(&t.start.trigger, t.start.room.as_ref());
        let end = self.trigger(&t.end.trigger, t.end.room.as_ref());
        let mut stages = Vec::new();
        for s in &t.stages {
            stages.push(self.trigger(&s.trigger, s.room.as_ref()));
        }
        Some(TimingDef {
            start: start?,
            end: end?,
            stages: stages.into_iter().collect::<Option<Vec<_>>>()?,
        })
    }

    /// Each map's chain. Keys a map leaves out reuse `[timing]`'s result, so their errors are
    /// reported once.
    pub(super) fn map_timings(&mut self, base: Option<&TimingDef>) -> Vec<MapTiming> {
        let m = self.m;
        if m.maps.is_empty() {
            return base
                .map(|b| MapTiming {
                    id: DEFAULT_MAP_ID.to_owned(),
                    timing: b.clone(),
                })
                .into_iter()
                .collect();
        }
        let mut out = Vec::new();
        for map in &m.maps {
            let start = match &map.start {
                Some(t) => self.trigger(&t.trigger, t.room.as_ref()),
                None => base.map(|b| b.start),
            };
            let end = match &map.end {
                Some(t) => self.trigger(&t.trigger, t.room.as_ref()),
                None => base.map(|b| b.end),
            };
            let stages = match &map.stages {
                Some(list) => {
                    let all: Vec<_> = list
                        .iter()
                        .map(|s| self.trigger(&s.trigger, s.room.as_ref()))
                        .collect();
                    all.into_iter().collect::<Option<Vec<_>>>()
                }
                None => base.map(|b| b.stages.clone()),
            };
            if let (Some(start), Some(end), Some(stages)) = (start, end, stages) {
                out.push(MapTiming {
                    id: map.id.get_ref().clone(),
                    timing: TimingDef { start, end, stages },
                });
            }
        }
        out
    }

    pub(super) fn trigger(
        &mut self,
        name: &Spanned<String>,
        room: Option<&Spanned<String>>,
    ) -> Option<TimingTrigger> {
        match (name.get_ref().as_str(), room) {
            ("run_start", None) => Some(TimingTrigger::RunStart),
            ("run_end", None) => Some(TimingTrigger::RunEnd),
            ("enter_room", Some(r)) => self.room_ref(r.get_ref(), r.span()).map(TimingTrigger::EnterRoom),
            ("enter_room", None) => {
                self.manifest_error(name.span(), "`enter_room` needs `room = \"<room id>\"`");
                None
            }
            ("run_start" | "run_end", Some(r)) => {
                self.manifest_error(r.span(), format!("`{}` takes no `room`", name.get_ref()));
                None
            }
            (other, _) => {
                self.manifest_error(
                    name.span(),
                    format!(
                        "timing trigger `{other}` is not in the vocabulary ({})",
                        TIMING_TRIGGERS.join(", ")
                    ),
                );
                None
            }
        }
    }

    /// A warning for each `enter_room` trigger of `[timing]` or a map whose room no warp, room
    /// creation spawn, cut room or start leads to ([`crate::reach`]). Each spelling once: a map
    /// key left out reuses `[timing]`'s.
    pub(super) fn reachable_timing_rooms(&mut self, def: &GameDef) {
        let reached = crate::reach::reachable(def);
        let m: &'a Manifest = self.m;
        let t = m.timing.get_ref();
        let mut rooms: Vec<&Spanned<String>> = Vec::new();
        let mut take = |room: Option<&'a Spanned<String>>| rooms.extend(room);
        take(t.start.room.as_ref());
        for s in &t.stages {
            take(s.room.as_ref());
        }
        take(t.end.room.as_ref());
        for map in &m.maps {
            take(map.start.as_ref().and_then(|t| t.room.as_ref()));
            for s in map.stages.iter().flatten() {
                take(s.room.as_ref());
            }
            take(map.end.as_ref().and_then(|t| t.room.as_ref()));
        }
        for r in rooms {
            let Some(id) = self.room_ids.get(r.get_ref()) else {
                continue;
            };
            if !reached.contains(id) {
                self.diags.push(Diagnostic::warning_at(
                    MANIFEST_PATH,
                    self.manifest_text,
                    r.span().start,
                    format!(
                        "timing room `{}` cannot be reached: no start, warp, room spawn, script room call or cut room leads there (docs/format/validation.md, step 7)",
                        r.get_ref()
                    ),
                ));
            }
        }
    }

    /// `[stripped]` and each map's `stripped_start` (docs/format/game.md, Stripped mode): rooms,
    /// kinds and instance names resolve; no ranked start room and no cut room's `to` is cut.
    pub(super) fn stripped_refs(&mut self, rooms: &[ParsedRoom<'_>]) {
        let m = self.m;
        let section = m.stripped.as_ref();
        let cut = section.and_then(|s| s.cut.as_ref());
        let cut_rooms: BTreeSet<&str> = cut
            .map(|c| c.rooms.iter().map(|r| r.room.get_ref().as_str()).collect())
            .unwrap_or_default();
        let starts = section
            .and_then(|s| s.start.as_ref())
            .into_iter()
            .chain(m.maps.iter().filter_map(|map| map.stripped_start.as_ref()));
        for start in starts {
            if let Some(room) = &start.room {
                self.room_ref(room.get_ref(), room.span());
                if cut_rooms.contains(room.get_ref().as_str()) {
                    self.manifest_error(
                        room.span(),
                        format!("ranked start room `{}` is cut", room.get_ref()),
                    );
                }
            }
        }
        // The default start: `[engine].new_game_room`, else `start_room`.
        let default_start = m
            .engine
            .as_ref()
            .and_then(|e| e.get_ref().new_game_room.as_ref())
            .unwrap_or(&m.start_room);
        let start_defaults = section
            .and_then(|s| s.start.as_ref())
            .and_then(|s| s.room.as_ref())
            .is_none();
        let map_overrides = !m.maps.is_empty()
            && m.maps
                .iter()
                .all(|map| map.stripped_start.as_ref().is_some_and(|s| s.room.is_some()));
        if start_defaults && !map_overrides && cut_rooms.contains(default_start.get_ref().as_str()) {
            self.manifest_error(
                default_start.span(),
                format!(
                    "ranked runs start in `{}` (no `[stripped].start.room`), which is cut",
                    default_start.get_ref()
                ),
            );
        }
        let Some(section) = section else {
            return;
        };
        if let Some(cut) = cut {
            for entry in &cut.rooms {
                self.room_ref(entry.room.get_ref(), entry.room.span());
                self.room_ref(entry.to.get_ref(), entry.to.span());
            }
            for kind in &cut.kinds {
                if !self.kind_ids.contains_key(kind.get_ref().as_str())
                    && !self.failed.contains(kind.get_ref())
                {
                    self.manifest_error(
                        kind.span(),
                        format!("cut kind `{}` is not a kind of this package", kind.get_ref()),
                    );
                }
            }
            for entry in &cut.instances {
                let Some(room) = rooms.iter().find(|r| r.file.id == *entry.room.get_ref()) else {
                    self.room_ref(entry.room.get_ref(), entry.room.span());
                    continue;
                };
                let named = room.file.instances.iter().any(|i| {
                    i.name
                        .as_ref()
                        .is_some_and(|n| n.get_ref() == entry.name.get_ref())
                });
                if !named {
                    self.manifest_error(
                        entry.name.span(),
                        format!(
                            "room `{}` has no instance named `{}`",
                            entry.room.get_ref(),
                            entry.name.get_ref()
                        ),
                    );
                }
            }
        }
        let regions = section
            .cutscenes
            .iter()
            .map(|c| (&c.kind, &c.room))
            .chain(section.dialogues.iter().map(|d| (&d.kind, &d.room)));
        for (kind, room) in regions {
            if let Some(kind) = kind
                && !self.kind_ids.contains_key(kind.get_ref().as_str())
                && !self.failed.contains(kind.get_ref())
            {
                self.manifest_error(
                    kind.span(),
                    format!("region kind `{}` is not a kind of this package", kind.get_ref()),
                );
            }
            if let Some(room) = room {
                self.room_ref(room.get_ref(), room.span());
            }
        }
    }

    /// `[stripped]` resolved to ids for the sim (#561), and each map's ranked start by map id
    /// (`stripped_start` merged over `[stripped].start`; one `main` entry without `[[maps]]`).
    /// Rooms, kinds and names that do not resolve were reported by [`Loader::stripped_refs`] and
    /// are left out here.
    pub(super) fn stripped_def(
        &mut self,
        vars: &BTreeMap<String, iwbtg_sim::def::VarId>,
    ) -> (StrippedDef, BTreeMap<String, iwbtg_sim::stripped::RankedStart>) {
        let m = self.m;
        let section = m.stripped.as_ref();
        let base = section.and_then(|s| s.start.as_ref());
        let start = self.ranked_start(base, None, vars);
        let mut map_starts = BTreeMap::new();
        if m.maps.is_empty() {
            map_starts.insert(DEFAULT_MAP_ID.to_owned(), start.clone());
        }
        for map in &m.maps {
            let s = self.ranked_start(base, map.stripped_start.as_ref(), vars);
            map_starts.insert(map.id.get_ref().clone(), s);
        }
        let mut def = StrippedDef {
            start,
            ..StrippedDef::default()
        };
        let Some(section) = section else {
            return (def, map_starts);
        };
        if let Some(cut) = &section.cut {
            for entry in &cut.rooms {
                if let (Some(&from), Some(&to)) = (
                    self.room_ids.get(entry.room.get_ref()),
                    self.room_ids.get(entry.to.get_ref()),
                ) {
                    def.cut_rooms.insert(from, to);
                }
            }
            def.cut_kinds = cut
                .kinds
                .iter()
                .filter_map(|k| self.kind_ids.get(k.get_ref()).copied())
                .collect();
            for entry in &cut.instances {
                let room = self.room_ids.get(entry.room.get_ref()).copied();
                let name = self.names.iter().position(|n| n == entry.name.get_ref());
                if let (Some(room), Some(name)) = (room, name.and_then(|n| u32::try_from(n).ok())) {
                    def.cut_instances.insert((room, NameId(name)));
                }
            }
        }
        def.cutscenes = section
            .cutscenes
            .iter()
            .filter_map(|c| self.region(&c.id, c.kind.as_ref(), c.room.as_ref()))
            .collect();
        def.dialogues = section
            .dialogues
            .iter()
            .filter_map(|d| {
                Some(DialogueRegion {
                    region: self.region(&d.id, d.kind.as_ref(), d.room.as_ref())?,
                    advance: Action::from_name(d.advance.get_ref())?,
                    timeout_ticks: *d.timeout_ticks.get_ref(),
                })
            })
            .collect();
        (def, map_starts)
    }

    /// A ranked start: `over`'s keys over `base`'s, globals merged by name (the map's value wins).
    /// A global named `difficulty` also sets the engine's difficulty, so it must be a whole
    /// number from 0 to 255.
    pub(super) fn ranked_start(
        &mut self,
        base: Option<&RankedStart>,
        over: Option<&RankedStart>,
        vars: &BTreeMap<String, iwbtg_sim::def::VarId>,
    ) -> SimRankedStart {
        let room = over
            .and_then(|s| s.room.as_ref())
            .or_else(|| base.and_then(|s| s.room.as_ref()))
            .and_then(|r| self.room_ids.get(r.get_ref()).copied());
        let mut merged: BTreeMap<&str, &Spanned<toml::Value>> = BTreeMap::new();
        for s in base.into_iter().chain(over) {
            for (name, value) in &s.globals {
                merged.insert(name.get_ref().as_str(), value);
            }
        }
        let mut out = SimRankedStart {
            room,
            ..SimRankedStart::default()
        };
        for (name, value) in merged {
            // Bad values were reported by the manifest check.
            let (Some(&var), Ok(v)) = (vars.get(name), global_value(value.get_ref())) else {
                continue;
            };
            if name == "difficulty" {
                match whole_difficulty(&v) {
                    Some(d) => out.difficulty = Some(d),
                    None => self.manifest_error(
                        value.span(),
                        "the ranked start's `difficulty` sets the engine difficulty: a whole number from 0 to 255",
                    ),
                }
            }
            out.globals.push((var, v));
        }
        out
    }

    /// A cutscene or dialogue region, when its kind or room resolves.
    pub(super) fn region(
        &self,
        id: &Spanned<String>,
        kind: Option<&Spanned<String>>,
        room: Option<&Spanned<String>>,
    ) -> Option<Region> {
        let holds = match (kind, room) {
            (Some(k), None) => RegionCondition::Kind(*self.kind_ids.get(k.get_ref())?),
            (None, Some(r)) => RegionCondition::Room(*self.room_ids.get(r.get_ref())?),
            _ => return None,
        };
        Some(Region {
            id: id.get_ref().clone(),
            holds,
        })
    }
}
