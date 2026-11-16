//! Particle files to `PartSystemDef` (docs/format/v0.md, Particles; #765).
//!
//! `particles/<id>.toml` is one particle system as the original's code builds it: `[types.<name>]`
//! (GM's `part_type_*` arguments), `[[emitters]]` in creation order (`part_emitter_region`,
//! `part_emitter_stream`, `part_emitter_burst`) and `[[create]]` (`part_particles_create`). The
//! `particle_system` built-in names one by id. Settings the sim's draw model refuses (CLONE_SPEC
//! numbers, "Particle draw rules"; `sim::particles` OPEN list) are load errors, so a package never
//! fails mid-run on them.

use std::collections::BTreeMap;

use iwbtg_sim::def::{PartCreateDef, PartEmitterDef, PartLook, PartSystemDef, PartTypeDef, SpriteId};
use iwbtg_sim::num::Real;
use iwbtg_sim::particles::{
    PartChild, PartColor, PartRange, PartType, PartTypeId, Region, RegionDistr, RegionShape,
};
use serde::Deserialize;
use toml::Spanned;

use crate::diag::Diagnostic;
use crate::files::PackageFiles;
use crate::number::Number;

/// GM's default particle life (`part_type_life`), in ticks.
const DEFAULT_LIFE: f64 = 100.0;

/// Resolves a sprite id named at `span` of `text` in file `path` (`None` after reporting it).
pub type SpriteLookup<'a> = dyn FnMut(&str, &str, &str, std::ops::Range<usize>) -> Option<SpriteId> + 'a;

/// A particle file (`particles/<id>.toml`).
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ParticleFile {
    /// `part_system_depth` (drawing only); default 0.
    #[serde(default)]
    pub depth: Option<Number>,
    /// Particle types by name; created in name order.
    #[serde(default)]
    pub types: BTreeMap<String, TypeFile>,
    /// Emitters in creation order.
    #[serde(default)]
    pub emitters: Vec<EmitterFile>,
    /// `part_particles_create` calls, run after every emitter's burst.
    #[serde(default)]
    pub create: Vec<CreateFile>,
}

/// One `[types.<name>]`.
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TypeFile {
    /// `part_type_shape`; default `pixel`. Not with `sprite`.
    #[serde(default)]
    pub shape: Option<ShapeName>,
    /// `part_type_sprite`. Not with `shape`.
    #[serde(default)]
    pub sprite: Option<SpriteFile>,
    /// `part_type_size`: `[min, max]` or `[min, max, increment, wiggle]`; default `[1, 1]`.
    #[serde(default)]
    pub size: Option<Vec<Number>>,
    /// `part_type_scale` x and y; default `[1, 1]`.
    #[serde(default)]
    pub scale: Option<[Number; 2]>,
    /// `part_type_speed`: `[min, max]` or `[min, max, increment, wiggle]`; default `[0, 0]`.
    #[serde(default)]
    pub speed: Option<Vec<Number>>,
    /// `part_type_direction`: `[min, max]` or `[min, max, increment, wiggle]`; default `[0, 0]`.
    #[serde(default)]
    pub direction: Option<Vec<Number>>,
    /// `part_type_gravity` amount and direction; default `[0, 270]`.
    #[serde(default)]
    pub gravity: Option<[Number; 2]>,
    /// `part_type_orientation`: `[min, max]` or `[min, max, increment, wiggle]`; default `[0, 0]`.
    #[serde(default)]
    pub orientation: Option<Vec<Number>>,
    /// `part_type_orientation` relative; default false.
    #[serde(default)]
    pub orientation_relative: bool,
    /// `part_type_color1/2/3`: one to three `"#rrggbb"`; default white.
    #[serde(default)]
    pub color: Option<Vec<String>>,
    /// `part_type_color_mix`: two `"#rrggbb"`.
    #[serde(default)]
    pub color_mix: Option<[String; 2]>,
    /// `part_type_color_rgb`: `[[rmin, rmax], [gmin, gmax], [bmin, bmax]]`.
    #[serde(default)]
    pub color_rgb: Option<[[Number; 2]; 3]>,
    /// `part_type_color_hsv`: `[[hmin, hmax], [smin, smax], [vmin, vmax]]`.
    #[serde(default)]
    pub color_hsv: Option<[[Number; 2]; 3]>,
    /// `part_type_alpha1/2/3`: one to three values, each a number or `{ random = [min, max] }`;
    /// default `[1]`.
    #[serde(default)]
    pub alpha: Option<Vec<AlphaValue>>,
    /// `part_type_blend` additive; default false.
    #[serde(default)]
    pub additive: bool,
    /// `part_type_life` `[min, max]` in ticks; default `[100, 100]`.
    #[serde(default)]
    pub life: Option<[Number; 2]>,
    /// `part_type_step`: particles of another type each tick.
    #[serde(default)]
    pub step: Option<ChildFile>,
    /// `part_type_death`: particles of another type when one dies.
    #[serde(default)]
    pub death: Option<ChildFile>,
}

