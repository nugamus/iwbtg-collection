//! Step 5: `object_order` ranks (docs/format/game.md), engine kinds, boot and spawnable ranks.

use super::*;

impl<'a> Loader<'a> {
    pub(super) fn load_order(&mut self) {
        let mut entries = Vec::new();
        let manifest = self.m;
        for item in manifest.object_order.get_ref() {
            let Some(name) = item.get_ref().kind() else {
                continue;
            };
            if self.dropped.contains(name) || self.failed.contains(name) {
                continue;
            }
            if self.kind_by_ref(name).is_none() {
                self.manifest_error(
                    item.span(),
                    format!("object_order names `{name}`, which is not a kind of this package"),
                );
                continue;
            }
            let variant = match item.get_ref().selector() {
                Ok(Some((key, value))) => match param_value(value) {
                    Ok(v) => Some(VariantSel {
                        key: key.to_owned(),
                        value: v,
                    }),
                    Err(e) => {
                        self.manifest_error(item.span(), e);
                        continue;
                    }
                },
                Ok(None) => None,
                Err(e) => {
                    self.manifest_error(item.span(), e);
                    continue;
                }
            };
            entries.push(OrderEntry {
                kind: name.to_owned(),
                variant,
            });
        }
        match resolve_ranks(&entries, &self.kinds) {
            Ok(t) => self.ranks = t,
            Err(d) => self.diags.push(d),
        }
    }

    /// The rank of an instance of `kind` with these params, or a CR4 rank after the listed
    /// entries (Trace) or an error (Strict, reported once per kind by the caller).
    pub(super) fn rank_or_append(&mut self, kind: KindId, params: &[&Params]) -> Option<u16> {
        let rank_kind = self.rank_kind.get(&kind).copied().unwrap_or(kind);
        let kind_params = self
            .kinds
            .get(kind.index()?)
            .map(|k| k.params.clone())
            .unwrap_or_default();
        let mut all: Vec<&Params> = vec![&kind_params];
        all.extend_from_slice(params);
        if let Some(r) = rank_of(&self.ranks, rank_kind, &all) {
            if rank_kind != kind && !self.ranks.entries.iter().any(|e| e.kind == kind) {
                self.ranks.entries.push(RankEntry {
                    kind,
                    variant: None,
                    rank: r,
                });
            }
            return Some(r);
        }
        let name = self.kinds.get(kind.index()?)?.name.clone();
        if !self.trace_mode() {
            return None;
        }
        let next = self
            .ranks
            .entries
            .iter()
            .map(|e| e.rank)
            .max()
            .map_or(0, |r| r.saturating_add(1));
        self.ranks.entries.push(RankEntry {
            kind: rank_kind,
            variant: None,
            rank: next,
        });
        if rank_kind != kind {
            self.ranks.entries.push(RankEntry {
                kind,
                variant: None,
                rank: next,
            });
        }
        self.appended.insert(rank_kind);
        self.note("CR4", name);
        Some(next)
    }

    pub(super) fn engine_kinds(&mut self) -> BTreeMap<EngineKind, KindId> {
        let manifest = self.m;
        let roles = manifest.engine.as_ref().map(|e| &e.get_ref().roles);
        if let Some(roles) = roles {
            for key in roles.keys() {
                if !ENGINE_ROLES.contains(&key.get_ref().as_str()) {
                    self.manifest_error(
                        key.span(),
                        format!(
                            "`{}` is not an engine role ({})",
                            key.get_ref(),
                            ENGINE_ROLES.join(", ")
                        ),
                    );
                }
            }
        }
        let mut out = BTreeMap::new();
        for (role, ek, builtin) in engine_roles() {
            let set = roles
                .and_then(|r| r.iter().find(|(k, _)| k.get_ref() == role))
                .map(|(_, v)| v);
            let kind = match set {
                Some(v) if v.get_ref() == "none" => None,
                Some(v) => {
                    let k = self
                        .kind_by_ref(v.get_ref())
                        .filter(|_| !self.dropped.contains(v.get_ref()));
                    if k.is_none() {
                        self.manifest_error(
                            v.span(),
                            format!("engine role `{role}`: `{}` is not a kind", v.get_ref()),
                        );
                    }
                    k
                }
                None => builtin.and_then(|b| self.default_engine_kind(b)),
            };
            if let Some(k) = kind {
                out.insert(ek, k);
            }
        }
        out
    }

    /// The kind an engine role uses by default: the one package object with that base (`@bullet`
    /// for `kid_bullet`, `@world` after CR3), else the earliest-ranked of several, else the bare
    /// built-in.
    pub(super) fn default_engine_kind(&self, b: Builtin) -> Option<KindId> {
        let candidates: Vec<KindId> = self
            .kinds
            .iter()
            .enumerate()
            .filter(|(_, k)| k.base == b && k.name.starts_with('@'))
            .filter_map(|(i, _)| u32::try_from(i).ok().map(KindId))
            .collect();
        match candidates.as_slice() {
            [] => self.kind_ids.get(b.name()).copied(),
            [one] => Some(*one),
            many => many
                .iter()
                .copied()
                .min_by_key(|k| self.ranks.rank_for(*k, None).unwrap_or(u16::MAX)),
        }
    }

    pub(super) fn boot(&mut self, engine: &BTreeMap<EngineKind, KindId>) -> Vec<KindId> {
        let manifest = self.m;
        match manifest.engine.as_ref().and_then(|e| e.get_ref().boot.as_ref()) {
            Some(list) => list
                .iter()
                .filter_map(|b| {
                    let k = self.kind_by_ref(b.get_ref());
                    if k.is_none() {
                        self.manifest_error(b.span(), format!("boot kind `{}` does not exist", b.get_ref()));
                    }
                    k
                })
                .collect(),
            None => engine.get(&EngineKind::World).copied().into_iter().collect(),
        }
    }

    /// CR4: engine kinds, boot kinds, kinds that room actions spawn and kind-typed params must
    /// have a rank.
    pub(super) fn spawnable_ranks(&mut self, rooms: &[RoomDef], engine: &BTreeMap<EngineKind, KindId>) {
        let mut need: BTreeSet<KindId> = engine.values().copied().collect();
        for r in rooms {
            for a in &r.on_create {
                let mut a = a;
                while let RoomAction::When { action, .. } = a {
                    a = action;
                }
                if let RoomAction::Spawn { kind, .. } = a {
                    need.insert(*kind);
                }
            }
            for i in &r.instances {
                need.extend(i.params.0.values().filter_map(kind_param));
            }
        }
        for k in &self.kinds {
            need.extend(k.params.0.values().filter_map(kind_param));
        }
        if let Some(boot) = self.m.engine.as_ref().and_then(|e| e.get_ref().boot.as_ref()) {
            need.extend(
                boot.iter()
                    .filter_map(|b| self.kind_ids.get(b.get_ref()).copied()),
            );
        }
        for kind in need {
            if self.ranks.entries.iter().any(|e| e.kind == kind) {
                continue;
            }
            if self.rank_or_append(kind, &[]).is_none() {
                let name = self
                    .kinds
                    .get(kind.index().unwrap_or(usize::MAX))
                    .map(|k| k.name.clone())
                    .unwrap_or_default();
                let span = self.m.object_order.span();
                self.manifest_error(
                    span,
                    format!("`{name}` can be spawned but object_order does not list it (CR4)"),
                );
            }
        }
    }
}
