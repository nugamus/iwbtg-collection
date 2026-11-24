//! Stored arrays and globals in scripts (#997; docs/architecture/sim/scripts.md, "Arrays and
//! globals").
//!
//! A variable that holds an array ([`Value::Array`]) reads as a [`VarArray`]: a copy of the
//! stored elements with the GM index rules (`this.pattern[3]`, `global.flags[i]`). Rhai's own
//! arrays read negative indices from the end and never grow, and Rhai does not let a host
//! replace their indexer, so a stored array is its own type. Writing an element through a
//! variable (`this.pattern[3] = 9.0`) changes the copy and Rhai writes the copy back through the
//! variable's setter; `let a = this.pattern; a[0] = 1.0;` changes only `a` (the GML 8 copy rule).
//!
//! `global` is a script variable bound to the [`Globals`] handle: `global.<name>` reads and
//! writes the run's globals (`World::globals`) by name.

use std::rc::Rc;

use rhai::{Array, Dynamic, Engine, FuncRegistration, ImmutableString};

use super::api::runtime_error;
use super::budget::cost;
use super::docs::Docs;
use super::host::Shared;
use super::inst::Inst;
use crate::def::script::{InitPolicy, ScriptRules, ValueModel};
use crate::error::SimError;
use crate::num::Real;
use crate::world::{MAX_ARRAY_SIZE, Value, assign_bare};

type Res<T> = Result<T, Box<rhai::EvalAltResult>>;

/// The name scripts use for the globals handle.
pub const GLOBAL: &str = "global";

/// A stored array as a script sees it: a copy of the elements, indexed by the GM rules.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct VarArray(pub Vec<Value>);

/// The handle behind `global`: reads and writes go to the run's globals.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Globals;

fn sim_err(e: SimError) -> Box<rhai::EvalAltResult> {
    runtime_error(e.to_string())
}

/// All elements of `items`, nested arrays included: what copying it costs.
fn deep_len(items: &[Value]) -> u64 {
    items
        .iter()
        .map(|v| 1 + v.as_array().map_or(0, deep_len))
        .fold(0u64, u64::saturating_add)
}

/// Charges the copy of `items` to the tick budget (`budget::COSTS`, `index$get$`).
fn charge_copy(s: &Shared, items: &[Value]) -> Res<()> {
    s.charge(cost("index$get$").1.saturating_mul(deep_len(items)))
}

fn script_rules(s: &Shared) -> Res<ScriptRules> {
    s.host
        .get()
        .map(|h| h.script_rules)
        .ok_or_else(|| runtime_error("arrays need a game (no script host)"))
}

/// The value of a variable or an element for a script. `what` names it in an error
/// ("variable `hp`", "array element 3"). An unset one follows the run's init policy.
pub(super) fn value_to_dynamic(s: &Shared, v: Option<&Value>, rules: ScriptRules, what: &str) -> Res<Dynamic> {
    match v {
        Some(Value::Real(r)) => Ok(Dynamic::from_float(r.get())),
        Some(Value::Str(s)) => Ok(Dynamic::from(ImmutableString::from(s.as_str()))),
        Some(Value::Inst(i)) => Ok(Dynamic::from(Inst(*i))),
        Some(Value::None) | None => match rules.init {
            InitPolicy::Zero => Ok(Dynamic::from_float(0.0)),
            InitPolicy::Error => Err(runtime_error(format!("{what} is not set"))),
        },
        Some(Value::Array(items)) => {
            charge_copy(s, items)?;
            Ok(Dynamic::from(VarArray(items.clone())))
        }
    }
}

