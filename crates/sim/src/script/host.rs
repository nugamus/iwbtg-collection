//! The Rhai host (`Engine::new_raw()`, our packages only, `eval`, `**`, `**=` disabled;
//! docs/plans/sim-core.md 11, decision 0001). WP0 declares [`ScriptRuntime`]; WP9 owns the file
//! and adds its fields (engine, compiled ASTs, budget counter).
#![allow(unused_variables, reason = "WP0 stub: WP9 fills the body")]

use super::{ScriptError, ScriptHost};
use crate::def::GameDef;
use crate::error::SimError;
use crate::events::EventKind;
use crate::world::{Ctx, Flow, InstId};

/// The game's compiled scripts and the engine that runs them. Built once per `Sim` from
/// `GameDef::scripts`, lent to every `Ctx` as `ctx.scripts`. `Default` is a host with no
/// scripts, for tests and games without scripts.
#[derive(Clone, Debug, Default)]
pub struct ScriptRuntime {}

impl ScriptRuntime {
    /// Compiles `def.scripts` (index = `ScriptId`). A compile error is a load-time `SimError`.
    pub fn new(def: &GameDef) -> Result<ScriptRuntime, SimError> {
        if def.scripts.is_empty() {
            return Ok(ScriptRuntime::default());
        }
        Err(SimError::unsupported(
            "scripted kinds (WP9: the script host is not built yet)",
        ))
    }
}

impl ScriptHost for ScriptRuntime {
    fn on_event(&self, ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, ScriptError> {
        unimplemented!("WP9: ScriptRuntime::on_event")
    }
}
