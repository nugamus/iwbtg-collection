//! The golden corpus: small engine worlds we build ourselves (no creator content), each run for a
//! fixed number of ticks on a fixed input, with one pinned state hash per case
//! (docs/architecture/sim/determinism.md; docs/architecture/testing.md).
//!
//! Two programs include this file with `#[path]`, so it uses only `iwbtg_sim`'s public API and
//! `std`:
//!
//! - `tests/golden_corpus.rs` checks every case against `expected.txt` on every CI target;
//! - `iwbtg determinism report <corpus>` (crates/tools) prints the same lines, and CI compares the
//!   output of Windows, Linux and wasm32-wasip1 byte for byte.
//!
//! A deliberate change to the sim or to `HASH_LAYOUT` re-pins `expected.txt` in the same PR
//! (`UPDATE_GOLDEN=1 cargo test -p iwbtg-sim --test golden_corpus`).
//!
//! The first cases drive probe kinds (`Builtin::Probe`) through the real tick pipeline: real
//! motion, collision, rooms and room changes. The kid courses and the script API case
//! (`courses.rs`, #786) run one room per kid family with the engine kinds and every built-in,
//! and a script that calls every script API function. After the case lines the report lists,
//! per built-in and per kid family, the first case that ran it (`builtin <name> <case>`,
//! `family <name> <case>`, `-` for none); `golden_corpus.rs` fails when one is missing.
//!
//! Running is not touching (#847): a built-in with a collision handler between it and the kid
//! or a kid bullet (on either side) also gets a `touched <name> <case>` line naming the first
//! case where such a pair overlapped in the collision phase, so its collision path ran. The
//! harness sees that from outside: it runs each tick with [`Hooks::ENGINE`] except for an
//! `overlaps` hook that calls the engine's and notes the pairs that met; sim state, hash and
//! RNG are untouched.
//!
//! Many built-ins get their own kid handler only under a param (a fake block that vanishes on
//! touch, a save you shoot, a spike that destroys bullets, #898), so a built-in being touched does
//! not say that handler ran. Each collision handler of a built-in's own event set that matches
//! the kid, a kid bullet or blood gets a `touched <name>/<handler classes> <case>` line too,
//! naming the first case where that handler's pair met.
//! Two params can add the same handler (`hidden_block` `stops_bullets` and
//! `destroyed_by_bullet`), so each `builtins::event_set_params` value that adds one gets a
//! `touched <name>/<param>=<value> <case>` line too, naming the first case where a handler it
//! adds ran on a kind with that value (#917).
#![allow(
    dead_code,
    reason = "the two programs that include this file use different parts"
)]

mod courses;

use std::cell::RefCell;
use std::collections::BTreeSet;

use iwbtg_sim::def::KidFamily;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, GameDef, KindDef, KindId, ParamValue, RoomDef, RoomId, ScriptDef, ScriptId, VarId,
};
use iwbtg_sim::error::SimError;
use iwbtg_sim::events::{ProbeAct, ProbeOn, ProbeState, ProbeStep, probe_event_set};
use iwbtg_sim::events::{event_set, pair_handler};
use iwbtg_sim::hash;
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::num::Real;
use iwbtg_sim::room::{RoomRequest, StartPoint};
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::tick::{self, Hooks};
use iwbtg_sim::world::{Ctx, InstId, KindState, SimStatus, SpawnInit, Value, World};

/// The file with the pinned lines, next to this module.
pub const EXPECTED_FILE: &str = "expected.txt";

/// What a case produced.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Outcome {
    /// Case name.
    pub name: &'static str,
    /// Ticks the world ran (fewer than asked when it halted).
    pub ticks: u64,
    /// `running`, or `halted: <error>`.
    pub status: String,
    /// `hash::state_hash` after the last tick, lowercase hex.
    pub hash: String,
}

impl Outcome {
    /// `name ticks status hash`, the line the report prints and `expected.txt` pins.
    pub fn line(&self) -> String {
        format!(
            "{} {} {} {}",
            self.name,
            self.ticks,
            self.status.replace(' ', "_"),
            self.hash
        )
    }
}

