//! The Rhai host (`Engine::new_raw()`, our packages only, `eval`, `**`, `**=` disabled;
//! docs/architecture/sim/scripts.md, decision 0001). [`Iws`] is the engine; [`ScriptRuntime`] is the
//! game's compiled scripts on top of it, and runs the script part of an event.
//!
//! World access without unsafe code: while a script runs, `on_event` parks the live `World` in
//! [`Shared`] (swapping a placeholder into `ctx.w`) and the registered functions work on the
//! parked world. A function that needs a whole `Ctx` (spawn, destroy, kill) takes the world out,
//! builds a `Ctx` for the call and puts the world back, so a nested script (a spawned
//! instance Create) parks it again. The world is always restored, also on errors.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::inst::Inst;
use super::{ScriptError, ScriptHost};
use crate::def::script::ScriptRules;
use crate::def::{Builtin, ClassMask, GameDef, KindId, ScriptId};
use crate::error::SimError;
use crate::events::{EventKind, EventSet};
use crate::rules::RunnerRules;
use crate::sound::DllMode;
use crate::world::{Ctx, Flow, InstId, Instance, World};

/// Deepest nesting of script events (a Create that spawns an instance whose Create spawns...).
const MAX_DEPTH: u32 = 8;

struct Compiled {
    name: String,
    ast: AST,
    /// Script functions as (name, parameter count).
    fns: BTreeSet<(String, usize)>,
}

pub(crate) struct Inner {
    iws: Iws,
    /// Shared with the [`PreparedScripts`] this run came from (#1093). `Rc`: the compiled ASTs
    /// are not `Send` under the sim's no-`sync` Rhai profile and never leave this thread.
    scripts: Rc<Vec<Compiled>>,
}

impl Inner {
    /// Calls the package script function `name` with `args` from inside a running script
    /// (#1533): `this` stays the calling instance, and the same engine means the same step
    /// budget, RNG and lent world as the caller. Returns the function's value.
    fn execute(&self, name: &str, args: Vec<Dynamic>) -> Result<Dynamic, ScriptError> {
        let shared = &self.iws.shared;
        let Some(this_inst) = shared.this_inst.get() else {
            return Err(plain("script_execute is only available inside a script call"));
        };
        if shared.depth.get() >= MAX_DEPTH {
            return Err(plain(
                "scripts nest too deeply (script_execute inside script_execute ...)",
            ));
        }
        let arity = args.len();
        let Some(script) = self
            .scripts
            .iter()
            .find(|s| s.fns.contains(&(name.to_owned(), arity)))
        else {
            return Err(plain(format!(
                "script_execute: no script function `{name}` taking {arity} argument{}",
                if arity == 1 { "" } else { "s" }
            )));
        };
        shared.depth.set(shared.depth.get() + 1);
        let mut this = Dynamic::from(Inst(this_inst));
        let opts = CallFnOptions::new().bind_this_ptr(&mut this).eval_ast(false);
        let mut scope = Scope::new();
        let result = self
            .iws
            .engine
            .call_fn_with_options::<Dynamic>(opts, &mut scope, &script.ast, name, args)
            .map_err(|e| {
                let mut d = script_error(&e);
                d.message = format!("{}: {} [script_execute `{name}`]", script.name, d.message);
                d
            });
        shared.depth.set(shared.depth.get() - 1);
        result
    }
}

/// The game's compiled scripts and the engine that runs them. Built once per `Sim` from
/// `GameDef::scripts`, lent to every `Ctx` as `ctx.scripts`. `Default` is a host with no
/// scripts, for tests and games without scripts. Clones share the engine.
#[derive(Clone, Default)]
pub struct ScriptRuntime {
    inner: Option<Rc<Inner>>,
}

impl std::fmt::Debug for ScriptRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptRuntime")
            .field("scripts", &self.inner.as_ref().map_or(0, |i| i.scripts.len()))
            .finish()
    }
}

