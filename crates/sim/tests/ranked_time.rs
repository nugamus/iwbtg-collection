//! #34: ranked time freezes in cutscene and dialogue regions; dialogue regions press `advance`
//! on a timeout (docs/format/v0.md, Stripped mode).
//!
//! Hand-built worlds as in `wp8b_timing`: a persistent probe walker that changes rooms, and probe
//! instances of region kinds. "Tick k" below is the tick that starts with `World::tick == k`;
//! `run_start` fires in tick 0 (split tick 1), so ticks 1 and on are the timer's.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, GameDef, KindDef, KindId, RoomDef, RoomId};
use iwbtg_sim::events::{ProbeAct, ProbeOn, ProbeState, ProbeStep, probe_event_set};
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::num::Real;
use iwbtg_sim::replay::{Header, MAGIC, REPLAY_FORMAT, Replay, StartMode, Trailer};
use iwbtg_sim::room::RoomRequest;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::stripped::{DialogueRegion, PlayMode, Region, RegionCondition, region_holds};
use iwbtg_sim::tick::{self, Hooks};
use iwbtg_sim::timing::{Split, TimingDef, TimingTrigger};
use iwbtg_sim::world::{Ctx, KindState, SpawnInit, World};

const HOOKS: Hooks = Hooks {
    sound_tick: |_| {},
    ..Hooks::ENGINE
};

const SHOOT: u32 = Action::Shoot.bit();

struct Fx {
    def: GameDef,
    rules: RunnerRules,
    scripts: ScriptRuntime,
    w: World,
    /// The region kind (`scene`): not persistent, so it stays in room 0.
    scene: KindId,
}

/// Four rooms, a walker on `route` (`(world tick, room)`: the `Goto` is requested in that
/// tick's End Step), and, when `scene_until` is given, one `scene` instance that destroys itself
/// in the End Step of that tick (`u64::MAX`: never).
fn fx(mode: PlayMode, timing: TimingDef, route: &[(u64, u32)], scene_until: Option<u64>) -> Fx {
    let mut b = GameDefBuilder::new("ranked-time");
    for i in 0..4 {
        b.room(RoomDef::new(format!("room_{i}"), 320, 240));
    }
    let mut k = KindDef::new("walker", Builtin::Probe);
    k.events = probe_event_set();
    k.persistent = true;
    let walker = b.kind(k);
    let mut k = KindDef::new("scene", Builtin::Probe);
    k.events = probe_event_set();
    let scene = b.kind(k);
    let mut def = b.build().expect("def");
    def.timing = timing;
    let mut fx = Fx {
        def,
        rules: RunnerRules::default(),
        scripts: ScriptRuntime::default(),
        w: World::new(RoomId(0), 1),
        scene,
    };
    fx.w.mode = mode;
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
    fx.spawn(walker, s);
    if let Some(until) = scene_until {
        let mut s = ProbeState::new("S", None);
        if until != u64::MAX {
            s.steps
                .push(ProbeStep::on(ProbeOn::EndStep, ProbeAct::DestroySelf).at(until));
        }
        fx.spawn(scene, s);
    }
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

    fn spawn(&mut self, kind: KindId, s: ProbeState) {
        let init = SpawnInit {
            run_create: false,
            ..SpawnInit::default()
        };
        let mut c = self.ctx();
        let id = c
            .spawn(kind, Real::new(0.0), Real::new(0.0), init)
            .expect("spawn");
        c.w.get_mut(id).expect("probe").state = KindState::Probe(s);
    }

    /// Runs one tick on `input`; returns its splits.
    fn tick(&mut self, input: InputFrame) -> Vec<Split> {
        let mut c = self.ctx();
        tick::run_tick_with(&mut c, input, &HOOKS).expect("tick")
    }

    /// Runs `n` ticks with no input; returns every split.
    fn run(&mut self, n: u64) -> Vec<Split> {
        (0..n).flat_map(|_| self.tick(InputFrame::EMPTY)).collect()
    }

    /// (ranked, frozen).
    fn time(&self) -> (u64, u64) {
        (self.w.timing.ranked_ticks, self.w.timing.frozen_ticks)
    }

    fn cutscene(&mut self, holds: RegionCondition) {
        self.def.stripped.cutscenes.push(Region {
            id: "cs".into(),
            holds,
        });
    }

    fn dialogue(&mut self, timeout_ticks: u32) {
        self.def.stripped.dialogues.push(DialogueRegion {
            region: Region {
                id: "talk".into(),
                holds: RegionCondition::Kind(self.scene),
            },
            advance: Action::Shoot,
            timeout_ticks,
        });
    }
}