/// One case: a name and a function that builds the world and says how long to run.
pub struct Case {
    /// Unique, lowercase, no spaces.
    pub name: &'static str,
    /// Ticks to run.
    pub ticks: u64,
    /// Builds the game and puts the first instances into room 0.
    pub build: fn() -> Fixture,
    /// The input of tick `t`.
    pub input: fn(u64) -> InputFrame,
}

/// All cases, in report order.
pub fn cases() -> Vec<Case> {
    let mut all = probe_cases();
    all.extend(courses::cases());
    all
}

/// The probe cases (the corpus before #786).
fn probe_cases() -> Vec<Case> {
    vec![
        Case {
            name: "spawn_alarms_rooms",
            ticks: 10_000,
            build: spawn_alarms_rooms,
            input: pattern_input,
        },
        Case {
            name: "motion_fractions",
            ticks: 4_000,
            build: motion_fractions,
            input: no_input,
        },
        Case {
            name: "keys_random",
            ticks: 6_000,
            build: keys,
            input: random_input,
        },
        Case {
            name: "scripted_object",
            ticks: 3_000,
            build: scripted_object,
            input: no_input,
        },
        Case {
            name: "non_finite_halts",
            ticks: 100,
            build: non_finite,
            input: no_input,
        },
    ]
}

/// Runs one case.
pub fn run(case: &Case) -> Outcome {
    let (fx, ran) = run_fixture(case);
    outcome(case, &fx, ran)
}

fn outcome(case: &Case, fx: &Fixture, ran: u64) -> Outcome {
    Outcome {
        name: case.name,
        ticks: ran,
        status: match &fx.w.status {
            SimStatus::Running => "running".to_owned(),
            SimStatus::Halted(e) => format!("halted:{e}"),
            other => format!("{other:?}"),
        },
        hash: hex(&hash::state_hash(&fx.w)),
    }
}

/// Runs one case and returns the finished fixture and the ticks it ran.
pub fn run_fixture(case: &Case) -> (Fixture, u64) {
    let (fx, ran, _) = run_covered(case);
    (fx, ran)
}

/// What a case ran: the built-ins that had a live instance at the start or after some tick,
/// the kid family when a kid was alive, and the built-ins the kid or a kid bullet met.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Seen {
    /// Indexes into `Builtin::ALL`.
    pub builtins: BTreeSet<usize>,
    /// The game's kid family, when a kid instance lived.
    pub family: Option<KidFamily>,
    /// Built-ins (indexes into `Builtin::ALL`) that had a live instance whose kind has a
    /// collision handler with a kid or kid bullet kind, on either side.
    pub handled: BTreeSet<usize>,
    /// Built-ins whose instance overlapped a kid or kid bullet in the collision phase.
    pub touched: BTreeSet<usize>,
    /// Own handlers (built-in index, [`handler_label`]) of a kind that had a live instance and
    /// that match a kid, kid bullet or blood kind.
    pub handled_variants: BTreeSet<Variant>,
    /// Own handlers that ran: their pair overlapped in the collision phase.
    pub touched_variants: BTreeSet<Variant>,
    /// Param values (built-in index, `<param>=<value>`) of a kind that had a live instance and
    /// that add an own kid, kid bullet or blood handler ([`param_variants`], #917).
    pub handled_params: BTreeSet<Variant>,
    /// Param values whose added handler ran on a kind with that value.
    pub touched_params: BTreeSet<Variant>,
    /// The kinds that had a live instance.
    kinds: BTreeSet<KindId>,
}

fn builtin_index(b: Builtin) -> Option<usize> {
    Builtin::ALL.iter().position(|x| *x == b)
}

fn base_of(def: &GameDef, kind: KindId) -> Option<Builtin> {
    def.kind(kind).map(|k| k.base)
}

fn is_kid(b: Builtin) -> bool {
    matches!(b, Builtin::Kid | Builtin::KidBullet)
}

