//! Shared fixture for the WP1 tests (docs/plans/sim-core.md 12, WP1 row).
//!
//! Included by the other `wp1_*.rs` files with `#[path]`; compiled alone it has no tests. The
//! pipeline runs with stand-in hooks, because motion (WP2), collision geometry (WP3), rooms
//! (WP4a), views (WP4b), timing (WP8b) and sound (WP10) are separate work packages:
//!
//! - motion moves nothing; global `END_PATH = Inst(id)` makes that instance's path end;
//! - an instance with a sprite has the box `round(x) +- 8`, `round(y) +- 8`; without, no box;
//! - two instances overlap when either has the global `touch_var(id) = 1`;
//! - the solid put-back appends `put ` to the log and runs the event;
//! - a room change sets `World::room` for `Goto` and counts `ROOM_CHANGES`.
#![allow(dead_code, reason = "each test file uses part of the fixture")]

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, GameDef, IRect, KindDef, KindId, RoomDef, RoomId, VarId};
use iwbtg_sim::error::SimError;
use iwbtg_sim::events::{ProbeState, probe_event_set};
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::motion::MotionEvents;
use iwbtg_sim::num::{Real, to_int_round};
use iwbtg_sim::room::RoomRequest;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::tick::{self, EventRun, Hooks};
use iwbtg_sim::world::{Ctx, Flow, InstId, KindState, SpawnInit, Value, World};

/// The probe log global.
pub const LOG: VarId = VarId(0);
/// Counts test room changes.
pub const ROOM_CHANGES: VarId = VarId(1);
/// `Inst(id)`: that instance's path ends in this tick's motion.
pub const END_PATH: VarId = VarId(2);

/// The global that marks `id` as touching everything.
pub fn touch_var(id: InstId) -> VarId {
    VarId(u32::try_from(id.0).expect("test ids fit u32"))
}

fn capture(_: &mut Ctx, _: InstId) -> Result<(), SimError> {
    Ok(())
}

fn motion(ctx: &mut Ctx, id: InstId) -> Result<MotionEvents, SimError> {
    Ok(MotionEvents {
        end_of_path: ctx.w.globals.get(&END_PATH) == Some(&Value::Inst(id)),
    })
}

fn bbox(ctx: &Ctx, id: InstId) -> Result<Option<IRect>, SimError> {
    let Some(i) = ctx.w.get(id) else {
        return Ok(None);
    };
    if i.img.sprite().is_none() {
        return Ok(None);
    }
    let x = to_int_round(i.m.x()).map_err(|e| SimError::from_num("x", e))?;
    let y = to_int_round(i.m.y()).map_err(|e| SimError::from_num("y", e))?;
    Ok(Some(IRect {
        left: x - 8,
        top: y - 8,
        right: x + 8,
        bottom: y + 8,
    }))
}

/// Whether `id` has its touch flag set.
pub fn touching(w: &World, id: InstId) -> bool {
    w.globals.get(&touch_var(id)) == Some(&Value::Real(Real::ONE))
}

fn overlaps(ctx: &Ctx, a: InstId, b: InstId) -> Result<bool, SimError> {
    Ok(touching(ctx.w, a) || touching(ctx.w, b))
}

/// Appends to the log global.
pub fn append(w: &mut World, text: &str) {
    match w.globals.get_mut(&LOG) {
        Some(Value::Str(s)) => s.push_str(text),
        _ => {
            w.globals.insert(LOG, Value::Str(text.to_owned()));
        }
    }
}

fn solid(ctx: &mut Ctx, _a: InstId, _b: InstId, run: EventRun) -> Result<Flow, SimError> {
    append(ctx.w, "put ");
    Ok(run(ctx))
}

fn view(_: &mut Ctx) -> Result<(), SimError> {
    Ok(())
}

fn room(ctx: &mut Ctx, req: RoomRequest) -> Result<(), SimError> {
    if let RoomRequest::Goto(r) = req {
        ctx.w.room = r;
    }
    let n = match ctx.w.globals.get(&ROOM_CHANGES) {
        Some(Value::Real(r)) => r.get(),
        _ => 0.0,
    };
    ctx.w.globals.insert(ROOM_CHANGES, Value::Real(Real::new(n + 1.0)));
    Ok(())
}