/// The function an event calls, and its argument.
pub(crate) fn event_call(ev: &EventKind) -> Option<(&'static str, Option<Dynamic>)> {
    Some(match ev {
        EventKind::Create => ("create", None),
        EventKind::Destroy => ("destroy", None),
        EventKind::BeginStep => ("begin_step", None),
        EventKind::Step => ("step", None),
        EventKind::EndStep => ("end_step", None),
        EventKind::Late => ("late", None),
        EventKind::Alarm(n) => ("alarm", Some(Dynamic::from_float(f64::from(*n)))),
        EventKind::User(n) => ("user", Some(Dynamic::from_float(f64::from(*n)))),
        EventKind::Collision { other, .. } => ("collide", Some(Dynamic::from(Inst(*other)))),
        EventKind::OutsideRoom => ("outside_room", None),
        EventKind::IntersectBoundary => ("intersect_boundary", None),
        EventKind::AnimationEnd => ("animation_end", None),
        EventKind::RoomStart => ("room_start", None),
        EventKind::RoomEnd => ("room_end", None),
        EventKind::GameStart => ("game_start", None),
        EventKind::EndOfPath => ("end_of_path", None),
        _ => return None,
    })
}

/// The script function a moment calls: the running timeline's `call` for `step` becomes
/// `moment_<call>`. Moments are not in [`event_call`] because the name lives in the timeline.
fn moment_call(def: &GameDef, inst: &Instance, step: u32) -> Option<String> {
    let run = inst.timeline.as_ref()?;
    let timeline = def.timelines.get(run.timeline.index()?)?;
    let index = timeline.moments.iter().position(|m| *m == step)?;
    timeline.calls.get(index).map(|call| format!("moment_{call}"))
}

/// The name a trigger-based kind's kid collision passes to `trigger(name)` (decision 0001).
///
/// The trigger built-in handles the kid classes itself (its event set), so that collision is
/// where the script's `trigger` event runs. The name is the instance's room-file name
/// (`PlacedInstance::name`, documented "for triggers and scripts"); an unnamed trigger keeps
/// the ordinary `collide` mapping because there is no name to pass.
fn trigger_name(
    def: &GameDef,
    inst: &Instance,
    with: ClassMask,
    fns: &BTreeSet<(String, usize)>,
) -> Option<String> {
    let kd = def.kind(inst.kind)?;
    if kd.base != Builtin::Trigger
        || !with.intersects(crate::builtins::warp::KIDS)
        || !fns.contains(&("trigger".to_owned(), 1))
    {
        return None;
    }
    def.names.get(inst.name?.index()?).cloned()
}

/// The package's compiled scripts and kind-name map, prepared once and reused across runs
/// (#1021, #1093).
///
/// It holds only immutable data. Each run builds its own [`Iws`] and per-run `Shared` state, so
/// two runs from the same artifact share no RNG, budget, log or parked world. The Rhai types
/// stay inside this module: `GameDef` never holds one.
#[derive(Clone)]
pub struct PreparedScripts {
    /// The package the scripts were compiled from; an artifact is not interchangeable.
    package: String,
    /// Compiled scripts, indexed by `ScriptId`. `Rc`: the ASTs are not `Send` and stay on the
    /// sim's thread (the client cache is a `NonSend` resource, #1093B).
    scripts: Rc<Vec<Compiled>>,
    /// Kind names for the API's kind arguments.
    kinds: Arc<BTreeMap<String, KindId>>,
}

impl std::fmt::Debug for PreparedScripts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedScripts")
            .field("package", &self.package)
            .field("scripts", &self.scripts.len())
            .finish_non_exhaustive()
    }
}