/// Whether a collision event can run between `kind` and a kid or kid bullet kind of `def`: one
/// of the two has a handler matching the other's classes.
fn handles_kid(def: &GameDef, kind: KindId) -> bool {
    let Some(k) = def.kind(kind) else {
        return false;
    };
    let mine = &event_set(def, kind).collisions;
    def.kinds.iter().filter(|kid| is_kid(kid.base)).any(|kid| {
        pair_handler(mine, kid.classes).is_some() || pair_handler(&kid.events.collisions, k.classes).is_some()
    })
}

/// One collision handler of a built-in's own event set: the built-in (index into
/// `Builtin::ALL`) and the handler's classes ([`handler_label`]).
pub type Variant = (usize, String);

/// What a handler of a variant line is run against: the kid, its bullets and its blood.
fn is_partner(b: Builtin) -> bool {
    matches!(b, Builtin::Kid | Builtin::KidBullet | Builtin::Blood)
}

/// A handler's classes by name, `+` between them (`kid+kid_flipped`); package classes are
/// `package`.
fn handler_label(mask: ClassMask) -> String {
    let mut names: Vec<&str> = ClassMask::NAMED
        .iter()
        .filter(|(c, _)| mask.contains(*c))
        .map(|(_, n)| *n)
        .collect();
    let named = ClassMask::NAMED.iter().fold(0, |m, (c, _)| m | c.0);
    if mask.0 & !named != 0 {
        names.push("package");
    }
    names.join("+")
}

/// The own handler of `kind` that runs for an other with `classes`, as a [`Variant`], when
/// `kind` is not itself a kid, kid bullet or blood kind.
fn variant(def: &GameDef, kind: KindId, classes: ClassMask) -> Option<Variant> {
    let base = base_of(def, kind).filter(|b| !is_partner(*b))?;
    let (_, with) = pair_handler(&event_set(def, kind).collisions, classes)?;
    Some((builtin_index(base)?, handler_label(with)))
}

/// The own handlers of `kind` that a kid, kid bullet or blood kind of `def` can run.
fn kind_variants(def: &GameDef, kind: KindId) -> impl Iterator<Item = Variant> + '_ {
    def.kinds
        .iter()
        .filter(|p| is_partner(p.base))
        .filter_map(move |p| variant(def, kind, p.classes))
}

/// The labels ([`handler_label`]) of the own handlers that a kid, kid bullet or blood kind of
/// `def` runs on a kind shaped like `kd`.
fn partner_labels(def: &GameDef, kd: &KindDef) -> BTreeSet<String> {
    let set = iwbtg_sim::builtins::event_set(kd.base, kd);
    def.kinds
        .iter()
        .filter(|p| is_partner(p.base))
        .filter_map(|p| pair_handler(&set.collisions, p.classes))
        .map(|(_, with)| handler_label(with))
        .collect()
}

/// A param value as a report word: `true`, `group_fade`, `1.0`.
fn value_text(v: &ParamValue) -> String {
    match v {
        ParamValue::Bool(b) => b.to_string(),
        ParamValue::Str(s) => s.clone(),
        ParamValue::Real(r) => r.to_string(),
        other => format!("{other:?}"),
    }
}

/// The values of `kind`'s `builtins::event_set_params` that add an own handler for a kid, kid
/// bullet or blood kind of `def` (#917), as `(built-in index, "<param>=<value>")`, each with the
/// handler labels it adds: those of the kind with only that param set, minus those of the kind
/// with none of them set. Two params that add the same handler (`hidden_block`
/// `stops_bullets` and `destroyed_by_bullet`) get one entry each.
fn param_variants(def: &GameDef, kind: KindId) -> Vec<(Variant, BTreeSet<String>)> {
    let Some(kd) = def.kind(kind).filter(|k| !is_partner(k.base)) else {
        return Vec::new();
    };
    let Some(i) = builtin_index(kd.base) else {
        return Vec::new();
    };
    let params = iwbtg_sim::builtins::event_set_params(kd.base);
    let mut bare = kd.clone();
    for p in params {
        bare.params.0.remove(*p);
    }
    let without = partner_labels(def, &bare);
    let mut out = Vec::new();
    for p in params {
        let Some(v) = kd.params.0.get(*p) else {
            continue;
        };
        let mut one = bare.clone();
        one.params.0.insert((*p).to_owned(), v.clone());
        let added: BTreeSet<String> = partner_labels(def, &one).difference(&without).cloned().collect();
        if !added.is_empty() {
            out.push(((i, format!("{p}={}", value_text(v))), added));
        }
    }
    out
}

