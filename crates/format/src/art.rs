//! Room art: the background colour and background layers of `rooms/<id>.toml`, the tile layers
//! of `rooms/<id>.tiles.toml` and the tileset sidecars `backgrounds/<id>.toml`
//! (docs/format/v0.md, Rooms and Backgrounds). None of it reaches the sim: the client draws it
//! and the loader only checks it.

use std::ops::Range;

use serde::Deserialize;
use toml::Spanned;

use crate::diag::Diagnostic;
use crate::files::PackageFiles;
use crate::manifest::toml_error;
use crate::number::Number;

/// Tile size when a tileset has no sidecar or its sidecar no `tile_size`: the IWBTG standard.
pub const DEFAULT_TILE_SIZE: (u32, u32) = (32, 32);

/// Most background layers in one room (GM8 rooms have 8 background slots).
pub const MAX_BACKGROUNDS: usize = 8;

/// UTF-8 byte order mark, dropped before parsing like everywhere else in the loader.
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// One entry of a room's `backgrounds` list, as written.
#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BackgroundFile {
    /// Image id (`backgrounds/<id>.png`).
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub image: Spanned<String>,
    /// Drawn in front of instances and tiles (GM "foreground image"); default false.
    #[serde(default)]
    pub foreground: bool,
    /// Where the image's top-left corner sits in the room, in pixels; default `[0, 0]`.
    #[serde(default)]
    pub offset: Option<[Number; 2]>,
    /// Scroll speed in pixels per tick, horizontal and vertical; default `[0, 0]`.
    #[serde(default)]
    pub speed: Option<[Number; 2]>,
    /// Repeat the image horizontally and vertically; default `[true, true]`.
    #[serde(default)]
    pub tile: Option<[bool; 2]>,
    /// Stretch the image to the room's size; default false.
    #[serde(default)]
    pub stretch: bool,
}

/// What a room draws besides its instances and tiles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoomArt {
    /// `background_color` as sRGB bytes; `None`: the room clears to nothing of its own.
    pub color: Option<[u8; 3]>,
    /// Background layers in drawing order (GM slot order).
    pub backgrounds: Vec<Background>,
}

/// One background layer.
#[derive(Clone, Debug, PartialEq)]
pub struct Background {
    /// Image id (`backgrounds/<id>.png`).
    pub image: String,
    /// Where the image id is written in the room file.
    pub span: Range<usize>,
    /// Drawn after every instance and tile.
    pub foreground: bool,
    /// Top-left corner in the room at the room's start.
    pub offset: (f64, f64),
    /// Pixels per tick.
    pub speed: (f64, f64),
    /// Repeat horizontally, vertically.
    pub tile: (bool, bool),
    /// Stretched to the room's size.
    pub stretch: bool,
}

impl Background {
    /// A layer with GM's defaults: tiled both ways from the room's corner, still, behind.
    pub fn new(image: impl Into<String>, span: Range<usize>) -> Self {
        Self {
            image: image.into(),
            span,
            foreground: false,
            offset: (0.0, 0.0),
            speed: (0.0, 0.0),
            tile: (true, true),
            stretch: false,
        }
    }

    /// Where the image's top-left corner is `ticks` ticks after the room started.
    pub fn position(&self, ticks: u64) -> (f64, f64) {
        let t = ticks as f64;
        (self.offset.0 + self.speed.0 * t, self.offset.1 + self.speed.1 * t)
    }
}

/// `background_color` must be `#rrggbb`.
pub fn parse_color(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    let c = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some([c(0)?, c(2)?, c(4)?])
}

/// Builds a room's art from its keys. `background = "<id>"` is short for one default layer;
/// a room sets it or `backgrounds`, not both.
pub(crate) fn room_art(
    background: Option<&Spanned<String>>,
    color: Option<&Spanned<String>>,
    backgrounds: &[Spanned<BackgroundFile>],
    err: &impl Fn(Range<usize>, String) -> Diagnostic,
) -> Result<RoomArt, Diagnostic> {
    let color = match color {
        Some(c) => Some(parse_color(c.get_ref()).ok_or_else(|| {
            err(
                c.span(),
                format!("background_color `{}` is not `#rrggbb`", c.get_ref()),
            )
        })?),
        None => None,
    };
    if let (Some(one), Some(_)) = (background, backgrounds.first()) {
        return Err(err(
            one.span(),
            "set `background` or `backgrounds`, not both".to_owned(),
        ));
    }
    if backgrounds.len() > MAX_BACKGROUNDS {
        return Err(err(
            backgrounds[MAX_BACKGROUNDS].span(),
            format!("a room has at most {MAX_BACKGROUNDS} backgrounds"),
        ));
    }
    let pair = |v: Option<[Number; 2]>, d: (f64, f64)| v.map_or(d, |[a, b]| (a.0, b.0));
    let mut layers: Vec<Background> = background
        .map(|b| Background::new(b.get_ref().clone(), b.span()))
        .into_iter()
        .collect();
    for entry in backgrounds {
        let b = entry.get_ref();
        layers.push(Background {
            image: b.image.get_ref().clone(),
            span: b.image.span(),
            foreground: b.foreground,
            offset: pair(b.offset, (0.0, 0.0)),
            speed: pair(b.speed, (0.0, 0.0)),
            tile: b.tile.map_or((true, true), |[h, v]| (h, v)),
            stretch: b.stretch,
        });
    }
    Ok(RoomArt {
        color,
        backgrounds: layers,
    })
}