impl PreparedScripts {
    /// Compiles every script of `def` under `rules`. A compile error is a load-time
    /// `SimError::Script` with the file and position, as before.
    pub fn new(def: &GameDef, rules: &RunnerRules) -> Result<PreparedScripts, SimError> {
        if def.scripts.is_empty() {
            return Ok(PreparedScripts {
                package: def.id.clone(),
                scripts: Rc::new(Vec::new()),
                kinds: Arc::new(BTreeMap::new()),
            });
        }
        let iws = Iws::new(rules.num.clone());
        let mut scripts = Vec::with_capacity(def.scripts.len());
        for s in &def.scripts {
            let ast = iws.compile(&s.source).map_err(|e| SimError::Script {
                message: format!("{}: {}", s.name, e.message),
            })?;
            let fns = ast
                .iter_fn_def()
                .map(|f| (f.name.to_string(), f.params.len()))
                .collect();
            scripts.push(Compiled {
                name: s.name.clone(),
                ast,
                fns,
            });
        }
        let kinds: BTreeMap<String, KindId> = def
            .kinds
            .iter()
            .zip(0u32..)
            .map(|(k, i)| (k.name.clone(), KindId(i)))
            .collect();
        Ok(PreparedScripts {
            package: def.id.clone(),
            scripts: Rc::new(scripts),
            kinds: Arc::new(kinds),
        })
    }

    /// How many scripts the artifact holds.
    pub fn script_count(&self) -> usize {
        self.scripts.len()
    }
}

impl ScriptRuntime {
    /// Compiles `def.scripts` (index = `ScriptId`) under the compatibility script model that
    /// `def` already selects. A compile error is a load-time `SimError`.
    pub fn new(def: &GameDef, rules: &RunnerRules) -> Result<ScriptRuntime, SimError> {
        ScriptRuntime::new_with_script_rules(def, rules, ScriptRules::gm8(def.uninit_reads_zero))
    }

    /// Compiles `def.scripts` under `script_rules` (decision 0005). `new` is this with the
    /// package's compatibility pair; a `standard` ruleset passes [`ScriptRules::standard`].
    pub fn new_with_script_rules(
        def: &GameDef,
        rules: &RunnerRules,
        script_rules: ScriptRules,
    ) -> Result<ScriptRuntime, SimError> {
        let prepared = PreparedScripts::new(def, rules)?;
        ScriptRuntime::new_run_with_script_rules(Arc::new(def.clone()), rules, script_rules, &prepared)
    }

    /// A run from a prepared artifact: a fresh [`Iws`] and per-run state, reusing only the
    /// compiled scripts (#1093). `new` is prepare-plus-run for callers without a cache.
    pub fn new_run(
        def: Arc<GameDef>,
        rules: &RunnerRules,
        prepared: &PreparedScripts,
    ) -> Result<ScriptRuntime, SimError> {
        let script_rules = ScriptRules::gm8(def.uninit_reads_zero);
        ScriptRuntime::new_run_with_script_rules(def, rules, script_rules, prepared)
    }

    /// [`ScriptRuntime::new_run`] under an explicit script model (decision 0005).
    pub fn new_run_with_script_rules(
        def: Arc<GameDef>,
        rules: &RunnerRules,
        script_rules: ScriptRules,
        prepared: &PreparedScripts,
    ) -> Result<ScriptRuntime, SimError> {
        if prepared.scripts.is_empty() {
            // No scripts means no kind has one, so there is no host to lend.
            return Ok(ScriptRuntime::default());
        }
        if prepared.package != def.id || prepared.scripts.len() != def.scripts.len() {
            return Err(SimError::invariant(format!(
                "prepared scripts for package `{}` cannot run package `{}`",
                prepared.package, def.id
            )));
        }
        let iws = Iws::new(rules.num.clone());
        let scripts = Rc::clone(&prepared.scripts);
        let kinds = Arc::clone(&prepared.kinds);
        let inner = Rc::new_cyclic(|weak| {
            let host = Host {
                def,
                rules: rules.clone(),
                script_rules,
                inner: weak.clone(),
                kinds,
            };
            // The cell is fresh, so this cannot fail.
            let _ = iws.shared.host.set(host);
            Inner { iws, scripts }
        });
        Ok(ScriptRuntime { inner: Some(inner) })
    }

