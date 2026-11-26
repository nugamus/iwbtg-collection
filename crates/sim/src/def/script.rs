//! Script sources as loaded from a package (docs/format/scripts.md;
//! docs/architecture/sim/scripts.md; decision 0001).
//!
//! `KindDef::script` is a [`ScriptId`](super::ScriptId) into `GameDef::scripts`. The sim keeps
//! the source text only; the script host (`script::ScriptRuntime`) compiles it when the `Sim`
//! is built, so `GameDef` never holds Rhai types. The loader fills the table from the package.

/// One script file of the package.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ScriptDef {
    /// Package path of the script (`scripts/boss1.rhai`), for error messages.
    pub name: String,
    /// The source text.
    pub source: String,
}

/// What reading a variable that was never assigned does (decision 0005).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum InitPolicy {
    /// The GM8 rule: the read gives 0 (19 of the 20 games, CLONE_SPEC 781).
    #[default]
    Zero,
    /// The standard rule: the read is a script error with a source span.
    Error,
}

/// How a stored script value keeps its type (decision 0005).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ValueModel {
    /// The GM8 rule: a bool is the real 1.0/0.0, and a value has no other type.
    #[default]
    Gm8,
    /// The standard rule: real, string, bool and instance handle are distinct types that
    /// survive from one event to the next.
    Typed,
}

/// The script-semantics pair a ruleset selects (decision 0005).
///
/// It is a value, not a package switch: `gm8` derives the compatibility pair from
/// `GameDef::uninit_reads_zero`, and `standard` uses [`ScriptRules::standard`]. The engine
/// never branches on the ruleset name.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ScriptRules {
    /// What an unset read does.
    pub init: InitPolicy,
    /// How values keep their type.
    pub values: ValueModel,
}

impl ScriptRules {
    /// The compatibility model. `uninit_reads_zero` is the package's existing `[engine]`
    /// switch, so a game that already turns the read into an error keeps that behavior.
    pub const fn gm8(uninit_reads_zero: bool) -> ScriptRules {
        ScriptRules {
            init: if uninit_reads_zero {
                InitPolicy::Zero
            } else {
                InitPolicy::Error
            },
            values: ValueModel::Gm8,
        }
    }

    /// The standard model: unset reads halt with a span, and values are typed.
    pub const fn standard() -> ScriptRules {
        ScriptRules {
            init: InitPolicy::Error,
            values: ValueModel::Typed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gm8_keeps_the_package_uninitialized_switch() {
        assert_eq!(ScriptRules::gm8(true).init, InitPolicy::Zero);
        assert_eq!(ScriptRules::gm8(false).init, InitPolicy::Error);
        assert_eq!(ScriptRules::gm8(true).values, ValueModel::Gm8);
        assert_eq!(ScriptRules::gm8(false).values, ValueModel::Gm8);
    }

    #[test]
    fn standard_is_explicit_and_typed() {
        let r = ScriptRules::standard();
        assert_eq!(r.init, InitPolicy::Error);
        assert_eq!(r.values, ValueModel::Typed);
        assert_ne!(r, ScriptRules::gm8(true));
    }
}