/// A script value as a stored [`Value`]. `name` names the variable in an error. A Rhai array
/// and a [`VarArray`] become [`Value::Array`], at most [`MAX_ARRAY_SIZE`] elements.
pub(super) fn dynamic_to_value(s: &Shared, d: &Dynamic, rules: ScriptRules, name: &str) -> Res<Value> {
    if d.is_unit() {
        return Ok(Value::None);
    }
    if let Ok(b) = d.as_bool() {
        return match rules.values {
            // GM8 has no bool value: a bool is the real 1.0/0.0 (Confirmed).
            ValueModel::Gm8 => Ok(Value::Real(if b { Real::ONE } else { Real::ZERO })),
            // The standard model keeps the bool as a bool. The value model cannot carry it
            // yet (world instance state is the entity-model seat's, #1175), so `standard`
            // refuses the silent erasure instead of turning the value into a real.
            ValueModel::Typed => Err(runtime_error(format!(
                "variable `{name}` cannot hold a bool yet under `standard`; \
                 write 1.0/0.0 and test it with gm_bool until the typed value model lands"
            ))),
        };
    }
    if let Ok(f) = super::inst::num(d, name) {
        let r = Real::new(f);
        if !r.is_finite() {
            return Err(sim_err(SimError::non_finite(name.to_owned())));
        }
        return Ok(Value::Real(r));
    }
    if let Some(s) = d.clone().try_cast::<ImmutableString>() {
        return Ok(Value::Str(s.to_string()));
    }
    if let Some(i) = d.clone().try_cast::<Inst>() {
        return Ok(Value::Inst(i.0));
    }
    if let Some(a) = d.read_lock::<VarArray>() {
        charge_copy(s, &a.0)?;
        return Ok(Value::Array(a.0.clone()));
    }
    if let Some(a) = d.read_lock::<Array>() {
        let items = a
            .iter()
            .enumerate()
            .map(|(i, x)| dynamic_to_value(s, x, rules, &format!("{name}[{i}]")))
            .collect::<Res<Vec<Value>>>()?;
        s.charge(cost("index$set$").1.saturating_mul(items.len() as u64))?;
        return Value::array(items).map_err(sim_err);
    }
    Err(runtime_error(format!(
        "variable `{name}` cannot hold a {}",
        d.type_name()
    )))
}

/// An array index under the GM rules: a whole number from 0, below [`MAX_ARRAY_SIZE`].
/// A fractional or negative index is `Unsupported`: what GM8 does with one is OPEN (research
/// `creation-code.md`, "Global arrays"; trace request NUM-ARRAY-IDX).
pub(crate) fn array_index(d: &Dynamic) -> Res<usize> {
    let f = super::inst::num(d, "array index")?;
    if !Real::new(f).is_finite() {
        return Err(sim_err(SimError::non_finite("array index")));
    }
    if f < 0.0 {
        return Err(sim_err(SimError::unsupported(format!(
            "array index {f}: a negative index is OPEN in the GM8 spec (research#130)"
        ))));
    }
    // gm-compare: ok (exact test for a whole index; the fraction is exact, `num::frac`)
    let whole = crate::num::round::frac(Real::new(f)).get() == 0.0;
    if !whole {
        return Err(sim_err(SimError::unsupported(format!(
            "array index {f}: a fractional index is OPEN in the GM8 spec (research#130)"
        ))));
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "f is a whole number from 0; a huge one saturates and fails the cap below"
    )]
    let i = f as usize;
    if i >= MAX_ARRAY_SIZE {
        return Err(sim_err(crate::world::instance::array_cap_error()));
    }
    Ok(i)
}

/// Writes `value` into element `i` of `items`, growing it with unset elements (GM8: "At the
/// moment you use an index the array is generated", research `creation-code.md`).
fn set_element(items: &mut Vec<Value>, i: usize, value: Value) {
    if items.len() <= i {
        items.resize(i + 1, Value::None);
    }
    items[i] = value;
}