fn sound(_: &mut Ctx) {}

fn timing(_: &mut World, _: &GameDef) -> Result<Vec<iwbtg_sim::timing::Split>, SimError> {
    Ok(Vec::new())
}

/// The stand-in hooks.
pub const HOOKS: Hooks = Hooks {
    capture_previous: capture,
    auto_motion: motion,
    bbox,
    overlaps,
    solid_resolve: solid,
    view_follow: view,
    room_change: room,
    sound_tick: sound,
    timing,
};

/// A probe kind: the probe's full event set, no classes, no sprite.
pub fn probe_kind(b: &mut GameDefBuilder, name: &str) -> KindId {
    let mut k = KindDef::new(name, Builtin::Probe);
    k.events = probe_event_set();
    b.kind(k)
}

/// A builder with two 800 x 608 rooms.
pub fn builder() -> GameDefBuilder {
    let mut b = GameDefBuilder::new("wp1");
    b.room(RoomDef::new("room_0", 800, 608));
    b.room(RoomDef::new("room_1", 800, 608));
    b
}

/// A probe state tagged `tag` that logs to [`LOG`].
pub fn p(tag: &str) -> ProbeState {
    ProbeState::new(tag, Some(LOG))
}

/// A world, its game and its rules.
pub struct Fx {
    /// The game.
    pub def: GameDef,
    /// The rules (tests change them).
    pub rules: RunnerRules,
    /// No scripts.
    pub scripts: ScriptRuntime,
    /// The world, in room 0 at tick 0.
    pub w: World,
}

impl Fx {
    /// A fixture for `def` with the default rules.
    pub fn new(def: GameDef) -> Fx {
        Fx {
            def,
            rules: RunnerRules::default(),
            scripts: ScriptRuntime::default(),
            w: World::new(RoomId(0), 1),
        }
    }

    /// A context on the fixture.
    pub fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            def: &self.def,
            rules: &self.rules,
            w: &mut self.w,
            scripts: &self.scripts,
            dll_mode: DllMode::Stub,
        }
    }

    /// Adds a probe instance with `state`, without running its Create.
    pub fn probe(&mut self, kind: KindId, x: f64, y: f64, state: ProbeState) -> InstId {
        let mut c = self.ctx();
        let init = SpawnInit {
            run_create: false,
            ..SpawnInit::default()
        };
        let id = c
            .spawn(kind, Real::new(x), Real::new(y), init)
            .expect("spawn");
        c.w.get_mut(id).expect("just spawned").state = KindState::Probe(state);
        id
    }

    /// Runs one tick with the stand-in hooks.
    pub fn tick(&mut self, input: InputFrame) {
        let mut c = self.ctx();
        tick::run_tick_with(&mut c, input, &HOOKS).expect("tick");
    }

    /// Runs `n` ticks with no input.
    pub fn idle(&mut self, n: u32) {
        for _ in 0..n {
            self.tick(InputFrame::EMPTY);
        }
    }

    /// The log so far.
    pub fn log(&self) -> String {
        match self.w.globals.get(&LOG) {
            Some(Value::Str(s)) => s.clone(),
            _ => String::new(),
        }
    }

    /// The log so far, cleared.
    pub fn take_log(&mut self) -> String {
        let s = self.log();
        self.w.globals.remove(&LOG);
        s
    }

    /// Sets a global.
    pub fn set(&mut self, var: VarId, v: Value) {
        self.w.globals.insert(var, v);
    }

    /// Marks `id` as touching everything (stand-in overlap).
    pub fn touch(&mut self, id: InstId, on: bool) {
        let v = if on { Real::ONE } else { Real::ZERO };
        self.set(touch_var(id), Value::Real(v));
    }
}

/// The log tokens of one tag, in order.
pub fn tokens_of<'a>(log: &'a str, tag: &str) -> Vec<&'a str> {
    log.split(' ').filter(|t| !t.is_empty() && t.ends_with(tag)).collect()
}

/// `Real` from an `f64`.
pub fn r(v: f64) -> Real {
    Real::new(v)
}

/// `Value::Real` from an `f64`.
pub fn vr(v: f64) -> Value {
    Value::Real(Real::new(v))
}