impl Seen {
    fn look(&mut self, fx: &Fixture) {
        for inst in fx.w.iter_alive() {
            let Some(base) = base_of(&fx.def, inst.kind) else {
                continue;
            };
            if let Some(i) = builtin_index(base) {
                self.builtins.insert(i);
            }
            self.kinds.insert(inst.kind);
            if base == Builtin::Kid {
                self.family = Some(fx.def.physics.preset);
            }
        }
    }
}

thread_local! {
    /// The built-ins [`observe_overlaps`] saw meet a kid or kid bullet in the running case.
    static TOUCHED: RefCell<BTreeSet<usize>> = const { RefCell::new(BTreeSet::new()) };
    /// The own handlers [`observe_overlaps`] saw run against a kid, kid bullet or blood.
    static TOUCHED_VARIANTS: RefCell<BTreeSet<Variant>> = const { RefCell::new(BTreeSet::new()) };
    /// The same per kind: (kind, handler label), for the param lines (#917).
    static TOUCHED_KINDS: RefCell<BTreeSet<(KindId, String)>> = const { RefCell::new(BTreeSet::new()) };
}

/// The engine's `overlaps` with the same result, noting the built-in of a pair that met a kid
/// or kid bullet, and the own handler of a built-in that met a kid, kid bullet or blood. The
/// collision phase tests a pair only when one side has a handler for the other, so a noted pair
/// is one whose collision event runs. The handler is picked from the instances' classes as the
/// collision phase picks it (`pair_handler`).
fn observe_overlaps(ctx: &Ctx, a: InstId, b: InstId) -> Result<bool, SimError> {
    let met = (Hooks::ENGINE.overlaps)(ctx, a, b)?;
    if met && let (Some(ia), Some(ib)) = (ctx.w.get(a), ctx.w.get(b)) {
        for (me, other) in [(ia, ib), (ib, ia)] {
            let partner = base_of(ctx.def, other.kind).is_some_and(is_partner);
            if partner && let Some(v) = variant(ctx.def, me.kind, other.classes) {
                TOUCHED_KINDS.with(|t| t.borrow_mut().insert((me.kind, v.1.clone())));
                TOUCHED_VARIANTS.with(|t| t.borrow_mut().insert(v));
            }
        }
    }
    if met {
        let base = |id| ctx.w.get(id).and_then(|i| base_of(ctx.def, i.kind));
        if let (Some(ba), Some(bb)) = (base(a), base(b)) {
            TOUCHED.with(|t| {
                let mut t = t.borrow_mut();
                for (me, other) in [(ba, bb), (bb, ba)] {
                    if is_kid(me) {
                        t.extend(builtin_index(other));
                    }
                }
            });
        }
    }
    Ok(met)
}

/// [`Hooks::ENGINE`] with [`observe_overlaps`]: what the cases run with.
const OBSERVE: Hooks = Hooks {
    overlaps: observe_overlaps,
    ..Hooks::ENGINE
};

