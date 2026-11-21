//! Decision 0004: kid script slots and the documented kid field API. A profile runs a script
//! function at a fixed slot in a kid event list, `this` bound to the kid, and the fields of the
//! decision's table read and write `KidState`. The dash is decision 0004's user check.

use iwbtg_sim::def::physics::{
    BoundaryModel, KeyOn, KidAlarm, KidKey, KidScript, KidScriptSlot, ScriptSlotDo,
};
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, GameDef, KidFamily, KindDef, KindId, PhysicsProfile, RoomDef, ScriptDef, ScriptId,
    VarId,
};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::hash::state_hash;
use iwbtg_sim::input::{Action, InputFrame};
use iwbtg_sim::kid::actions::common::CommonAction;
use iwbtg_sim::kid::{self, KidAction, KidState};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::tick;
use iwbtg_sim::world::{Ctx, Flow, InstId, KindState, SpawnInit, World};

fn r(v: f64) -> Real {
    Real::new(v)
}

fn slot(function: &str) -> KidAction {
    KidAction::Script(KidScriptSlot {
        action: ScriptSlotDo::Script,
        function: function.into(),
    })
}

const DASH: &str = r#"
const DASH_SPEED = 12.0;
const DASH_TICKS = 12;

fn start_dash() {
    this.dash_dir = this.image_xscale;
    this.set_alarm(0, DASH_TICKS);
}

fn dash() {
    if this.get_alarm(0) > 0 {
        this.hspeed = DASH_SPEED * this.dash_dir;
        this.djump = false;
    }
}
"#;

/// A profile with the dash slot, a Step script slot after the native `walk`, a press-triggered
/// start slot and an alarm-0 row (so alarm 0 counts down).
fn dash_profile() -> PhysicsProfile {
    let mut p = iwbtg_sim::kid::presets::defaults(KidFamily::A1);
    p.events.create = vec![KidAction::Common(CommonAction::CreateDefaults)];
    p.events.step = vec![KidAction::Common(CommonAction::Walk), slot("dash")];
    p.events.keys = vec![KidKey {
        action: Action::Extra1,
        on: KeyOn::Press,
        run: vec![slot("start_dash")],
    }];
    p.events.alarms = vec![KidAlarm {
        n: 0,
        run: Vec::new(),
    }];
    p.events.collisions = Vec::new();
    p.events.boundary = BoundaryModel::None;
    p.script = KidScript {
        file: Some(ScriptId(0)),
        vars: vec!["dash_dir".into()],
    };
    p
}

struct Fx {
    def: GameDef,
    rules: RunnerRules,
    scripts: ScriptRuntime,
    w: World,
    kid: InstId,
    block: KindId,
}

/// A world with a kid kind, a block kind, `profile`, `source` as script 0 and the profile's vars
/// interned.
fn fx(profile: PhysicsProfile, source: &str, vars: &[&str], seed: u32) -> Fx {
    let mut b = GameDefBuilder::new("kid-slots");
    let idle = b.sprite_rect((11, 21), (5, 12));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(idle);
    k.classes = ClassMask::KID;
    let kid_kind = b.kind(k);
    let mut block = KindDef::new("block", Builtin::Block);
    block.sprite = Some(idle);
    block.classes = ClassMask::BLOCK;
    block.solid = true;
    let block_kind = b.kind(block);
    let room = b.room(RoomDef::new("r", 320, 240));
    b.start_room(room);
    b.physics(profile.clone());
    let mut def = b.build().expect("def");
    def.kinds[kid_kind.index().expect("index")].events = kid::event_set(&profile);
    def.scripts.push(ScriptDef {
        name: "scripts/kid.rhai".into(),
        source: source.into(),
    });
    for (i, name) in vars.iter().enumerate() {
        def.var_names
            .insert((*name).to_owned(), VarId(u32::try_from(i).expect("small")));
    }
    let rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("rules");
    let scripts = ScriptRuntime::new(&def, &rules).expect("scripts compile");
    let mut w = World::new(def.start_room, seed);
    let kid = {
        let mut ctx = Ctx {
            def: &def,
            rules: &rules,
            w: &mut w,
            scripts: &scripts,
            dll_mode: DllMode::Stub,
        };
        ctx.spawn(kid_kind, r(100.0), r(100.0), SpawnInit::default())
            .expect("kid spawns")
    };
    Fx {
        def,
        rules,
        scripts,
        w,
        kid,
        block: block_kind,
    }
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

    fn tick(&mut self, input: InputFrame) {
        let mut c = self.ctx();
        tick::run_tick(&mut c, input).expect("tick");
    }

    fn kid(&self) -> &KidState {
        match &self.w.get(self.kid).expect("kid").state {
            KindState::Kid(s) => s,
            _ => unreachable!("kid"),
        }
    }

    fn hspeed(&self) -> f64 {
        self.w.get(self.kid).expect("kid").m.hspeed().get()
    }
}

fn press(action: Action) -> InputFrame {
    InputFrame {
        held: 0,
        pressed: action.bit(),
        released: 0,
    }
}