/// The art of room `id`, from `rooms/<id>.toml`. The loader has already checked the file.
pub fn load_room_art(files: &PackageFiles, id: &str) -> Result<RoomArt, Diagnostic> {
    let path = format!("rooms/{id}.toml");
    let text = text_of(files, &path)?;
    crate::rooms::parse_room(id, &path, &text).map(|r| r.art)
}

// -- Tilesets ---------------------------------------------------------------------------------

/// A tileset sidecar `backgrounds/<id>.toml`, as written.
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TilesetFile {
    /// Width and height of one tile in pixels; default `[32, 32]`. A tile without its own size
    /// (`src = [x, y]`) is this big, and a `run` without `step` repeats at this spacing.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<[u32; 2]>"))]
    pub tile_size: Option<Spanned<[u32; 2]>>,
    /// Pixels between neighbouring tiles in the image, horizontal and vertical; default
    /// `[0, 0]`. Editors use it to cut the image into a palette; drawing never does, since every
    /// tile names its own source.
    #[serde(default)]
    pub separation: Option<[u32; 2]>,
}

/// A tileset's settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tileset {
    /// Width and height of one tile.
    pub tile_size: (u32, u32),
    /// Pixels between tiles in the image.
    pub separation: (u32, u32),
}

impl Default for Tileset {
    fn default() -> Self {
        Self {
            tile_size: DEFAULT_TILE_SIZE,
            separation: (0, 0),
        }
    }
}

/// Parses a tileset sidecar. `path` is for diagnostics.
pub fn parse_tileset(path: &str, text: &str) -> Result<Tileset, Diagnostic> {
    let raw: TilesetFile = toml::from_str(text).map_err(|e| toml_error(path, text, &e))?;
    let mut set = Tileset::default();
    if let Some(size) = raw.tile_size {
        let [w, h] = *size.get_ref();
        if w == 0 || h == 0 {
            return Err(Diagnostic::error_at(
                path,
                text,
                size.span().start,
                "tile_size must be at least 1 x 1",
            ));
        }
        set.tile_size = (w, h);
    }
    if let Some([x, y]) = raw.separation {
        set.separation = (x, y);
    }
    Ok(set)
}

/// The settings of tileset `id`: its sidecar, or the defaults when it has none.
pub fn load_tileset(files: &PackageFiles, id: &str) -> Result<Tileset, Diagnostic> {
    let path = format!("backgrounds/{id}.toml");
    if !files.contains(&path) {
        return Ok(Tileset::default());
    }
    parse_tileset(&path, &text_of(files, &path)?)
}

// -- Tiles ------------------------------------------------------------------------------------

/// A tile file `rooms/<room_id>.tiles.toml`, as written. Decorative only: tiles never affect
/// the sim.
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TilesFile {
    /// Layers, back to front by their `depth`.
    pub layers: Vec<TileLayerFile>,
}

/// One tile layer, as written.
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TileLayerFile {
    /// GM tile layer depth: a larger depth is farther back, in the same order as instances.
    pub depth: Number,
    /// Tileset id (`backgrounds/<id>.png`).
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub tileset: Spanned<String>,
    /// The tiles of the layer.
    #[cfg_attr(feature = "schema", schemars(with = "Vec<TileFile>"))]
    pub tiles: Vec<Spanned<TileFile>>,
}

/// One tile or a run of identical tiles, as written.
#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TileFile {
    /// Position in the room (top-left).
    pub x: Number,
    /// Position in the room (top-left).
    pub y: Number,
    /// Source in the tileset image: `[x, y]` for a tile of the tileset's `tile_size`, or
    /// `[x, y, w, h]` for a tile of its own size.
    #[cfg_attr(feature = "schema", schemars(with = "SrcShape"))]
    pub src: Spanned<Vec<Number>>,
    /// `[columns, rows]`: a grid of the same tile; default `[1, 1]`.
    pub run: Option<[u32; 2]>,
    /// Spacing of a `run`; default the tile's size.
    pub step: Option<[Number; 2]>,
}

