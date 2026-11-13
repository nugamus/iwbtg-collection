//! Room reachability (docs/format/v0.md, Validation step 7; #504): which rooms a run can get to
//! from where runs start, through the room references the package holds as data. The loader
//! warns about a timing room outside this set. A warning, not an error: a room change the data
//! does not show (a `room_goto` of a computed room, a script-created kind's next or previous)
//! can still lead there.
//!
//! **Roots**: `start_room`, `[engine].new_game_room`, the first room in room order and the boot
//! room (`game_restart` and the reset go there), and every ranked start (`[stripped].start` and
//! each map's).
//!
//! **Edges**: from a room to every room a `room` param names (`room_to` of `warp`,
//! `difficulty_warp`, `edge_warp` and `time_limit`, the per-edge rooms of `edge_warp`), on a
//! placed instance or a room creation `spawn` there, the instance's params over its kind's, and
//! every text param that is a room id (a script kind's untyped `roomTo = "stage2"`). A
//! kind no room places or spawns (only a script or a spawner creates it) can appear anywhere,
//! so the rooms its own params name are roots. A cut room leads to the room it is cut to.
//!
//! **Script room calls** (#731, `script::validate::room_calls`): a room placing or spawning a
//! kind whose script passes a room literal to `room_goto` leads to that room; one whose kind's
//! script calls `room_goto_next` or `room_goto_previous` leads to its neighbours in room order
//! (room ids follow `room_order`). For a kind no room places or spawns, its `room_goto` literals
//! are roots, while its next and previous calls add nothing: the room it runs in is not known
//! from the data.

use std::collections::{BTreeMap, BTreeSet};

use iwbtg_sim::def::{GameDef, KindId, ParamValue, Params, RoomAction, RoomId, RoomRole};
use iwbtg_sim::script::validate::{RoomCalls, room_calls};

/// Every room reachable from the roots, in id order.
pub fn reachable(def: &GameDef) -> BTreeSet<RoomId> {
    let names: BTreeMap<&str, RoomId> = def
        .rooms
        .iter()
        .enumerate()
        .filter_map(|(i, r)| Some((r.name.as_str(), RoomId(u32::try_from(i).ok()?))))
        .collect();
    let calls = room_calls(def);
    let rooms = Rooms { def, names, calls };
    let mut edges: BTreeMap<RoomId, BTreeSet<RoomId>> = BTreeMap::new();
    let mut roots: BTreeSet<RoomId> = BTreeSet::new();
    roots.insert(def.start_room);
    roots.insert(def.new_game_room);
    if !def.rooms.is_empty() {
        roots.insert(RoomId(0));
    }
    for (i, r) in def.rooms.iter().enumerate() {
        if r.role == RoomRole::Boot
            && let Ok(i) = u32::try_from(i)
        {
            roots.insert(RoomId(i));
        }
    }
    roots.extend(def.stripped.start.room);
    roots.extend(def.maps.iter().filter_map(|m| m.start.room));

    let mut used: BTreeSet<KindId> = BTreeSet::new();
    for (i, r) in def.rooms.iter().enumerate() {
        let Ok(i) = u32::try_from(i) else { continue };
        let out = edges.entry(RoomId(i)).or_default();
        let mut here: BTreeSet<KindId> = BTreeSet::new();
        for inst in &r.instances {
            here.insert(inst.kind);
            rooms.named(inst.kind, &inst.params, out);
        }
        for a in &r.on_create {
            rooms.spawns(a, out, &mut here);
        }
        for &k in &here {
            if let Some(c) = rooms.script_calls(k) {
                out.extend(c.goto.iter().filter_map(|n| rooms.names.get(n.as_str()).copied()));
                if c.next {
                    out.extend(i.checked_add(1).map(RoomId));
                }
                if c.previous {
                    out.extend(i.checked_sub(1).map(RoomId));
                }
            }
        }
        used.extend(here);
    }
    for (i, k) in def.kinds.iter().enumerate() {
        let Ok(i) = u32::try_from(i) else { continue };
        if !used.contains(&KindId(i)) {
            rooms.params(&k.params, &mut roots);
            if let Some(c) = rooms.script_calls(KindId(i)) {
                roots.extend(c.goto.iter().filter_map(|n| rooms.names.get(n.as_str()).copied()));
            }
        }
    }
    for (&from, &to) in &def.stripped.cut_rooms {
        edges.entry(from).or_default().insert(to);
    }

    let mut seen: BTreeSet<RoomId> = BTreeSet::new();
    let mut todo: Vec<RoomId> = roots.into_iter().collect();
    while let Some(r) = todo.pop() {
        if def.room(r).is_none() || !seen.insert(r) {
            continue;
        }
        if let Some(next) = edges.get(&r) {
            todo.extend(next.iter().copied());
        }
    }
    seen
}

/// The package's rooms by name, to read script kinds' params, and its scripts' room calls.
struct Rooms<'d> {
    def: &'d GameDef,
    names: BTreeMap<&'d str, RoomId>,
    calls: Vec<RoomCalls>,
}

impl Rooms<'_> {
    /// The room calls of `kind`'s script, if it has one.
    fn script_calls(&self, kind: KindId) -> Option<&RoomCalls> {
        let k = self.def.kinds.get(kind.index()?)?;
        self.calls.get(k.script?.index()?)
    }

    fn spawns(&self, a: &RoomAction, out: &mut BTreeSet<RoomId>, used: &mut BTreeSet<KindId>) {
        match a {
            RoomAction::Spawn { kind, params, .. } => {
                used.insert(*kind);
                self.named(*kind, params, out);
            }
            RoomAction::When { action, .. } => self.spawns(action, out, used),
            _ => {}
        }
    }

    /// The rooms an instance of `kind` with `params` names: its params over its kind's.
    fn named(&self, kind: KindId, params: &Params, out: &mut BTreeSet<RoomId>) {
        if let Some(k) = kind.index().and_then(|i| self.def.kinds.get(i)) {
            for (key, v) in &k.params.0 {
                if !params.0.contains_key(key) {
                    out.extend(self.room_of(v));
                }
            }
        }
        self.params(params, out);
    }

    fn params(&self, params: &Params, out: &mut BTreeSet<RoomId>) {
        out.extend(params.0.values().filter_map(|v| self.room_of(v)));
    }

    /// A `room` param, or text that is a room id: a script kind's params are untyped, so its
    /// `roomTo = "stage2"` stays text.
    fn room_of(&self, v: &ParamValue) -> Option<RoomId> {
        match v {
            ParamValue::Room(r) => Some(*r),
            ParamValue::Str(s) => self.names.get(s.as_str()).copied(),
            _ => None,
        }
    }
}