/// [`run_fixture`] that also records what the case ran.
pub fn run_covered(case: &Case) -> (Fixture, u64, Seen) {
    let mut fx = (case.build)();
    let mut seen = Seen::default();
    seen.look(&fx);
    TOUCHED.with(|t| t.borrow_mut().clear());
    TOUCHED_VARIANTS.with(|t| t.borrow_mut().clear());
    TOUCHED_KINDS.with(|t| t.borrow_mut().clear());
    let mut ran = 0;
    for t in 0..case.ticks {
        if fx.w.status != SimStatus::Running {
            break;
        }
        let input = (case.input)(t);
        let mut ctx = fx.ctx();
        // A halting tick returns the error and leaves the world `Halted`; the status says so.
        let _ = tick::run_tick_with(&mut ctx, input, &OBSERVE);
        ran = fx.w.tick;
        seen.look(&fx);
    }
    seen.touched = TOUCHED.with(|t| std::mem::take(&mut *t.borrow_mut()));
    // Kinds do not change during a run, so the handlers are checked once per kind that lived.
    seen.handled = seen
        .kinds
        .iter()
        .filter(|k| handles_kid(&fx.def, **k))
        .filter_map(|k| base_of(&fx.def, *k).and_then(builtin_index))
        .collect();
    seen.touched_variants = TOUCHED_VARIANTS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    seen.handled_variants = seen
        .kinds
        .iter()
        .flat_map(|k| kind_variants(&fx.def, *k))
        .collect();
    let touched_kinds = TOUCHED_KINDS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    for k in &seen.kinds {
        for (pv, added) in param_variants(&fx.def, *k) {
            if added.iter().any(|l| touched_kinds.contains(&(*k, l.clone()))) {
                seen.touched_params.insert(pv.clone());
            }
            seen.handled_params.insert(pv);
        }
    }
    (fx, ran, seen)
}

/// Every kid family, in declaration order.
pub const FAMILIES: [KidFamily; 7] = [
    KidFamily::A1,
    KidFamily::A2,
    KidFamily::A3,
    KidFamily::A4,
    KidFamily::A5,
    KidFamily::B,
    KidFamily::C,
];

