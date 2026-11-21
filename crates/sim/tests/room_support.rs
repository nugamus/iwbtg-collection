//! Shared fixture for the room tests: a recording [`Probe`] host standing in for the event layer
//! (every kind is a probe kind, every event logs `<Event> <kind name>`), and the small games the
//! groups build on it.

use iwbtg_sim::def::NameId;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{
    Builtin, EngineKind, GameDef, GlobalIndex, KindDef, KindId, ParamValue, Params, RoomAction, RoomDef,
    RoomId, RoomWhen, VarId,
};
use iwbtg_sim::error::SimError;
use iwbtg_sim::events::EventKind;
use iwbtg_sim::num::Real;
use iwbtg_sim::room::{RoomHost, RoomRequest, StartPoint, perform_change_with, start_game_with};
use iwbtg_sim::rules::{RunnerRules, TickRules};
use iwbtg_sim::save::SaveSlot;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::stripped::PlayMode;
use iwbtg_sim::world::{Ctx, Flow, InstId, Instance, SimStatus, SpawnInit, Value, World};

pub(crate) type React = Box<dyn FnMut(&mut Probe, &mut Ctx, InstId, &str, &EventKind) -> Flow>;

/// The recording host.
#[derive(Default)]
pub(crate) struct Probe {
    pub(crate) log: Vec<String>,
    /// When set, events are logged with `+g` while this global is set.
    pub(crate) watch: Option<VarId>,
    pub(crate) react: Option<React>,
    /// Kind name and params of every spawn that had params.
    pub(crate) params: Vec<(String, Params)>,
}

pub(crate) fn event_name(ev: &EventKind) -> &'static str {
    match ev {
        EventKind::Create => "Create",
        EventKind::RoomStart => "RoomStart",
        EventKind::RoomEnd => "RoomEnd",
        EventKind::GameStart => "GameStart",
        _ => "Other",
    }
}

impl RoomHost for Probe {
    fn dispatch_order(
        &mut self,
        w: &World,
        _rules: &TickRules,
        filter: &dyn Fn(&Instance) -> bool,
    ) -> Vec<InstId> {
        let mut v: Vec<(u16, InstId)> = w
            .instances()
            .iter()
            .filter(|i| filter(i))
            .map(|i| (i.rank, i.id))
            .collect();
        v.sort();
        v.into_iter().map(|(_, id)| id).collect()
    }

    fn dispatch(&mut self, ctx: &mut Ctx, me: InstId, ev: &EventKind) -> Flow {
        let kind = ctx
            .w
            .get(me)
            .map(|i| i.kind)
            .expect("dispatch to a stored instance");
        let name = ctx.def.kind(kind).expect("known kind").name.clone();
        let g = match self.watch {
            Some(v) if ctx.w.globals.contains_key(&v) => "+g",
            _ => "",
        };
        self.log.push(format!("{} {name}{g}", event_name(ev)));
        match self.react.take() {
            Some(mut react) => {
                let flow = react(self, ctx, me, &name, ev);
                self.react = Some(react);
                flow
            }
            None => Flow::Continue,
        }
    }

    fn spawn(
        &mut self,
        ctx: &mut Ctx,
        kind: KindId,
        x: Real,
        y: Real,
        init: SpawnInit,
    ) -> Result<InstId, SimError> {
        let id = ctx.w.alloc_id()?;
        let k = ctx.def.kind(kind).expect("known kind");
        let rank = init
            .rank
            .or_else(|| ctx.def.rank_for(kind, init.variant.as_ref()))
            .expect("ranked kind");
        if !init.params.0.is_empty() {
            self.params.push((k.name.clone(), init.params.clone()));
        }
        let mut inst = Instance::new(id, kind, rank, x, y);
        inst.persistent = k.persistent;
        inst.depth = k.depth;
        inst.classes = k.classes;
        inst.name = init.name;
        inst.group = init.group;
        ctx.w.insert(inst)?;
        if init.run_create && self.dispatch(ctx, id, &EventKind::Create) == Flow::Halt {
            return Err(match &ctx.w.status {
                SimStatus::Halted(e) => e.clone(),
                _ => SimError::invariant("halt without status"),
            });
        }
        Ok(id)
    }

