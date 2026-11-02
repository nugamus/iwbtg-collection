//! `object_order` to dispatch ranks (docs/plans/sim-core.md 1.3, 8.2 step 5, CR4).
//!
//! An entry is a kind name (`block`, `@world`) or a selector on one of the kind's params
//! (`{ kind = "spike", dir = "up" }`), so originals that collapsed into one kind keep their
//! original index order (converter OPEN 13). The rank is the entry's position. An instance takes
//! the rank of the first selector of its kind that its params match, else the kind's plain entry.
//! The check that every placed or spawnable kind has a rank is done by `load`, which knows which
//! kinds those are.

use std::collections::BTreeSet;

use iwbtg_sim::def::{KindDef, KindId, ParamValue, Params, RankEntry, RankTable, VariantSel};

use crate::diag::Diagnostic;
use crate::manifest::MANIFEST_PATH;

/// One `object_order` entry: a kind name, maybe with a variant selector.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct OrderEntry {
    /// Kind name (`block`, `@world`).
    pub kind: String,
    /// Selector (`{ kind = "spike", dir = "up" }`).
    pub variant: Option<VariantSel>,
}

/// Ranks for every kind; every kind placed or spawnable must get one.
///
/// Kinds are found by [`KindDef::name`]. An unknown kind, a repeated entry or more than 65,536
/// entries is an error.
pub fn resolve_ranks(order: &[OrderEntry], kinds: &[KindDef]) -> Result<RankTable, Diagnostic> {
    let mut entries = Vec::with_capacity(order.len());
    let mut seen = BTreeSet::new();
    for (i, entry) in order.iter().enumerate() {
        let Some(kind) = kinds.iter().position(|k| k.name == entry.kind) else {
            return Err(Diagnostic::file_error(
                MANIFEST_PATH,
                format!(
                    "object_order names `{}`, which is not a kind of this package",
                    entry.kind
                ),
            ));
        };
        let key = (
            kind,
            entry
                .variant
                .as_ref()
                .map(|v| (v.key.clone(), format!("{:?}", v.value))),
        );
        if !seen.insert(key) {
            return Err(Diagnostic::file_error(
                MANIFEST_PATH,
                format!("object_order lists `{}` twice", describe(entry)),
            ));
        }
        let (Ok(rank), Ok(kind)) = (u16::try_from(i), u32::try_from(kind)) else {
            return Err(Diagnostic::file_error(
                MANIFEST_PATH,
                "object_order has more than 65,536 entries",
            ));
        };
        entries.push(RankEntry {
            kind: KindId(kind),
            variant: entry.variant.clone(),
            rank,
        });
    }
    Ok(RankTable { entries })
}

/// The rank of an instance of `kind` whose params (the kind's, then the instance's on top) are
/// `params`: the first selector entry of the kind that matches, else the plain entry.
pub fn rank_of(table: &RankTable, kind: KindId, params: &[&Params]) -> Option<u16> {
    let value_of = |key: &str| params.iter().rev().find_map(|p| p.0.get(key));
    let of_kind = || table.entries.iter().filter(|e| e.kind == kind);
    of_kind()
        .find(|e| {
            e.variant
                .as_ref()
                .is_some_and(|v| value_of(&v.key).is_some_and(|p| param_eq(p, &v.value)))
        })
        .or_else(|| of_kind().find(|e| e.variant.is_none()))
        .map(|e| e.rank)
}

/// Selector equality: reals compare by bits, so `1` and `1.0` match and nothing is rounded.
fn param_eq(a: &ParamValue, b: &ParamValue) -> bool {
    match (a, b) {
        (ParamValue::Real(x), ParamValue::Real(y)) => x.to_bits() == y.to_bits(),
        _ => a == b,
    }
}

/// `spike` or `spike{dir = "up"}` for messages.
pub fn describe(entry: &OrderEntry) -> String {
    match &entry.variant {
        None => entry.kind.clone(),
        Some(v) => format!("{}{{{} = {:?}}}", entry.kind, v.key, v.value),
    }
}

#[cfg(test)]
mod tests {
    use iwbtg_sim::def::Builtin;
    use iwbtg_sim::num::Real;

    use super::*;

    fn kinds() -> Vec<KindDef> {
        vec![
            KindDef::new("kid", Builtin::Kid),
            KindDef::new("spike", Builtin::Spike),
            KindDef::new("@world", Builtin::World),
        ]
    }

    fn sel(kind: &str, dir: Option<&str>) -> OrderEntry {
        OrderEntry {
            kind: kind.into(),
            variant: dir.map(|d| VariantSel {
                key: "dir".into(),
                value: ParamValue::Str(d.into()),
            }),
        }
    }

    #[test]
    fn selectors_keep_interleaved_order() {
        let order = [
            sel("kid", None),
            sel("spike", Some("up")),
            sel("@world", None),
            sel("spike", Some("down")),
            sel("spike", None),
        ];
        let t = resolve_ranks(&order, &kinds()).unwrap();
        let p = |d: &str| {
            let mut p = Params::default();
            p.0.insert("dir".into(), ParamValue::Str(d.into()));
            p
        };
        assert_eq!(rank_of(&t, KindId(1), &[&p("up")]), Some(1));
        assert_eq!(rank_of(&t, KindId(1), &[&p("down")]), Some(3));
        assert_eq!(rank_of(&t, KindId(1), &[&p("left")]), Some(4));
        assert_eq!(rank_of(&t, KindId(1), &[]), Some(4));
        // Instance params win over the kind's.
        assert_eq!(rank_of(&t, KindId(1), &[&p("down"), &p("up")]), Some(1));
        assert_eq!(rank_of(&t, KindId(2), &[]), Some(2));
        assert_eq!(t.rank_for(KindId(0), None), Some(0));
    }

    #[test]
    fn real_selectors_match_integers() {
        let order = [OrderEntry {
            kind: "spike".into(),
            variant: Some(VariantSel {
                key: "variant".into(),
                value: ParamValue::Real(Real::new(2.0)),
            }),
        }];
        let t = resolve_ranks(&order, &kinds()).unwrap();
        let mut p = Params::default();
        p.0.insert("variant".into(), ParamValue::Real(Real::new(2.0)));
        assert_eq!(rank_of(&t, KindId(1), &[&p]), Some(0));
        assert_eq!(rank_of(&t, KindId(1), &[]), None);
    }

    #[test]
    fn unknown_and_repeated_entries_fail() {
        assert!(resolve_ranks(&[sel("boss", None)], &kinds()).is_err());
        let d = resolve_ranks(&[sel("spike", Some("up")), sel("spike", Some("up"))], &kinds()).unwrap_err();
        assert!(d.message.contains("twice"), "{}", d.message);
    }
}
