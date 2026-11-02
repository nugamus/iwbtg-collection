use iwbtg_format::PackageFiles;
use iwbtg_format::load::{LoadMode, load_game};

#[test]
fn dump() {
    let dir = std::env::var("PKG").unwrap();
    let files = PackageFiles::load_dir(dir).unwrap();
    let def = load_game(
        &files,
        LoadMode::Trace {
            unmapped_kinds_inert: true,
        },
    )
    .unwrap();
    let room = &def.rooms[4];
    println!("room {} {} instances", room.name, room.instances.len());
    let mut counts = std::collections::BTreeMap::new();
    for i in &room.instances {
        *counts
            .entry(def.kinds[i.kind.0 as usize].name.clone())
            .or_insert(0) += 1;
    }
    println!("{counts:?}");
    for i in &room.instances {
        let k = &def.kinds[i.kind.0 as usize];
        if (i.y.get() > 540.0 && i.y.get() < 620.0 && i.x.get() < 100.0) || k.name == "kid_start" {
            println!(
                "{} at {},{} classes {:?} solid {} mask {:?} base {:?}",
                k.name,
                i.x.get(),
                i.y.get(),
                k.classes,
                k.solid,
                k.mask.is_some(),
                k.base
            );
        }
    }
    println!("start {:?} new {:?}", def.start_room, def.new_game_room);
    for (n, r) in def.rooms.iter().enumerate() {
        println!("{n} {}", r.name);
    }
}

#[test]
fn dump2() {
    let dir = std::env::var("PKG").unwrap();
    let files = PackageFiles::load_dir(dir).unwrap();
    let def = load_game(
        &files,
        LoadMode::Trace {
            unmapped_kinds_inert: true,
        },
    )
    .unwrap();
    for k in &def.kinds {
        if k.name == "kid" || k.name == "block" || k.name == "@block_2" {
            println!(
                "{} base {:?} classes {:?} sprite {:?} mask {:?} solid {}",
                k.name, k.base, k.classes, k.sprite, k.mask, k.solid
            );
            if let Some(s) = k.sprite {
                let sp = &def.sprites[s.0 as usize];
                println!("  sprite {:?} origin {:?} masks {:?}", s, sp.origin, sp.masks);
            }
            if let Some(m) = k.mask {
                println!("  mask {:?}", def.masks[m.0 as usize].bbox);
            }
        }
    }
    for m in [8usize, 0, 11, 14] {
        println!("mask {m}: {:?} shape {:?}", def.masks[m].bbox, def.masks[m].shape);
    }
    println!(
        "kid events {:?}",
        def.kinds
            .iter()
            .find(|k| k.name == "kid")
            .map(|k| k.events.collisions.clone())
    );
}

#[test]
fn run() {
    use iwbtg_sim::input::InputFrame;
    use iwbtg_sim::room::StartPoint;
    use iwbtg_sim::sim::{Sim, SimOptions};
    use iwbtg_sim::sound::DllMode;
    let dir = std::env::var("PKG").unwrap();
    let files = PackageFiles::load_dir(dir).unwrap();
    let def = std::sync::Arc::new(
        load_game(
            &files,
            LoadMode::Trace {
                unmapped_kinds_inert: true,
            },
        )
        .unwrap(),
    );
    let room = def.rooms.iter().position(|r| r.name == "room_1").unwrap();
    let mut sim = Sim::new(
        def.clone(),
        SimOptions {
            start: StartPoint::Room {
                room: iwbtg_sim::def::RoomId(room as u32),
                difficulty: 0,
            },
            seed: 0,
            dll_mode: DllMode::Model,
        },
    )
    .unwrap();
    for t in 0..4 {
        let r = sim.step(InputFrame::EMPTY);
        let w = sim.world();
        let n = w.iter_alive().count();
        let blocks: Vec<_> = w
            .iter_alive()
            .filter(|i| i.classes.intersects(iwbtg_sim::def::ClassMask::BLOCK))
            .map(|i| (i.m.x().get(), i.m.y().get()))
            .take(4)
            .collect();
        let kid: Vec<_> = w
            .iter_alive()
            .filter(|i| i.classes.intersects(iwbtg_sim::def::ClassMask::KID))
            .map(|i| (i.m.x().get(), i.m.y().get(), i.m.vspeed().get()))
            .collect();
        {
            use iwbtg_sim::collide::{Target, query};
            let mut w2 = w.clone();
            let rules = iwbtg_sim::rules::RunnerRules::for_runner(def.runner, def.engine_rev).unwrap();
            let scripts = iwbtg_sim::script::ScriptRuntime::new(&def).unwrap();
            let kid_id = w2
                .iter_alive()
                .find(|i| i.classes.intersects(iwbtg_sim::def::ClassMask::KID))
                .map(|i| i.id)
                .unwrap();
            let (kx, ky) = {
                let k = w2.get(kid_id).unwrap();
                (k.m.x(), k.m.y())
            };
            let ctx = iwbtg_sim::world::Ctx {
                def: &def,
                rules: &rules,
                w: &mut w2,
                scripts: &scripts,
                dll_mode: DllMode::Model,
            };
            let hit = query::place_meeting(
                &ctx,
                kid_id,
                kx,
                ky,
                Target::Class(iwbtg_sim::def::ClassMask::BLOCK),
            )
            .unwrap();
            let hit_below = query::place_meeting(
                &ctx,
                kid_id,
                kx,
                ky + iwbtg_sim::num::Real::new(1.0),
                Target::Class(iwbtg_sim::def::ClassMask::BLOCK),
            )
            .unwrap();
            println!("t{t} place_meeting block now {hit} below {hit_below}");
        }
        println!("t{t} alive {n} blocks {blocks:?} kid {kid:?} {:?}", r.status);
    }
}