    /// Starts a tick: the per-tick budget counter goes back to 0. The tick loop calls it at row 0.
    pub fn begin_tick(&self) {
        if let Some(i) = &self.inner {
            i.iws.shared.budget.begin_tick();
        }
    }

    /// The engine, for budget counters and the log (`iwbtg profile`, tests).
    pub fn engine(&self) -> Option<&Iws> {
        self.inner.as_ref().map(|i| &i.iws)
    }

    /// The events script `id` handles, from the functions it defines (`create` always runs and is
    /// not part of an event set). Collision handlers are not included: which classes a
    /// `collide(other)` listens to is up to the kind.
    pub fn event_set(&self, id: ScriptId) -> EventSet {
        let mut set = EventSet::default();
        let Some(script) = self
            .inner
            .as_ref()
            .and_then(|i| id.index().and_then(|n| i.scripts.get(n)))
        else {
            return set;
        };
        let has = |name: &str, arity: usize| script.fns.contains(&(name.to_owned(), arity));
        set.begin_step = has("begin_step", 0);
        set.step = has("step", 0);
        set.end_step = has("end_step", 0);
        set.late = has("late", 0);
        set.alarms = if has("alarm", 1) { 0x0FFF } else { 0 };
        set.user = if has("user", 1) { 0xFFFF } else { 0 };
        set.outside_room = has("outside_room", 0);
        set.intersect_boundary = has("intersect_boundary", 0);
        set.animation_end = has("animation_end", 0);
        set.room_start = has("room_start", 0);
        set.room_end = has("room_end", 0);
        set.moments = script
            .fns
            .iter()
            .filter_map(|(name, arity)| {
                (*arity == 0)
                    .then_some(name.as_str())
                    .and_then(|n| n.strip_prefix("moment_"))
                    .map(str::to_owned)
            })
            .collect();
        set
    }
}

fn plain(message: impl Into<String>) -> ScriptError {
    ScriptError {
        message: message.into(),
    }
}

impl ScriptRuntime {
    /// Runs the script function `fname` of script `sid` on `me`, with `this` bound to `me` and
    /// `arg` as the one argument if there is one. `where_` names the call in an error.
    fn invoke(
        &self,
        ctx: &mut Ctx,
        sid: ScriptId,
        me: InstId,
        fname: &str,
        arg: Option<Dynamic>,
        where_: &str,
    ) -> Result<Flow, ScriptError> {
        let Some(inner) = &self.inner else {
            return Err(plain("a scripted kind ran but the game has no script host"));
        };
        let Some(script) = sid.index().and_then(|n| inner.scripts.get(n)) else {
            return Err(plain(format!("kind has unknown script {}", sid.0)));
        };
        let arity = usize::from(arg.is_some());
        if !script.fns.contains(&(fname.to_owned(), arity)) {
            return Err(plain(format!(
                "script `{}` has no function `{fname}` taking {arity} argument{}",
                script.name,
                if arity == 1 { "" } else { "s" }
            )));
        }
        let shared = &inner.iws.shared;
        if shared.depth.get() >= MAX_DEPTH {
            return Err(plain(
                "scripts nest too deeply (spawn inside Create inside spawn ...)",
            ));
        }
        if shared.depth.get() == 0 {
            shared.budget.begin_call();
        }
        shared.depth.set(shared.depth.get() + 1);
        shared.dll_mode.set(ctx.dll_mode);
        // `script_execute` binds its callee's `this` to the instance of the call running now.
        let prev_this = shared.this_inst.replace(Some(me));
        let live = std::mem::replace(ctx.w, World::new(crate::def::RoomId(0), 0));
        let prev = shared.world.borrow_mut().replace(live);
        let result = if prev.is_some() {
            Err(plain("script events re-entered with the world already lent"))
        } else {
            let mut this = Dynamic::from(Inst(me));
            let opts = CallFnOptions::new().bind_this_ptr(&mut this).eval_ast(false);
            let mut scope = Scope::new();
            let args: Vec<Dynamic> = arg.into_iter().collect();
            inner
                .iws
                .engine
                .call_fn_with_options::<Dynamic>(opts, &mut scope, &script.ast, fname, args)
                .map(|_| Flow::Continue)
                .map_err(|e| {
                    let mut d = script_error(&e);
                    d.message = format!("{}: {} [{where_}]", script.name, d.message);
                    d
                })
        };
        let back = shared.world.borrow_mut().take();
        shared.this_inst.set(prev_this);
        shared.depth.set(shared.depth.get() - 1);
        match back {
            Some(w) => *ctx.w = w,
            None => return Err(plain("the lent world was lost during a script event")),
        }
        result
    }

