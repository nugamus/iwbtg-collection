//! Step 2: sprites and their masks (docs/format/assets.md).

use super::*;

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
        let mut aliases = Vec::new();
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
                        let alias = match &pack {
                            Some(pack) => format!("{}{pack}/{alias}", crate::ids::ENGINE_PREFIX),
                            None => alias,
                        };
                        aliases.push((
                            key.clone(),
                            alias,
                            path.clone(),
                            spans.mask.clone().unwrap_or(0..0),
                        ));
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
        for (id, alias, path, span) in aliases {
            match self.sprite_ids.get(&alias).map(|e| e.def.masks.clone()) {
                Some(masks) => {
                    if let Some(e) = self.sprite_ids.get_mut(&id) {
                        e.def.masks = masks;
                    }
                }
                None => {
                    let text = read_text(self.files, &path, &mut Vec::new()).unwrap_or("");
                    self.error(&path, text, span, format!("mask sprite `{alias}` does not exist"));
                }
            }
        }
        for id in oversized {
            self.note("CR15", id);
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