/// The report: one line per case, then one coverage line per built-in and per kid family
/// naming the first case that ran it (`-` for none), then one `touched <name> <case>` line per
/// built-in with a kid or kid bullet collision handler in a case that ran it, naming the first
/// case where the two met (`-` for none), then one `touched <name>/<handler classes> <case>`
/// line per own handler of a built-in that matches a kid, kid bullet or blood kind in a case
/// that ran it, naming the first case where that handler ran (`-` for none), in built-in order,
/// then one `touched <name>/<param>=<value> <case>` line per [`param_variants`] value in a case
/// that ran it, naming the first case where a handler it adds ran (#917).
pub fn report() -> String {
    let mut out = String::new();
    let mut builtin_case: Vec<Option<&'static str>> = vec![None; Builtin::ALL.len()];
    let mut family_case: Vec<Option<&'static str>> = vec![None; FAMILIES.len()];
    let mut handled = vec![false; Builtin::ALL.len()];
    let mut touched_case: Vec<Option<&'static str>> = vec![None; Builtin::ALL.len()];
    let mut variant_case: std::collections::BTreeMap<Variant, Option<&'static str>> =
        std::collections::BTreeMap::new();
    let mut param_case: std::collections::BTreeMap<Variant, Option<&'static str>> =
        std::collections::BTreeMap::new();
    for case in cases() {
        let (fx, ran, seen) = run_covered(&case);
        out.push_str(&outcome(&case, &fx, ran).line());
        out.push('\n');
        for i in seen.builtins {
            builtin_case[i].get_or_insert(case.name);
        }
        for i in seen.handled {
            handled[i] = true;
        }
        for i in seen.touched {
            touched_case[i].get_or_insert(case.name);
        }
        for v in seen.handled_variants {
            variant_case.entry(v).or_insert(None);
        }
        for v in seen.touched_variants {
            variant_case.entry(v).or_insert(None).get_or_insert(case.name);
        }
        for v in seen.handled_params {
            param_case.entry(v).or_insert(None);
        }
        for v in seen.touched_params {
            param_case.entry(v).or_insert(None).get_or_insert(case.name);
        }
        if let Some(f) = seen.family.and_then(|f| FAMILIES.iter().position(|x| *x == f)) {
            family_case[f].get_or_insert(case.name);
        }
    }
    for (b, case) in Builtin::ALL.iter().zip(&builtin_case) {
        out.push_str(&format!("builtin {} {}\n", b.name(), case.unwrap_or("-")));
    }
    for (f, case) in FAMILIES.iter().zip(&family_case) {
        out.push_str(&format!("family {f:?} {}\n", case.unwrap_or("-")));
    }
    for (i, b) in Builtin::ALL.iter().enumerate() {
        if handled[i] || touched_case[i].is_some() {
            let case = touched_case[i].unwrap_or("-");
            out.push_str(&format!("touched {} {case}\n", b.name()));
        }
    }
    for ((i, label), case) in &variant_case {
        let name = Builtin::ALL[*i].name();
        let case = case.unwrap_or("-");
        out.push_str(&format!("touched {name}/{label} {case}\n"));
    }
    for ((i, param), case) in &param_case {
        let name = Builtin::ALL[*i].name();
        let case = case.unwrap_or("-");
        out.push_str(&format!("touched {name}/{param} {case}\n"));
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------------------------
// Fixture

/// A game and a world, driven with the engine's own hooks.
pub struct Fixture {
    def: GameDef,
    rules: RunnerRules,
    scripts: ScriptRuntime,
    /// The world.
    pub w: World,
}

impl Fixture {
    fn new(def: GameDef, seed: u32) -> Fixture {
        let start = def.start_room;
        Fixture {
            def,
            rules: RunnerRules::default(),
            scripts: ScriptRuntime::default(),
            w: World::new(start, seed),
        }
    }

    /// A game started the way `Sim::new` starts it: the runner's rules, the compiled scripts,
    /// boot instances, the start room (`room::start_game`).
    fn started(def: GameDef, seed: u32) -> Fixture {
        let mut rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("golden rules");
        rules.num.float_model = def.float_model;
        let scripts = ScriptRuntime::new(&def, &rules).expect("golden scripts compile");
        let start = def.start_room;
        let mut w = World::new(start, seed);
        w.next_id = World::first_runtime_id(&def);
        let mut fx = Fixture {
            def,
            rules,
            scripts,
            w,
        };
        let mut c = fx.ctx();
        iwbtg_sim::room::start_game(
            &mut c,
            &StartPoint::Room {
                room: start,
                difficulty: 0,
            },
        )
        .expect("golden start");
        fx
    }

    fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            def: &self.def,
            rules: &self.rules,
            w: &mut self.w,
            scripts: &self.scripts,
            dll_mode: DllMode::Stub,
        }
    }

    /// Adds a probe instance with `state`, without running its Create.
    fn probe(&mut self, kind: KindId, x: f64, y: f64, state: ProbeState) -> InstId {
        let mut c = self.ctx();
        let init = SpawnInit {
            run_create: false,
            ..SpawnInit::default()
        };
        let id = c
            .spawn(kind, Real::new(x), Real::new(y), init)
            .expect("golden spawn");
        c.w.get_mut(id).expect("just spawned").state = KindState::Probe(state);
        id
    }
}

fn probe_kind(b: &mut GameDefBuilder, name: &str, sprite: bool) -> KindId {
    let mut k = KindDef::new(name, Builtin::Probe);
    k.events = probe_event_set();
    k.classes = ClassMask::NONE;
    if sprite {
        k.sprite = Some(b.sprite_rect((16, 16), (8, 8)));
    }
    b.kind(k)
}

fn builder() -> GameDefBuilder {
    let mut b = GameDefBuilder::new("golden");
    b.room(RoomDef::new("room_0", 800, 608));
    b.room(RoomDef::new("room_1", 800, 608));
    b
}

fn r(v: f64) -> Real {
    Real::new(v)
}

fn vr(v: f64) -> Value {
    Value::Real(r(v))
}

const COUNTER: VarId = VarId(10);

// ---------------------------------------------------------------------------------------------
// Inputs

fn no_input(_: u64) -> InputFrame {
    InputFrame::default()
}