/// One `part_type_alpha1/2/3` value.
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum AlphaValue {
    /// A fixed value.
    Fixed(Number),
    /// `{ random = [min, max] }`: the original's code passes a random value (go-the-spook-house
    /// `random_range(0.12, 0.32)`, client#826). Drawing only: the client picks one value per
    /// built system from its own generator; the original's RNG draw stays with the code that
    /// makes it.
    Random(RandomAlpha),
}

/// `{ random = [min, max] }`.
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RandomAlpha {
    /// The range, `[min, max]`.
    pub random: [Number; 2],
}

/// `part_type_sprite`.
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SpriteFile {
    /// A `sprites/<id>` id.
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub id: Spanned<String>,
    /// Animate through the frames; default false.
    #[serde(default)]
    pub animate: bool,
    /// Stretch the animation over the life; default false.
    #[serde(default)]
    pub stretch: bool,
    /// Start on a random frame (one draw); default false.
    #[serde(default)]
    pub random: bool,
}

/// `part_type_step` or `part_type_death`.
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ChildFile {
    /// A type of this file.
    #[serde(rename = "type")]
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub ty: Spanned<String>,
    /// Particles per tick (step) or at death.
    pub count: i32,
}

/// `pt_shape_*`.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs, reason = "GM's shape names")]
pub enum ShapeName {
    Pixel,
    Disk,
    Square,
    Line,
    Star,
    Circle,
    Ring,
    Sphere,
    Flare,
    Spark,
    Explosion,
    Cloud,
    Smoke,
    Snow,
}

impl ShapeName {
    fn word(self) -> &'static str {
        match self {
            ShapeName::Pixel => "pixel",
            ShapeName::Disk => "disk",
            ShapeName::Square => "square",
            ShapeName::Line => "line",
            ShapeName::Star => "star",
            ShapeName::Circle => "circle",
            ShapeName::Ring => "ring",
            ShapeName::Sphere => "sphere",
            ShapeName::Flare => "flare",
            ShapeName::Spark => "spark",
            ShapeName::Explosion => "explosion",
            ShapeName::Cloud => "cloud",
            ShapeName::Smoke => "smoke",
            ShapeName::Snow => "snow",
        }
    }
}

/// `ps_shape_*`.
#[derive(Deserialize, Clone, Copy, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RegionShapeName {
    /// Rectangle (default).
    #[default]
    Rectangle,
    /// Ellipse.
    Ellipse,
    /// Diamond.
    Diamond,
    /// Line from the top-left to the bottom-right corner.
    Line,
}

/// `ps_distr_*`.
#[derive(Deserialize, Clone, Copy, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum DistrName {
    /// Linear (default).
    #[default]
    Linear,
    /// Gaussian: more particles in the middle.
    Gaussian,
    /// Inverse gaussian: more particles at the edges.
    Invgaussian,
}

/// One `[[emitters]]`.
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct EmitterFile {
    /// `part_emitter_region` xmin, xmax, ymin, ymax, in room coordinates.
    pub region: [Number; 4],
    /// Region shape; default `rectangle`.
    #[serde(default)]
    pub shape: RegionShapeName,
    /// Distribution; default `linear`.
    #[serde(default)]
    pub distr: DistrName,
    /// `part_emitter_stream`.
    #[serde(default)]
    pub stream: Option<CountFile>,
    /// `part_emitter_burst`, once, when the system is built.
    #[serde(default)]
    pub burst: Option<CountFile>,
}

