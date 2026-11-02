//! Shared helpers for the WP3 collision tests (`tests/wp3_*.rs`).
//!
//! Instances get their sprite, mask, scale and angle through [`image`], which builds an
//! `Image` by deserializing a mirror of its fields: the `Image` setters are WP1 stubs until WP1
//! merges, and the collision code only reads the getters. [`image`] checks every getter, so a
//! change to `Image`'s field order fails here with a clear message instead of a wrong test.
#![allow(dead_code, reason = "each test binary uses a different part of the helpers")]

use iwbtg_sim::anim::Image;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::def::{Builtin, ClassMask, GameDef, KindDef, KindId, MaskId, RoomDef, SpriteId};
use iwbtg_sim::motion::Motion;
use iwbtg_sim::num::Real;
use iwbtg_sim::rules::RunnerRules;
use iwbtg_sim::script::ScriptRuntime;
use iwbtg_sim::sound::DllMode;
use iwbtg_sim::world::{InstId, Instance};
use iwbtg_sim::{Ctx, World};
use serde::Serialize;

/// Field-for-field mirror of `anim::Image` (serde order).
#[derive(Serialize)]
struct ImageMirror {
    sprite: Option<SpriteId>,
    mask: Option<MaskId>,
    index: f64,
    speed: f64,
    xscale: f64,
    yscale: f64,
    angle: f64,
    alpha: f64,
}

/// Image settings for a test instance.
#[derive(Clone, Copy, Debug)]
pub struct Img {
    pub sprite: Option<SpriteId>,
    pub mask: Option<MaskId>,
    pub xscale: f64,
    pub yscale: f64,
    pub angle: f64,
}

impl Img {
    /// Sprite `s` at unit scale, angle 0.
    pub fn sprite(s: SpriteId) -> Img {
        Img {
            sprite: Some(s),
            mask: None,
            xscale: 1.0,
            yscale: 1.0,
            angle: 0.0,
        }
    }

    /// With `image_xscale` `x`.
    pub fn xscale(self, x: f64) -> Img {
        Img { xscale: x, ..self }
    }

    /// With `image_yscale` `y`.
    pub fn yscale(self, y: f64) -> Img {
        Img { yscale: y, ..self }
    }

    /// With `image_angle` `a`.
    pub fn angle(self, a: f64) -> Img {
        Img { angle: a, ..self }
    }
}

/// An `Image` with these settings, image_index 0, image_speed 1, alpha 1.
pub fn image(i: Img) -> Image {
    let m = ImageMirror {
        sprite: i.sprite,
        mask: i.mask,
        index: 0.0,
        speed: 1.0,
        xscale: i.xscale,
        yscale: i.yscale,
        angle: i.angle,
        alpha: 1.0,
    };
    let bytes = postcard::to_allocvec(&m).expect("mirror serializes");
    let img: Image = postcard::from_bytes(&bytes).expect("Image layout changed: update wp3_support::ImageMirror");
    let ok = img.sprite() == i.sprite
        && img.mask() == i.mask
        && img.xscale() == Real::new(i.xscale)
        && img.yscale() == Real::new(i.yscale)
        && img.angle() == Real::new(i.angle)
        && img.alpha() == Real::ONE;
    assert!(ok, "Image layout changed: update wp3_support::ImageMirror");
    img
}

/// Field-for-field mirror of `motion::Motion` (serde order).
#[derive(Serialize)]
struct MotionMirror {
    x: f64,
    y: f64,
    xstart: f64,
    ystart: f64,
    xprevious: f64,
    yprevious: f64,
    hspeed: f64,
    vspeed: f64,
    speed: f64,
    direction: f64,
    gravity: f64,
    gravity_direction: f64,
    friction: f64,
}

/// A `Motion` at `pos` with this previous position and speeds (direction and speed left 0;
/// the collision code never reads them). Checked like [`image`].
pub fn motion(pos: (f64, f64), prev: (f64, f64), speed: (f64, f64)) -> Motion {
    let m = MotionMirror {
        x: pos.0,
        y: pos.1,
        xstart: pos.0,
        ystart: pos.1,
        xprevious: prev.0,
        yprevious: prev.1,
        hspeed: speed.0,
        vspeed: speed.1,
        speed: 0.0,
        direction: 0.0,
        gravity: 0.0,
        gravity_direction: 270.0,
        friction: 0.0,
    };
    let bytes = postcard::to_allocvec(&m).expect("mirror serializes");
    let mo: Motion = postcard::from_bytes(&bytes).expect("Motion layout changed: update wp3_support::MotionMirror");
    let ok = mo.x() == Real::new(pos.0)
        && mo.y() == Real::new(pos.1)
        && mo.xprevious() == Real::new(prev.0)
        && mo.yprevious() == Real::new(prev.1)
        && mo.hspeed() == Real::new(speed.0)
        && mo.vspeed() == Real::new(speed.1);
    assert!(ok, "Motion layout changed: update wp3_support::MotionMirror");
    mo
}

/// The up-spike triangle of 18 of the 19 spike games: 32x32, 2 px at the tip, widening by 1 px
/// on each side every 2 rows, 544 pixels (CLONE_SPEC collision, Confirmed).
pub fn spike_rows() -> Vec<String> {
    (0..32)
        .map(|r: i32| {
            let k = r / 2;
            (0..32)
                .map(|c| if (15 - k..=16 + k).contains(&c) { '#' } else { '.' })
                .collect()
        })
        .collect()
}

