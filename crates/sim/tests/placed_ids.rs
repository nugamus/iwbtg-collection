//! #669: room-placed instances keep their GM8 ids (`gm_id`), runtime ids start at
//! `first_runtime_id`, and a placed id already in use is not created again (CLONE_SPEC,
//! Instances, "Placed instance ids"; register R29).

mod placed_ids {
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{Builtin, ClassMask, GameDef, KindDef, KindId, RoomDef, RoomId};
    use iwbtg_sim::num::Real;
    use iwbtg_sim::room::{RoomRequest, StartPoint, perform_change, start_game};
    use iwbtg_sim::rules::RunnerRules;
    use iwbtg_sim::script::ScriptRuntime;
    use iwbtg_sim::sound::DllMode;
    use iwbtg_sim::world::{Ctx, InstId, SpawnInit, World};

    struct G {
        def: GameDef,
        room: RoomId,
        other: RoomId,
        kid: KindId,
        block: KindId,
    }

    /// eat-the-lemon room13 in small: two blocks out of id order in the file, then the two
    /// persistent kids 104190 and 104180 (file order is not id order either). `other` shares
    /// the kid ids, as duplicated rooms do.
    fn game(first_runtime_id: Option<u32>) -> G {
        let mut b = GameDefBuilder::new("placed-ids");
        let spr = b.sprite_rect((4, 4), (0, 0));
        let mut k = KindDef::new("kid", Builtin::Inert);
        k.persistent = true;
        k.classes = ClassMask::KID;
        k.sprite = Some(spr);
        let kid = b.kind(k);
        let block = b.kind(KindDef::new("block", Builtin::Inert));
        let room = b.room(RoomDef::new("room13", 800, 608));
        let other = b.room(RoomDef::new("room12", 800, 608));
        for (r, kind, y, id) in [
            (room, block, 0.0, 104_194),
            (room, block, 32.0, 104_132),
            (room, kid, 566.0, 104_190),
            (room, kid, 278.0, 104_180),
            (other, kid, 278.0, 104_180),
        ] {
            b.place(r, kind, 48.0, y).unwrap().gm_id = Some(id);
        }
        let mut def = b.build().unwrap();
        def.first_runtime_id = first_runtime_id;
        G {
            def,
            room,
            other,
            kid,
            block,
        }
    }

    fn with<T>(g: &G, w: &mut World, f: impl FnOnce(&mut Ctx) -> T) -> T {
        let rules = RunnerRules::default();
        let scripts = ScriptRuntime::default();
        let mut ctx = Ctx {
            def: &g.def,
            rules: &rules,
            w,
            scripts: &scripts,
            dll_mode: DllMode::Stub,
        };
        f(&mut ctx)
    }

    fn started(g: &G) -> World {
        let mut w = World::new(g.room, 1);
        w.next_id = World::first_runtime_id(&g.def);
        let start = StartPoint::Room {
            room: g.room,
            difficulty: 0,
        };
        with(g, &mut w, |c| start_game(c, &start).unwrap());
        w
    }

    fn ids(w: &World, kind: KindId) -> Vec<u64> {
        w.iter_alive().filter(|i| i.kind == kind).map(|i| i.id.0).collect()
    }

    fn spawn(g: &G, w: &mut World, kind: KindId) -> InstId {
        with(g, w, |c| c.spawn(kind, Real::new(48.0), Real::new(400.0), SpawnInit::default()).unwrap())
    }

    #[test]
    fn placed_instances_get_their_file_ids_in_id_order() {
        let g = game(Some(105_106));
        let mut w = started(&g);
        assert_eq!(ids(&w, g.kid), [104_180, 104_190]);
        assert_eq!(ids(&w, g.block), [104_132, 104_194]);
        let kid_at_278 = w.get(InstId(104_180)).map(|i| i.m.y());
        assert_eq!(kid_at_278, Some(Real::new(278.0)));
        assert_eq!(spawn(&g, &mut w, g.block), InstId(105_106), "runtime ids start at first_runtime_id");
    }

    /// famb-multikid-restart-c: the placed kids are destroyed, the load makes a runtime kid,
    /// R restarts the room: the placed kids come back with their ids, before the loaded kid.
    #[test]
    fn a_restart_recreates_placed_kids_below_the_runtime_kid() {
        let g = game(Some(105_106));
        let mut w = started(&g);
        with(&g, &mut w, |c| {
            c.destroy(InstId(104_180));
            c.destroy(InstId(104_190));
        });
        let loaded = spawn(&g, &mut w, g.kid);
        with(&g, &mut w, |c| perform_change(c, RoomRequest::Restart).unwrap());
        assert_eq!(ids(&w, g.kid), [104_180, 104_190, loaded.0]);
        assert_eq!(ids(&w, g.block), [104_132, 104_194], "the blocks come back with the same ids");
        assert_eq!(spawn(&g, &mut w, g.block), InstId(loaded.0 + 1), "the counter only grows");
    }

    /// placed-ids rows 6 and 11: persistent placed kids that are alive survive a restart and a
    /// room that shares their ids, and are not created again.
    #[test]
    fn a_placed_id_in_use_is_not_created_again() {
        let g = game(Some(105_106));
        let mut w = started(&g);
        let count = w.iter_alive().count();
        with(&g, &mut w, |c| perform_change(c, RoomRequest::Restart).unwrap());
        assert_eq!(ids(&w, g.kid), [104_180, 104_190]);
        assert_eq!(w.iter_alive().count(), count);
        with(&g, &mut w, |c| perform_change(c, RoomRequest::Goto(g.other)).unwrap());
        assert_eq!(ids(&w, g.kid), [104_180, 104_190], "room12 places 104180, which is carried in");
        assert_eq!(w.get(InstId(104_180)).map(|i| i.m.y()), Some(Real::new(278.0)));
    }

    #[test]
    fn without_first_runtime_id_the_counter_starts_above_the_placed_ids() {
        assert_eq!(World::first_runtime_id(&game(None).def), 104_195);
        assert_eq!(World::first_runtime_id(&game(Some(105_106)).def), 105_106);
        let mut plain = game(None);
        for r in &mut plain.def.rooms {
            for p in &mut r.instances {
                p.gm_id = None;
            }
        }
        assert_eq!(World::first_runtime_id(&plain.def), InstId::FIRST.0);
    }

    #[test]
    fn a_placed_id_at_or_above_the_counter_is_an_error() {
        let g = game(Some(105_106));
        let mut w = World::new(g.room, 1);
        w.next_id = 104_190;
        let start = StartPoint::Room {
            room: g.room,
            difficulty: 0,
        };
        let err = with(&g, &mut w, |c| start_game(c, &start)).unwrap_err();
        assert!(err.to_string().contains("placed id 104194 is not below"), "{err}");
    }
}