/// The two forms of a tile's `src`.
#[cfg(feature = "schema")]
#[derive(schemars::JsonSchema)]
#[serde(untagged)]
#[allow(dead_code, reason = "only describes the schema")]
enum SrcShape {
    /// `[x, y]`: a tile of the tileset's `tile_size`.
    At([Number; 2]),
    /// `[x, y, w, h]`: a tile of its own size.
    Rect([Number; 4]),
}

/// One tile layer, with every size resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct TileLayer {
    /// Depth: larger is farther back.
    pub depth: f64,
    /// Tileset id.
    pub tileset: String,
    /// Tiles in file order.
    pub tiles: Vec<Tile>,
}

/// One tile or run of tiles.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    /// Top-left of the first tile in the room.
    pub pos: (f64, f64),
    /// Top-left of the source rectangle in the tileset image.
    pub src: (f64, f64),
    /// Width and height of the source rectangle (and of the tile in the room).
    pub size: (f64, f64),
    /// Columns, rows.
    pub run: (u32, u32),
    /// Spacing of the run.
    pub step: (f64, f64),
}

impl Tile {
    /// The top-left corners of every tile of the run, row by row.
    pub fn positions(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        let (cols, rows) = self.run;
        (0..rows).flat_map(move |r| {
            (0..cols).map(move |c| {
                (
                    self.pos.0 + f64::from(c) * self.step.0,
                    self.pos.1 + f64::from(r) * self.step.1,
                )
            })
        })
    }
}

/// Parses a tile file. `tile_size` gives a tileset's tile size; `path` is for diagnostics.
pub fn parse_tiles(
    path: &str,
    text: &str,
    mut tile_size: impl FnMut(&str) -> (u32, u32),
) -> Result<Vec<TileLayer>, Diagnostic> {
    let raw: TilesFile = toml::from_str(text).map_err(|e| toml_error(path, text, &e))?;
    let err = |span: Range<usize>, msg: &str| Diagnostic::error_at(path, text, span.start, msg);
    let mut layers = Vec::with_capacity(raw.layers.len());
    for layer in raw.layers {
        let (tw, th) = tile_size(layer.tileset.get_ref());
        let mut tiles = Vec::with_capacity(layer.tiles.len());
        for entry in &layer.tiles {
            let t = entry.get_ref();
            let (src, size) = match t.src.get_ref().as_slice() {
                [x, y] => ((x.0, y.0), (f64::from(tw), f64::from(th))),
                [x, y, w, h] => ((x.0, y.0), (w.0, h.0)),
                _ => return Err(err(t.src.span(), "src must be [x, y] or [x, y, w, h]")),
            };
            if !(size.0 > 0.0 && size.1 > 0.0) {
                return Err(err(t.src.span(), "a tile must be at least 1 x 1"));
            }
            let run = t.run.map_or((1, 1), |[c, r]| (c, r));
            if run.0 == 0 || run.1 == 0 {
                return Err(err(entry.span(), "run must be at least [1, 1]"));
            }
            tiles.push(Tile {
                pos: (t.x.0, t.y.0),
                src,
                size,
                run,
                step: t.step.map_or(size, |[x, y]| (x.0, y.0)),
            });
        }
        layers.push(TileLayer {
            depth: layer.depth.0,
            tileset: layer.tileset.into_inner(),
            tiles,
        });
    }
    Ok(layers)
}

/// The tile layers of room `id`: `rooms/<id>.tiles.toml` with sizes from the tileset sidecars.
/// A room without a tile file has none. A broken sidecar falls back to the default size here;
/// [`check`] reports it.
pub fn load_tiles(files: &PackageFiles, id: &str) -> Result<Vec<TileLayer>, Diagnostic> {
    let path = format!("rooms/{id}.tiles.toml");
    if !files.contains(&path) {
        return Ok(Vec::new());
    }
    let text = text_of(files, &path)?;
    parse_tiles(&path, &text, |set| {
        load_tileset(files, set).unwrap_or_default().tile_size
    })
}

