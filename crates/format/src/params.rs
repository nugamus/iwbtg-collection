//! Param validation against `builtins::param_specs` (docs/format/builtins.md).
//!
//! Every kind's params, and every placed instance's params merged over its kind's (a placed
//! instance overrides), are checked: each key must be a param of the base built-in, its value
//! must have the spec's type (an enum word must be one of the words), and the built-in's own
//! `init_state` must accept the set (required params, combinations). A kind no room places is
//! checked on its own params, ignoring required ones it lacks (an instance may give them).
//!
//! Kinds with no spec (`kid`, `inert`, scripted, probe) are skipped. In `LoadMode::Trace`
//! converter output may not match the specs yet, so the problems are warnings.
//!
//! A format-0 package may still use a **move** param (the survey's one-game parameters): the
//! loader keeps each as a documented legacy alias so old packages and their `gameplay_hash` do
//! not change (#1076, #1375). A use by a package whose id is not one of the alias's games is a
//! warning that names the replacing issue; the alias, not the use, is the thing to remove later.

use std::collections::BTreeSet;

use iwbtg_sim::builtins::{self, ParamSpec, ParamType};
use iwbtg_sim::def::{Builtin, GameDef, KindDef, KindId, ParamValue, Params};

use crate::diag::{Diagnostic, Severity};

/// Where a kind's own params were written.
fn kind_file(k: &KindDef) -> String {
    match k.name.strip_prefix('@') {
        Some(id) => format!("objects/{id}.toml"),
        None => "game.toml".to_owned(),
    }
}

fn report(diags: &mut Vec<Diagnostic>, severity: Severity, file: &str, message: String) {
    diags.push(Diagnostic {
        file: file.to_owned(),
        line: 1,
        column: 1,
        severity,
        message,
    });
}

/// Why `value` does not fit `spec`, if it does not.
fn type_problem(spec: &ParamSpec, value: &ParamValue) -> Option<String> {
    problem(spec.name, spec.ty, value)
}

/// Why `value` does not fit type `ty` of param `name`, if it does not. A list param takes a list
/// of at least one value of its inner type, or a single value.
fn problem(name: &str, ty: ParamType, value: &ParamValue) -> Option<String> {
    let wrong = |want: &str| Some(format!("param `{name}` must be {want}"));
    match (ty, value) {
        (ParamType::List(_), ParamValue::List(items)) if items.is_empty() => Some(format!(
            "param `{name}` is an empty list; give at least one value or leave it out"
        )),
        (ParamType::List(inner), ParamValue::List(items)) => {
            items.iter().find_map(|v| problem(name, *inner, v))
        }
        (ParamType::List(inner), v) => problem(name, *inner, v),
        (ParamType::Real, ParamValue::Real(_))
        | (ParamType::Bool, ParamValue::Bool(_))
        | (ParamType::Room, ParamValue::Room(_))
        | (ParamType::Kind, ParamValue::Kind(_))
        | (ParamType::Sound, ParamValue::Sound(_))
        | (ParamType::Particles, ParamValue::Particles(_))
        | (ParamType::Text, ParamValue::Str(_)) => None,
        // A reference the loader could not resolve is already an error of its own.
        (ParamType::Room | ParamType::Kind | ParamType::Sound | ParamType::Particles, ParamValue::Str(_)) => {
            None
        }
        (ParamType::Enum(words), ParamValue::Str(s)) => {
            if words.contains(&s.as_str()) {
                None
            } else {
                Some(format!(
                    "param `{name}` is `{s}`; it must be one of: {}",
                    words.join(", ")
                ))
            }
        }
        (ParamType::Real, _) => wrong("a number"),
        (ParamType::Bool, _) => wrong("true or false"),
        (ParamType::Enum(words), _) => wrong(&format!("one of: {}", words.join(", "))),
        (ParamType::Room, _) => wrong("a room"),
        (ParamType::Kind, _) => wrong("a kind"),
        (ParamType::Sound, _) => wrong("a sound"),
        (ParamType::Text, _) => wrong("text"),
        (ParamType::Particles, _) => wrong("a particles/<id> file"),
    }
}

/// Checks the params of `params` against `specs`; the messages are for `who`.
fn check_set(specs: &[ParamSpec], params: &Params, who: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (key, value) in &params.0 {
        match specs.iter().find(|s| s.name == key) {
            None => out.push(format!("{who}: unknown param `{key}`")),
            Some(spec) => {
                if let Some(p) = type_problem(spec, value) {
                    out.push(format!("{who}: {p}"));
                }
            }
        }
    }
    out
}

/// Lists in the params of a base without specs (a script kind's params are single values; only a
/// built-in's list params take lists).
fn untyped_lists(params: &Params, who: &str) -> Vec<String> {
    params
        .0
        .iter()
        .filter(|(_, v)| matches!(v, ParamValue::List(_)))
        .map(|(k, _)| format!("{who}: param `{k}` is a list; only a built-in's list params take lists"))
        .collect()
}

/// `kind` params with `over` on top.
fn merged(base: &Params, over: &Params) -> Params {
    let mut p = base.clone();
    for (k, v) in &over.0 {
        p.0.insert(k.clone(), v.clone());
    }
    p
}