    fn load_save(&mut self, ctx: &mut Ctx, slot: &SaveSlot) -> Result<InstId, SimError> {
        self.log.push("Load".into());
        let kid = ctx.def.engine_kind(EngineKind::Kid).expect("kid kind");
        let id = self.spawn(ctx, kid, slot.x, slot.y, SpawnInit::default())?;
        // A load may also request the saved room, as saveExe does.
        ctx.w.pending = Some(RoomRequest::Goto(slot.room));
        Ok(id)
    }
}

/// A probe kind that handles Room Start and Room End.
pub(crate) fn probe_kind(b: &mut GameDefBuilder, name: &str, persistent: bool) -> KindId {
    let mut k = KindDef::new(name, Builtin::Probe);
    k.persistent = persistent;
    k.events.room_start = true;
    k.events.room_end = true;
    b.kind(k)
}

pub(crate) fn room(name: &str, speed: u32) -> RoomDef {
    let mut r = RoomDef::new(name, 800, 608);
    r.file_speed = speed;
    r
}

pub(crate) fn with_ctx<T>(
    def: &GameDef,
    rules: &RunnerRules,
    w: &mut World,
    f: impl FnOnce(&mut Ctx) -> T,
) -> T {
    let scripts = ScriptRuntime::default();
    let mut ctx = Ctx {
        def,
        rules,
        w,
        scripts: &scripts,
        dll_mode: DllMode::Stub,
    };
    f(&mut ctx)
}

pub(crate) fn start(
    def: &GameDef,
    rules: &RunnerRules,
    host: &mut Probe,
    at: StartPoint,
) -> (World, Result<(), SimError>) {
    let mut w = World::new(def.start_room, 1);
    let r = with_ctx(def, rules, &mut w, |ctx| start_game_with(ctx, &at, host));
    (w, r)
}

pub(crate) fn change(
    def: &GameDef,
    rules: &RunnerRules,
    w: &mut World,
    host: &mut Probe,
    req: RoomRequest,
) -> Result<(), SimError> {
    with_ctx(def, rules, w, |ctx| perform_change_with(ctx, req, host))
}

pub(crate) fn ids_of(w: &World, def: &GameDef, name: &str) -> Vec<InstId> {
    w.instances()
        .iter()
        .filter(|i| i.alive && def.kind(i.kind).is_some_and(|k| k.name == name))
        .map(|i| i.id)
        .collect()
}

pub(crate) fn slot(room: RoomId) -> SaveSlot {
    SaveSlot {
        room,
        x: Real::new(49.0),
        y: Real::new(119.0),
        precision: Default::default(),
        flipped: false,
        facing: 1,
        difficulty: 0,
        deaths: 3,
        time: 7,
        bosses: 0,
        items: 0,
        clear: false,
    }
}

/// The order game: kinds ranked kid, blk, world, ps, spawned, so dispatch order differs
/// from file and creation order.
pub(crate) struct Order {
    pub(crate) def: GameDef,
    pub(crate) a: RoomId,
    pub(crate) b: RoomId,
    pub(crate) g: VarId,
}

pub(crate) fn order_game() -> Order {
    let mut b = GameDefBuilder::new("order");
    let world = probe_kind(&mut b, "world", true);
    let ps = probe_kind(&mut b, "ps", false);
    let blk = probe_kind(&mut b, "blk", false);
    let spawned = probe_kind(&mut b, "spawned", false);
    let kid = probe_kind(&mut b, "kid", true);
    b.order(&[kid, blk, world, ps, spawned]);
    b.boot(world);
    b.engine_kind(EngineKind::Kid, kid);
    let g = VarId(0);
    let mut ra = room("a", 30);
    ra.on_create = vec![
        RoomAction::SetGlobal {
            var: g,
            value: Value::Real(Real::ONE),
            difficulty: None,
        },
        RoomAction::Spawn {
            kind: spawned,
            x: Real::ZERO,
            y: Real::ZERO,
            params: Default::default(),
        },
    ];
    let a = b.room(ra);
    let rb = b.room(room("b", 50));
    b.place(a, ps, 32.0, 96.0).unwrap();
    b.place(a, blk, 0.0, 0.0).unwrap();
    b.place(rb, blk, 0.0, 0.0).unwrap();
    Order {
        def: b.build().unwrap(),
        a,
        b: rb,
        g,
    }
}

