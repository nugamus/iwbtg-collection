//! Step 3: kinds, from built-ins and `objects/<id>.toml` (docs/format/objects.md), their params,
//! variant and class-derived kinds, event sets and collision handler ties.

use super::*;

impl<'a> Loader<'a> {
    pub(super) fn push_kind(&mut self, kind: KindDef) -> KindId {
        let id = KindId(u32::try_from(self.kinds.len()).unwrap_or(u32::MAX));
        self.kind_ids.insert(kind.name.clone(), id);
        self.kinds.push(kind);
        id
    }

    pub(super) fn kind_mut(&mut self, id: KindId) -> Option<&mut KindDef> {
        self.kinds.get_mut(id.index()?)
    }

    /// One kind per package-nameable built-in, in `Builtin::ALL` order, with its defaults.
    pub(super) fn builtin_kinds(&mut self) {
        for b in Builtin::ALL {
            if Builtin::from_name(b.name()).is_none() {
                continue;
            }
            let mut k = KindDef::new(b.name(), b);
            k.classes = kind_classes(b, &Params::default(), ClassMask::NONE);
            k.solid = objects::default_solid(b);
            k.persistent = objects::default_persistent(b);
            self.push_kind(k);
        }
        let manifest = self.m;
        let text = self.manifest_text;
        for (name, d) in &manifest.defaults {
            let Some(&kid) = self.kind_ids.get(name.get_ref()) else {
                self.manifest_error(
                    name.span(),
                    format!("[defaults.{}]: not a built-in kind", name.get_ref()),
                );
                continue;
            };
            let d = d.get_ref();
            let mut out = Defaults::default();
            if let Some(s) = &d.sprite {
                out.sprite = self.sprite_ref(s.get_ref(), MANIFEST_PATH, text, s.span());
            }
            if let Some(s) = &d.mask {
                out.mask = self
                    .sprite_ref(s.get_ref(), MANIFEST_PATH, text, s.span())
                    .and_then(|s| self.mask_of(s));
            }
            for v in &d.variants {
                let mut when = Vec::new();
                let (mut sprite, mut mask) = (None, None);
                for (key, value) in v.get_ref() {
                    match (key.as_str(), value) {
                        ("sprite", toml::Value::String(s)) => {
                            sprite = self.sprite_ref(s, MANIFEST_PATH, text, v.span())
                        }
                        ("mask", toml::Value::String(s)) => {
                            mask = self
                                .sprite_ref(s, MANIFEST_PATH, text, v.span())
                                .and_then(|s| self.mask_of(s));
                        }
                        _ => match param_value(value) {
                            Ok(p) => when.push((key.clone(), p)),
                            Err(e) => self.manifest_error(v.span(), format!("variant param `{key}`: {e}")),
                        },
                    }
                }
                out.variants.push((when, sprite, mask));
            }
            self.apply_defaults(kid, out);
        }
        if self.trace_mode() {
            self.trace_defaults();
        }
    }

    pub(super) fn apply_defaults(&mut self, kind: KindId, d: Defaults) {
        if let Some(k) = self.kind_mut(kind) {
            k.sprite = d.sprite.or(k.sprite);
            k.mask = d.mask.or(k.mask);
        }
        self.defaults.insert(kind, d);
    }