fn press(bits: u32) -> InputFrame {
    InputFrame {
        held: bits,
        pressed: bits,
        released: 0,
    }
}

#[test]
fn a_kind_region_freezes_the_timer_while_an_instance_exists() {
    for (mode, want) in [(PlayMode::Stripped, (29, 10)), (PlayMode::Original, (39, 0))] {
        let mut f = fx(mode, TimingDef::default(), &[], Some(10));
        let scene = f.scene;
        f.cutscene(RegionCondition::Kind(scene));
        f.run(40);
        // Ticks 1-10 start with the scene alive (it goes in tick 10's End Step): frozen. Ticks
        // 11-39 are ranked. Play original ignores [stripped].
        assert_eq!(f.time(), want, "{mode:?}");
    }
}

#[test]
fn a_room_region_freezes_the_timer_and_ranked_plus_frozen_is_the_run() {
    let timing = TimingDef {
        start: TimingTrigger::RunStart,
        end: TimingTrigger::EnterRoom(RoomId(2)),
        stages: vec![],
    };
    let mut f = fx(PlayMode::Stripped, timing, &[(10, 1), (20, 0), (30, 2)], None);
    f.cutscene(RegionCondition::Room(RoomId(1)));
    let splits = f.run(40);
    let end = splits.last().expect("end split").tick;
    assert_eq!(splits.first().map(|s| s.tick), Some(1));
    assert_eq!(end, 31);
    // In room 1 at the start of ticks 11-20: frozen. Nothing counts after the end.
    assert_eq!(f.time(), (20, 10));
    assert_eq!(f.time().0 + f.time().1, end - 1, "end.tick - start.tick");
}

#[test]
fn region_holds_says_whether_the_next_tick_is_frozen() {
    // #573: the client's skip steps while `region_holds`; each of those ticks is frozen, and the
    // first tick after it is ranked.
    for mode in [PlayMode::Stripped, PlayMode::Original] {
        let mut f = fx(mode, TimingDef::default(), &[], Some(10));
        let scene = f.scene;
        f.cutscene(RegionCondition::Kind(scene));
        f.run(1);
        let mut skipped = 0;
        while region_holds(&f.w, &f.def) {
            f.tick(InputFrame::EMPTY);
            skipped += 1;
        }
        if mode == PlayMode::Stripped {
            // Ticks 1-10 start with the scene alive.
            assert_eq!((skipped, f.time()), (10, (0, 10)));
            f.tick(InputFrame::EMPTY);
            assert_eq!(f.time(), (1, 10), "the next tick is ranked");
        } else {
            assert_eq!(skipped, 0, "Play original has no regions");
        }
    }
}

#[test]
fn nothing_counts_before_the_timer_starts() {
    let timing = TimingDef {
        start: TimingTrigger::EnterRoom(RoomId(1)),
        end: TimingTrigger::RunEnd,
        stages: vec![],
    };
    let mut f = fx(PlayMode::Stripped, timing, &[(10, 1)], Some(u64::MAX));
    let scene = f.scene;
    f.cutscene(RegionCondition::Kind(scene));
    f.run(20);
    // Starts at split tick 11; the scene stayed in room 0, so ticks 11-19 are ranked.
    assert_eq!(f.time(), (9, 0));
}

/// The ticks (by `World::tick` at their start) whose frame had `advance` pressed.
fn presses(f: &mut Fx, inputs: &[InputFrame]) -> Vec<u64> {
    let mut out = Vec::new();
    for &input in inputs {
        let at = f.w.tick;
        f.tick(input);
        if f.w.input.frame.was_pressed(Action::Shoot) {
            out.push(at);
        }
    }
    out
}

#[test]
fn a_dialogue_advances_itself_at_exactly_timeout_ticks() {
    let mut f = fx(PlayMode::Stripped, TimingDef::default(), &[], Some(u64::MAX));
    f.dialogue(5);
    // The region holds from tick 0: its 5th tick is tick 4, then every 5th.
    assert_eq!(presses(&mut f, &[InputFrame::EMPTY; 20]), [4, 9, 14, 19]);
    let frame = f.w.input.frame;
    assert!(frame.is_held(Action::Shoot), "held and pressed");
    assert_eq!(f.time(), (0, 19), "a dialogue freezes the timer too");

    let mut f = fx(PlayMode::Original, TimingDef::default(), &[], Some(u64::MAX));
    f.dialogue(5);
    assert!(
        presses(&mut f, &[InputFrame::EMPTY; 20]).is_empty(),
        "Play original"
    );

    let mut f = fx(PlayMode::Stripped, TimingDef::default(), &[], Some(u64::MAX));
    f.dialogue(1);
    assert_eq!(presses(&mut f, &[InputFrame::EMPTY; 3]), [0, 1, 2], "timeout 1");
}