/// playerStart: creates the kid in its Room Start when none exists.
pub(crate) fn ps_spawns_kid() -> React {
    Box::new(|host, ctx, _me, name, ev| {
        if name == "ps" && *ev == EventKind::RoomStart && ids_of(ctx.w, ctx.def, "kid").is_empty() {
            let kid = ctx.def.engine_kind(EngineKind::Kid).unwrap();
            host.spawn(ctx, kid, Real::new(49.0), Real::new(119.0), SpawnInit::default())
                .unwrap();
        }
        Flow::Continue
    })
}

/// Two rooms, `p` persistent or not, a persistent `keeper` kind in boot and a persistent
/// `sleeper` placed in the first room.
pub(crate) fn two_rooms(first_persistent: bool) -> (GameDef, RoomId, RoomId) {
    let mut b = GameDefBuilder::new("persist");
    let keeper = probe_kind(&mut b, "keeper", true);
    let sleeper = probe_kind(&mut b, "sleeper", true);
    let local = probe_kind(&mut b, "local", false);
    b.boot(keeper);
    let mut r0 = room("p", 50);
    r0.persistent = first_persistent;
    let p = b.room(r0);
    let q = b.room(room("q", 60));
    b.place(p, sleeper, 0.0, 0.0).unwrap();
    b.place(p, local, 5.0, 5.0).unwrap();
    (b.build().unwrap(), p, q)
}

pub(crate) fn deactivate(w: &mut World, def: &GameDef, name: &str) -> InstId {
    let id = ids_of(w, def, name)[0];
    w.get_mut(id).unwrap().active = false;
    id
}

/// kaenbyou's load path: rSelectStage -> rLoad (creation code loadGame) -> room14.
pub(crate) fn load_game() -> (GameDef, RoomId, RoomId, RoomId) {
    let mut b = GameDefBuilder::new("load");
    let world = probe_kind(&mut b, "world", true);
    let kid = probe_kind(&mut b, "kid", true);
    let marker = probe_kind(&mut b, "marker", false);
    b.boot(world);
    b.engine_kind(EngineKind::Kid, kid);
    let select = b.room(room("rSelectStage", 50));
    let mut rl = room("rLoad", 30);
    rl.on_create = vec![RoomAction::LoadGame];
    let load = b.room(rl);
    let saved = b.room(room("room14", 30));
    b.place(load, marker, 0.0, 0.0).unwrap();
    b.place(saved, marker, 1.0, 1.0).unwrap();
    (b.build().unwrap(), select, load, saved)
}

/// A room whose marker requests the same room again in every Room Start.
pub(crate) fn looping(max: u8) -> (Result<(), SimError>, usize, World) {
    let mut b = GameDefBuilder::new("loop");
    let marker = probe_kind(&mut b, "marker", false);
    let r0 = b.room(room("start", 50));
    let lp = b.room(room("loop", 50));
    b.place(lp, marker, 0.0, 0.0).unwrap();
    let def = b.build().unwrap();
    let mut rules = RunnerRules::default();
    rules.room.max_chained_changes = max;
    let mut host = Probe {
        react: Some(Box::new(move |_, ctx, _, _, ev| {
            if *ev == EventKind::RoomStart {
                ctx.w.pending = Some(RoomRequest::Goto(lp));
                return Flow::AbortTick;
            }
            Flow::Continue
        })),
        ..Probe::default()
    };
    let (mut w, r) = start(
        &def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: r0,
            difficulty: 0,
        },
    );
    r.unwrap();
    let res = change(&def, &rules, &mut w, &mut host, RoomRequest::Goto(lp));
    let entries = host.log.iter().filter(|l| *l == "Create marker").count();
    (res, entries, w)
}

/// A persistent room whose creation code spawns a `made` instance with params.
pub(crate) fn revisit_game() -> (GameDef, RoomId, RoomId) {
    let mut b = GameDefBuilder::new("revisit");
    let made = probe_kind(&mut b, "made", false);
    let mut r0 = room("p", 50);
    r0.persistent = true;
    r0.on_create = vec![RoomAction::Spawn {
        kind: made,
        x: Real::ZERO,
        y: Real::ZERO,
        params: num_param(),
    }];
    let p = b.room(r0);
    let q = b.room(room("q", 50));
    (b.build().unwrap(), p, q)
}