/// A type and a number (`stream`, `burst`).
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CountFile {
    /// A type of this file.
    #[serde(rename = "type")]
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub ty: Spanned<String>,
    /// Stream: `n > 0` particles per tick, `n < 0` one with chance `1 / (|n| + 1)`. Burst: `n`
    /// particles.
    #[cfg_attr(feature = "schema", schemars(with = "i32"))]
    pub number: Spanned<i32>,
}

/// One `[[create]]`.
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateFile {
    /// Where, in room coordinates.
    pub at: [Number; 2],
    /// A type of this file.
    #[serde(rename = "type")]
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub ty: Spanned<String>,
    /// How many; below 1 makes nothing.
    pub number: i32,
}

/// Reads every particle file. The result's index is the `PartSystemDefId`; ids are the file stems
/// in path order. `sprite` resolves a sprite id (`None` after reporting it).
pub fn load(
    files: &PackageFiles,
    sprite: &mut SpriteLookup<'_>,
    diags: &mut Vec<Diagnostic>,
) -> Vec<PartSystemDef> {
    let mut out = Vec::new();
    for (path, bytes) in files.iter() {
        let Some(id) = path
            .strip_prefix("particles/")
            .and_then(|p| p.strip_suffix(".toml"))
        else {
            continue;
        };
        if id.contains('/') {
            continue;
        }
        let Ok(text) = std::str::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)) else {
            diags.push(Diagnostic::file_error(path, "file is not valid UTF-8"));
            continue;
        };
        let file = match toml::from_str::<ParticleFile>(text) {
            Ok(f) => f,
            Err(e) => {
                diags.push(Diagnostic::error_at(
                    path,
                    text,
                    e.span().map_or(0, |s| s.start),
                    e.message().to_owned(),
                ));
                continue;
            }
        };
        let mut b = Build {
            path,
            text,
            diags,
            failed: false,
        };
        let def = b.system(id, &file, sprite);
        if !b.failed {
            out.push(def);
        }
    }
    out
}

struct Build<'a> {
    path: &'a str,
    text: &'a str,
    diags: &'a mut Vec<Diagnostic>,
    failed: bool,
}