    /// Runs the kid slot function `fname` of script `sid` for the kid `me` (decision 0004):
    /// `this` is bound to the kid and `other`, for a collision slot, is the one argument. The
    /// caller (`kid::run_script_slot`) checks the room request. A missing function is an error
    /// here even though the loader checks it, so a definition built by hand cannot skip it.
    pub fn call_slot(
        &self,
        ctx: &mut Ctx,
        sid: ScriptId,
        me: InstId,
        fname: &str,
        other: Option<InstId>,
    ) -> Result<Flow, SimError> {
        let tick = ctx.w.tick;
        let where_ = format!("kid slot `{fname}`, instance {}, tick {tick}", me.0);
        let arg = other.map(|o| Dynamic::from(Inst(o)));
        self.invoke(ctx, sid, me, fname, arg, &where_)
            .map_err(SimError::from)
    }
}

impl ScriptHost for ScriptRuntime {
    fn on_event(&self, ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Result<Flow, ScriptError> {
        let Some(inst) = ctx.w.get(me) else {
            return Ok(Flow::Continue);
        };
        let kind = inst.kind;
        let Some(sid) = ctx.def.kind(kind).and_then(|k| k.script) else {
            return Ok(Flow::Continue);
        };
        let Some(inner) = &self.inner else {
            return Err(plain("a scripted kind ran but the game has no script host"));
        };
        let Some(script) = sid.index().and_then(|n| inner.scripts.get(n)) else {
            return Err(plain(format!("kind has unknown script {}", sid.0)));
        };
        let (fname, arg): (Cow<'_, str>, Option<Dynamic>) = match ev {
            EventKind::Collision { with, .. } => {
                if let Some(name) = trigger_name(ctx.def, inst, *with, &script.fns) {
                    (Cow::Borrowed("trigger"), Some(Dynamic::from(name)))
                } else {
                    let Some((name, arg)) = event_call(ev) else {
                        return Ok(Flow::Continue);
                    };
                    (Cow::Borrowed(name), arg)
                }
            }
            EventKind::Moment(step) => {
                let Some(name) = moment_call(ctx.def, inst, *step) else {
                    return Ok(Flow::Continue);
                };
                (Cow::Owned(name), None)
            }
            _ => {
                let Some((name, arg)) = event_call(ev) else {
                    return Ok(Flow::Continue);
                };
                (Cow::Borrowed(name), arg)
            }
        };
        let arity = usize::from(arg.is_some());
        if !script.fns.contains(&(fname.to_string(), arity)) {
            return Ok(Flow::Continue);
        }
        let kind_name = ctx.def.kind(kind).map_or("?", |k| k.name.as_str()).to_owned();
        let tick = ctx.w.tick;
        let where_ = format!("kind {kind_name}, instance {}, {ev:?}, tick {tick}", me.0);
        self.invoke(ctx, sid, me, fname.as_ref(), arg, &where_)
    }
}

// ---- The IWS engine ----

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::{Rc, Weak};

use rhai::packages::{
    ArithmeticPackage, BasicArrayPackage, BasicMapPackage, BasicStringPackage, CorePackage, LogicPackage,
    Package,
};
use rhai::{AST, CallFnOptions, Dynamic, Engine, EvalAltResult, FuncArgs, OptimizationLevel, Scope};

use super::budget::Budget;
use crate::rng::{Rng, Stream};
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
    /// The live world while a script runs (see the module docs).
    world: RefCell<Option<World>>,
    pub(crate) host: OnceCell<Host>,
    pub(crate) dll_mode: Cell<DllMode>,
    pub(crate) depth: Cell<u32>,
    /// The instance `this` is bound to in the script call running now (`script_execute`, #1533).
    pub(crate) this_inst: Cell<Option<InstId>>,
    /// Top-level consts seen while a script is lexed (`consts.rs`).
    consts: RefCell<super::consts::ConstScan>,
}