#[test]
fn the_dash_runs_from_a_step_slot_and_a_press_slot() {
    let mut f = fx(dash_profile(), DASH, &["dash_dir"], 7);
    assert!(f.kid().djump, "Create refreshes the double jump");

    // The press slot arms the alarm; the Step slot (after native `walk`) carries the dash.
    f.tick(press(Action::Extra1));
    assert_eq!(f.hspeed(), 12.0, "the dash overrides walk's 0");
    assert!(!f.kid().djump, "the dash spends the double jump");

    // DASH_TICKS more Steps carry it (the alarm reads 12 on the press tick, so 11 follow).
    for _ in 0..11 {
        f.tick(InputFrame::default());
        assert_eq!(f.hspeed(), 12.0);
    }
    f.tick(InputFrame::default());
    assert!(
        f.hspeed().abs() < 1e-9,
        "walk runs again once the alarm is spent: {}",
        f.hspeed()
    );
}

#[test]
fn the_same_tick_sequence_hashes_the_same() {
    let run = |seed: u32| {
        let mut f = fx(dash_profile(), DASH, &["dash_dir"], seed);
        let mut hashes = Vec::new();
        f.tick(press(Action::Extra1));
        hashes.push(state_hash(&f.w));
        for _ in 0..11 {
            f.tick(InputFrame::default());
            hashes.push(state_hash(&f.w));
        }
        hashes
    };
    assert_eq!(run(7), run(7));
}

#[test]
fn a_collision_slot_receives_other_and_rows_run_in_order() {
    let source = r#"
fn touch(o) {
    this.touched_x = o.x;
    this.touched = true;
}
fn touch_again(o) {
    this.second = o.exists();
}
"#;
    let mut p = dash_profile();
    p.events.step = Vec::new();
    p.events.keys = Vec::new();
    p.events.alarms = Vec::new();
    p.events.collisions = vec![
        iwbtg_sim::def::physics::KidCollision {
            with: ClassMask::BLOCK,
            run: iwbtg_sim::kid::KidCollisionRun::Script(KidScriptSlot {
                action: ScriptSlotDo::Script,
                function: "touch".into(),
            }),
        },
        // A second row with the same `with`: both run, in row order.
        iwbtg_sim::def::physics::KidCollision {
            with: ClassMask::BLOCK,
            run: iwbtg_sim::kid::KidCollisionRun::Script(KidScriptSlot {
                action: ScriptSlotDo::Script,
                function: "touch_again".into(),
            }),
        },
    ];
    p.script = KidScript {
        file: Some(ScriptId(0)),
        vars: vec!["touched_x".into(), "touched".into(), "second".into()],
    };
    let mut f = fx(p, source, &["touched_x", "touched", "second"], 1);
    let (block, kid) = (f.block, f.kid);
    let other = {
        let mut c = f.ctx();
        c.spawn(block, r(140.0), r(100.0), SpawnInit::default())
            .expect("block")
    };
    let flow = {
        let mut c = f.ctx();
        kid::on_event(
            &mut c,
            kid,
            &EventKind::Collision {
                with: ClassMask::BLOCK,
                other,
            },
        )
        .expect("collision")
    };
    assert_eq!(flow, Flow::Continue);
    let var = |name: &str| {
        let id = f.def.var_names[name];
        f.w.get(kid).expect("kid").vars.get(&id).cloned()
    };
    assert_eq!(var("touched_x"), Some(iwbtg_sim::world::Value::Real(r(140.0))));
    assert_eq!(var("touched"), Some(iwbtg_sim::world::Value::Real(Real::ONE)));
    assert_eq!(var("second"), Some(iwbtg_sim::world::Value::Real(Real::ONE)));
}