/// Registers stored arrays (`VarArray`) and `global` (`Globals`).
/// The `index$get$`/`index$set$` descriptions are shared with the instance indexers (`inst.rs`).
pub fn register_array_api(engine: &mut Engine, shared: &Rc<Shared>, docs: &mut Docs) {
    engine.register_type_with_name::<VarArray>("VarArray");
    engine.register_type_with_name::<Globals>("Globals");

    // ---- Stored arrays ----
    {
        let s = Rc::clone(shared);
        engine.register_indexer_get(move |a: &mut VarArray, i: Dynamic| -> Res<Dynamic> {
            let i = array_index(&i)?;
            let rules = script_rules(&s)?;
            value_to_dynamic(&s, a.0.get(i), rules, &format!("array element {i}"))
        });
        let s = Rc::clone(shared);
        engine.register_indexer_set(move |a: &mut VarArray, i: Dynamic, v: Dynamic| -> Res<()> {
            let i = array_index(&i)?;
            let rules = script_rules(&s)?;
            let value = dynamic_to_value(&s, &v, rules, &format!("array element {i}"))?;
            set_element(&mut a.0, i, value);
            Ok(())
        });
        docs.add("len", "`a.len()` or `len(a)`: the number of elements of an array, a stored one included (an INT).");
        #[allow(clippy::cast_possible_wrap, reason = "at most MAX_ARRAY_SIZE elements")]
        engine.register_fn("len", |a: &mut VarArray| a.0.len() as rhai::INT);
    }

    // ---- Globals: `global.<name>` ----
    {
        let s = Rc::clone(shared);
        engine.register_indexer_get(move |_: &mut Globals, name: ImmutableString| -> Res<Dynamic> {
            s.with_world(|w, host| {
                let v = crate::room::known_var(&host.def, w, &name).and_then(|id| w.globals.get(&id));
                value_to_dynamic(&s, v, host.script_rules, &format!("global `{name}`"))
            })?
        });
        let s = Rc::clone(shared);
        // Pure: `global` is a constant handle, and the write goes to the world, not to it.
        FuncRegistration::new_index_setter().with_purity(true).register_into_engine(
            engine,
            move |_: &mut Globals, name: ImmutableString, v: Dynamic| -> Res<()> {
                s.with_world(|w, host| {
                    let value = dynamic_to_value(&s, &v, host.script_rules, &name)?;
                    let id = crate::room::run_time_var(&host.def, w, &name).map_err(sim_err)?;
                    match host.script_rules.values {
                        ValueModel::Gm8 => assign_bare(&mut w.globals, id, value),
                        ValueModel::Typed => {
                            w.globals.insert(id, value);
                        }
                    }
                    Ok(())
                })?
            },
        );
    }
    #[allow(
        deprecated,
        reason = "Rhai marks the volatile variable resolver hook; the pin keeps it stable"
    )]
    engine.on_var(|name, index, _| {
        // `index` is 0 when the parser found no local of that name, so `let global = ..`
        // still shadows the handle.
        Ok((index == 0 && name == GLOBAL).then(|| Dynamic::from(Globals)))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices_follow_the_gm_rules() {
        assert_eq!(array_index(&Dynamic::from_int(3)).ok(), Some(3));
        assert_eq!(array_index(&Dynamic::from_float(3.0)).ok(), Some(3));
        for bad in [-1.0, 1.5, -0.5, 10_000.0, 1e300] {
            assert!(array_index(&Dynamic::from_float(bad)).is_err(), "{bad}");
        }
        assert!(array_index(&Dynamic::from_int(-1)).is_err());
        assert!(array_index(&Dynamic::from(ImmutableString::from("1"))).is_err());
    }

    #[test]
    fn a_write_past_the_end_grows_with_unset_elements() {
        let mut items = vec![Value::Real(Real::ONE)];
        set_element(&mut items, 3, Value::Real(Real::ZERO));
        assert_eq!(
            items,
            vec![Value::Real(Real::ONE), Value::None, Value::None, Value::Real(Real::ZERO)]
        );
    }

    #[test]
    fn copy_cost_counts_nested_elements() {
        let inner = Value::Array(vec![Value::None, Value::None]);
        assert_eq!(deep_len(&[inner, Value::None]), 4);
    }
}