/// What the registered functions need beyond the world: the game, the rules, the way back to
/// the runtime and the kind names.
pub(crate) struct Host {
    /// Shared with the `Sim`, not deep-copied per run (#1093).
    pub(crate) def: Arc<GameDef>,
    pub(crate) rules: RunnerRules,
    /// The script-semantics pair this run selected (decision 0005).
    pub(crate) script_rules: ScriptRules,
    pub(crate) inner: Weak<Inner>,
    /// Shared with the prepared artifact (#1093).
    pub(crate) kinds: Arc<BTreeMap<String, KindId>>,
}

impl std::fmt::Debug for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Host").finish_non_exhaustive()
    }
}

impl Host {
    /// Runs `script_execute`'s call from a registered function (#1533).
    pub(crate) fn script_execute(&self, name: &str, args: Vec<Dynamic>) -> Result<Dynamic, ScriptError> {
        let inner = self
            .inner
            .upgrade()
            .ok_or_else(|| plain("the script host is gone"))?;
        inner.execute(name, args)
    }
}

type Res<T> = Result<T, Box<EvalAltResult>>;

impl Shared {
    /// Runs `f` on the script's RNG: the live world's `script` stream when a script runs inside a
    /// tick (the shared gameplay generator under `gm8`), else the lent one.
    pub(crate) fn with_rng<R>(&self, f: impl FnOnce(&mut Rng) -> R) -> R {
        if let Ok(mut w) = self.world.try_borrow_mut()
            && let Some(w) = w.as_mut()
        {
            return f(w.stream_mut(Stream::Script));
        }
        f(&mut self.rng.borrow_mut())
    }

    pub(crate) fn current_tick(&self) -> u64 {
        match self.world.try_borrow().ok().as_deref() {
            Some(Some(w)) => w.tick,
            _ => self.tick.get(),
        }
    }

    pub(crate) fn current_room_tick(&self) -> u64 {
        match self.world.try_borrow().ok().as_deref() {
            Some(Some(w)) => w.room_tick,
            _ => self.room_tick.get(),
        }
    }

    /// Runs `f` on the lent world and the host info.
    pub(crate) fn with_world<R>(&self, f: impl FnOnce(&mut World, &Host) -> R) -> Res<R> {
        let host = self
            .host
            .get()
            .ok_or_else(|| super::api::runtime_error("instances need a game (no script host)"))?;
        let mut cell = self
            .world
            .try_borrow_mut()
            .map_err(|_| super::api::runtime_error("the world is busy"))?;
        let w = cell
            .as_mut()
            .ok_or_else(|| super::api::runtime_error("no world: the call is outside a tick"))?;
        Ok(f(w, host))
    }