impl Build<'_> {
    fn error(&mut self, at: usize, msg: impl Into<String>) {
        self.diags
            .push(Diagnostic::error_at(self.path, self.text, at, msg));
        self.failed = true;
    }

    /// The offset of `[types.<name>]` (or of the name) for messages about a type.
    fn type_at(&self, name: &str) -> usize {
        self.text
            .find(&format!("[types.{name}]"))
            .or_else(|| self.text.find(name))
            .unwrap_or(0)
    }

    fn type_ref(&mut self, names: &[&String], r: &Spanned<String>) -> u32 {
        match names.iter().position(|n| *n == r.get_ref()) {
            Some(i) => u32::try_from(i).unwrap_or(u32::MAX),
            None => {
                self.error(
                    r.span().start,
                    format!("particle type `{}` is not in this file", r.get_ref()),
                );
                0
            }
        }
    }

    fn system(&mut self, id: &str, file: &ParticleFile, sprite: &mut SpriteLookup<'_>) -> PartSystemDef {
        let names: Vec<&String> = file.types.keys().collect();
        let mut types = Vec::with_capacity(names.len());
        for (name, t) in &file.types {
            let at = self.type_at(name);
            let ty = self.part_type(name, t, at, &names, sprite);
            types.push(ty);
        }
        self.check_children(&types);
        let mut emitters = Vec::with_capacity(file.emitters.len());
        for em in &file.emitters {
            let stream = em.stream.as_ref().map(|s| {
                if *s.number.get_ref() == 0 {
                    self.error(
                        s.number.span().start,
                        "a stream of 0 is not traced (sim N10, research#158)",
                    );
                }
                (self.type_ref(&names, &s.ty), *s.number.get_ref())
            });
            let burst = em.burst.as_ref().map(|s| {
                if *s.number.get_ref() == 0 {
                    self.error(
                        s.number.span().start,
                        "a burst of 0 is not traced (sim N10, research#167)",
                    );
                }
                (self.type_ref(&names, &s.ty), *s.number.get_ref())
            });
            let r = em.region;
            emitters.push(PartEmitterDef {
                rect: [real(r[0]), real(r[1]), real(r[2]), real(r[3])],
                region: Region {
                    shape: match em.shape {
                        RegionShapeName::Rectangle => RegionShape::Rectangle,
                        RegionShapeName::Ellipse => RegionShape::Ellipse,
                        RegionShapeName::Diamond => RegionShape::Diamond,
                        RegionShapeName::Line => RegionShape::Line,
                    },
                    distr: match em.distr {
                        DistrName::Linear => RegionDistr::Linear,
                        DistrName::Gaussian => RegionDistr::Gaussian,
                        DistrName::Invgaussian => RegionDistr::InvGaussian,
                    },
                },
                stream,
                burst,
            });
        }
        let creates = file
            .create
            .iter()
            .map(|c| PartCreateDef {
                at: [real(c.at[0]), real(c.at[1])],
                ty: self.type_ref(&names, &c.ty),
                number: c.number,
            })
            .collect();
        PartSystemDef {
            name: id.to_owned(),
            types,
            emitters,
            creates,
            depth: file.depth.map_or(Real::ZERO, real),
        }
    }

    fn part_type(
        &mut self,
        name: &str,
        t: &TypeFile,
        at: usize,
        names: &[&String],
        sprite: &mut SpriteLookup<'_>,
    ) -> PartTypeDef {
        let who = format!("particle type `{name}`");
        let range =
            |b: &mut Self, key: &str, v: &Option<Vec<Number>>, default: f64| -> (PartRange, [Real; 2]) {
                match v.as_deref() {
                    None => (PartRange::fixed(default), [Real::ZERO; 2]),
                    Some([lo, hi]) => (PartRange::new(lo.0, hi.0), [Real::ZERO; 2]),
                    Some([lo, hi, inc, wig]) => (PartRange::new(lo.0, hi.0), [real(*inc), real(*wig)]),
                    Some(_) => {
                        b.error(
                            at,
                            format!("{who}: `{key}` is [min, max] or [min, max, increment, wiggle]"),
                        );
                        (PartRange::fixed(default), [Real::ZERO; 2])
                    }
                }
            };
        let (size, size_change) = range(self, "size", &t.size, 1.0);
        let (speed, speed_change) = range(self, "speed", &t.speed, 0.0);
        let (direction, direction_change) = range(self, "direction", &t.direction, 0.0);
        let (orientation, orientation_change) = range(self, "orientation", &t.orientation, 0.0);
        let life = t.life.map_or(PartRange::fixed(DEFAULT_LIFE), |[lo, hi]| {
            PartRange::new(lo.0, hi.0)
        });

        let set = [
            t.color.is_some(),
            t.color_mix.is_some(),
            t.color_rgb.is_some(),
            t.color_hsv.is_some(),
        ];
        if set.iter().filter(|s| **s).count() > 1 {
            self.error(
                at,
                format!("{who}: set only one of `color`, `color_mix`, `color_rgb`, `color_hsv`"),
            );
        }
        let mut colors = Vec::new();
        let color = if let Some(c) = &t.color {
            if c.is_empty() || c.len() > 3 {
                self.error(at, format!("{who}: `color` holds one to three colors"));
            }
            colors = c.iter().map(|s| self.hex(at, &who, s)).collect();
            PartColor::Fixed
        } else if let Some(c) = &t.color_mix {
            colors = c.iter().map(|s| self.hex(at, &who, s)).collect();
            PartColor::Mix
        } else if let Some(c) = t.color_rgb {
            PartColor::Rgb(c.map(|[lo, hi]| PartRange::new(lo.0, hi.0)))
        } else if let Some(c) = t.color_hsv {
            PartColor::Hsv(c.map(|[lo, hi]| PartRange::new(lo.0, hi.0)))
        } else {
            colors.push(0xFF_FF_FF);
            PartColor::Fixed
        };
        let alpha: Vec<PartRange> = match &t.alpha {
            None => vec![PartRange::fixed(1.0)],
            Some(a) => {
                if a.is_empty() || a.len() > 3 {
                    self.error(at, format!("{who}: `alpha` holds one to three values"));
                }
                a.iter()
                    .map(|v| match v {
                        AlphaValue::Fixed(n) => PartRange::fixed(n.0),
                        AlphaValue::Random(RandomAlpha { random: [lo, hi] }) => {
                            if hi.0 < lo.0 {
                                self.error(at, format!("{who}: `alpha` random range has max below min"));
                            }
                            PartRange::new(lo.0, hi.0)
                        }
                    })
                    .collect()
            }
        };
        if t.shape.is_some() && t.sprite.is_some() {
            self.error(at, format!("{who}: set `shape` or `sprite`, not both"));
        }
        let look_sprite = t.sprite.as_ref().and_then(|s| {
            let id = sprite(s.id.get_ref(), self.path, self.text, s.id.span());
            if id.is_none() {
                self.failed = true;
            }
            id.map(|i| (i, s.animate, s.stretch))
        });
        let child = |b: &mut Self, c: &Option<ChildFile>, key: &str| {
            c.as_ref().map(|c| {
                // A negative death count is a chance per dying particle (Confirmed (recorder),
                // research#167); a negative step count is only Inferred (research#168).
                if c.count == 0 || (key == "step" && c.count < 0) {
                    b.error(
                        c.ty.span().start,
                        format!("{who}: a `{key}` count of {} is not traced (sim N10, research#167, research#168)", c.count),
                    );
                }
                PartChild {
                    count: c.count,
                    ty: PartTypeId(b.type_ref(names, &c.ty)),
                }
            })
        };
        let step = child(self, &t.step, "step");
        let death = child(self, &t.death, "death");
        if (step.is_some() || death.is_some()) && (life.draws() || life.max.get() < life.min.get()) {
            self.error(
                at,
                format!(
                    "{who}: step or death particles need a fixed life: which draw gives a random life is not in the spec (N10)"
                ),
            );
        }
        let [gravity, gravity_dir] = t.gravity.map_or([Real::ZERO, Real::new(270.0)], |g| g.map(real));
        PartTypeDef {
            name: name.to_owned(),
            draws: PartType {
                size,
                speed,
                direction,
                orientation,
                life,
                color,
                random_subimage: t.sprite.as_ref().is_some_and(|s| s.random),
                step,
                death,
            },
            look: PartLook {
                shape: if t.sprite.is_some() {
                    None
                } else {
                    Some(t.shape.unwrap_or(ShapeName::Pixel).word().to_owned())
                },
                sprite: look_sprite,
                size_change,
                scale: t.scale.map_or([Real::ONE; 2], |s| s.map(real)),
                speed_change,
                direction_change,
                orientation_change,
                orientation_relative: t.orientation_relative,
                gravity: [gravity, gravity_dir],
                colors,
                alpha,
                additive: t.additive,
            },
        }
    }

    /// Step or death types that have step or death particles of their own (the sim refuses them).
    fn check_children(&mut self, types: &[PartTypeDef]) {
        for t in types {
            for c in [t.draws.step, t.draws.death].into_iter().flatten() {
                let Some(ct) = usize::try_from(c.ty.0).ok().and_then(|i| types.get(i)) else {
                    continue;
                };
                if ct.draws.step.is_some() || ct.draws.death.is_some() {
                    let at = self.type_at(&t.name);
                    self.error(
                        at,
                        format!(
                            "particle type `{}`: its step or death type `{}` has step or death particles of its own (not traced; N10)",
                            t.name, ct.name
                        ),
                    );
                }
            }
        }
    }

    fn hex(&mut self, at: usize, who: &str, s: &str) -> u32 {
        match s
            .strip_prefix('#')
            .filter(|h| h.len() == 6)
            .and_then(|h| u32::from_str_radix(h, 16).ok())
        {
            Some(v) => v,
            None => {
                self.error(at, format!("{who}: color `{s}` is not \"#rrggbb\""));
                0
            }
        }
    }
}

fn real(n: Number) -> Real {
    Real::new(n.0)
}
