//! Loader tests for particle files (docs/format/particles.md, #765). Split from `wp7b_loader.rs`
//! (#792), whose package helpers they use.

use crate::wp7b_loader::*;
use iwbtg_format::PackageFiles;
use iwbtg_format::load::load_report;
use iwbtg_sim::def::{Builtin, ParamValue, PartSystemDefId};
use iwbtg_sim::num::Real;
use iwbtg_sim::particles::{PartRange, Particles, Region, RegionDistr, RegionShape};
use iwbtg_sim::rng::Rng;

// ---- particles (#765) -------------------------------------------------------------------------

const PARTICLES: &str = r##"#:schema ../particles.schema.json
depth = 10

[types.spark]
shape = "flare"
size = [0.5, 1, -0.01, 0]
direction = [0, 360]
life = [20, 20]
color = ["#ffffff", "#ff0000"]
alpha = [1, 0]
additive = true
death = { type = "dust", count = 2 }

[types.dust]
sprite = { id = "block", animate = true, random = true }
speed = [1, 2]
life = [10, 30]

[[emitters]]
region = [0, 800, 0, 608]
shape = "ellipse"
distr = "invgaussian"
stream = { type = "dust", number = -20 }

[[emitters]]
region = [100, 200, 100, 200]
burst = { type = "spark", number = 5 }

[[create]]
at = [400, 304]
type = "spark"
number = 3
"##;

/// The basic package with `files` and `@effect` in `object_order`, placed in stage1.
fn effect_package(files: &[(&str, &str)]) -> PackageFiles {
    let files: Vec<(String, String)> = files
        .iter()
        .map(|(p, t)| ((*p).to_owned(), (*t).to_owned()))
        .collect();
    package("", move |f| {
        let game = String::from_utf8(f[0].1.clone())
            .unwrap()
            .replace("\"playerface\"]", "\"playerface\", \"@effect\"]");
        put(f, "game.toml", &game);
        put(
            f,
            "rooms/stage1.toml",
            &STAGE1.replace(
                "
]
",
                "
  { kind = \"@effect\", x = 0, y = 0 },
]
",
            ),
        );
        for (p, t) in &files {
            put(f, p, t);
        }
    })
}

#[test]
fn a_particle_file_loads_and_a_particle_system_names_it() {
    let files = with_objects(
        &[
            ("particles/fx.toml", PARTICLES),
            (
                "objects/effect.toml",
                "base = \"particle_system\"\nsystem = \"fx\"\non_room_end = \"destroy\"\n",
            ),
        ],
        None,
    );
    let (def, _) = load_ok(&files, STRICT);
    let [s] = def.particle_systems.as_slice() else {
        panic!("one system: {:?}", def.particle_systems);
    };
    assert_eq!(s.name, "fx");
    assert_eq!(s.depth, Real::new(10.0));
    // Types in name order; step and death types are indexes into that list.
    let names: Vec<&str> = s.types.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["dust", "spark"]);
    let (dust, spark) = (&s.types[0], &s.types[1]);
    assert_eq!(spark.draws.death.map(|c| (c.ty.0, c.count)), Some((0, 2)));
    assert_eq!(spark.draws.size, PartRange::new(0.5, 1.0));
    assert_eq!(spark.look.size_change, [Real::new(-0.01), Real::ZERO]);
    assert_eq!(spark.look.shape.as_deref(), Some("flare"));
    assert_eq!(spark.look.colors, [0xFF_FF_FF, 0xFF_00_00]);
    assert!(spark.look.additive);
    assert!(dust.draws.random_subimage && dust.look.shape.is_none());
    assert_eq!(
        dust.look.sprite.map(|(_, animate, stretch)| (animate, stretch)),
        Some((true, false))
    );
    assert_eq!(dust.draws.life, PartRange::new(10.0, 30.0));
    // Emitters in file order with their regions, streams and bursts.
    assert_eq!(s.emitters.len(), 2);
    assert_eq!(
        s.emitters[0].region,
        Region {
            shape: RegionShape::Ellipse,
            distr: RegionDistr::InvGaussian
        }
    );
    assert_eq!(s.emitters[0].stream, Some((0, -20)));
    assert_eq!(s.emitters[0].burst, None);
    assert_eq!(s.emitters[1].rect, [100.0, 200.0, 100.0, 200.0].map(Real::new));
    assert_eq!(s.emitters[1].burst, Some((1, 5)));
    assert_eq!(s.creates.len(), 1);
    assert_eq!((s.creates[0].ty, s.creates[0].number), (1, 3));
    // The object's `system` param resolves to the file.
    let effect = def
        .kinds
        .iter()
        .find(|k| k.base == Builtin::ParticleSystem && k.name.contains("effect"))
        .expect("effect kind");
    assert_eq!(
        effect.params.0.get("system"),
        Some(&ParamValue::Particles(PartSystemDefId(0)))
    );
    assert!(effect.events.room_end && !effect.events.room_start);
}