    /// Runs `f` with a full `Ctx` on the lent world. Scripts run by `f` (a spawned instance
    /// Create) park the world again.
    pub(crate) fn with_ctx<R>(&self, f: impl FnOnce(&mut Ctx) -> R) -> Res<R> {
        let host = self
            .host
            .get()
            .ok_or_else(|| super::api::runtime_error("instances need a game (no script host)"))?;
        let inner = host
            .inner
            .upgrade()
            .ok_or_else(|| super::api::runtime_error("the script host is gone"))?;
        let rt = ScriptRuntime { inner: Some(inner) };
        let mut w = self
            .world
            .borrow_mut()
            .take()
            .ok_or_else(|| super::api::runtime_error("no world: the call is outside a tick"))?;
        let r = {
            let mut ctx = Ctx {
                def: host.def.as_ref(),
                rules: &host.rules,
                w: &mut w,
                scripts: &rt,
                dll_mode: self.dll_mode.get(),
            };
            f(&mut ctx)
        };
        *self.world.borrow_mut() = Some(w);
        Ok(r)
    }

    /// Charges `units` of host work to the tick budget; a script error when the budget is spent.
    pub(crate) fn charge(&self, units: u64) -> Res<()> {
        if self.budget.charge(units) {
            Ok(())
        } else {
            Err(super::api::runtime_error("script budget exceeded"))
        }
    }

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
    pub(crate) engine: Engine,
    pub(crate) shared: Rc<Shared>,
    docs: super::docs::Docs,
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
            world: RefCell::new(None),
            host: OnceCell::new(),
            dll_mode: Cell::new(DllMode::Stub),
            depth: Cell::new(0),
            this_inst: Cell::new(None),
            consts: RefCell::default(),
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
        let mut docs = super::docs::Docs::default();
        super::api::register_numeric(&mut engine, &shared, &mut docs);
        super::inst::register_instance_api(&mut engine, &shared, &mut docs);
        super::inst::register_kid_api(&mut engine, &shared, &mut docs);
        super::array::register_array_api(&mut engine, &shared, &mut docs);
        super::collide::register_collision_api(&mut engine, &shared, &mut docs);

        {
            // A top-level `const` is visible inside functions (#366).
            let s = Rc::clone(&shared);
            #[allow(
                deprecated,
                reason = "Rhai marks the volatile `internals` hook; the pin keeps it stable"
            )]
            engine.on_parse_token(move |tok, _, _| s.consts.borrow_mut().token(tok));
        }
        engine.disable_symbol("eval");
        engine.disable_symbol("**");
        engine.disable_symbol("**=");
        engine.set_optimization_level(OptimizationLevel::Simple);
        engine.set_strict_variables(true);
        engine.set_fast_operators(false);
        engine.set_max_call_levels(32);
        engine.set_max_expr_depths(64, 32);
        engine.set_max_string_size(4096);
        // One cap for stored and script arrays (#984).
        engine.set_max_array_size(crate::world::MAX_ARRAY_SIZE);
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
        Iws { engine, shared, docs }
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
        self.compile_ast(source).map_err(|e| ScriptError {
            message: format!("compile error: {e}"),
        })
    }

    /// Forgets the consts of the last script (before lexing a new one).
    pub(crate) fn reset_consts(&self) {
        self.shared.consts.borrow_mut().reset();
    }

    /// Compiles `source`, keeping Rhai's parse error (the validator reports its position).
    pub(crate) fn compile_ast(&self, source: &str) -> Result<AST, rhai::ParseError> {
        self.reset_consts();
        // `global` is a constant of the compile scope, so strict variables accept it; the
        // engine's variable resolver gives its value at run time (`array.rs`, #997).
        let mut scope = Scope::new();
        scope.push_constant(super::array::GLOBAL, super::array::Globals);
        self.engine.compile_with_scope(&scope, source)
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

    /// The Rhai engine, read-only, for `iwbtg script api-docs` (it lists the registered API).
    pub fn rhai_engine(&self) -> &Engine {
        &self.engine
    }

    /// The one-line descriptions of the registered API (`iwbtg script api-docs`).
    pub fn docs(&self) -> &super::docs::Docs {
        &self.docs
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
