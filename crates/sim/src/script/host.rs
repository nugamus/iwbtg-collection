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

// ---- The IWS engine (WP9.1) ----

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rhai::packages::{
    ArithmeticPackage, BasicArrayPackage, BasicMapPackage, BasicStringPackage, CorePackage, LogicPackage,
    Package,
};
use rhai::{AST, Dynamic, Engine, EvalAltResult, FuncArgs, OptimizationLevel, Scope};

use super::budget::Budget;
use crate::rng::Rng;
use crate::rules::num::NumRules;

/// The most lines the script log keeps (the log is not part of the state hash).
const LOG_CAP: usize = 256;

/// State the registered functions share with the host: the run's number rules, the RNG and tick
/// counters lent for the current tick, the budget and the capped log. Interior mutability is
/// allowed here only (architecture "Scripts").
#[derive(Debug)]
pub struct Shared {
    pub(crate) num: NumRules,
    pub(crate) rng: RefCell<Rng>,
    pub(crate) tick: Cell<u64>,
    pub(crate) room_tick: Cell<u64>,
    pub(crate) budget: Budget,
    log: RefCell<Vec<String>>,
}

impl Shared {
    fn push_log(&self, line: String) {
        let mut log = self.log.borrow_mut();
        if log.len() < LOG_CAP {
            log.push(line);
        }
    }
}

/// The IWS engine (decision 0001 "Engine setup"): `Engine::new_raw()` with only our packages,
/// `eval`, `**` and `**=` disabled, fixed limits, strict variables and a fixed optimizer.
/// Built once per `Sim`; the client, the server and the CLI all build it through [`Iws::new`].
pub struct Iws {
    engine: Engine,
    shared: Rc<Shared>,
}

impl std::fmt::Debug for Iws {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Iws")
            .field("shared", &self.shared)
            .finish_non_exhaustive()
    }
}

impl Iws {
    /// The engine for a run with these number rules and the default budget.
    pub fn new(num: NumRules) -> Iws {
        let shared = Rc::new(Shared {
            num,
            rng: RefCell::new(Rng::new(0)),
            tick: Cell::new(0),
            room_tick: Cell::new(0),
            budget: Budget::new(),
            log: RefCell::new(Vec::new()),
        });
        let mut engine = Engine::new_raw();
        // Only language-core packages: no `BasicMathPackage` (its sin, cos, pow and round call
        // std, and its round is half away from zero) and no time package.
        engine.register_global_module(CorePackage::new().as_shared_module());
        engine.register_global_module(ArithmeticPackage::new().as_shared_module());
        engine.register_global_module(LogicPackage::new().as_shared_module());
        engine.register_global_module(BasicStringPackage::new().as_shared_module());
        engine.register_global_module(BasicArrayPackage::new().as_shared_module());
        engine.register_global_module(BasicMapPackage::new().as_shared_module());
        // Our functions override the packages' (the `/` on two integers gives a FLOAT).
        super::api::register_numeric(&mut engine, &shared);

        engine.disable_symbol("eval");
        engine.disable_symbol("**");
        engine.disable_symbol("**=");
        engine.set_optimization_level(OptimizationLevel::Simple);
        engine.set_strict_variables(true);
        engine.set_fast_operators(false);
        engine.set_max_call_levels(32);
        engine.set_max_expr_depths(64, 32);
        engine.set_max_string_size(4096);
        engine.set_max_array_size(10_000);
        engine.set_max_map_size(1024);
        engine.set_max_variables(256);
        engine.set_max_functions(512);
        {
            let s = Rc::clone(&shared);
            engine.on_print(move |text| s.push_log(text.to_owned()));
            let s = Rc::clone(&shared);
            engine.on_debug(move |text, source, pos| {
                s.push_log(format!(
                    "{}{pos}: {text}",
                    source.map_or(String::new(), |x| format!("{x} "))
                ));
            });
        }
        {
            let s = Rc::clone(&shared);
            engine.on_progress(move |ops| {
                if s.budget.exceeded(ops) {
                    Some(Dynamic::from("script budget exceeded"))
                } else {
                    None
                }
            });
        }
        Iws { engine, shared }
    }

    /// Starts a tick: the budget counter resets and the tick counters and the RNG are lent to
    /// the scripts. [`Iws::end_tick`] gives the RNG back.
    pub fn begin_tick(&self, tick: u64, room_tick: u64, rng: Rng) {
        self.shared.budget.begin_tick();
        self.shared.tick.set(tick);
        self.shared.room_tick.set(room_tick);
        *self.shared.rng.borrow_mut() = rng;
    }

    /// The RNG as the scripts left it.
    pub fn end_tick(&self) -> Rng {
        self.shared.rng.borrow().clone()
    }

    /// The budget counters.
    pub fn budget(&self) -> &Budget {
        &self.shared.budget
    }

    /// The captured `print` and `debug` lines (capped, never part of the state hash).
    pub fn log(&self) -> Vec<String> {
        self.shared.log.borrow().clone()
    }

    /// Compiles `source` under the IWS profile.
    pub fn compile(&self, source: &str) -> Result<AST, ScriptError> {
        self.engine.compile(source).map_err(|e| ScriptError {
            message: format!("compile error: {e}"),
        })
    }

    /// Evaluates one expression (tests, the validator's constant checks).
    pub fn eval<T: Clone + Send + Sync + 'static>(&self, source: &str) -> Result<T, ScriptError> {
        self.engine
            .eval_expression::<T>(source)
            .map_err(|e| script_error(&e))
    }

    /// Runs `source` as a statement list and returns the last value.
    pub fn run_value<T: Clone + Send + Sync + 'static>(&self, source: &str) -> Result<T, ScriptError> {
        self.engine.eval::<T>(source).map_err(|e| script_error(&e))
    }

    /// Calls the script function `name` of `ast` with `args`.
    pub fn call_fn<T: Clone + Send + Sync + 'static>(
        &self,
        ast: &AST,
        name: &str,
        args: impl FuncArgs,
    ) -> Result<T, ScriptError> {
        let mut scope = Scope::new();
        self.engine
            .call_fn::<T>(&mut scope, ast, name, args)
            .map_err(|e| script_error(&e))
    }
}

/// A Rhai error as a [`ScriptError`] with its position; budget exhaustion has its own message.
pub(crate) fn script_error(e: &EvalAltResult) -> ScriptError {
    let message = match e {
        EvalAltResult::ErrorTerminated(..) => format!("script budget exceeded{}", e.position()),
        other => other.to_string(),
    };
    ScriptError { message }
}
