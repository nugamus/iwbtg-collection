//! Views in the script API (#1531): the `view_*` reads and `set_view_*` writes over
//! `World::views`, read back on the next tick, with the camera follow running after them. Part
//! of the conformance module, so it runs on every CI target.

use iwbtg_sim::collide::Target;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, GameDef, KindDef, KindId, RoomDef, RoomId, ScriptDef, ScriptId, VarId};
use iwbtg_sim::events::EventKind;
use iwbtg_sim::hash::state_hash;
use iwbtg_sim::input::InputFrame;
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::validate::validate;
use iwbtg_sim::script::{ScriptHost, ScriptRuntime};
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::tick;
use iwbtg_sim::world::{Ctx, InstId, SpawnInit, Value, World};

const SCRIPT: &str = r#"
fn user(n) {
    let n = to_int(n);
    if n == 0 {
        set_view_xview(0, 12.0);
        set_view_yview(0, 34.0);
        set_view_wview(0, 400.0);
        set_view_hview(0, 300.0);
        set_view_angle(0, 5.0);
        set_view_enabled(0, true);
        set_view_visible(0, true);
        set_view_hborder(0, 8.0);
        set_view_vborder(0, 9.0);
        set_view_hspeed(0, 2.0);
        set_view_vspeed(0, 3.0);
        this.res = 1.0;
    }
    else if n == 1 { this.res = view_xview(0) + view_yview(0) + view_wview(0) + view_hview(0) + view_angle(0) + view_hborder(0) + view_vborder(0) + view_hspeed(0) + view_vspeed(0); }
    else if n == 2 { this.res = if view_enabled(0) { 1.0 } else { 0.0 }; }
    else if n == 3 { this.res = if view_visible(0) { 1.0 } else { 0.0 }; }
    else if n == 4 { set_view_object(0, "holder"); }
    else if n == 5 { this.name = view_object(0); }
    else if n == 6 { this.res = view_current(); }
    else if n == 7 { set_view_xview(8, 1.0); }
    else if n == 8 { set_view_object(0, ()); }
    else if n == 9 { this.res = if view_object(0) == () { 1.0 } else { 0.0 }; }
    else if n == 10 { set_view_xview(-1.0, 1.0); }
    else if n == 11 {
        set_view_xview(0, 0.0);
        set_view_yview(0, 0.0);
        set_view_wview(0, 50.0);
        set_view_hview(0, 50.0);
        set_view_hborder(0, 0.0);
        set_view_vborder(0, 0.0);
        set_view_hspeed(0, -1.0);
        set_view_vspeed(0, -1.0);
        set_view_enabled(0, true);
        set_view_visible(0, true);
        set_view_object(0, "holder");
    }
}
"#;

const RES: VarId = VarId(0);
const NAME: VarId = VarId(1);

struct Game {
    def: GameDef,
    rules: RunnerRules,
    scripts: ScriptRuntime,
    w: World,
    kind: KindId,
}

impl Game {
    fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            def: &self.def,
            rules: &self.rules,
            w: &mut self.w,
            scripts: &self.scripts,
            dll_mode: DllMode::Stub,
        }
    }

    fn holder(&mut self) -> InstId {
        let kind = self.kind;
        self.ctx()
            .spawn(kind, Real::new(100.0), Real::new(100.0), SpawnInit::default())
            .expect("spawn")
    }

    fn user(&mut self, me: InstId, n: u8) -> Result<(), String> {
        let mut ctx = self.ctx();
        let scripts = ctx.scripts;
        scripts
            .on_event(&mut ctx, me, &EventKind::User(n))
            .map(|_| ())
            .map_err(|e| e.message)
    }

    fn tick(&mut self) {
        let mut c = self.ctx();
        tick::run_tick(&mut c, InputFrame::default()).expect("tick");
    }

    fn var(&self, me: InstId, id: VarId) -> Option<&Value> {
        self.w.get(me).expect("holder").vars.get(&id)
    }

    fn res(&self, me: InstId) -> f64 {
        match self.var(me, RES) {
            Some(Value::Real(r)) => r.get(),
            other => panic!("no res: {other:?}"),
        }
    }

    fn view(&self, i: usize) -> &iwbtg_sim::view::View {
        &self.w.views[i]
    }
}

