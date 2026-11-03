//! WP9.2 (#342): the script host and the instance API. Scripted kinds on a hand-built world:
//! Create, Step, Alarm and Collision handlers, custom variables, spawn and destroy, errors with
//! a position, the nesting and budget limits, and determinism.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, GameDef, KindDef, KindId, RoomDef, RoomId, ScriptDef, ScriptId, VarId,
};
use iwbtg_sim::events::EventSet;
use iwbtg_sim::hash::state_hash;
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::num::Real;
use iwbtg_sim::output::OutputEvent;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::{ScriptOutput, ScriptRuntime};
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::tick;
use iwbtg_sim::world::{Ctx, InstId, SimStatus, SpawnInit, World};

struct Fx {
    def: GameDef,
    rules: RunnerRules,
    scripts: ScriptRuntime,
    w: World,
}

/// Kinds: 0 `walker` (script), 1 `target` (script, class package 0), 2 `plain`.
fn fx(source: &str, walker_events: bool) -> Fx {
    let mut b = GameDefBuilder::new("wp9");
    let room = b.room(RoomDef::new("room", 320, 240));
    let spr = b.sprite_rect((16, 16), (0, 0));
    let mut walker = KindDef::new("walker", Builtin::Block);
    walker.script = Some(ScriptId(0));
    walker.sprite = Some(spr);
    let mut target = KindDef::new("target", Builtin::Block);
    target.script = Some(ScriptId(0));
    target.sprite = Some(spr);
    target.classes = ClassMask::package(0).expect("class");
    let plain = KindDef::new("plain", Builtin::Block);
    b.kind(walker);
    b.kind(target);
    b.kind(plain);
    b.start_room(room);
    let mut def = b.build().expect("def");
    def.scripts.push(ScriptDef {
        name: "scripts/walker.rhai".into(),
        source: source.into(),
    });
    def.var_names.insert("hp".into(), VarId(0));
    let rules = RunnerRules::default();
    let scripts = ScriptRuntime::new(&def, &rules).expect("scripts compile");
    if walker_events {
        let mut set: EventSet = scripts.event_set(ScriptId(0));
        set.collisions.push(ClassMask::package(0).expect("class"));
        def.kinds[0].events = set.clone();
        def.kinds[1].events = scripts.event_set(ScriptId(0));
    }
    Fx {
        def,
        rules,
        scripts,
        w: World::new(RoomId(0), 42),
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

    fn spawn(&mut self, kind: u32, x: f64, y: f64) -> InstId {
        let mut c = self.ctx();
        c.spawn(KindId(kind), Real::new(x), Real::new(y), SpawnInit::default())
            .expect("spawn")
    }

    fn run(&mut self, n: u32) {
        for _ in 0..n {
            let mut c = self.ctx();
            tick::run_tick(&mut c, InputFrame::default()).expect("tick");
        }
    }

    fn x(&self, id: InstId) -> f64 {
        self.w.get(id).expect("inst").m.x().get()
    }

    fn halted(&self) -> Option<String> {
        match &self.w.status {
            SimStatus::Halted(e) => Some(e.to_string()),
            _ => None,
        }
    }
}

const WALKER: &str = r#"
fn create() {
    this.hp = 3.0;
    this.set_alarm(0, 5);
}
fn step() {
    this.x += 1.0;
}
fn alarm(n) {
    if n == 0 {
        this.hp = this.hp + 10;
        this.y = 100.0;
    }
}
fn collide(other) {
    other.x = 200.0;
    emit("hit");
}
"#;

fn run_walker() -> (f64, f64, [u8; 32]) {
    let mut f = fx(WALKER, true);
    let a = f.spawn(0, 10.0, 0.0);
    let b = f.spawn(1, 14.0, 0.0);
    f.run(8);
    assert_eq!(f.halted(), None);
    let hp = match f.w.get(a).expect("a").vars.get(&VarId(0)) {
        Some(iwbtg_sim::world::Value::Real(r)) => r.get(),
        other => panic!("hp is {other:?}"),
    };
    assert!(f.w.output.iter().any(|e| matches!(
        e,
        OutputEvent::Script(ScriptOutput::Emit { name }) if name == "hit"
    )));
    (f.x(b), hp, state_hash(&f.w))
}

#[test]
fn create_step_alarm_and_collide_run_and_are_deterministic() {
    let (bx, hp, h1) = run_walker();
    // Collide moved `target` away on the first tick; its own step then added 1 per tick.
    assert!(bx >= 200.0, "target x = {bx}");
    assert_eq!(hp, 13.0, "Create set 3, alarm[0] added 10");
    let (_, _, h2) = run_walker();
    assert_eq!(h1, h2, "two runs hash the same");
}

#[test]
fn spawn_destroy_instances_and_exists() {
    let src = r#"
fn create() {
    let p = instance_create("plain", 5, 6, #{ });
    this.hp = p;
    let all = instances("plain");
    if all.len() != 1 { throw "instances"; }
    if !exists(p) { throw "exists"; }
    destroy(p);
    if exists(p) { throw "still exists"; }
}
fn step() { if this.kind != "walker" { throw "kind"; } }
"#;
    let mut f = fx(src, true);
    let a = f.spawn(0, 0.0, 0.0);
    assert_eq!(f.halted(), None);
    f.run(2);
    assert_eq!(f.halted(), None);
    assert!(f.w.exists(a));
}

#[test]
fn a_script_error_halts_with_file_line_and_column() {
    let src = "fn step() {\n    let a = 1.0;\n    this.hp2 = 3.0;\n}\n";
    let mut f = fx(src, true);
    f.spawn(0, 0.0, 0.0);
    f.run(1);
    let e = f.halted().expect("halted");
    assert!(e.contains("scripts/walker.rhai"), "{e}");
    assert!(e.contains("line 3"), "{e}");
    assert!(e.contains("walker"), "{e}");
    assert!(e.contains("tick"), "{e}");
}

#[test]
fn a_destroyed_handle_is_an_error_not_a_crash() {
    let src = "fn create() { let p = instance_create(\"plain\", 0, 0); destroy(p); p.x = 1.0; }";
    let mut f = fx(src, true);
    let mut c = f.ctx();
    let r = c.spawn(KindId(0), Real::new(0.0), Real::new(0.0), SpawnInit::default());
    assert!(r.is_err(), "Create failed, so the spawn fails");
    let e = f.halted().expect("halted");
    assert!(e.contains("no longer exists"), "{e}");
}

#[test]
fn a_runaway_loop_halts_the_sim() {
    let src = "fn step() { let i = 0; while true { i += 1; } }";
    let mut f = fx(src, true);
    f.spawn(0, 0.0, 0.0);
    f.run(1);
    let e = f.halted().expect("halted");
    assert!(e.contains("budget"), "{e}");
}

#[test]
fn spawn_chains_stop_at_the_nesting_limit() {
    let src = "fn create() { instance_create(\"walker\", 0, 0); }";
    let mut f = fx(src, true);
    let mut c = f.ctx();
    let r = c.spawn(KindId(0), Real::new(0.0), Real::new(0.0), SpawnInit::default());
    assert!(r.is_err() || c.w.status != SimStatus::Running);
    let e = f.halted().expect("halted");
    assert!(e.contains("nest"), "{e}");
}

#[test]
fn the_rng_is_the_worlds() {
    let src = "fn create() { this.hp = random(1000.0); }";
    let mut f = fx(src, true);
    let a = f.spawn(0, 0.0, 0.0);
    let hp = match f.w.get(a).expect("a").vars.get(&VarId(0)) {
        Some(iwbtg_sim::world::Value::Real(r)) => r.get(),
        other => panic!("hp is {other:?}"),
    };
    let mut g = World::new(RoomId(0), 42);
    let expect = g.rng.random(Real::new(1000.0)).get();
    assert_eq!(hp, expect);
}

#[test]
fn instance_create_and_instances_charge_the_budget() {
    let src = "fn create() { let p = instance_create(\"plain\", 5, 6); let all = instances(\"plain\"); this.hp = to_float(all.len()); }";
    let mut f = fx(src, true);
    f.scripts.begin_tick();
    f.spawn(0, 0.0, 0.0);
    let used = f.scripts.engine().expect("engine").budget().used();
    // 50 for the spawn, 1 for the one instance returned, and a few operations.
    assert!((51..=90).contains(&used), "used {used}");
}

#[test]
fn a_call_may_create_256_instances_and_not_257() {
    let ok = "fn create() { for i in 0..256 { instance_create(\"plain\", 0, 0); } }";
    let mut f = fx(ok, true);
    f.spawn(0, 0.0, 0.0);
    assert_eq!(f.halted(), None);

    let over = "fn create() {\n    for i in 0..257 {\n        instance_create(\"plain\", 0, 0);\n    }\n}\n";
    let mut f = fx(over, true);
    let e = f
        .ctx()
        .spawn(KindId(0), Real::new(0.0), Real::new(0.0), SpawnInit::default())
        .expect_err("the cap stops Create")
        .to_string();
    assert!(
        e.contains("spawn cap exceeded") && e.contains("at most 256 instances"),
        "{e}"
    );
    assert!(
        e.contains("scripts/walker.rhai") && e.contains("line 3") && e.contains("tick 0"),
        "{e}"
    );
}

#[test]
fn the_scripts_may_create_4096_instances_per_tick_and_not_more() {
    let src = "fn step() { for i in 0..256 { instance_create(\"plain\", 0, 0); } }";
    // 16 calls of 256 are exactly 4,096.
    let mut f = fx(src, true);
    for _ in 0..16 {
        f.spawn(0, 0.0, 0.0);
    }
    f.run(1);
    assert_eq!(f.halted(), None);

    let mut f = fx(src, true);
    for _ in 0..17 {
        f.spawn(0, 0.0, 0.0);
    }
    f.run(1);
    let e = f.halted().expect("halted");
    assert!(
        e.contains("at most 4096 instances per tick") && e.contains("tick 0"),
        "{e}"
    );
}

#[test]
fn a_room_may_not_grow_past_50000_live_instances() {
    // 16 calls of 256 are 4,096 a tick, the most the scripts may spawn; 13 ticks pass 50,000.
    let src = "fn step() { for i in 0..256 { instance_create(\"plain\", 0, 0); } }";
    let mut f = fx(src, true);
    for _ in 0..16 {
        f.spawn(0, 0.0, 0.0);
    }
    f.run(12);
    assert_eq!(f.halted(), None, "12 ticks hold 49,152 + 16 instances");
    f.run(1);
    let e = f.halted().expect("halted");
    assert!(
        e.contains("instance cap exceeded") && e.contains("50000 live instances"),
        "{e}"
    );
}
