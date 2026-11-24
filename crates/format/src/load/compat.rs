//! `[compat]` against the resolved physics (decision 0005, #1160).
//!
//! The manifest validator has already checked the closed deviation catalogue, the types and the
//! provenance. Here the loader checks the two things that need the package's resolved profile:
//! the profile bundle names the physics preset's family, and every deviation names the value the
//! package already resolves to. A format-0 declaration therefore cannot change an outcome, and
//! [`crate::load::gameplay_hash`] normalizes it away.

use super::*;
use crate::compat as catalogue;

impl Loader<'_> {
    /// Checks the explicit `[compat] profile` and deviations against `physics` and every room
    /// profile. Called once, after the room profiles are loaded.
    pub(super) fn check_compat(&mut self, physics: &PhysicsProfile) {
        let m = self.m;
        let Some(compat) = &m.compat else { return };
        let c = compat.get_ref();
        let mut errors: Vec<(Range<usize>, String)> = Vec::new();
        if let Some(profile) = &c.profile
            && let Ok(p) = catalogue::Profile::parse(profile.get_ref())
        {
            let expected = catalogue::family_name(physics.preset);
            if p.family != expected {
                errors.push((
                    profile.span(),
                    format!(
                        "profile `{}` names family `{}`, but the physics preset is `{expected}`; \
                         in format 0 the profile is the preset's bundle",
                        p.id(),
                        p.family
                    ),
                ));
            }
        }
        for dev in &c.deviation {
            let id = dev.id.get_ref();
            let Some(declared) = dev.value.get_ref().as_str() else {
                continue;
            };
            let Some(resolved) = catalogue::resolved(id, physics) else {
                continue;
            };
            if declared != resolved {
                errors.push((
                    dev.value.span(),
                    format!(
                        "deviation `{id}` declares `{declared}`, but the package resolves it to \
                         `{resolved}`; in format 0 a declaration must name the resolved value \
                         (set it in `physics.toml` instead)"
                    ),
                ));
            }
            if let Some(room) = self
                .room_physics
                .iter()
                .find(|room| catalogue::resolved(id, room).is_some_and(|v| v != resolved))
            {
                let room_value = catalogue::resolved(id, room).unwrap_or_default();
                errors.push((
                    dev.id.span(),
                    format!(
                        "deviation `{id}` cannot be package-wide: a room profile resolves it to \
                         `{room_value}` and the game to `{resolved}`"
                    ),
                ));
            }
        }
        for (span, message) in errors {
            self.manifest_error(span, message);
        }
    }
}