/// Jump held in a 7-tick pattern, Right in a 13-tick pattern.
fn pattern_input(t: u64) -> InputFrame {
    let held = |t: u64| -> u32 {
        let mut h = 0;
        if t % 7 < 3 {
            h |= Action::Jump.bit();
        }
        if t % 13 < 9 {
            h |= Action::Right.bit();
        }
        h
    };
    InputFrame::from_held(if t == 0 { 0 } else { held(t - 1) }, held(t))
}

/// A fixed pseudo-random stream (a 64-bit LCG), with sub-tick taps: edges that are not in the
/// held bits.
fn random_input(t: u64) -> InputFrame {
    let mut s = t
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let mut next = || {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (s >> 33) as u32
    };
    let held = next() & Action::MASK;
    let pressed = (next() & Action::MASK) | (held & next());
    let released = next() & Action::MASK & !held;
    InputFrame {
        held,
        pressed,
        released,
    }
}

// ---------------------------------------------------------------------------------------------
// Cases

/// A parent probe that spawns short-lived children, sets alarms, reads keys and changes rooms.
fn spawn_alarms_rooms() -> Fixture {
    let mut b = builder();
    let parent = probe_kind(&mut b, "parent", false);
    let kid = probe_kind(&mut b, "child", true);
    let mut fx = Fixture::new(b.build().expect("golden def"), 1);
    let mut child = ProbeState::new("C", None);
    child.steps = vec![
        ProbeStep::on(ProbeOn::Create, ProbeAct::SetAlarm(0, r(7.0))),
        ProbeStep::on(ProbeOn::Alarm(0), ProbeAct::DestroySelf),
        ProbeStep::on(ProbeOn::Step, ProbeAct::SetGlobal(COUNTER, vr(1.0))).every(3),
        ProbeStep::on(ProbeOn::Step, ProbeAct::Move(r(0.75), r(-0.5))),
    ];
    let mut s = ProbeState::new("P", None);
    s.steps = vec![
        ProbeStep::on(
            ProbeOn::Step,
            ProbeAct::Spawn {
                kind: kid,
                x: r(100.5),
                y: r(-3.25),
                child: Some(Box::new(child)),
            },
        )
        .every(5),
        ProbeStep::on(ProbeOn::BeginStep, ProbeAct::DirectRead(Action::Jump.bit())).every(2),
        ProbeStep::on(ProbeOn::KeyPress, ProbeAct::SetGlobal(COUNTER, vr(2.0))),
        ProbeStep::on(
            ProbeOn::EndStep,
            ProbeAct::RequestRoom(RoomRequest::Goto(RoomId(1))),
        )
        .every(1000),
        ProbeStep::on(
            ProbeOn::EndStep,
            ProbeAct::RequestRoom(RoomRequest::Goto(RoomId(0))),
        )
        .every(1500),
    ];
    fx.probe(parent, 0.0, 0.0, s);
    fx
}

/// Moves by steps that are not exactly representable, so rounding is exercised every tick.
fn motion_fractions() -> Fixture {
    let mut b = builder();
    let mover = probe_kind(&mut b, "mover", true);
    let mut fx = Fixture::new(b.build().expect("golden def"), 7);
    for (i, (dx, dy)) in [(0.1, 0.2), (-0.3, 0.7), (1.0 / 3.0, -2.0 / 3.0), (0.0, 0.05)]
        .into_iter()
        .enumerate()
    {
        let mut s = ProbeState::new(format!("M{i}"), None);
        s.steps = vec![
            ProbeStep::on(ProbeOn::Step, ProbeAct::Move(r(dx), r(dy))),
            ProbeStep::on(ProbeOn::Step, ProbeAct::SetImageSpeed(r(0.25 * (i as f64 + 1.0)))),
        ];
        fx.probe(mover, 400.0 + 17.0 * i as f64, 300.0, s);
    }
    fx
}