pub(crate) fn num_param() -> Params {
    let mut params = Params::default();
    params.0.insert("num".into(), ParamValue::Real(Real::new(2.0)));
    params
}

/// The order game with a third room `c`, the `ps` placed instance named `tut`.
pub(crate) fn stripped_game() -> (Order, RoomId) {
    let mut o = order_game();
    let c = RoomId(u32::try_from(o.def.rooms.len()).unwrap());
    o.def.rooms.push(room("c", 50));
    o.def.names.push("tut".into());
    o.def.rooms[0].instances[0].name = Some(NameId(0));
    (o, c)
}

pub(crate) fn start_in(
    def: &GameDef,
    mode: PlayMode,
    host: &mut Probe,
    at: StartPoint,
) -> (World, Result<(), SimError>) {
    let rules = RunnerRules::default();
    let mut w = World::new(def.start_room, 1);
    w.mode = mode;
    let r = with_ctx(def, &rules, &mut w, |ctx| start_game_with(ctx, &at, host));
    (w, r)
}

/// A room with a kid (when `with_kid`) and two `junk` instances, whose creation code is
/// `rows`; sound 0 is `snd`.
pub(crate) fn rows_game(
    with_kid: bool,
    rows: impl FnOnce(KindId) -> Vec<RoomAction>,
) -> (GameDef, RoomId, KindId) {
    use iwbtg_sim::def::{ClassMask, SoundDef, SoundKind};
    let mut b = GameDefBuilder::new("rows");
    let mut k = KindDef::new("kid", Builtin::Probe);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    // Inert: `destroy` runs the real Destroy dispatch, which the probe host does not stand in for.
    let junk = b.kind(KindDef::new("junk", Builtin::Inert));
    let mut r = room("stage", 50);
    r.on_create = rows(junk);
    let stage = b.room(r);
    if with_kid {
        b.place(stage, kid, 10.0, 20.0).unwrap();
    }
    b.place(stage, junk, 0.0, 0.0).unwrap();
    b.place(stage, junk, 32.0, 0.0).unwrap();
    let mut def = b.build().unwrap();
    def.sounds = vec![SoundDef {
        name: "snd".into(),
        kind: SoundKind::Normal,
        length_ticks: 100,
        volume: Real::ONE,
        pan: Real::ZERO,
    }];
    (def, stage, junk)
}

pub(crate) fn run_rows(def: &GameDef, stage: RoomId, difficulty: u8) -> World {
    let rules = RunnerRules::default();
    let mut host = Probe::default();
    let (w, r) = start(
        def,
        &rules,
        &mut host,
        StartPoint::Room {
            room: stage,
            difficulty,
        },
    );
    r.expect("the start succeeds");
    w
}

pub(crate) fn when(w: RoomWhen, a: RoomAction) -> RoomAction {
    RoomAction::When {
        when: w,
        action: Box::new(a),
    }
}

pub(crate) fn below(n: f64) -> RoomWhen {
    RoomWhen {
        difficulty_below: Some(Real::new(n)),
        ..RoomWhen::default()
    }
}

pub(crate) fn kid_pos(w: &World, def: &GameDef) -> (Real, Real) {
    let kid = w
        .instances()
        .iter()
        .find(|i| def.kind(i.kind).unwrap().name == "kid")
        .unwrap();
    (kid.m.x(), kid.m.y())
}

/// showdown room028: the stage global, then below the threshold move the kid and save.
pub(crate) fn showdown_rows(_: KindId) -> Vec<RoomAction> {
    vec![
        RoomAction::SetGlobalIndexed {
            var: "stage".into(),
            index: GlobalIndex::Savenum,
            value: Value::Real(Real::new(29.0)),
        },
        when(
            below(3.0),
            RoomAction::MoveKid {
                x: Real::new(241.0),
                y: Real::new(503.0),
            },
        ),
        when(below(3.0), RoomAction::SaveGame),
    ]
}