    /// Trace mode: default sprites of bare built-ins from the trace map's `builtin` rows (CR12).
    pub(super) fn trace_defaults(&mut self) {
        let Some(trace) = &self.trace else {
            return;
        };
        let mut rows: Vec<(u32, String, Selector, String)> = trace
            .objects
            .values()
            .filter(|o| o.status == TraceStatus::Builtin && !o.kind.starts_with('@'))
            .filter_map(|o| {
                let sprite = o.sprite.clone()?;
                let params = o
                    .params
                    .iter()
                    .map(|(k, v)| (k.clone(), trace_param(v)))
                    .collect();
                Some((o.index.unwrap_or(u32::MAX), o.kind.clone(), params, sprite))
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut found: BTreeMap<KindId, Defaults> = BTreeMap::new();
        for (_, kind, params, sprite) in rows {
            let Some(&kid) = self.kind_ids.get(&kind) else {
                continue;
            };
            if self.defaults.contains_key(&kid) {
                continue;
            }
            let Some(sid) = self.sprite_ids.get(&sprite).map(|e| e.id) else {
                continue;
            };
            let d = found.entry(kid).or_default();
            if params.is_empty() {
                d.sprite.get_or_insert(sid);
            } else if !d.variants.iter().any(|(w, _, _)| *w == params) {
                d.variants.push((params, Some(sid), None));
            }
        }
        for (kid, d) in found {
            let name = self
                .kinds
                .get(kid.index().unwrap_or(usize::MAX))
                .map(|k| k.name.clone());
            self.note("CR12", name.unwrap_or_default());
            self.apply_defaults(kid, d);
        }
    }

    pub(super) fn load_objects(&mut self) {
        let paths: Vec<(String, String)> = self
            .files
            .iter()
            .filter_map(|(p, _)| {
                let stem = p.strip_prefix("objects/")?.strip_suffix(".toml")?;
                (!stem.contains('/')).then(|| (stem.to_owned(), p.to_owned()))
            })
            .collect();
        for (id, path) in paths {
            let Some(text) = read_text(self.files, &path, &mut self.diags) else {
                continue;
            };
            if let Err(e) = check_id(&id) {
                self.diags
                    .push(Diagnostic::file_error(&path, format!("object {e}")));
                continue;
            }
            match objects::parse_object(&id, &path, text) {
                Ok(o) => {
                    self.object_kind(&o, &path, text);
                    let name = format!("@{id}");
                    if !self.kind_ids.contains_key(&name) && !self.dropped.contains(&name) {
                        self.failed.insert(name);
                    }
                }
                Err(d) => {
                    self.diags.push(d);
                    self.failed.insert(format!("@{id}"));
                }
            }
        }
    }

    /// The trace map's status for `@id`.
    pub(super) fn trace_status(&self, kind: &str) -> Option<TraceStatus> {
        self.trace.as_ref()?.object_for_kind(kind).map(|(_, o)| o.status)
    }

    pub(super) fn object_kind(&mut self, o: &ObjectFile, path: &str, text: &str) {
        let name = format!("@{}", o.id);
        let script_name = o.script.as_ref().map(|s| s.get_ref().as_str());
        // CR3: engine plumbing never loads as a script.
        let infra = matches!(script_name, Some("world" | "init"))
            || self.trace_status(&name) == Some(TraceStatus::Infra);
        let mut base = None;
        let mut script = None;
        if infra {
            let engine = [o.script.as_ref(), o.base.as_ref()]
                .into_iter()
                .flatten()
                .filter_map(|s| Builtin::from_name(s.get_ref()))
                .find(|b| engine_builtin(*b));
            match engine {
                Some(b) => {
                    self.note("CR3", format!("{name} as built-in `{}`", b.name()));
                    base = Some(b);
                }
                None => {
                    self.note("CR3", format!("{name} dropped"));
                    self.dropped.insert(name);
                    return;
                }
            }
        } else {
            if let Some(b) = &o.base {
                match Builtin::from_name(b.get_ref()) {
                    Some(builtin) => base = Some(builtin),
                    None if self.trace_mode() => {
                        self.note("CR10", format!("{name} base `{}`", b.get_ref()));
                        base = Some(Builtin::Inert);
                    }
                    None => {
                        // The accepted set is the kind names plus the additive behaviour bases
                        // (docs/format/builtins.md, decision 0005); name both so a typo of
                        // either is diagnosable.
                        let bases: Vec<&str> = iwbtg_sim::def::traits::BEHAVIOUR_BASES
                            .iter()
                            .map(|(name, _, _)| *name)
                            .collect();
                        self.error(
                            path,
                            text,
                            b.span(),
                            format!(
                                "`{}` is not a built-in kind or a behaviour base ({})",
                                b.get_ref(),
                                bases.join(", ")
                            ),
                        );
                        return;
                    }
                }
            }
            if let Some(s) = &o.script {
                let script_path = format!("scripts/{}.rhai", s.get_ref());
                if let Err(e) = check_id(s.get_ref()) {
                    self.error(path, text, s.span(), format!("script {e}"));
                    return;
                }
                match read_text(self.files, &script_path, &mut Vec::new()) {
                    Some(source) if source.len() > MAX_SCRIPT_BYTES => {
                        self.error(path, text, s.span(), format!("`{script_path}` is over 256 KB"));
                        return;
                    }
                    Some(source) => {
                        let sid = ScriptId(u32::try_from(self.scripts.len()).unwrap_or(u32::MAX));
                        self.scripts.push(ScriptDef {
                            name: script_path,
                            source: source.to_owned(),
                        });
                        script = Some(sid);
                        base.get_or_insert(Builtin::Scripted);
                    }
                    None => match self.mode {
                        LoadMode::Trace {
                            unmapped_kinds_inert: true,
                        } => {
                            self.note("CR1, CR2", name.clone());
                            base = Some(Builtin::Inert);
                        }
                        _ => {
                            self.error(path, text, s.span(), format!("`{script_path}` does not exist"));
                            return;
                        }
                    },
                }
            }
        }
        let Some(base) = base else {
            self.diags
                .push(Diagnostic::file_error(path, "an object needs `base` or `script`"));
            return;
        };
        let mut kind = KindDef::new(name.clone(), base);
        kind.script = script;
        if let Some(s) = &o.sprite {
            kind.sprite = self.sprite_ref(s.get_ref(), path, text, s.span());
        }
        if let Some(s) = &o.mask {
            kind.mask = self
                .sprite_ref(s.get_ref(), path, text, s.span())
                .and_then(|s| self.mask_of(s));
        }
        kind.solid = o.solid.unwrap_or_else(|| objects::default_solid(kind.base));
        kind.visible = o.visible.unwrap_or(true);
        kind.depth = o.depth.as_ref().map_or(Real::ZERO, |d| Real::new(*d.get_ref()));
        kind.persistent = o
            .persistent
            .unwrap_or_else(|| objects::default_persistent(kind.base));
        if let Some(name) = &o.timeline {
            match self.timeline_ids.get(name.get_ref()).copied() {
                Some(id) => kind.timeline = Some(id),
                None => self.error(
                    path,
                    text,
                    name.span(),
                    format!("unknown timeline `{}`", name.get_ref()),
                ),
            }
        }
        for (key, value) in &o.params {
            match param_value(value.get_ref()) {
                Ok(p) => {
                    kind.params.0.insert(key.clone(), p);
                }
                Err(e) => self.error(path, text, value.span(), format!("param `{key}`: {e}")),
            }
        }
        let mut extra = ClassMask::NONE;
        for c in &o.counts_as {
            match self.class_named(c.get_ref()) {
                Some(m) => extra |= m,
                None => self.error(
                    path,
                    text,
                    c.span(),
                    format!(
                        "too many package classes for `{}` (at most {})",
                        c.get_ref(),
                        ClassMask::PACKAGE_COUNT
                    ),
                ),
            }
        }
        let mut collides = Vec::new();
        for c in &o.collides_with {
            match self.class_named(c.get_ref()) {
                Some(m) => collides.push(m),
                None => self.error(
                    path,
                    text,
                    c.span(),
                    format!(
                        "too many package classes for `{}` (at most {})",
                        c.get_ref(),
                        ClassMask::PACKAGE_COUNT
                    ),
                ),
            }
        }
        if !collides.is_empty() {
            self.collides.insert(name.clone(), collides);
        }
        if !o.roles.is_empty() {
            if self.trace_mode() {
                self.note("CR11", name.clone());
                for r in &o.roles {
                    extra |= legacy_role_classes(r.get_ref()).unwrap_or(ClassMask::NONE);
                }
            } else {
                self.error(
                    path,
                    text,
                    o.roles[0].span(),
                    "`roles` is not a v0 key; use `counts_as`",
                );
            }
        }
        kind.classes = kind_classes(base, &kind.params, extra);
        let id = self.push_kind(kind);
        if extra != ClassMask::NONE {
            self.kind_extra.insert(id, extra);
        }
    }

    /// A named class, or the next package class bit for a new name.
    pub(super) fn class_named(&mut self, name: &str) -> Option<ClassMask> {
        if let Some(c) = ClassMask::from_name(name) {
            return Some(c);
        }
        let n = self
            .package_classes
            .iter()
            .position(|c| c == name)
            .unwrap_or_else(|| {
                self.package_classes.push(name.to_owned());
                self.package_classes.len() - 1
            });
        ClassMask::package(u32::try_from(n).ok()?)
    }

    /// Resolves kind params whose spec type names a room, kind or sound.
    pub(super) fn resolve_kind_params(&mut self) {
        for i in 0..self.kinds.len() {
            let (base, params, name) = {
                let k = &self.kinds[i];
                (k.base, k.params.clone(), k.name.clone())
            };
            let file = name.strip_prefix('@').map(|id| format!("objects/{id}.toml"));
            let mut resolved = Params::default();
            for (key, value) in params.0 {
                let v = self.resolve_param(base, &key, value, file.as_deref().unwrap_or(MANIFEST_PATH));
                resolved.0.insert(key, v);
            }
            self.kinds[i].params = resolved;
        }
    }

    /// The param's spec type, if the base declares one.
    pub(super) fn spec_type(&mut self, base: Builtin, key: &str) -> Option<ParamType> {
        let specs = match self.spec_cache.get(&base) {
            Some(s) => *s,
            None => {
                let s = builtins::param_specs(base);
                self.spec_cache.insert(base, s);
                s
            }
        };
        specs.iter().find(|s| s.name == key).map(|s| s.ty)
    }

    /// Turns a text param into a room, kind or sound reference when the base's spec says so; a
    /// list param resolves each of its values by the list's inner type.
    pub(super) fn resolve_param(
        &mut self,
        base: Builtin,
        key: &str,
        value: ParamValue,
        file: &str,
    ) -> ParamValue {
        let ty = self.spec_type(base, key);
        match (ty, value) {
            (Some(ParamType::List(inner)), ParamValue::List(items)) => ParamValue::List(
                items
                    .into_iter()
                    .map(|v| self.resolve_ref(Some(*inner), key, v, file))
                    .collect(),
            ),
            (Some(ParamType::List(inner)), v) => self.resolve_ref(Some(*inner), key, v, file),
            (ty, v) => self.resolve_ref(ty, key, v, file),
        }
    }

    /// Turns text into a reference of type `ty` (a room, kind, sound or particles file).
    pub(super) fn resolve_ref(
        &mut self,
        ty: Option<ParamType>,
        key: &str,
        value: ParamValue,
        file: &str,
    ) -> ParamValue {
        let ParamValue::Str(s) = &value else {
            return value;
        };
        let found = match ty {
            Some(ParamType::Room) => self.room_ids.get(s).copied().map(ParamValue::Room),
            Some(ParamType::Kind) => self.kind_by_ref(s).map(ParamValue::Kind),
            Some(ParamType::Sound) => self
                .sounds
                .iter()
                .position(|d| d.name == *s)
                .and_then(|i| u32::try_from(i).ok())
                .map(|i| ParamValue::Sound(iwbtg_sim::def::SoundId(i))),
            Some(ParamType::Particles) => self
                .particles
                .iter()
                .position(|d| d.name == *s)
                .and_then(|i| u32::try_from(i).ok())
                .map(|i| ParamValue::Particles(iwbtg_sim::def::PartSystemDefId(i))),
            _ => return value,
        };
        match found {
            Some(v) => v,
            None => {
                let why = if s.starts_with(crate::ids::ENGINE_PREFIX) {
                    crate::engine_pack::unresolved(self.files, s, "sound")
                } else {
                    format!("`{s}` does not exist")
                };
                self.diags
                    .push(Diagnostic::file_error(file, format!("param `{key}`: {why}")));
                value
            }
        }
    }

    /// A kind by reference (`@id` or a bare built-in). In Trace mode an unknown bare name becomes
    /// an inert kind of that name (CR10).
    pub(super) fn kind_by_ref(&mut self, name: &str) -> Option<KindId> {
        if let Some(&k) = self.kind_ids.get(name) {
            return Some(k);
        }
        if name.starts_with('@') || !self.trace_mode() || check_id(name).is_err() {
            return None;
        }
        self.note("CR10", name.to_owned());
        Some(self.push_kind(KindDef::new(name, Builtin::Inert)))
    }

    /// The kind an instance uses: `@id`, a bare built-in or (bare built-in with matching variant
    /// defaults) a derived variant kind, then (params that pick other classes) a kind derived
    /// with those classes. `Err(true)` when the kind was dropped (CR3).
    pub(super) fn instance_kind(&mut self, name: &str, params: &Params) -> Result<KindId, bool> {
        let kind = self.variant_kind(name, params)?;
        Ok(self.class_kind(kind, params))
    }

    /// `kind`, or a kind derived from it when the instance's params that pick classes or events
    /// (`objects::kind_params`) give other classes or another event set than the kind's (#659,
    /// #853): `<kind>[k=v,...]` with those params, ranked by the kind's `object_order` entries.
    /// Instance classes and events come from the kind (`World::spawn`, `KindDef::events`), as a
    /// GM instance's come from its object.
    pub(super) fn class_kind(&mut self, kind: KindId, params: &Params) -> KindId {
        let Some(k) = self.kinds.get(kind.index().unwrap_or(usize::MAX)) else {
            return kind;
        };
        let picked: Vec<(String, ParamValue)> = objects::kind_params(k.base)
            .into_iter()
            .filter_map(|key| params.0.get(key).map(|v| (key.to_owned(), v.clone())))
            .collect();
        if picked.is_empty() {
            return kind;
        }
        let mut merged = k.params.clone();
        merged.0.extend(picked.iter().cloned());
        let extra = self.kind_extra.get(&kind).copied().unwrap_or(ClassMask::NONE);
        if !objects::picks_other_kind(k, &merged, extra) {
            return kind;
        }
        let classes = kind_classes(k.base, &merged, extra);
        let tag = picked
            .iter()
            .map(|(key, v)| format!("{key}={}", show_param(v)))
            .collect::<Vec<_>>()
            .join(",");
        if let Some(&d) = self.class_derived.get(&(kind, tag.clone())) {
            return d;
        }
        let mut derived = k.clone();
        derived.name = format!("{}[{tag}]", derived.name);
        derived.params = merged;
        derived.classes = classes;
        let rank = self.rank_kind.get(&kind).copied().unwrap_or(kind);
        let id = self.push_kind(derived);
        if extra != ClassMask::NONE {
            self.kind_extra.insert(id, extra);
        }
        self.class_derived.insert((kind, tag), id);
        self.rank_kind.insert(id, rank);
        id
    }

    /// `instance_kind` before the class-picking params: the named kind or its defaults variant.
    pub(super) fn variant_kind(&mut self, name: &str, params: &Params) -> Result<KindId, bool> {
        if self.dropped.contains(name) {
            return Err(true);
        }
        let Some(kind) = self.kind_by_ref(name) else {
            return Err(false);
        };
        let Some(defaults) = self.defaults.get(&kind) else {
            return Ok(kind);
        };
        let kind_params = self
            .kinds
            .get(kind.index().unwrap_or(usize::MAX))
            .map(|k| k.params.clone())
            .unwrap_or_default();
        let Some(index) = defaults
            .variants
            .iter()
            .position(|(w, _, _)| variant_matches(w, params, &kind_params))
        else {
            return Ok(kind);
        };
        Ok(self.derive_variant(kind, index).unwrap_or(kind))
    }

    /// The kind derived for `kind`'s `[defaults]` variant `index`, made on first use: the kind
    /// with the variant's params, sprite and mask, named `<kind>[<params>]`.
    pub(super) fn derive_variant(&mut self, kind: KindId, index: usize) -> Option<KindId> {
        if let Some(&d) = self.derived.get(&(kind, index)) {
            return Some(d);
        }
        let (when, sprite, mask) = self.defaults.get(&kind)?.variants.get(index)?.clone();
        let mut derived = self.kinds.get(kind.index()?).cloned()?;
        let tag: Vec<String> = when
            .iter()
            .map(|(k, v)| format!("{k}={}", show_param(v)))
            .collect();
        derived.name = format!("{}[{}]", derived.name, tag.join(","));
        derived.sprite = sprite.or(derived.sprite);
        derived.mask = mask.or(derived.mask);
        for (k, v) in when {
            derived.params.0.insert(k, v);
        }
        derived.classes = kind_classes(derived.base, &derived.params, ClassMask::NONE);
        let id = self.push_kind(derived);
        self.derived.insert((kind, index), id);
        self.rank_kind.insert(id, kind);
        Some(id)
    }

    /// Every `[defaults]` variant with its derived kind, for `GameDef::variants`. Runs after the
    /// rooms, so the variants no room places get their kinds last and the placed kinds' ids do
    /// not move. A script `instance_create` takes them (#701), ranked as the kind's
    /// `object_order` entry for the variant's params, when it has one.
    pub(super) fn all_variants(&mut self) -> Vec<KindVariant> {
        let wanted: Vec<(KindId, usize)> = self
            .defaults
            .iter()
            .flat_map(|(k, d)| (0..d.variants.len()).map(move |i| (*k, i)))
            .collect();
        let mut out = Vec::new();
        for (kind, index) in wanted {
            let Some(variant) = self.derive_variant(kind, index) else {
                continue;
            };
            let Some((when, _, _)) = self.defaults.get(&kind).and_then(|d| d.variants.get(index)) else {
                continue;
            };
            out.push(KindVariant {
                kind,
                when: when.clone(),
                variant,
            });
            if self.ranks.entries.iter().any(|e| e.kind == variant) {
                continue;
            }
            let params = self
                .kinds
                .get(variant.index().unwrap_or(usize::MAX))
                .map(|k| k.params.clone())
                .unwrap_or_default();
            if let Some(rank) = rank_of(&self.ranks, kind, &[&params]) {
                self.ranks.entries.push(RankEntry {
                    kind: variant,
                    variant: None,
                    rank,
                });
            }
        }
        out
    }

    pub(super) fn event_sets(&mut self, physics: &PhysicsProfile) {
        for i in 0..self.kinds.len() {
            let kind = self.kinds[i].clone();
            let mut set = if kind.base == Builtin::Kid {
                iwbtg_sim::kid::event_set(physics)
            } else {
                builtins::event_set(kind.base, &kind)
            };
            // A derived kind (`<kind>[...]`) listens to what its kind's script does.
            let root = u32::try_from(i)
                .ok()
                .and_then(|i| self.rank_kind.get(&KindId(i)))
                .and_then(|r| r.index());
            let name = root
                .and_then(|r| self.kinds.get(r))
                .map_or(&kind.name, |r| &r.name);
            if let Some(masks) = self.collides.get(name) {
                set.collisions.extend(masks.iter().copied());
            }
            self.kinds[i].events = set;
        }
    }

    /// Adds what each kind's script handles to its event set (the `scripted` built-in and a
    /// base with a script): the script's functions, from the compiled script. Needs the finished
    /// definition, so it runs after it is built. A script that does not compile is an error.
    pub(super) fn merge_script_events(&mut self, def: &mut GameDef) {
        if def.scripts.is_empty() {
            return;
        }
        let rules = match RunnerRules::for_runner(def.runner, def.engine_rev) {
            Ok(mut r) => {
                r.num.float_model = def.float_model;
                r
            }
            Err(e) => {
                self.diags
                    .push(Diagnostic::file_error(MANIFEST_PATH, format!("scripts: {e}")));
                return;
            }
        };
        let runtime = match ScriptRuntime::new(def, &rules) {
            Ok(r) => r,
            Err(e) => {
                self.diags
                    .push(Diagnostic::file_error(MANIFEST_PATH, format!("scripts: {e}")));
                return;
            }
        };
        for k in &mut def.kinds {
            let Some(sid) = k.script else { continue };
            let s = runtime.event_set(sid);
            let e = &mut k.events;
            e.begin_step |= s.begin_step;
            e.step |= s.step;
            e.end_step |= s.end_step;
            e.late |= s.late;
            e.alarms |= s.alarms;
            e.user |= s.user;
            e.outside_room |= s.outside_room;
            e.intersect_boundary |= s.intersect_boundary;
            e.animation_end |= s.animation_end;
            e.room_start |= s.room_start;
            e.room_end |= s.room_end;
            e.moments.extend(s.moments);
        }
    }

    /// A warning for each kind whose classes match two equally specific collision handlers of a
    /// kind ([`crate::ties`]): the sim runs the first in handler order, where GM would run the
    /// nearest ancestor's. Kinds the loader derived (`name[...]`) report under their base name,
    /// once, on the object file when there is one.
    pub(super) fn handler_ties(&mut self, def: &GameDef) {
        let base = |id: KindId| {
            def.kind(id)
                .map_or("", |k| k.name.split('[').next().unwrap_or(&k.name))
                .to_owned()
        };
        let mut seen: BTreeSet<(String, String, ClassMask, ClassMask)> = BTreeSet::new();
        for t in crate::ties::ties(def) {
            let (handler, other) = (base(t.handler), base(t.other));
            if !seen.insert((handler.clone(), other.clone(), t.picked, t.tied)) {
                continue;
            }
            let object = other.strip_prefix('@').map(|id| format!("objects/{id}.toml"));
            let file = object
                .as_deref()
                .filter(|o| self.files.contains(o))
                .unwrap_or(MANIFEST_PATH);
            let (picked, tied) = (self.class_list(t.picked), self.class_list(t.tied));
            self.diags.push(Diagnostic::file_warning(
                file,
                format!(
                    "kind `{other}` matches two equally specific collision handlers of `{handler}`, `{picked}` and `{tied}`: only `{picked}` runs, by handler order, where GM runs the nearest parent's (docs/format/validation.md, step 7; sim register C8)"
                ),
            ));
        }
    }

    /// A class mask as its package-format names, `a | b`.
    pub(super) fn class_list(&self, mask: ClassMask) -> String {
        let mut names: Vec<String> = ClassMask::NAMED
            .iter()
            .filter(|(c, _)| mask.intersects(*c))
            .map(|(_, n)| (*n).to_owned())
            .collect();
        for (n, name) in self.package_classes.iter().enumerate() {
            if let Some(c) = u32::try_from(n).ok().and_then(ClassMask::package)
                && mask.intersects(c)
            {
                names.push(name.clone());
            }
        }
        names.join(" | ")
    }
}
