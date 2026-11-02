//! Param validation against `builtins::param_specs` (docs/plans/sim-core.md 8.2 step 7).
//! WP0 creates the file; WP7b owns it.
//!
//! Every kind's params, and every placed instance's params merged over its kind's (a placed
//! instance overrides), are checked: each key must be a param of the base built-in, its value
//! must have the spec's type (an enum word must be one of the words), and the built-in's own
//! `init_state` must accept the set (required params, combinations). A kind no room places is
//! checked on its own params, ignoring required ones it lacks (an instance may give them).
//!
//! Kinds with no spec (`kid`, `inert`, scripted, probe) are skipped. In `LoadMode::Trace`
//! converter output may not match the specs yet, so the problems are warnings.

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
    let wrong = |want: &str| Some(format!("param `{}` must be {want}", spec.name));
    match (spec.ty, value) {
        (ParamType::Real, ParamValue::Real(_))
        | (ParamType::Bool, ParamValue::Bool(_))
        | (ParamType::Room, ParamValue::Room(_))
        | (ParamType::Kind, ParamValue::Kind(_))
        | (ParamType::Sound, ParamValue::Sound(_))
        | (ParamType::Text, ParamValue::Str(_)) => None,
        // A reference the loader could not resolve is already an error of its own.
        (ParamType::Room | ParamType::Kind | ParamType::Sound, ParamValue::Str(_)) => None,
        (ParamType::Enum(words), ParamValue::Str(s)) => {
            if words.contains(&s.as_str()) {
                None
            } else {
                Some(format!(
                    "param `{}` is `{s}`; it must be one of: {}",
                    spec.name,
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

/// Checks every kind's and placed instance's params against `builtins::param_specs` of its
/// base (step 7), after `load::load_game` (WP7a) has built the definition. Problems are
/// diagnostics: errors, or warnings when `trace` (converter output).
pub fn check(def: &GameDef, trace: bool, diags: &mut Vec<Diagnostic>) {
    let severity = if trace { Severity::Warning } else { Severity::Error };
    let mut pending = std::collections::BTreeSet::new();
    let mut specs_of = |b: Builtin| crate::load::pending_call(&mut pending, || builtins::param_specs(b));
    let init_of = |b: Builtin, p: &Params| {
        let p = p.clone();
        let mut pending = std::collections::BTreeSet::new();
        crate::load::pending_call(&mut pending, || builtins::init_state(b, &p))
    };
    let mut placed: Vec<bool> = vec![false; def.kinds.len()];
    let mut kind_bad: Vec<bool> = vec![false; def.kinds.len()];
    let mut seen: Vec<(KindId, Params)> = Vec::new();
    // The kind's own params.
    for (i, k) in def.kinds.iter().enumerate() {
        if skipped(k.base) {
            continue;
        }
        let Some(specs) = specs_of(k.base) else {
            continue;
        };
        let who = format!("`{}` ({})", k.name, k.base.name());
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
                continue;
            }
            let Some(specs) = specs_of(k.base) else {
                continue;
            };
            let who = format!("an instance of `{}` ({})", k.name, k.base.name());
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
            if let Some(Err(e)) = init_of(k.base, &all) {
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
        if specs_of(k.base).is_none() {
            continue;
        }
        if let Some(Err(e)) = init_of(k.base, &k.params) {
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