#[test]
fn the_documented_fields_read_and_write_kid_state() {
    let source = r#"
fn write() {
    this.djump = false;
    this.tjump = true;
    this.onPlatform = true;
    this.ladder = true;
    this.frozen = true;
    this.frozen2 = true;
    this.muteki = true;
    this.curJumps = 3;
    this.tempHspeed = 1.5;
    this.maxSpeed = 4.0;
    this.maxVspeed = 5.0;
    this.maxFallSpeed = 6.0;
    this.scale = 2.0;
}

fn snapshot() {
    this.seen_cur = this.curJumps;
    this.seen_max = this.maxSpeed;
    this.seen_yflag = this.yflag;
    this.seen_facing = this.facing;
    this.seen_walk = this.walk_h;
    this.seen_jump = this.jump;
    this.seen_jump2 = this.jump2;
}

fn facing_sets_image_xscale() {
    this.image_xscale = -1.0;
}
"#;
    let vars = [
        "seen_cur",
        "seen_max",
        "seen_yflag",
        "seen_facing",
        "seen_walk",
        "seen_jump",
        "seen_jump2",
    ];
    let mut f = fx(dash_profile(), source, &vars, 3);
    let sid = ScriptId(0);
    let kid = f.kid;
    {
        let mut c = f.ctx();
        c.scripts
            .call_slot(&mut c, sid, kid, "write", None)
            .expect("write slot");
    }
    let s = f.kid();
    assert!(!s.djump && s.tjump && s.on_platform && s.ladder);
    assert!(s.frozen && s.frozen2 && s.invincible);
    assert_eq!(s.cur_jumps, 3);
    assert_eq!(s.temp_hspeed, r(1.5));
    assert_eq!(s.run_speed, r(4.0));
    assert_eq!(s.max_vspeed, r(5.0));
    assert_eq!(s.max_fall_speed, r(6.0));
    assert_eq!(s.scale, r(2.0));

    {
        let mut c = f.ctx();
        c.scripts
            .call_slot(&mut c, sid, kid, "snapshot", None)
            .expect("snapshot slot");
    }
    let var = |name: &str| {
        let id = f.def.var_names[name];
        f.w.get(f.kid).expect("kid").vars.get(&id).cloned()
    };
    assert_eq!(var("seen_cur"), Some(iwbtg_sim::world::Value::Real(r(3.0))));
    assert_eq!(var("seen_max"), Some(iwbtg_sim::world::Value::Real(r(4.0))));
    assert_eq!(var("seen_yflag"), Some(iwbtg_sim::world::Value::Real(r(1.0))));
    assert_eq!(var("seen_facing"), Some(iwbtg_sim::world::Value::Real(r(1.0))));
    assert_eq!(var("seen_walk"), Some(iwbtg_sim::world::Value::Real(Real::ZERO)));
    assert_eq!(var("seen_jump"), Some(iwbtg_sim::world::Value::Real(r(8.5))));

    {
        let mut c = f.ctx();
        c.scripts
            .call_slot(&mut c, sid, kid, "facing_sets_image_xscale", None)
            .expect("facing slot");
    }
    assert_eq!(f.kid().facing, -1, "image_xscale stays the one facing setter");
    assert_eq!(f.w.get(f.kid).expect("kid").img.xscale(), r(-1.0));
}

#[test]
fn read_only_fields_and_bad_counters_are_script_errors() {
    let source = r#"
fn write_yflag() { this.yflag = -1.0; }
fn bad_count() { this.curJumps = 1.5; }
fn out_of_range() { this.curJumps = 300; }
"#;
    let mut f = fx(dash_profile(), source, &[], 1);
    let kid = f.kid;
    for name in ["write_yflag", "bad_count", "out_of_range"] {
        let mut c = f.ctx();
        let err = c
            .scripts
            .call_slot(&mut c, ScriptId(0), kid, name, None)
            .expect_err("a script error");
        assert!(err.to_string().contains("script"), "{name}: {err}");
    }
}

#[test]
fn a_room_request_ends_the_rest_of_the_list() {
    let source = r#"
fn leave_room() { room_goto("r2"); }
"#;
    let mut p = dash_profile();
    p.events.create = vec![KidAction::Common(CommonAction::CreateDefaults)];
    p.events.step = vec![slot("leave_room"), KidAction::Common(CommonAction::Walk)];
    p.events.keys = Vec::new();
    p.events.alarms = Vec::new();
    p.script = KidScript {
        file: Some(ScriptId(0)),
        vars: Vec::new(),
    };

    let mut b = GameDefBuilder::new("kid-slot-room");
    let idle = b.sprite_rect((11, 21), (5, 12));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.sprite = Some(idle);
    k.classes = ClassMask::KID;
    let kid_kind = b.kind(k);
    let r1 = b.room(RoomDef::new("r1", 320, 240));
    let _r2 = b.room(RoomDef::new("r2", 320, 240));
    b.start_room(r1);
    b.physics(p.clone());
    let mut def = b.build().expect("def");
    def.kinds[kid_kind.index().expect("index")].events = kid::event_set(&p);
    def.scripts.push(ScriptDef {
        name: "scripts/kid.rhai".into(),
        source: source.into(),
    });
    let rules = RunnerRules::for_runner(def.runner, def.engine_rev).expect("rules");
    let scripts = ScriptRuntime::new(&def, &rules).expect("scripts compile");
    let mut w = World::new(def.start_room, 1);
    let mut c = Ctx {
        def: &def,
        rules: &rules,
        w: &mut w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    let kid = c
        .spawn(kid_kind, r(100.0), r(100.0), SpawnInit::default())
        .expect("kid");
    c.w.input.latch(InputFrame {
        held: Action::Right.bit(),
        pressed: 0,
        released: 0,
    });
    let flow = kid::on_event(&mut c, kid, &EventKind::Step).expect("step");
    assert_eq!(flow, Flow::AbortTick, "the slot's room request stops the list");
    assert_eq!(c.w.room_requests, 1);
    assert_eq!(
        c.w.get(kid).expect("kid").m.hspeed(),
        Real::ZERO,
        "walk after the slot did not run"
    );
}

#[test]
fn a_slot_function_the_script_lacks_is_an_error() {
    let mut f = fx(dash_profile(), "fn other() { }", &[], 1);
    let kid = f.kid;
    let mut c = f.ctx();
    let err = c
        .scripts
        .call_slot(&mut c, ScriptId(0), kid, "dash", None)
        .expect_err("missing function");
    assert!(err.to_string().contains("no function `dash`"), "{err}");
}
