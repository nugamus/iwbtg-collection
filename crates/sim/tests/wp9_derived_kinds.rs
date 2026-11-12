//! #694: scripts and the kinds the loader derives from a kind (`[defaults.<kind>]` variants and
//! placed instances whose params pick other classes, #659). `instances(kind)` covers them, and
//! `instance_create` with class-picking params takes the derived kind or fails. #701: with params
//! that match a `[defaults.<kind>]` variant it takes the variant's kind, as a placed instance does.

use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, ClassMask, GameDef, KindDef, KindId, KindVariant, ParamValue, Params, RoomDef, RoomId,
    ScriptDef, ScriptId, VarId,
};
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{Ctx, InstId, SimStatus, SpawnInit, Value, World};

struct Fx {
    def: GameDef,
    rules: RunnerRules,
    scripts: ScriptRuntime,
    w: World,
}

fn variant(v: f64) -> Params {
    let mut p = Params::default();
    p.0.insert("variant".into(), ParamValue::Real(Real::new(v)));
    p
}

fn down() -> ParamValue {
    ParamValue::Str("down".into())
}

/// Kinds as the loader makes them: 0 `runner` (script), 1 `water`, 2 `water[variant=2]` (derived
/// for a placed instance), 3 `waterfall` (another kind whose name starts with `water`), 4 `spike`
/// and 5 `spike[dir=down]`, its `[defaults.spike]` variant for `dir = "down"`.
fn fx(source: &str) -> Fx {
    let mut b = GameDefBuilder::new("derived");
    let room = b.room(RoomDef::new("room", 320, 240));
    let mut runner = KindDef::new("runner", Builtin::Block);
    runner.script = Some(ScriptId(0));
    b.kind(runner);
    let mut water = KindDef::new("water", Builtin::Water);
    water.classes = ClassMask::WATER1;
    b.kind(water);
    let mut water2 = KindDef::new("water[variant=2]", Builtin::Water);
    water2.classes = ClassMask::WATER2;
    water2.params = variant(2.0);
    b.kind(water2);
    let mut waterfall = KindDef::new("waterfall", Builtin::Water);
    waterfall.classes = ClassMask::WATER1;
    b.kind(waterfall);
    let mut spike = KindDef::new("spike", Builtin::Killer);
    spike.classes = ClassMask::KILLER;
    b.kind(spike);
    let mut spike_down = KindDef::new("spike[dir=down]", Builtin::Killer);
    spike_down.classes = ClassMask::KILLER;
    spike_down.params.0.insert("dir".into(), down());
    b.kind(spike_down);
    b.start_room(room);
    let mut def = b.build().expect("def");
    def.variants.push(KindVariant {
        kind: KindId(4),
        when: vec![("dir".into(), down())],
        variant: KindId(5),
    });
    def.scripts.push(ScriptDef {
        name: "scripts/runner.rhai".into(),
        source: source.into(),
    });
    def.var_names.insert("n".into(), VarId(0));
    let rules = RunnerRules::default();
    let scripts = ScriptRuntime::new(&def, &rules).expect("scripts compile");
    def.kinds[0].events = scripts.event_set(ScriptId(0));
    Fx {
        def,
        rules,
        scripts,
        w: World::new(RoomId(0), 42),
    }
}

impl Fx {
    fn spawn(&mut self, kind: u32) -> InstId {
        let mut c = Ctx {
            def: &self.def,
            rules: &self.rules,
            w: &mut self.w,
            scripts: &self.scripts,
            dll_mode: DllMode::Stub,
        };
        c.spawn(KindId(kind), Real::new(0.0), Real::new(0.0), SpawnInit::default())
            .expect("spawn")
    }

    fn halted(&self) -> Option<String> {
        match &self.w.status {
            SimStatus::Halted(e) => Some(e.to_string()),
            _ => None,
        }
    }

    fn n(&self, id: InstId) -> f64 {
        match self.w.get(id).expect("inst").vars.get(&VarId(0)) {
            Some(Value::Real(r)) => r.get(),
            other => panic!("n is {other:?}"),
        }
    }

    fn kinds_of(&self, classes: ClassMask) -> Vec<u32> {
        self.w
            .iter_alive()
            .filter(|i| i.classes.intersects(classes))
            .map(|i| i.kind.0)
            .collect()
    }
}

#[test]
fn instances_of_a_kind_cover_the_kinds_derived_from_it() {
    let src = r#"fn create() { this.n = to_float(instances("water").len() * 10 + instances("water[variant=2]").len()); }"#;
    let mut f = fx(src);
    f.spawn(1);
    f.spawn(2);
    f.spawn(2);
    f.spawn(3);
    let r = f.spawn(0);
    assert_eq!(f.halted(), None);
    // `water` covers itself and `water[variant=2]`, not `waterfall`; the derived kind only itself.
    assert_eq!(f.n(r), 32.0);
}

#[test]
fn a_spawn_whose_params_pick_a_class_takes_the_derived_kind() {
    let src = r#"fn create() { instance_create("water", 0, 0, #{ variant: 2 }); instance_create("water", 0, 0, #{ variant: 1 }); instance_create("water", 0, 0); }"#;
    let mut f = fx(src);
    f.spawn(0);
    assert_eq!(f.halted(), None);
    assert_eq!(
        f.kinds_of(ClassMask::WATER2),
        vec![2],
        "variant 2 is water[variant=2]"
    );
    // Variant 1 picks the kind's own class, so it keeps the kind, as the loader does.
    assert_eq!(f.kinds_of(ClassMask::WATER1), vec![1, 1]);
}

#[test]
fn a_spawn_whose_params_pick_a_class_no_kind_has_is_an_error() {
    let src = r#"fn create() { instance_create("water", 0, 0, #{ variant: 3 }); }"#;
    let mut f = fx(src);
    let mut c = Ctx {
        def: &f.def,
        rules: &f.rules,
        w: &mut f.w,
        scripts: &f.scripts,
        dll_mode: DllMode::Stub,
    };
    let _ = c.spawn(KindId(0), Real::new(0.0), Real::new(0.0), SpawnInit::default());
    let e = f.halted().expect("halted");
    assert!(e.contains("`water` with variant = 3 picks other classes than the kind's, and no kind derived from `water` has them"), "{e}");
    assert!(f.kinds_of(ClassMask::WATER3).is_empty());
}

#[test]
fn a_spawn_whose_params_match_a_defaults_variant_takes_the_variant_kind() {
    let src = r#"fn create() { instance_create("spike", 0, 0, #{ dir: "down" }); instance_create("spike", 0, 0, #{ dir: "up" }); instance_create("spike", 0, 0); }"#;
    let mut f = fx(src);
    f.spawn(0);
    assert_eq!(f.halted(), None);
    // `dir: "down"` matches the variant, so it gets `spike[dir=down]` (its sprite and mask); the
    // others keep `spike`, as placed instances do.
    assert_eq!(f.kinds_of(ClassMask::KILLER), vec![5, 4, 4]);
}