/// The built system is what the sim's model builds from the def: the loaded package runs.
#[test]
fn a_loaded_particle_system_builds_in_the_model() {
    let files = with_objects(&[("particles/fx.toml", PARTICLES)], None);
    let (def, _) = load_ok(&files, STRICT);
    let mut p = Particles::default();
    let mut rng = Rng::new(7);
    p.build(&mut rng, &def.particle_systems[0]).unwrap();
    assert_ne!(rng, Rng::new(7), "the burst and the create draw");
    for _ in 0..30 {
        p.update(&mut rng).unwrap();
    }
}

/// A negative burst and a negative death count are chances (#769; CLONE_SPEC numbers "Particle
/// draw rules" 8, research#167), not load errors; they reach the sim as written.
#[test]
fn negative_burst_and_death_counts_load() {
    let text = "[types.a]\nlife = [5, 5]\n[types.b]\nlife = [5, 5]\ndeath = { type = \"a\", count = -5 }\n\
                [[emitters]]\nregion = [0, 1, 0, 1]\nburst = { type = \"b\", number = -10 }\n";
    let files = package("", |f| put(f, "particles/p1.toml", text));
    let (def, _) = load_ok(&files, STRICT);
    let s = &def.particle_systems[0];
    assert_eq!(s.emitters[0].burst, Some((1, -10)));
    assert_eq!(s.types[1].draws.death.map(|c| c.count), Some(-5));
}

/// `system` may list files (rely objEffect's two systems) and `on_room_start_except` lists
/// rooms (go-the-spook-house oRainSys); both resolve each value, and a single value still works
/// (client#826).
#[test]
fn particle_system_list_params_resolve() {
    let files = with_objects(
        &[
            ("particles/fx.toml", PARTICLES),
            ("particles/rain.toml", "depth = -100000\n"),
            (
                "objects/effect.toml",
                "base = \"particle_system\"\nsystem = [\"rain\", \"fx\"]\non_room_start = \"clear\"\n\
                 on_room_start_except = [\"stage1\"]\non_room_start_destroys_instance = true\n",
            ),
        ],
        None,
    );
    let (def, _) = load_ok(&files, STRICT);
    let effect = def
        .kinds
        .iter()
        .find(|k| k.base == Builtin::ParticleSystem && k.name.contains("effect"))
        .expect("effect kind");
    let id = |name: &str| {
        let i = def.particle_systems.iter().position(|s| s.name == name).unwrap();
        ParamValue::Particles(PartSystemDefId(i as u32))
    };
    assert_eq!(
        effect.params.0.get("system"),
        Some(&ParamValue::List(vec![id("rain"), id("fx")]))
    );
    let stage1 = def.rooms.iter().position(|r| r.name == "stage1").unwrap();
    assert_eq!(
        effect.params.0.get("on_room_start_except"),
        Some(&ParamValue::List(vec![ParamValue::Room(iwbtg_sim::def::RoomId(
            stage1 as u32
        ))]))
    );
    assert!(effect.events.room_start);
}

/// Unknown files and rooms in a list, an empty list, and a list on a param that takes one value
/// are load errors (client#826).
#[test]
fn particle_system_list_mistakes_are_errors() {
    for (obj, needle) in [
        ("system = [\"fx\", \"nope\"]\n", "`nope` does not exist"),
        ("system = []\n", "param `system` is an empty list"),
        (
            "system = \"fx\"\non_room_start_except = [\"stage1\", \"nowhere\"]\n",
            "`nowhere` does not exist",
        ),
        (
            "system = \"fx\"\non_room_start_except = []\n",
            "param `on_room_start_except` is an empty list",
        ),
        (
            "system = \"fx\"\non_room_start = [\"clear\"]\n",
            "param `on_room_start` must be one of",
        ),
    ] {
        let obj = format!("base = \"particle_system\"\n{obj}");
        let files = effect_package(&[("particles/fx.toml", PARTICLES), ("objects/effect.toml", &obj)]);
        let r = load_report(&files, STRICT);
        assert!(
            errors(&r).iter().any(|m| m.contains(needle)),
            "{obj}\nwanted `{needle}`:\n{}",
            render(&r)
        );
    }
}