/// Reads the keys directly and by events under a random input with sub-tick taps.
fn keys() -> Fixture {
    let mut b = builder();
    let reader = probe_kind(&mut b, "reader", false);
    let mut fx = Fixture::new(b.build().expect("golden def"), 3);
    let mut s = ProbeState::new("K", Some(VarId(0)));
    s.steps = vec![
        ProbeStep::on(ProbeOn::BeginStep, ProbeAct::DirectRead(Action::Jump.bit())),
        ProbeStep::on(ProbeOn::EndStep, ProbeAct::DirectRead(Action::Shoot.bit())).every(2),
        ProbeStep::on(ProbeOn::KeyPress, ProbeAct::SetGlobal(COUNTER, vr(1.0))),
        ProbeStep::on(ProbeOn::KeyRelease, ProbeAct::SetGlobal(COUNTER, vr(2.0))),
        ProbeStep::on(ProbeOn::KeyHeld, ProbeAct::SetAlarm(1, r(3.0))),
        ProbeStep::on(ProbeOn::Alarm(1), ProbeAct::SetGlobal(COUNTER, vr(3.0))),
    ];
    fx.probe(reader, 10.0, 10.0, s);
    fx
}

/// One probe reaches the non-finite path (docs/architecture/sim/numbers.md): the sim halts with
/// `NonFinite` and freezes, on every target.
fn non_finite() -> Fixture {
    let mut b = builder();
    let bad = probe_kind(&mut b, "bad", true);
    let mut fx = Fixture::new(b.build().expect("golden def"), 5);
    let mut s = ProbeState::new("X", None);
    s.steps = vec![
        ProbeStep::on(ProbeOn::Step, ProbeAct::Move(r(1.5), r(0.5))),
        ProbeStep::on(ProbeOn::Step, ProbeAct::Move(r(f64::NAN), r(0.0))).at(12),
    ];
    fx.probe(bad, 50.0, 50.0, s);
    fx
}

/// Scripted objects (WP9): a walker script with create, step, alarm and collide bounces between
/// two walls and spawns sparks (another script, destroyed by its alarm) from alarm draws of the
/// one RNG. The scripts are the files `scripted_walker.rhai` and `scripted_spark.rhai`; the
/// format crate's loader test loads the same text through a package.
fn scripted_object() -> Fixture {
    let mut b = builder();
    let sprite = b.sprite_rect((16, 16), (8, 8));
    let mut walker = KindDef::new("walker", Builtin::Block);
    walker.script = Some(ScriptId(0));
    walker.sprite = Some(sprite);
    let mut spark = KindDef::new("spark", Builtin::Block);
    spark.script = Some(ScriptId(1));
    let mut wall = KindDef::new("wall", Builtin::Block);
    wall.sprite = Some(sprite);
    wall.classes = ClassMask::package(0).expect("class");
    b.kind(walker);
    b.kind(spark);
    b.kind(wall);
    let mut def = b.build().expect("golden def");
    def.scripts.push(ScriptDef {
        name: "scripts/walker.rhai".into(),
        source: include_str!("scripted_walker.rhai").into(),
    });
    def.scripts.push(ScriptDef {
        name: "scripts/spark.rhai".into(),
        source: include_str!("scripted_spark.rhai").into(),
    });
    def.var_names.insert("hp".into(), VarId(1));
    let rules = RunnerRules::default();
    let scripts = ScriptRuntime::new(&def, &rules).expect("golden scripts compile");
    let mut events = scripts.event_set(ScriptId(0));
    events.collisions.push(ClassMask::package(0).expect("class"));
    def.kinds[0].events = events;
    def.kinds[1].events = scripts.event_set(ScriptId(1));
    let mut fx = Fixture::new(def, 9);
    fx.scripts = scripts;
    let mut c = fx.ctx();
    for (kind, x, y) in [
        (2, 300.0, 300.0),
        (2, 600.0, 300.0),
        (0, 150.0, 300.0),
        (0, 450.0, 300.0),
    ] {
        c.spawn(KindId(kind), r(x), r(y), SpawnInit::default())
            .expect("golden spawn");
    }
    fx
}
