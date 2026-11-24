//! Step 4: `rooms/<room_id>.toml` (docs/format/rooms.md): rooms, views, placed instances,
//! creation-code rows; Step 8's inert twins and warp targets.

use super::*;

impl<'a> Loader<'a> {
    pub(super) fn parse_rooms(&mut self) -> Vec<ParsedRoom<'a>> {
        let files = self.files;
        let mut rooms = Vec::new();
        for (p, _) in files.iter() {
            let Some(stem) = p.strip_prefix("rooms/").and_then(|s| s.strip_suffix(".toml")) else {
                continue;
            };
            if stem.contains('/') || stem.contains('.') {
                continue; // `<id>.tiles.toml`: decorative, never the sim's.
            }
            let Some(text) = read_text(files, p, &mut self.diags) else {
                continue;
            };
            if let Err(e) = check_id(stem) {
                self.diags.push(Diagnostic::file_error(p, format!("room {e}")));
                continue;
            }
            match parse_room(stem, p, text) {
                Ok(file) => {
                    for bg in &file.art.backgrounds {
                        let png = format!("backgrounds/{}.png", bg.image);
                        if !files.contains(&png) {
                            self.diags.push(Diagnostic::error_at(
                                p,
                                text,
                                bg.span.start,
                                format!("background `{}` has no image `{png}`", bg.image),
                            ));
                        }
                    }
                    rooms.push(ParsedRoom {
                        path: p.to_owned(),
                        text,
                        file,
                    });
                }
                Err(d) => self.diags.push(d),
            }
        }
        // Room order: `room_order`, else (Trace) the trace map's original order, then path order.
        let mut order: Vec<String> = Vec::new();
        if let Some(list) = &self.m.room_order {
            for r in list.get_ref() {
                if !rooms.iter().any(|p| p.file.id == *r.get_ref()) {
                    self.manifest_error(
                        r.span(),
                        format!("room_order names `{}`, which has no room file", r.get_ref()),
                    );
                } else if order.contains(r.get_ref()) {
                    self.manifest_error(r.span(), format!("room_order lists `{}` twice", r.get_ref()));
                } else {
                    order.push(r.get_ref().clone());
                }
            }
        } else if let Some(trace) = &self.trace {
            let mut by_order: Vec<(u32, String)> = trace
                .rooms
                .values()
                .filter_map(|r| Some((r.order?, r.id.clone())))
                .filter(|(_, id)| rooms.iter().any(|p| p.file.id == *id))
                .collect();
            by_order.sort();
            by_order.dedup_by(|a, b| a.1 == b.1);
            if !by_order.is_empty() {
                self.note("CR14", format!("{} rooms", by_order.len()));
            }
            order.extend(by_order.into_iter().map(|(_, id)| id));
        }
        for r in &rooms {
            if !order.contains(&r.file.id) {
                order.push(r.file.id.clone());
            }
        }
        rooms.sort_by_key(|r| order.iter().position(|o| *o == r.file.id).unwrap_or(usize::MAX));
        for (i, r) in rooms.iter().enumerate() {
            self.room_ids
                .insert(r.file.id.clone(), RoomId(u32::try_from(i).unwrap_or(u32::MAX)));
        }
        rooms
    }

    pub(super) fn room_ref(&mut self, name: &str, span: Range<usize>) -> Option<RoomId> {
        let r = self.room_ids.get(name).copied();
        if r.is_none() {
            self.manifest_error(span, format!("room `{name}` does not exist"));
        }
        r
    }

    /// Step 6: every variable name the loader can see, sorted (docs/architecture/sim/world.md).
    pub(super) fn intern(&self, rooms: &[ParsedRoom<'_>]) -> BTreeMap<String, iwbtg_sim::def::VarId> {
        let mut names: BTreeSet<String> = BTreeSet::new();
        for k in &self.kinds {
            names.extend(k.params.0.keys().cloned());
        }
        // A kid profile's `[script] vars` are declared content, not run-time inferences
        // (decision 0004).
        names.extend(self.kid_script_vars.iter().cloned());
        for r in rooms {
            for i in &r.file.instances {
                names.extend(i.params.keys().cloned());
            }
            for a in &r.file.on_create {
                match a.get_ref() {
                    ActionFile::SetGlobal { var, index: None, .. } => {
                        names.insert(var.clone());
                    }
                    // The element lives in the global's array (#998), not in a `stage[n]`
                    // variable; only the index global is known at load.
                    ActionFile::SetGlobal {
                        index: Some(IndexFile::Global { global }),
                        ..
                    } if !is_engine_index(global) => {
                        names.insert(global.clone());
                    }
                    ActionFile::Spawn { params, .. } => names.extend(params.keys().cloned()),
                    _ => {}
                }
            }
        }
        // Ranked start globals (`[stripped].start`, each map's `stripped_start`).
        let m = self.m;
        let starts = m
            .stripped
            .as_ref()
            .and_then(|s| s.start.as_ref())
            .into_iter()
            .chain(m.maps.iter().filter_map(|map| map.stripped_start.as_ref()));
        for start in starts {
            names.extend(start.globals.keys().map(|k| k.get_ref().clone()));
        }
        intern_vars(names.into_iter()).ids
    }

    pub(super) fn build_rooms(
        &mut self,
        rooms: &[ParsedRoom<'_>],
        vars: &BTreeMap<String, iwbtg_sim::def::VarId>,
    ) -> Vec<RoomDef> {
        let mut out = Vec::with_capacity(rooms.len());
        let mut missing_role = 0usize;
        let mut traced_speed = 0usize;
        let mut unranked: BTreeSet<KindId> = BTreeSet::new();
        for r in rooms {
            let f = &r.file;
            let (path, text) = (r.path.as_str(), r.text);
            let mut def = RoomDef::new(f.id.clone(), f.size.0, f.size.1);
            def.persistent = f.persistent;
            def.role = match f.role {
                Some(role) => role.role(),
                None => {
                    missing_role += 1;
                    RoomRole::Play
                }
            };
            def.no_restart = f.no_restart;
            def.djump_grants_tjump = f.djump_grants_tjump;
            if let Some(name) = &f.physics {
                def.physics = self.room_physics_ref(name, path, text);
            }
            def.file_speed = match &f.file_speed {
                Some(s) => *s.get_ref(),
                None => match self
                    .trace
                    .as_ref()
                    .and_then(|t| t.room_by_id(&f.id))
                    .and_then(|(_, t)| t.speed)
                {
                    Some(speed) if speed > 0 => {
                        traced_speed += 1;
                        speed
                    }
                    _ => self.m.tick_rate(),
                },
            };
            def.views_enabled = f.views_enabled;
            for (n, v) in f.views.iter().enumerate() {
                let view = self.view(v.get_ref(), path, text, v.span());
                if let Some(slot) = def.views.get_mut(n) {
                    *slot = view;
                }
            }
            let mut seen_names = BTreeSet::new();
            let mut dropped = 0usize;
            for inst in &f.instances {
                match self.placed(inst, path, text, &mut seen_names, &mut unranked) {
                    Placed::Instances(mut list) => def.instances.append(&mut list),
                    Placed::Dropped => dropped += inst.positions().count(),
                    Placed::Failed => {}
                }
            }
            if dropped > 0 {
                self.note("CR3", format!("{} instances removed from {}", dropped, f.id));
            }
            for a in &f.on_create {
                if let Some(action) = self.action(a.get_ref(), vars, path, text, a.span()) {
                    def.on_create.push(action);
                }
            }
            out.push(def);
        }
        if missing_role > 0 {
            self.trace_note("CR6", format!("{missing_role} rooms"));
        }
        if traced_speed > 0 {
            self.note("CR13", format!("{traced_speed} rooms"));
        }
        out
    }

    pub(super) fn view(
        &mut self,
        v: &crate::rooms::ViewFile,
        path: &str,
        text: &str,
        span: Range<usize>,
    ) -> ViewDef {
        let r = |n: &crate::number::Number| Real::new(n.0);
        let [vx, vy, vw, vh] = &v.view;
        let port = v
            .port
            .as_ref()
            .map_or((Real::ZERO, Real::ZERO, r(vw), r(vh)), |[a, b, c, d]| {
                (r(a), r(b), r(c), r(d))
            });
        let follow = match &v.follow {
            None => None,
            Some(name) => {
                let k = self.kind_by_ref(name.get_ref());
                if k.is_none() && !self.failed.contains(name.get_ref()) {
                    self.error(
                        path,
                        text,
                        name.span(),
                        format!("view follows `{}`, which is not a kind", name.get_ref()),
                    );
                }
                k
            }
        };
        let _ = span;
        let (hb, vb) = v.border.as_ref().map_or((32.0, 32.0), |[a, b]| (a.0, b.0));
        let (hs, vs) = v.speed.as_ref().map_or((-1.0, -1.0), |[a, b]| (a.0, b.0));
        ViewDef {
            visible: v.visible,
            view: (r(vx), r(vy), r(vw), r(vh)),
            port,
            follow,
            hborder: Real::new(hb),
            vborder: Real::new(vb),
            hspeed: Real::new(hs),
            vspeed: Real::new(vs),
        }
    }

    pub(super) fn placed(
        &mut self,
        inst: &InstanceFile,
        path: &str,
        text: &str,
        seen_names: &mut BTreeSet<String>,
        unranked: &mut BTreeSet<KindId>,
    ) -> Placed {
        let mut params = Params::default();
        for (key, value) in &inst.params {
            match param_value(value.get_ref()) {
                Ok(p) => {
                    params.0.insert(key.clone(), p);
                }
                Err(e) => {
                    self.error(path, text, value.span(), format!("param `{key}`: {e}"));
                    return Placed::Failed;
                }
            }
        }
        if self.failed.contains(inst.kind.get_ref()) {
            return Placed::Failed;
        }
        let kind = match self.instance_kind(inst.kind.get_ref(), &params) {
            Ok(k) => k,
            Err(true) => return Placed::Dropped,
            Err(false) => {
                self.error(
                    path,
                    text,
                    inst.kind.span(),
                    format!("`{}` is not a kind of this package", inst.kind.get_ref()),
                );
                return Placed::Failed;
            }
        };
        let base = self
            .kinds
            .get(kind.index().unwrap_or(usize::MAX))
            .map_or(Builtin::Inert, |k| k.base);
        let mut resolved = Params::default();
        for (key, value) in params.0 {
            let v = self.resolve_param(base, &key, value, path);
            resolved.0.insert(key, v);
        }
        let Some(rank) = self.rank_or_append(kind, &[&resolved]) else {
            if unranked.insert(kind) {
                let name = self
                    .kinds
                    .get(kind.index().unwrap_or(usize::MAX))
                    .map(|k| k.name.clone())
                    .unwrap_or_default();
                self.error(
                    path,
                    text,
                    inst.kind.span(),
                    format!("`{name}` is placed but object_order does not list it"),
                );
            }
            return Placed::Failed;
        };
        let name = match &inst.name {
            None => None,
            Some(n) => {
                if !seen_names.insert(n.get_ref().clone()) {
                    self.error(
                        path,
                        text,
                        n.span(),
                        format!("instance name `{}` is used twice in this room", n.get_ref()),
                    );
                }
                if inst.count() > 1 {
                    self.error(path, text, n.span(), "a named instance cannot have `run`");
                }
                Some(NameId(intern_in(&mut self.names, n.get_ref())))
            }
        };
        let group = inst
            .group
            .as_ref()
            .map(|g| GroupId(intern_in(&mut self.groups, g.get_ref())));
        let list = inst
            .positions()
            .map(|(x, y)| PlacedInstance {
                kind,
                rank,
                x: Real::new(x),
                y: Real::new(y),
                xscale: Real::new(inst.xscale),
                yscale: Real::new(inst.yscale),
                angle: Real::new(inst.angle),
                visible: inst.visible,
                depth: inst.depth.map(Real::new),
                params: resolved.clone(),
                name,
                group,
                gm_id: inst.gm_id.as_ref().map(|i| *i.get_ref()),
            })
            .collect();
        Placed::Instances(list)
    }

    pub(super) fn action(
        &mut self,
        a: &ActionFile,
        vars: &BTreeMap<String, iwbtg_sim::def::VarId>,
        path: &str,
        text: &str,
        span: Range<usize>,
    ) -> Option<RoomAction> {
        let action = self.bare_action(a, vars, path, text, span.clone())?;
        match a.when() {
            None => Some(action),
            Some(w) => {
                let when = self.when(w, path, text, span)?;
                Some(RoomAction::When {
                    when,
                    action: Box::new(action),
                })
            }
        }
    }

    /// A row's `when` (#629).
    pub(super) fn when(
        &mut self,
        w: &WhenFile,
        path: &str,
        text: &str,
        span: Range<usize>,
    ) -> Option<RoomWhen> {
        if w.difficulty_below.is_none() && w.kid_exists.is_none() && w.kind_absent.is_none() {
            self.error(
                path,
                text,
                span,
                "`when` needs `difficulty_below`, `kid_exists` or `kind_absent`",
            );
            return None;
        }
        let kind_absent = match &w.kind_absent {
            None => None,
            Some(kind) => Some(self.action_kind("when.kind_absent", kind, path, text, span)?),
        };
        Some(RoomWhen {
            difficulty_below: w.difficulty_below.map(|n| Real::new(n.0)),
            kid_exists: w.kid_exists,
            kind_absent,
        })
    }

    /// A kind named by a room action, or an error.
    pub(super) fn action_kind(
        &mut self,
        what: &str,
        kind: &str,
        path: &str,
        text: &str,
        span: Range<usize>,
    ) -> Option<KindId> {
        let k = self.kind_by_ref(kind).filter(|_| !self.dropped.contains(kind));
        // A kind whose own file failed to load was reported there already.
        if k.is_none() && !self.failed.contains(kind) {
            self.error(
                path,
                text,
                span,
                format!("{what}: `{kind}` is not a kind of this package"),
            );
        }
        k
    }

    /// A sound named by a room action, or an error.
    pub(super) fn action_sound(
        &mut self,
        what: &str,
        sound: &str,
        path: &str,
        text: &str,
        span: Range<usize>,
    ) -> Option<SoundId> {
        let found = self
            .sounds
            .iter()
            .position(|d| d.name == sound)
            .and_then(|i| u32::try_from(i).ok())
            .map(SoundId);
        if found.is_none() {
            self.error(
                path,
                text,
                span,
                format!("{what}: sound `{sound}` does not exist"),
            );
        }
        found
    }

    pub(super) fn bare_action(
        &mut self,
        a: &ActionFile,
        vars: &BTreeMap<String, iwbtg_sim::def::VarId>,
        path: &str,
        text: &str,
        span: Range<usize>,
    ) -> Option<RoomAction> {
        match a {
            ActionFile::SetGlobal {
                var,
                index: Some(index),
                value,
                ..
            } => {
                let value = match global_value(value) {
                    Ok(v) => v,
                    Err(e) => {
                        self.error(path, text, span, format!("set_global `{var}`: {e}"));
                        return None;
                    }
                };
                let index = match index {
                    IndexFile::Number(n) => {
                        let v = n.0;
                        if v.fract() != 0.0 || !(0.0..=f64::from(u32::MAX)).contains(&v) {
                            self.error(
                                path,
                                text,
                                span,
                                format!("set_global `{var}`: `index` must be a whole number from 0"),
                            );
                            return None;
                        }
                        // Checked just above: a whole number in 0..=u32::MAX.
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        GlobalIndex::Const(v as u32)
                    }
                    IndexFile::Global { global } if is_engine_index(global) => GlobalIndex::Savenum,
                    IndexFile::Global { global } => GlobalIndex::Global(*vars.get(global)?),
                };
                Some(RoomAction::SetGlobalIndexed {
                    var: var.clone(),
                    index,
                    value,
                })
            }
            ActionFile::SetGlobal { var, value, .. } => {
                let value = match global_value(value) {
                    Ok(v) => v,
                    Err(e) => {
                        self.error(path, text, span, format!("set_global `{var}`: {e}"));
                        return None;
                    }
                };
                let difficulty = if var == "difficulty" {
                    let whole = whole_difficulty(&value);
                    if whole.is_none() {
                        self.error(
                            path,
                            text,
                            span,
                            "set_global `difficulty` sets the engine difficulty: a whole number from 0 to 255",
                        );
                        return None;
                    }
                    whole
                } else {
                    None
                };
                let var = *vars.get(var)?;
                Some(RoomAction::SetGlobal {
                    var,
                    value,
                    difficulty,
                })
            }
            ActionFile::Spawn {
                kind, x, y, params, ..
            } => {
                let k = self.action_kind("spawn", kind, path, text, span.clone())?;
                let base = self.kinds.get(k.index()?)?.base;
                let mut p = Params::default();
                for (key, value) in params {
                    match param_value(value) {
                        Ok(v) => {
                            let v = self.resolve_param(base, key, v, path);
                            p.0.insert(key.clone(), v);
                        }
                        Err(e) => self.error(path, text, span.clone(), format!("spawn param `{key}`: {e}")),
                    }
                }
                Some(RoomAction::Spawn {
                    kind: k,
                    x: Real::new(x.0),
                    y: Real::new(y.0),
                    params: p,
                })
            }
            ActionFile::LoadGame { .. } => Some(RoomAction::LoadGame),
            ActionFile::DestroySelf { .. } => Some(RoomAction::DestroySelf),
            ActionFile::SaveGame { .. } => Some(RoomAction::SaveGame),
            ActionFile::SaveDeathTime { .. } => Some(RoomAction::SaveDeathTime),
            ActionFile::MoveKid { x, y, .. } => Some(RoomAction::MoveKid {
                x: Real::new(x.0),
                y: Real::new(y.0),
            }),
            ActionFile::Destroy { kind, .. } => Some(RoomAction::Destroy {
                kind: self.action_kind("destroy", kind, path, text, span)?,
            }),
            ActionFile::PlaySound { sound, .. } => Some(RoomAction::PlaySound {
                sound: self.action_sound("play_sound", sound, path, text, span)?,
                looping: false,
            }),
            ActionFile::LoopSound { sound, .. } => Some(RoomAction::PlaySound {
                sound: self.action_sound("loop_sound", sound, path, text, span)?,
                looping: true,
            }),
            ActionFile::StopSound { sound, .. } => Some(RoomAction::StopSound {
                sound: self.action_sound("stop_sound", sound, path, text, span)?,
            }),
        }
    }

    /// Trace mode: a placed instance whose built-in refuses its params (a warp with no
    /// `room_to`, CR5) would stop the sim at room start. It becomes an instance of an inert twin
    /// of its kind (same sprite, mask, solid and classes; no events, no params), as an unmapped
    /// kind does. `params::check` already warns about the params.
    pub(super) fn inert_twins(&mut self, rooms: &mut [RoomDef]) {
        let mut twins: BTreeMap<KindId, KindId> = BTreeMap::new();
        for room in rooms.iter_mut() {
            for inst in &mut room.instances {
                let Some(kind) = self.kinds.get(inst.kind.index().unwrap_or(usize::MAX)) else {
                    continue;
                };
                if matches!(
                    kind.base,
                    Builtin::Kid | Builtin::Inert | Builtin::Scripted | Builtin::Probe
                ) {
                    continue;
                }
                let mut merged = kind.params.clone();
                for (k, v) in &inst.params.0 {
                    merged.0.insert(k.clone(), v.clone());
                }
                let base = kind.base;
                let refused = matches!(
                    pending_call(&mut self.pending, || builtins::init_state(base, &merged)),
                    Some(Err(_))
                );
                if !refused {
                    continue;
                }
                let twin = match twins.get(&inst.kind) {
                    Some(t) => *t,
                    None => {
                        let mut t = kind.clone();
                        t.name = format!("{}[inert]", t.name);
                        t.base = Builtin::Inert;
                        t.params = Params::default();
                        t.events = iwbtg_sim::events::EventSet::default();
                        let id = self.push_kind(t);
                        twins.insert(inst.kind, id);
                        id
                    }
                };
                inst.kind = twin;
                inst.params = Params::default();
            }
        }
    }

    /// CR5: warps whose creation code (`roomTo`) was not converted have no `room_to`.
    pub(super) fn warp_targets(&mut self, rooms: &[RoomDef]) {
        for r in rooms {
            let missing = r
                .instances
                .iter()
                .filter(|i| {
                    self.kinds
                        .get(i.kind.index().unwrap_or(usize::MAX))
                        .is_some_and(|k| {
                            k.base == Builtin::Warp
                                && !k.params.0.contains_key("room_to")
                                && !i.params.0.contains_key("room_to")
                        })
                })
                .count();
            if missing > 0 {
                self.trace_note("CR5", format!("{} in {}", missing, r.name));
            }
        }
    }
}
