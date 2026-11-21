//! Step 2: sprites and their masks (docs/format/assets.md).

use super::*;

/// A sprite's `mask` alias and where its key is written, for diagnostics.
struct MaskAlias {
    /// The sprite key the alias names (an engine pack sprite is prefixed with its pack).
    target: String,
    /// The sidecar path, for a diagnostic that points at the `mask` key.
    path: String,
    /// The `mask` key's byte span in that sidecar.
    span: Range<usize>,
}

/// Why a `mask` chain does not end at a sprite's own masks.
enum AliasFault<'a> {
    /// A sprite in the chain names itself.
    SelfAlias(&'a str),
    /// The chain returns to a sprite it already passed through.
    Cycle(Vec<String>),
}

/// Follows `start`'s `mask` chain to the sprite whose own masks it names.
///
/// A chain is resolved transitively (`a -> b -> c` gives `a` and `b` the masks of `c`), so the
/// result does not depend on the order the sidecars were visited. A self reference or a cycle has
/// no terminal sprite and is a load error.
fn resolve_alias<'a>(
    start: &'a str,
    aliases: &'a BTreeMap<String, MaskAlias>,
) -> Result<&'a str, AliasFault<'a>> {
    let mut chain = vec![start];
    let mut seen = BTreeSet::new();
    let mut key = start;
    loop {
        if !seen.insert(key) {
            return Err(AliasFault::Cycle(chain.iter().map(|k| (*k).to_owned()).collect()));
        }
        let Some(alias) = aliases.get(key) else {
            return Ok(key);
        };
        if alias.target == key {
            return Err(AliasFault::SelfAlias(key));
        }
        key = alias.target.as_str();
        chain.push(key);
    }
}

impl<'a> Loader<'a> {
    pub(super) fn load_sprites(&mut self) {
        // A package sprite is keyed by its id; a bundled engine pack sprite
        // (`engine/<pack>/sprites/<id>.toml`) by `engine:<pack>/<id>`, the way packages refer to it.
        let sidecars: Vec<(String, String, String, Option<String>)> = self
            .files
            .iter()
            .filter_map(|(p, _)| {
                if let Some(stem) = p.strip_prefix("sprites/").and_then(|s| s.strip_suffix(".toml")) {
                    return (!stem.contains('/'))
                        .then(|| (stem.to_owned(), stem.to_owned(), p.to_owned(), None));
                }
                let (pack, stem) = crate::engine_pack::sprite_sidecar(p)?;
                Some((
                    format!("{}{pack}/{stem}", crate::ids::ENGINE_PREFIX),
                    stem.to_owned(),
                    p.to_owned(),
                    Some(pack.to_owned()),
                ))
            })
            .collect();
        let mut aliases: BTreeMap<String, MaskAlias> = BTreeMap::new();
        let mut oversized = Vec::new();
        for (key, id, path, pack) in sidecars {
            let Some(text) = read_text(self.files, &path, &mut self.diags) else {
                continue;
            };
            if let Err(e) = check_id(&id) {
                self.diags
                    .push(Diagnostic::file_error(&path, format!("sprite {e}")));
                continue;
            }
            let (sidecar, spans) = match parse_sidecar(&path, text) {
                Ok(s) => s,
                Err(d) => {
                    self.diags.push(d);
                    continue;
                }
            };
            let png_path = match &pack {
                Some(pack) => format!("{}sprites/{id}.png", crate::engine_pack::pack_dir(pack)),
                None => format!("sprites/{id}.png"),
            };
            let png = self.files.get(&png_path);
            if png.is_none() && sidecar.frames > 0 && sidecar.collision.shape().is_some() {
                self.error(
                    &path,
                    text,
                    spans.frames.clone(),
                    format!("sprite `{id}` has no `{png_path}`"),
                );
                continue;
            }
            let (img_w, img_h) = sidecar.image_size();
            if img_w > u64::from(MAX_IMAGE_SIDE) || img_h > u64::from(MAX_IMAGE_SIDE) {
                if self.trace_mode() {
                    oversized.push(key.clone());
                } else {
                    self.error(
                        &path,
                        text,
                        spans.frames.clone(),
                        format!(
                            "the image is {img_w} x {img_h} px; images are at most {MAX_IMAGE_SIDE} px a side (set `columns` to wrap the frames into rows)"
                        ),
                    );
                    continue;
                }
            }
            match load_sprite_at(&png_path[..png_path.len() - 4], png.unwrap_or(&[]), &sidecar) {
                Ok((mut def, masks)) => {
                    let offset = u32::try_from(self.masks.len()).unwrap_or(u32::MAX);
                    def.masks = offset_masks(&def.masks, offset);
                    self.masks.extend(masks);
                    let sid = SpriteId(u32::try_from(self.sprite_ids.len()).unwrap_or(u32::MAX));
                    if let Some(alias) = sidecar.mask.clone() {
                        // A pack sprite's mask alias names a sprite of the same pack.
                        let target = match &pack {
                            Some(pack) => format!("{}{pack}/{alias}", crate::ids::ENGINE_PREFIX),
                            None => alias,
                        };
                        aliases.insert(
                            key.clone(),
                            MaskAlias {
                                target,
                                path: path.clone(),
                                span: spans.mask.clone().unwrap_or(0..0),
                            },
                        );
                    }
                    self.sprite_ids.insert(key.clone(), SpriteEntry { id: sid, def });
                }
                Err(d) => self.diags.push(d),
            }
        }
        // Sprite ids follow the key order of the map (package ids and `engine:` keys together).
        for (n, entry) in self.sprite_ids.values_mut().enumerate() {
            entry.id = SpriteId(u32::try_from(n).unwrap_or(u32::MAX));
        }
        self.resolve_mask_aliases(&aliases);
        for id in oversized {
            self.note("CR15", id);
        }
    }