/// A script kind's params are single values: a list is an error there (client#826).
#[test]
fn a_script_kind_param_list_is_an_error() {
    let files = with_objects(
        &[
            ("objects/s.toml", "script = \"s\"\nxs = [1, 2]\n"),
            ("scripts/s.rhai", "fn create() {}\n"),
        ],
        None,
    );
    let r = load_report(&files, STRICT);
    assert!(
        errors(&r).iter().any(|m| m.contains("param `xs` is a list")),
        "{}",
        render(&r)
    );
}

/// An alpha value may be `{ random = [min, max] }` (go-the-spook-house's rain, client#826): a
/// range in the def, drawing only, next to the fixed values.
#[test]
fn a_random_alpha_loads_as_a_range() {
    let text = "[types.rain]\nalpha = [{ random = [0.12, 0.32] }, 0]\n";
    let files = package("", |f| put(f, "particles/p1.toml", text));
    let (def, _) = load_ok(&files, STRICT);
    assert_eq!(
        def.particle_systems[0].types[0].look.alpha,
        [PartRange::new(0.12, 0.32), PartRange::fixed(0.0)]
    );
}

#[test]
fn particle_mistakes_are_errors() {
    let base = "[types.a]\nlife = [5, 5]\n";
    for (text, needle) in [
        (
            "[[emitters]]\nregion = [0, 1, 0, 1]\nstream = { type = \"b\", number = 1 }\n",
            "particle type `b` is not in this file",
        ),
        (
            "[[emitters]]\nregion = [0, 1, 0, 1]\nstream = { type = \"a\", number = 0 }\n",
            "a stream of 0",
        ),
        (
            "[[emitters]]\nregion = [0, 1, 0, 1]\nburst = { type = \"a\", number = 0 }\n",
            "a burst of 0",
        ),
        (
            "[types.b]\nlife = [5, 5]\ndeath = { type = \"a\", count = 0 }\n",
            "a `death` count of 0",
        ),
        (
            "[types.b]\nlife = [5, 5]\nstep = { type = \"a\", count = -1 }\n",
            "a `step` count of -1",
        ),
        ("[types.b]\nsize = [1, 2, 3]\n", "`size` is [min, max]"),
        (
            "[types.b]\nshape = \"disk\"\nsprite = { id = \"block\" }\n",
            "not both",
        ),
        (
            "[types.b]\nsprite = { id = \"nope\" }\n",
            "sprite `nope` does not exist",
        ),
        ("[types.b]\ncolor = [\"red\"]\n", "is not \"#rrggbb\""),
        (
            "[types.b]\ncolor = [\"#000000\"]\ncolor_mix = [\"#000000\", \"#ffffff\"]\n",
            "set only one of",
        ),
        (
            "[types.b]\nlife = [5, 10]\ndeath = { type = \"a\", count = 1 }\n",
            "need a fixed life",
        ),
        (
            "[types.a2]\nlife = [5, 5]\ndeath = { type = \"a\", count = 1 }\n[types.b]\nlife = [5, 5]\nstep = { type = \"a2\", count = 1 }\n",
            "has step or death particles of its own",
        ),
        (
            "[types.b]\nalpha = [{ random = [0.5, 0.2] }]\n",
            "`alpha` random range has max below min",
        ),
        ("[types.b]\nwiggle = 1\n", "unknown field"),
    ] {
        let text = format!("{base}{text}");
        let files = package("", |f| put(f, "particles/p1.toml", &text));
        let r = load_report(&files, STRICT);
        let e = errors(&r);
        assert!(
            e.iter()
                .any(|m| m.starts_with("particles/p1.toml:") && m.contains(needle)),
            "{text}\nwanted `{needle}`:\n{}",
            render(&r)
        );
    }
}

#[test]
fn a_particle_system_needs_a_known_system() {
    for obj in [
        "base = \"particle_system\"\n",
        "base = \"particle_system\"\nsystem = \"nope\"\n",
    ] {
        let files = effect_package(&[("objects/effect.toml", obj)]);
        let r = load_report(&files, STRICT);
        assert!(
            errors(&r).iter().any(|m| m.contains("system")),
            "{obj}:\n{}",
            render(&r)
        );
    }
}