/// Whether the base has no spec to check against.
const fn skipped(b: Builtin) -> bool {
    matches!(
        b,
        Builtin::Kid | Builtin::Inert | Builtin::Scripted | Builtin::Probe
    )
}

/// A format-0 package's legacy-alias warnings (#1076, #1375).
struct LegacyCheck<'a> {
    format: u32,
    package_id: &'a str,
    /// Kinds and params already warned about, so a placed instance does not repeat its kind.
    seen: BTreeSet<(usize, &'static str)>,
}

impl LegacyCheck<'_> {
    /// Warns about a survey **move** param used outside the games its legacy alias allows. One
    /// warning per kind and param: the kind's own params are checked first, then the placed
    /// instances that set it, so the file is the kind's or the room's.
    fn check(
        &mut self,
        specs: &[ParamSpec],
        params: &Params,
        kind: usize,
        who: &str,
        file: &str,
        diags: &mut Vec<Diagnostic>,
    ) {
        if self.format != 0 {
            return;
        }
        for key in params.0.keys() {
            let Some(spec) = specs.iter().find(|s| s.name == key) else {
                continue;
            };
            let Some(legacy) = spec.legacy else { continue };
            if legacy.allows(self.package_id) || !self.seen.insert((kind, spec.name)) {
                continue;
            }
            report(
                diags,
                Severity::Warning,
                file,
                format!(
                    "{who}: param `{key}` is a format-0 legacy alias for {} and moves in #{}; \
                     package `{}` is not one of those games",
                    legacy.games.join(", "),
                    legacy.issue,
                    self.package_id
                ),
            );
        }
    }
}

/// Checks every kind's and placed instance's params against `builtins::param_specs` of its
/// base (step 7), after `load::load_game` has built the definition. Problems are
/// diagnostics: errors, or warnings when `trace` (converter output).
pub fn check(def: &GameDef, format: u32, trace: bool, diags: &mut Vec<Diagnostic>) {
    let severity = if trace { Severity::Warning } else { Severity::Error };
    let specs_of = |b: Builtin| builtins::param_specs(b);
    let init_of = |b: Builtin, p: &Params| {
        let p = p.clone();
        builtins::init_state(b, &p)
    };
    let mut placed: Vec<bool> = vec![false; def.kinds.len()];
    let mut kind_bad: Vec<bool> = vec![false; def.kinds.len()];
    let mut seen: Vec<(KindId, Params)> = Vec::new();
    let mut legacy = LegacyCheck {
        format,
        package_id: &def.id,
        seen: BTreeSet::new(),
    };
    // The kind's own params.
    for (i, k) in def.kinds.iter().enumerate() {
        if skipped(k.base) {
            let who = format!("`{}` ({})", k.name, k.base.name());
            for msg in untyped_lists(&k.params, &who) {
                report(diags, severity, &kind_file(k), msg);
            }
            continue;
        }
        let specs = specs_of(k.base);
        let who = format!("`{}` ({})", k.name, k.base.name());
        legacy.check(specs, &k.params, i, &who, &kind_file(k), diags);
        for msg in check_set(specs, &k.params, &who) {
            kind_bad[i] = true;
            report(diags, severity, &kind_file(k), msg);
        }
    }
    // Placed instances, merged over their kind.
    for room in &def.rooms {
        let file = format!("rooms/{}.toml", room.name);
        for inst in &room.instances {
            let Some(idx) = inst.kind.index() else { continue };
            if let Some(slot) = placed.get_mut(idx) {
                *slot = true;
            }
            let Some(k) = def.kinds.get(idx) else { continue };
            if skipped(k.base) {
                let who = format!("an instance of `{}` ({})", k.name, k.base.name());
                for msg in untyped_lists(&inst.params, &who) {
                    report(diags, severity, &file, msg);
                }
                continue;
            }
            let specs = specs_of(k.base);
            let who = format!("an instance of `{}` ({})", k.name, k.base.name());
            legacy.check(specs, &inst.params, idx, &who, &file, diags);
            let mut bad = kind_bad.get(idx).copied().unwrap_or(false);
            for msg in check_set(specs, &inst.params, &who) {
                bad = true;
                report(diags, severity, &file, msg);
            }
            if bad {
                continue;
            }
            let all = merged(&k.params, &inst.params);
            if seen.iter().any(|(id, p)| *id == inst.kind && *p == all) {
                continue;
            }
            seen.push((inst.kind, all.clone()));
            if let Err(e) = init_of(k.base, &all) {
                report(diags, severity, &file, format!("{who}: {e}"));
            }
        }
    }
    // Kinds nothing places: their own params, without the required ones they lack.
    for (i, k) in def.kinds.iter().enumerate() {
        if skipped(k.base)
            || placed.get(i).copied().unwrap_or(true)
            || kind_bad.get(i).copied().unwrap_or(false)
        {
            continue;
        }
        if let Err(e) = init_of(k.base, &k.params) {
            // A param the kind does not set: required outright (`required`) or only together with
            // others (an edge warp needs `room_to` or per-edge rooms). A placed instance supplies
            // them, so an unplaced default kind is not an error for lacking them.
            let missing_required = !k.params.0.contains_key(&e.param);
            if !missing_required {
                report(
                    diags,
                    severity,
                    &kind_file(k),
                    format!("`{}` ({}): {e}", k.name, k.base.name()),
                );
            }
        }
    }
}