    /// Applies every `mask` alias after all chains are resolved, so the masks a sprite gets do not
    /// depend on sidecar order. See [`resolve_alias`].
    fn resolve_mask_aliases(&mut self, aliases: &BTreeMap<String, MaskAlias>) {
        let mut resolved = Vec::with_capacity(aliases.len());
        for (id, alias) in aliases {
            let text = read_text(self.files, &alias.path, &mut Vec::new()).unwrap_or("");
            match resolve_alias(id, aliases) {
                Ok(base) => resolved.push((
                    id.clone(),
                    base.to_owned(),
                    alias.path.clone(),
                    alias.span.clone(),
                )),
                Err(AliasFault::SelfAlias(key)) if key == id => self.error(
                    &alias.path,
                    text,
                    alias.span.clone(),
                    format!("sprite `{id}` uses itself as its mask"),
                ),
                Err(AliasFault::SelfAlias(key)) => self.error(
                    &alias.path,
                    text,
                    alias.span.clone(),
                    format!("sprite `{id}`'s mask chain reaches `{key}`, which uses itself as its mask"),
                ),
                Err(AliasFault::Cycle(chain)) => self.error(
                    &alias.path,
                    text,
                    alias.span.clone(),
                    format!("sprite `{id}`'s mask chain {} is a cycle", chain.join(" -> ")),
                ),
            }
        }
        // Copy only after every chain is resolved: a copy never feeds another alias.
        for (id, base, path, span) in resolved {
            match self.sprite_ids.get(&base).map(|e| e.def.masks.clone()) {
                Some(masks) => {
                    if let Some(entry) = self.sprite_ids.get_mut(&id) {
                        entry.def.masks = masks;
                    }
                }
                None => {
                    let text = read_text(self.files, &path, &mut Vec::new()).unwrap_or("");
                    self.error(&path, text, span, format!("mask sprite `{base}` does not exist"));
                }
            }
        }
    }

    pub(super) fn sprites_in_order(&mut self) -> Vec<SpriteDef> {
        std::mem::take(&mut self.sprite_ids)
            .into_values()
            .map(|e| e.def)
            .collect()
    }

    /// A sprite by id, or an error at `span` of `file`.
    pub(super) fn sprite_ref(
        &mut self,
        name: &str,
        file: &str,
        text: &str,
        span: Range<usize>,
    ) -> Option<SpriteId> {
        match self.sprite_ids.get(name) {
            Some(e) => Some(e.id),
            None if name.starts_with(crate::ids::ENGINE_PREFIX) => {
                let msg = crate::engine_pack::unresolved(self.files, name, "sprite");
                self.error(file, text, span, msg);
                None
            }
            None => {
                self.error(file, text, span, format!("sprite `{name}` does not exist"));
                None
            }
        }
    }

    /// The mask a sprite gives an object-level `mask`: its shared mask, or its first frame's.
    pub(super) fn mask_of(&self, sprite: SpriteId) -> Option<MaskId> {
        let e = self.sprite_ids.values().find(|e| e.id == sprite)?;
        match &e.def.masks {
            MaskSet::Shared(m) => Some(*m),
            MaskSet::PerFrame(v) => v.first().copied(),
        }
    }
}