#[test]
fn a_real_press_resets_the_count_and_holding_does_not() {
    let mut f = fx(PlayMode::Stripped, TimingDef::default(), &[], Some(u64::MAX));
    f.dialogue(5);
    let mut inputs = [InputFrame::EMPTY; 20];
    inputs[3] = press(SHOOT);
    // Holding shoot after the press is no new press: the count runs on.
    for i in inputs.iter_mut().take(12).skip(4) {
        *i = InputFrame {
            held: SHOOT,
            pressed: 0,
            released: 0,
        };
    }
    // Tick 3 is the player's press; ticks 4-8 count to 5, so tick 8 is pressed; then 13, 18.
    assert_eq!(presses(&mut f, &inputs), [3, 8, 13, 18]);
}

#[test]
fn the_derived_press_is_press_only() {
    // #574: no text box in the 20 games advances on a release, so the tick after a derived
    // press runs exactly as the player's frame says: no `released` edge, nothing held.
    let mut f = fx(PlayMode::Stripped, TimingDef::default(), &[], Some(u64::MAX));
    f.dialogue(5);
    for tick in 0..12 {
        f.tick(InputFrame::EMPTY);
        let frame = f.w.input.frame;
        assert!(!frame.was_released(Action::Shoot), "tick {tick}");
        assert_eq!(
            frame.is_held(Action::Shoot),
            tick == 4 || tick == 9,
            "tick {tick}"
        );
    }
}

#[test]
fn a_dialogue_region_that_ends_resets_its_count() {
    // The scene goes in tick 2's End Step: ticks 0-2 count to 3, then it no longer holds.
    let mut f = fx(PlayMode::Stripped, TimingDef::default(), &[], Some(2));
    f.dialogue(5);
    assert!(presses(&mut f, &[InputFrame::EMPTY; 20]).is_empty());
    assert_eq!(f.w.timing.dialogue_waits, [0]);
}

#[test]
fn a_replay_never_stores_the_derived_press() {
    let make = || {
        let mut f = fx(PlayMode::Stripped, TimingDef::default(), &[], Some(30));
        f.dialogue(4);
        f
    };
    let mut live = make();
    let recorded = vec![InputFrame::EMPTY; 40];
    let derived = presses(&mut live, &recorded);
    assert_eq!(derived, [3, 7, 11, 15, 19, 23, 27], "the sim pressed shoot");
    let replay = Replay {
        header: Header {
            magic: MAGIC,
            replay_format: REPLAY_FORMAT,
            sim_version: iwbtg_sim::SIM_VERSION,
            package_id: "ranked-time".into(),
            gameplay_hash: [0; 32],
            engine_rev: 0,
            rng_seed: 1,
            start_mode: StartMode::NewGame,
            play_mode: PlayMode::Stripped,
            map: "main".into(),
            mods: iwbtg_sim::mods::ModSet::NONE,
            client_build_id: String::new(),
        },
        inputs: recorded,
        trailer: Trailer {
            total_ticks: 40,
            splits: vec![(0, 1)],
            ranked_ticks: live.w.timing.ranked_ticks,
            pauses: vec![],
            checkpoints: vec![],
            final_hash: iwbtg_sim::hash::state_hash(&live.w),
        },
    };
    let decoded = Replay::decode(&replay.encode().expect("encode")).expect("decode");
    assert!(
        decoded
            .inputs
            .iter()
            .all(|f| f.held & SHOOT == 0 && f.pressed & SHOOT == 0),
        "no stored frame has the derived press"
    );
    assert_eq!(decoded.trailer.ranked_ticks, 9, "ticks 31-39 after the dialogue");
    // Re-simulating the stored frames derives the same presses and the same world.
    let mut again = make();
    assert_eq!(presses(&mut again, &decoded.inputs), derived);
    assert_eq!(iwbtg_sim::hash::state_hash(&again.w), decoded.trailer.final_hash);
    assert_eq!(again.w.timing.ranked_ticks, decoded.trailer.ranked_ticks);
}