/// Checks every tile file and tileset sidecar: syntax, sizes, and that each tileset has its
/// image. Room background images are checked with the room ([`crate::load`]).
pub fn check(files: &PackageFiles, out: &mut Vec<Diagnostic>) {
    for (path, _) in files.iter() {
        if let Some(id) = path
            .strip_prefix("backgrounds/")
            .and_then(|s| s.strip_suffix(".toml"))
            .filter(|s| !s.contains('/'))
            && let Err(d) = load_tileset(files, id)
        {
            out.push(d);
        }
        let is_tiles = path
            .strip_prefix("rooms/")
            .and_then(|s| s.strip_suffix(".tiles.toml"))
            .is_some_and(|s| !s.contains('/'));
        if !is_tiles {
            continue;
        }
        let text = match text_of(files, path) {
            Ok(t) => t,
            Err(d) => {
                out.push(d);
                continue;
            }
        };
        let raw: TilesFile = match toml::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                out.push(toml_error(path, &text, &e));
                continue;
            }
        };
        if let Err(d) = parse_tiles(path, &text, |_| DEFAULT_TILE_SIZE) {
            out.push(d);
        }
        for layer in &raw.layers {
            let id = layer.tileset.get_ref();
            if !files.contains(&format!("backgrounds/{id}.png")) {
                out.push(Diagnostic::error_at(
                    path,
                    &text,
                    layer.tileset.span().start,
                    format!("tileset `{id}` has no image `backgrounds/{id}.png`"),
                ));
            }
        }
    }
}

/// A package file as text, without a byte order mark.
fn text_of(files: &PackageFiles, path: &str) -> Result<String, Diagnostic> {
    let bytes = files
        .get(path)
        .ok_or_else(|| Diagnostic::file_error(path, format!("the package has no `{path}`")))?;
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    String::from_utf8(bytes.to_vec()).map_err(|_| Diagnostic::file_error(path, "file is not valid UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tile_takes_its_size_from_the_tileset_unless_it_has_its_own() {
        let text = "layers = [\n  { depth = 1000000, tileset = \"t\", tiles = [\n    { x = 0, y = 576, src = [32, 0], run = [25, 1] },\n    { x = 8, y = 8, src = [0, 0, 16, 8], run = [2, 1], step = [20, 0] },\n  ] },\n]\n";
        let layers = parse_tiles("rooms/a.tiles.toml", text, |_| (32, 16)).unwrap();
        let t = &layers[0].tiles;
        assert_eq!(layers[0].depth, 1_000_000.0);
        assert_eq!(
            (t[0].size, t[0].step, t[0].run),
            ((32.0, 16.0), (32.0, 16.0), (25, 1))
        );
        assert_eq!((t[1].size, t[1].step), ((16.0, 8.0), (20.0, 0.0)));
        assert_eq!(t[1].positions().collect::<Vec<_>>(), [(8.0, 8.0), (28.0, 8.0)]);
    }

    #[test]
    fn bad_tiles_are_errors_at_their_place() {
        let bad = |tile: &str| {
            let text = format!("layers = [{{ depth = 0, tileset = \"t\", tiles = [{tile}] }}]\n");
            parse_tiles("rooms/a.tiles.toml", &text, |_| (32, 32))
                .unwrap_err()
                .message
        };
        assert!(bad("{ x = 0, y = 0, src = [1, 2, 3] }").contains("src must be"));
        assert!(bad("{ x = 0, y = 0, src = [0, 0, 0, 4] }").contains("at least 1 x 1"));
        assert!(bad("{ x = 0, y = 0, src = [0, 0], run = [0, 1] }").contains("run must be"));
        assert!(bad("{ x = 0, y = 0, src = [0, 0], size = 3 }").contains("unknown field"));
    }

    #[test]
    fn tileset_sidecar() {
        let t = parse_tileset(
            "backgrounds/t.toml",
            "tile_size = [16, 24]\nseparation = [1, 2]\n",
        )
        .unwrap();
        assert_eq!(
            t,
            Tileset {
                tile_size: (16, 24),
                separation: (1, 2)
            }
        );
        assert_eq!(
            parse_tileset("backgrounds/t.toml", "").unwrap(),
            Tileset::default()
        );
        let e = parse_tileset("backgrounds/t.toml", "tile_size = [0, 32]\n").unwrap_err();
        assert!(e.message.contains("at least 1 x 1"), "{e}");
        assert!(parse_tileset("backgrounds/t.toml", "offset = [0, 0]\n").is_err());
    }

    #[test]
    fn a_scrolling_background_moves_by_its_speed_each_tick() {
        let mut b = Background::new("sky", 0..0);
        b.offset = (10.0, -4.0);
        b.speed = (1.0, 0.5);
        assert_eq!(b.position(0), (10.0, -4.0));
        assert_eq!(b.position(4), (14.0, -2.0));
    }

    #[test]
    fn colors() {
        assert_eq!(parse_color("#ff8000"), Some([255, 128, 0]));
        assert_eq!(parse_color("ff8000"), None);
        assert_eq!(parse_color("#ff80"), None);
        assert_eq!(parse_color("#ff80zz"), None);
    }
}