fn game() -> Game {
    let mut b = GameDefBuilder::new("views1531");
    let room = b.room(RoomDef::new("room", 320, 240));
    let kind = b.kind(KindDef::new("holder", Builtin::Block));
    b.start_room(room);
    let mut def = b.build().expect("def");
    def.scripts.push(ScriptDef {
        name: "scripts/views.rhai".into(),
        source: SCRIPT.into(),
    });
    def.kinds[kind.0 as usize].script = Some(ScriptId(0));
    def.var_names.insert("res".into(), RES);
    def.var_names.insert("name".into(), NAME);
    let issues = validate(&def);
    assert!(issues.is_empty(), "the validator accepts views: {issues:?}");
    let rules = RunnerRules::default();
    let scripts = ScriptRuntime::new(&def, &rules).expect("scripts compile");
    Game {
        def,
        rules,
        scripts,
        w: World::new(RoomId(0), 7),
        kind,
    }
}

fn approx(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

/// #1531: every write lands in the hashed view state and reads back after a tick.
#[test]
fn writes_are_read_back_on_the_next_tick() {
    let mut g = game();
    let me = g.holder();
    let before = state_hash(&g.w);
    g.user(me, 0).expect("write");
    g.tick();
    let v = g.view(0).clone();
    approx(v.x.get(), 12.0);
    approx(v.y.get(), 34.0);
    approx(v.w.get(), 400.0);
    approx(v.h.get(), 300.0);
    approx(v.angle.get(), 5.0);
    approx(v.hborder.get(), 8.0);
    approx(v.vborder.get(), 9.0);
    approx(v.hspeed.get(), 2.0);
    approx(v.vspeed.get(), 3.0);
    assert!(v.enabled && v.visible);
    assert_ne!(state_hash(&g.w), before, "the view is hashed state");
    g.user(me, 1).expect("read");
    approx(
        g.res(me),
        12.0 + 34.0 + 400.0 + 300.0 + 5.0 + 8.0 + 9.0 + 2.0 + 3.0,
    );
    g.user(me, 2).expect("enabled");
    approx(g.res(me), 1.0);
    g.user(me, 3).expect("visible");
    approx(g.res(me), 1.0);
    g.user(me, 6).expect("view_current");
    approx(g.res(me), 0.0);
}

/// #1531: `set_view_object` sets and clears the followed kind; the read returns its name.
#[test]
fn the_view_object_follows_and_clears() {
    let mut g = game();
    let me = g.holder();
    g.user(me, 4).expect("follow the holder");
    g.user(me, 5).expect("read the target");
    assert_eq!(g.var(me, NAME), Some(&Value::Str("holder".into())));
    let kind = g.kind;
    assert_eq!(g.view(0).follow, Some(Target::Kind(kind)));
    g.user(me, 8).expect("clear");
    g.user(me, 9).expect("read the cleared target");
    approx(g.res(me), 1.0);
    assert_eq!(g.view(0).follow, None);
}

/// #1531: the camera follow runs after the event that wrote the view, so a following view moves
/// on the same tick.
#[test]
fn the_follow_runs_after_the_write() {
    let mut g = game();
    let me = g.holder();
    g.user(me, 11).expect("a tight following view");
    g.tick();
    // The holder is at (100, 100); a 50x50 view at 0 with no border and a snapping speed lands
    // at 100 - 50 on both axes, inside the 320x240 room.
    approx(g.view(0).x.get(), 50.0);
    approx(g.view(0).y.get(), 50.0);
}

/// #1531: a view index outside 0-7 is a script error.
#[test]
fn an_out_of_range_view_index_is_an_error() {
    let mut g = game();
    let me = g.holder();
    let e = g.user(me, 7).expect_err("view 8");
    assert!(e.contains("not a view (0 to 7)"), "{e}");
    let e = g.user(me, 10).expect_err("negative index");
    assert!(e.contains("not a whole number (0 to 7)"), "{e}");
}

/// #1531: a literal kind passed to `set_view_object` is checked by the validator.
#[test]
fn the_validator_checks_a_literal_view_kind() {
    let mut g = game();
    g.def.scripts[0].source = "fn user(n) { set_view_object(0, \"nope\"); }".into();
    let issues = validate(&g.def);
    assert!(issues.iter().any(|i| i.message.contains("nope")), "{issues:?}");
}
