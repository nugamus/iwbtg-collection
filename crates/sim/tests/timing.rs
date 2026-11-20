//! WP8b: timing triggers at tick row 16 (docs/plans/sim-core.md 2.1 row 16, 9.4).
//!
//! Hand-built worlds with probe kinds that change rooms; `run_end` is raised by hand, the way
//! `game_clear` raises it. Sound is stubbed (`sound::tick` is WP10's stub), nothing else is.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, GameDef, KindDef, RoomDef, RoomId};
use iwbtg_sim::events::{ProbeAct, ProbeOn, ProbeState, ProbeStep, probe_event_set};
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::num::Real;
use iwbtg_sim::room::RoomRequest;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::tick::{self, Hooks};
use iwbtg_sim::timing::{self, Split, TimingDef, TimingTrigger};
use iwbtg_sim::world::{Ctx, KindState, SpawnInit, World};

const HOOKS: Hooks = Hooks {
    sound_tick: |_| {},
    ..Hooks::ENGINE
};

struct Fx {
    def: GameDef,
    rules: RunnerRules,
    scripts: ScriptRuntime,
    w: World,
}

/// Four rooms. The probe goes to room 1 at tick 10, room 2 at tick 20, room 3 at tick 30 (the
/// `Goto` is requested at the end of the tick whose world tick is that value).
fn fx(timing: TimingDef, route: &[(u64, u32)]) -> Fx {
    let mut b = GameDefBuilder::new("wp8b");
    for i in 0..4 {
        b.room(RoomDef::new(format!("room_{i}"), 320, 240));
    }
    let mut k = KindDef::new("walker", Builtin::Probe);
    k.events = probe_event_set();
    k.persistent = true;
    let walker = b.kind(k);
    let mut def = b.build().expect("def");
    def.timing = timing;
    let mut fx = Fx {
        def,
        rules: RunnerRules::default(),
        scripts: ScriptRuntime::default(),
        w: World::new(RoomId(0), 1),
    };
    let mut s = ProbeState::new("W", None);
    for &(at, room) in route {
        s.steps.push(
            ProbeStep::on(
                ProbeOn::EndStep,
                ProbeAct::RequestRoom(RoomRequest::Goto(RoomId(room))),
            )
            .at(at),
        );
    }
    let mut c = fx.ctx();
    let init = SpawnInit {
        run_create: false,
        ..SpawnInit::default()
    };
    let id = c
        .spawn(walker, Real::new(0.0), Real::new(0.0), init)
        .expect("spawn");
    c.w.get_mut(id).expect("walker").state = KindState::Probe(s);
    fx
}

impl Fx {
    fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            def: &self.def,
            rules: &self.rules,
            w: &mut self.w,
            scripts: &self.scripts,
            dll_mode: DllMode::Stub,
        }
    }

    /// Runs `n` ticks and returns every split, in order.
    fn run(&mut self, n: u64) -> Vec<Split> {
        let mut all = Vec::new();
        for _ in 0..n {
            let mut c = self.ctx();
            all.extend(tick::run_tick_with(&mut c, InputFrame::default(), &HOOKS).expect("tick"));
        }
        all
    }
}

fn split(index: u16, tick: u64) -> Split {
    Split { index, tick }
}

fn enter(room: u32) -> TimingTrigger {
    TimingTrigger::EnterRoom(RoomId(room))
}

#[test]
fn run_start_fires_once_on_the_first_tick() {
    let mut f = fx(TimingDef::default(), &[]);
    assert_eq!(f.run(50), vec![split(0, 1)]);
}

#[test]
fn stages_fire_in_order_when_their_rooms_are_entered() {
    let t = TimingDef {
        start: TimingTrigger::RunStart,
        end: TimingTrigger::RunEnd,
        stages: vec![enter(1), enter(2)],
    };
    let mut f = fx(t, &[(10, 1), (20, 2), (30, 3)]);
    let got = f.run(40);
    // The request at world tick 10 ends that tick; the room is entered in it, and row 16 sees the
    // new room with tick 11.
    assert_eq!(got, vec![split(0, 1), split(1, 11), split(2, 21)]);
}

#[test]
fn a_stage_is_not_reached_out_of_order() {
    let t = TimingDef {
        start: TimingTrigger::RunStart,
        end: TimingTrigger::RunEnd,
        stages: vec![enter(1), enter(2)],
    };
    // Room 2 before room 1: stage 2 waits for stage 1, and fires when room 2 is looked at after
    // room 1 was entered (the level condition holds the tick room 1 is left for 2 only if the
    // kid is in 2 at that time).
    let mut f = fx(t, &[(10, 2), (20, 1), (30, 2)]);
    let got = f.run(40);
    assert_eq!(got, vec![split(0, 1), split(1, 21), split(2, 31)]);
}

#[test]
fn run_end_fires_once_and_then_nothing_fires() {
    let t = TimingDef {
        start: TimingTrigger::RunStart,
        end: TimingTrigger::RunEnd,
        stages: vec![enter(1)],
    };
    let mut f = fx(t, &[(10, 1)]);
    f.run(15);
    timing::raise_run_end(&mut f.w);
    assert_eq!(f.run(1), vec![split(2, 16)], "end after one stage has index 2");
    timing::raise_run_end(&mut f.w);
    assert_eq!(f.run(5), vec![], "the run is over");
}

#[test]
fn a_raise_only_counts_in_its_own_tick() {
    let mut f = fx(TimingDef::default(), &[]);
    // Raised, then a tick with row 16: the end fires in that tick, not a later one.
    f.run(3);
    timing::raise_run_end(&mut f.w);
    assert_eq!(f.run(1), vec![split(1, 4)]);
}

#[test]
fn nothing_fires_before_the_start_trigger() {
    let t = TimingDef {
        start: enter(1),
        end: TimingTrigger::RunEnd,
        stages: vec![],
    };
    let mut f = fx(t, &[(10, 1)]);
    f.run(5);
    timing::raise_run_end(&mut f.w);
    assert_eq!(f.run(1), vec![], "run_end before the start is dropped");
    assert_eq!(f.run(10), vec![split(0, 11)]);
    timing::raise_run_end(&mut f.w);
    assert_eq!(f.run(1), vec![split(1, 17)]);
}

#[test]
fn several_triggers_can_fire_in_one_tick() {
    let t = TimingDef {
        start: TimingTrigger::RunStart,
        end: TimingTrigger::RunEnd,
        stages: vec![enter(0), TimingTrigger::RunStart],
    };
    let mut f = fx(t, &[]);
    assert_eq!(f.run(1), vec![split(0, 1), split(1, 1), split(2, 1)]);
}

#[test]
fn timing_state_is_part_of_the_state_hash() {
    let mut a = fx(TimingDef::default(), &[]);
    let mut b = fx(TimingDef::default(), &[]);
    a.run(5);
    b.run(5);
    assert_eq!(
        iwbtg_sim::hash::state_hash(&a.w),
        iwbtg_sim::hash::state_hash(&b.w)
    );
    timing::raise_run_end(&mut b.w);
    assert_ne!(
        iwbtg_sim::hash::state_hash(&a.w),
        iwbtg_sim::hash::state_hash(&b.w)
    );
}