/// A test game: kinds and sprites, plus a world to put instances in.
pub struct Game {
    pub def: GameDef,
    pub rules: RunnerRules,
    pub w: World,
    pub scripts: ScriptRuntime,
    /// The kid's sprite: an 11x21 rectangle, origin (5, 12), the box of maskPlayer
    /// (x-5..x+5, y-12..y+8).
    pub kid_sprite: SpriteId,
    /// A 32x32 rectangle, origin (0, 0) (block).
    pub block_sprite: SpriteId,
    /// The precise up spike, origin (0, 0).
    pub spike_sprite: SpriteId,
    /// The precise up spike with a centre origin (16, 16), as kaenbyou's sprSpikeM.
    pub spike_centre_sprite: SpriteId,
    /// A 32x8 rectangle, origin (0, 0) (platform).
    pub platform_sprite: SpriteId,
    pub kid: KindId,
    pub block: KindId,
    pub spike: KindId,
    pub platform: KindId,
    pub vine: KindId,
}

/// Builds the test game in a room of `w` x `h`.
pub fn game(w: u32, h: u32) -> Game {
    let mut b = GameDefBuilder::new("wp3");
    let kid_sprite = b.sprite_rect((11, 21), (5, 12));
    let block_sprite = b.sprite_rect((32, 32), (0, 0));
    let rows = spike_rows();
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let spike_sprite = b.sprite_bits(&rows, (0, 0));
    let spike_centre_sprite = b.sprite_bits(&rows, (16, 16));
    let platform_sprite = b.sprite_rect((32, 8), (0, 0));
    let mut k = KindDef::new("kid", Builtin::Kid);
    k.classes = ClassMask::KID;
    let kid = b.kind(k);
    let mut k = KindDef::new("block", Builtin::Block);
    k.classes = ClassMask::BLOCK;
    k.solid = true;
    let block = b.kind(k);
    let mut k = KindDef::new("spike", Builtin::Probe);
    k.classes = ClassMask::KILLER;
    let spike = b.kind(k);
    let mut k = KindDef::new("platform", Builtin::Probe);
    k.classes = ClassMask::PLATFORM | ClassMask::BLOCK;
    let platform = b.kind(k);
    let mut k = KindDef::new("vine", Builtin::Probe);
    k.classes = ClassMask::VINE_R;
    let vine = b.kind(k);
    b.room(RoomDef::new("r", w, h));
    let def = b.build().expect("test game builds");
    Game {
        def,
        rules: RunnerRules::default(),
        w: World::new(iwbtg_sim::def::RoomId(0), 1),
        scripts: ScriptRuntime::default(),
        kid_sprite,
        block_sprite,
        spike_sprite,
        spike_centre_sprite,
        platform_sprite,
        kid,
        block,
        spike,
        platform,
        vine,
    }
}

impl Game {
    /// Adds an instance of `kind` at `(x, y)` with this image; classes and solid from the kind,
    /// rank = kind index.
    pub fn add(&mut self, kind: KindId, x: f64, y: f64, img: Img) -> InstId {
        let kd = self.def.kind(kind).expect("kind exists").clone();
        let id = self.w.alloc_id().expect("id");
        let rank = u16::try_from(kind.0).expect("small kind index");
        let mut inst = Instance::new(id, kind, rank, Real::new(x), Real::new(y));
        inst.classes = kd.classes;
        inst.solid = kd.solid;
        inst.img = image(img);
        self.w.insert(inst).expect("insert");
        id
    }

    /// Adds a kid facing `xscale` at `(x, y)`.
    pub fn add_kid(&mut self, x: f64, y: f64, xscale: f64) -> InstId {
        let s = self.kid_sprite;
        self.add(self.kid, x, y, Img::sprite(s).xscale(xscale))
    }

    /// Adds a solid 32x32 block at `(x, y)`.
    pub fn add_block(&mut self, x: f64, y: f64) -> InstId {
        let s = self.block_sprite;
        self.add(self.block, x, y, Img::sprite(s))
    }

    /// Moves an instance by replacing its motion (the `Motion` setters are WP2's).
    pub fn put(&mut self, id: InstId, x: f64, y: f64) {
        let inst = self.w.get_mut(id).expect("instance exists");
        inst.m = Motion::new_at(Real::new(x), Real::new(y));
    }

    /// A context over the game.
    pub fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            def: &self.def,
            rules: &self.rules,
            w: &mut self.w,
            scripts: &self.scripts,
            dll_mode: DllMode::Stub,
        }
    }
}

/// Shorthand for `Real::new`.
pub fn r(v: f64) -> Real {
    Real::new(v)
}

/// Runs `f`; when it reaches a WP0 stub of another work package (`unimplemented!("WPn: ...")`),
/// prints a skip notice and returns `None`. Any other panic is re-raised.
pub fn unless_stubbed<T>(what: &str, f: impl FnOnce() -> T + std::panic::UnwindSafe) -> Option<T> {
    match std::panic::catch_unwind(f) {
        Ok(v) => Some(v),
        Err(e) => {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_default();
            if msg.contains("not implemented: WP") {
                eprintln!("skipped {what}: needs a stub that is not merged yet ({msg})");
                None
            } else {
                std::panic::resume_unwind(e)
            }
        }
    }
}
