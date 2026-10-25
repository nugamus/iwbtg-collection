//! Script sources as loaded from a package (docs/plans/sim-core.md 8.2, 11; decision 0001).
//!
//! `KindDef::script` is a [`ScriptId`](super::ScriptId) into `GameDef::scripts`. The sim keeps
//! the source text only; the script host (`script::ScriptRuntime`, WP9) compiles it when the
//! `Sim` is built, so `GameDef` never holds Rhai types. WP7a fills the table from the package;
//! WP0 declares; WP9 owns this file.

/// One script file of the package.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ScriptDef {
    /// Package path of the script (`scripts/boss1.rhai`), for error messages.
    pub name: String,
    /// The source text.
    pub source: String,
}
