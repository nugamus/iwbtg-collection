//! Loading a package folder or `.iwbz` file for the client: files, engine pack, the game
//! definition and the sprite images. No Bevy in here, so it is tested like any other code.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use iwbtg_format::load::load_report;
use iwbtg_format::{LoadMode, PackageFiles};
use iwbtg_sim::def::GameDef;

/// Why a package did not open.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// A file could not be read.
    #[error("{0}")]
    Files(String),
    /// The package has errors; one line each, with file, line and column.
    #[error("the package has errors:\n{}", .0.join("\n"))]
    Invalid(Vec<String>),
}

/// An opened package.
pub struct Package {
    /// Every file, with the engine pack bundled. Shared behind an [`Arc`]: the run's readers
    /// (room art, the HUD, the results, the backdrop) clone this handle, not the bytes.
    pub files: Arc<PackageFiles>,
    /// The loaded game.
    pub def: Arc<GameDef>,
    /// The package's `gameplay_hash`, for the replay header (`replays.rs`).
    pub gameplay_hash: [u8; 32],
    /// The package's `sounds/<id>.ogg` and `music/<id>.ogg` by sound name, as shared handles.
    /// Built once here; each run's `audio::Sounds` clones the handles instead of copying every
    /// OGG (`crates/format` owns the `Vec<u8>`, so one copy at open is what `Arc<[u8]>` needs).
    pub sound_bytes: HashMap<String, Arc<[u8]>>,
}

/// One decoded sprite strip.
pub struct SpriteImage {
    /// Strip width and height in pixels.
    pub size: (u32, u32),
    /// RGBA8 pixels, row by row.
    pub rgba: Vec<u8>,
}

/// The package command-line arguments.
#[derive(clap::Args, Debug, Default)]
pub struct PackageArgs {
    /// Package folder (a folder with a game.toml) or `.iwbz` file. Not needed with `--select`,
    /// `--ui-gallery`, `--card-preview`, `--controls`, `--bind` or `--show-bindings`.
    #[arg(required_unless_present_any = ["select", "ui_gallery", "card_preview", "controls", "bind", "show_bindings"])]
    pub package: Option<std::path::PathBuf>,
    /// Load converter output: unmapped script kinds become inert.
    #[arg(long)]
    pub trace_mode: bool,
}

/// The package's place in the module list (main.rs): its flags, and the "nothing to play" check
/// once the standalone modes have had their turn.
pub struct Registration;

impl crate::registration::ClientModule for Registration {
    type Args = PackageArgs;

    fn standalone(_args: &PackageArgs, startup: &crate::registration::Startup) -> Option<Result<(), String>> {
        // Clap requires the package unless `--select`, `--ui-gallery`, `--bind` or
        // `--show-bindings` is given; with only the bindings flags there is nothing to play.
        (startup.package.is_none() && startup.library.is_none()).then_some(Ok(()))
    }
}

/// Opens the package at `dir` and loads it. A folder is read and gets its engine pack bundled
/// from `../_engines`; an `.iwbz` file (docs/format/iwbz.md) is unpacked, refusing anything a
/// package may not hold, and already carries its pack. `trace_mode` is for research converter
/// output.
pub fn open(dir: &Path, trace_mode: bool) -> Result<Package, OpenError> {
    if is_iwbz(dir) {
        let err = |e: String| OpenError::Files(format!("{}: {e}", dir.display()));
        // Refuse an over-limit file before reading it; `from_iwbz_bytes` checks bytes that are
        // already in memory (the browser's fetch) too.
        let len = std::fs::metadata(dir).map_err(|e| err(e.to_string()))?.len();
        if len > iwbtg_format::iwbz::MAX_ARCHIVE_BYTES {
            return Err(err(format!(
                "over {} bytes",
                iwbtg_format::iwbz::MAX_ARCHIVE_BYTES
            )));
        }
        let bytes = std::fs::read(dir).map_err(|e| err(e.to_string()))?;
        return from_iwbz_bytes(&bytes, trace_mode, &dir.display());
    }
    let mut files =
        PackageFiles::load_dir(dir).map_err(|e| OpenError::Files(format!("{}: {e}", dir.display())))?;
    iwbtg_format::engine_pack::bundle_sibling(dir, &mut files)
        .map_err(|e| OpenError::Files(format!("{}: {e}", dir.display())))?;
    from_files(files, trace_mode)
}

/// Loads a package from an `.iwbz` already in memory: the browser fetches one over HTTP
/// (docs/architecture/client/web.md). `source` names it in messages, a path or the page's URL.
pub fn from_iwbz_bytes(
    bytes: &[u8],
    trace_mode: bool,
    source: &dyn std::fmt::Display,
) -> Result<Package, OpenError> {
    let files = unpack_iwbz(bytes).map_err(|e| OpenError::Files(format!("{source}: {e}")))?;
    from_files(files, trace_mode)
}

/// Unpacks an `.iwbz` already in memory, refusing one over the archive limit. [`from_iwbz_bytes`]
/// adds the game definition; the browser times the two steps apart for its startup measurement
/// (#1489, `crates/client/web/measure_startup.py`).
pub fn unpack_iwbz(bytes: &[u8]) -> Result<PackageFiles, String> {
    use iwbtg_format::iwbz;
    if bytes.len() as u64 > iwbz::MAX_ARCHIVE_BYTES {
        return Err(format!("over {} bytes", iwbz::MAX_ARCHIVE_BYTES));
    }
    iwbz::unpack(bytes).map_err(|e| e.to_string())
}

/// Builds a package from files already in memory: the game definition and its `gameplay_hash`.
/// [`open`] reads them from a folder or `.iwbz`; the browser from its `fetch`.
pub fn from_files(files: PackageFiles, trace_mode: bool) -> Result<Package, OpenError> {
    let mode = if trace_mode {
        LoadMode::Trace {
            unmapped_kinds_inert: true,
        }
    } else {
        LoadMode::Strict
    };
    // `load_game` plus the hash, which a load with an error does not have.
    let report = load_report(&files, mode);
    let (Some(def), Some(gameplay_hash)) = (report.def, report.gameplay_hash) else {
        return Err(OpenError::Invalid(
            report.diagnostics.iter().map(ToString::to_string).collect(),
        ));
    };
    // One pass over the sound files when the package opens; a run only clones the handles.
    let sound_bytes = crate::audio::sound_bytes(&files);
    Ok(Package {
        files: Arc::new(files),
        def: Arc::new(def),
        gameplay_hash,
        sound_bytes,
    })
}

/// True when `path` is an `.iwbz` file (by its extension) rather than a package folder.
pub fn is_iwbz(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(iwbtg_format::iwbz::EXTENSION))
        && !path.is_dir()
}

/// Reads what a game-select preview draws from an `.iwbz` (#1022): `game.toml`, the key art
/// images, the first ranked room and the background images its art uses, and the `cherry`
/// sprite. Nothing else is read, and the archive is seeked rather than loaded whole (#1591), so
/// a large package lists without decoding its assets; [`open`] still validates the whole
/// package when the game starts.
pub fn read_iwbz(path: &Path) -> Result<PackageFiles, OpenError> {
    use iwbtg_format::iwbz;
    let err = |e: String| OpenError::Files(format!("{}: {e}", path.display()));
    let file = std::fs::File::open(path).map_err(|e| err(e.to_string()))?;
    let mut archive = iwbz::Archive::open(file).map_err(|e| err(e.to_string()))?;
    preview_from_iwbz(&mut archive).map_err(|e| err(e.to_string()))
}

/// The manifest only, for game select's first pass (#1593): a folder's `game.toml`, or an
/// archive's through the seek-based reader. No art or room data is read, so a large library
/// lists from its metadata before any preview is built.
pub fn read_manifest(path: &Path) -> Result<PackageFiles, OpenError> {
    if is_iwbz(path) {
        let err = |e: String| OpenError::Files(format!("{}: {e}", path.display()));
        let file = std::fs::File::open(path).map_err(|e| err(e.to_string()))?;
        let mut archive = iwbtg_format::iwbz::Archive::open(file).map_err(|e| err(e.to_string()))?;
        return archive
            .read_selected(|name| name == "game.toml")
            .map_err(|e| err(e.to_string()));
    }
    let mut files = PackageFiles::new();
    let bytes = std::fs::read(path.join("game.toml"))
        .map_err(|e| OpenError::Files(format!("{}: {e}", path.display())))?;
    let _ = files.insert("game.toml", bytes);
    Ok(files)
}

/// The selective reads behind [`read_iwbz`]: the manifest, then the files its key art, first
/// ranked room and cherry need, then the background images that room's art names. A manifest
/// that does not parse leaves only `game.toml`, which the caller treats as unreadable. The
/// directory is checked once by [`iwbtg_format::iwbz::Archive::open`]; each pass reads only the
/// entries it needs.
fn preview_from_iwbz<R: std::io::Read + std::io::Seek>(
    archive: &mut iwbtg_format::iwbz::Archive<R>,
) -> Result<PackageFiles, iwbtg_format::iwbz::IwbzError> {
    use std::collections::BTreeSet;

    let mut files = archive.read_selected(|name| name == "game.toml")?;
    let Some(manifest) = files
        .get("game.toml")
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|t| iwbtg_format::Manifest::parse(t).ok())
    else {
        return Ok(files);
    };
    let room = iwbtg_format::key_art::first_ranked_room(&manifest);
    let mut wanted: BTreeSet<String> = ["game.toml", "sprites/cherry.toml", "sprites/cherry.png"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    if let Some(section) = &manifest.key_art {
        wanted.extend(
            section
                .image
                .iter()
                .chain(section.maps.values())
                .map(|path| path.get_ref().clone()),
        );
    }
    wanted.insert(format!("rooms/{room}.toml"));
    files = archive.read_selected(|name| wanted.contains(name))?;
    if let Ok(art) = iwbtg_format::art::load_room_art(&files, room) {
        let before = wanted.len();
        wanted.extend(
            art.backgrounds
                .iter()
                .filter(|b| !b.foreground)
                .map(|b| format!("backgrounds/{}.png", b.image)),
        );
        if wanted.len() > before {
            files = archive.read_selected(|name| wanted.contains(name))?;
        }
    }
    Ok(files)
}

/// The image of every sprite of the game, in `SpriteId` order. A sprite without a picture (no
/// frames) or with an undecodable one is `None`.
pub fn sprite_images(files: &PackageFiles) -> Vec<Option<SpriteImage>> {
    iwbtg_format::sprites::sprite_pngs(files)
        .iter()
        .map(|(_, path)| files.get(path).and_then(decode_png))
        .collect()
}

pub fn decode_png(bytes: &[u8]) -> Option<SpriteImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let px = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => px
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(SpriteImage {
        size: (info.width, info.height),
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn golden() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../sim/tests/golden/one_room.pkg")
    }

    #[test]
    fn the_golden_package_opens() {
        let p = open(&golden(), false).unwrap();
        assert!(p.def.room(p.def.start_room).is_some());
        assert_eq!(sprite_images(&p.files).len(), p.def.sprites.len());
    }

    #[test]
    fn the_demo_package_opens_and_every_sprite_decodes() {
        let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/input.pkg");
        let p = open(&demo, false).unwrap();
        let images = sprite_images(&p.files);
        assert_eq!(images.len(), p.def.sprites.len());
        assert!(images.iter().all(Option::is_some));
    }

    /// #852: the showcase opens, every sprite decodes, and a scripted route clears it without a
    /// death: three rooms, two stage splits, then the star's `run_end`. Each jump is
    /// `(x, hold, double_after, double_hold)`: once the kid's x reaches `x` in that room, jump
    /// held for `hold` ticks, then (when `double_after` is not 0) a double jump that many ticks
    /// after the first, held `double_hold` ticks. Right is held all the way.
    #[test]
    fn the_showcase_clears_with_a_scripted_route() {
        use iwbtg_sim::input::{Action, InputFrame};
        use iwbtg_sim::sim::{Sim, SimOptions};
        use iwbtg_sim::sound::DllMode;

        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("showcase.pkg");
        let p = open(&dir, false).unwrap();
        assert!(sprite_images(&p.files).iter().all(Option::is_some));
        let route: [&[(f64, u32, u32, u32)]; 3] = [
            &[(310.0, 20, 0, 0), (440.0, 16, 18, 20), (530.0, 20, 0, 0)],
            &[
                (215.0, 20, 0, 0),
                (380.0, 20, 0, 0),
                (445.0, 16, 20, 20),
                (1180.0, 20, 0, 0),
                (1296.0, 20, 0, 0),
                (1424.0, 20, 0, 0),
                (1640.0, 16, 18, 20),
                (1905.0, 16, 20, 20),
            ],
            &[(280.0, 20, 0, 0), (470.0, 20, 0, 0)],
        ];
        let options = SimOptions {
            start: iwbtg_sim::room::StartPoint::NewGame { difficulty: 0 },
            seed: 0,
            dll_mode: DllMode::Model,
            mode: iwbtg_sim::stripped::PlayMode::Original,
            map: None,
        };
        let mut sim = Sim::new(Arc::clone(&p.def), options).unwrap();
        let (right, jump) = (Action::Right.bit(), Action::Jump.bit());
        let (mut rooms, mut next, mut since_jump) = (Vec::new(), 0usize, None::<(u32, usize)>);
        let mut prev_y = None;
        for t in 0..4000u32 {
            let w = sim.world();
            if w.timing.ended {
                assert_eq!(rooms.len(), 3, "the clear is in the third room");
                assert_eq!(w.timing.stages_done, 2, "both stage splits fired");
                return;
            }
            if rooms.last() != Some(&w.room) {
                rooms.push(w.room);
                (next, since_jump) = (0, None);
            }
            let kid = w.kid(&sim.rules().tick).and_then(|id| w.get(id));
            let Some(kid) = kid else {
                assert!(t < 2 || w.tick == 0, "no kid at tick {t} in room {}", rooms.len());
                sim.step(InputFrame::default());
                continue;
            };
            let (x, y) = (kid.m.x().get(), kid.m.y().get());
            let jumps = route[rooms.len() - 1];
            let mut frame = InputFrame {
                held: right,
                ..InputFrame::default()
            };
            if t == 0 {
                frame.pressed |= right;
            }
            let grounded = prev_y == Some(y);
            prev_y = Some(y);
            if since_jump.is_none() && grounded && next < jumps.len() && x >= jumps[next].0 {
                since_jump = Some((0, next));
                next += 1;
            }
            if let Some((n, i)) = since_jump {
                let (_, hold, double_after, double_hold) = jumps[i];
                let first = n < hold;
                let second = double_after > 0 && n >= double_after && n < double_after + double_hold;
                if first || second {
                    frame.held |= jump;
                }
                if n == 0 || (double_after > 0 && n == double_after) {
                    frame.pressed |= jump;
                }
                if n + 1 == hold || (double_after > 0 && n + 1 == double_after + double_hold) {
                    frame.released |= jump;
                }
                let end = if double_after > 0 {
                    double_after + double_hold
                } else {
                    hold
                };
                since_jump = (n + 1 < end).then_some((n + 1, i));
            }
            let stages = sim.world().timing.stages_done;
            sim.step(frame);
            let w = sim.world();
            assert!(
                w.kid(&sim.rules().tick).is_some() || w.room != *rooms.last().unwrap(),
                "the kid died at tick {t} in room {} near ({x}, {y}), stages {stages}",
                rooms.len()
            );
        }
        panic!("no clear in 4000 ticks; reached room {}", rooms.len());
    }

    /// #1140: R after a death goes through the empty boot room before the boot tick loads the
    /// save, so the retry leaves one kid at the saved spot. With no boot room, the courtyard's
    /// `kid_start` made a kid during `game_restart` and the load made a second one beside it.
    #[test]
    fn a_showcase_retry_leaves_one_kid() {
        use iwbtg_sim::input::{Action, InputFrame};
        use iwbtg_sim::sim::{Sim, SimOptions};
        use iwbtg_sim::sound::DllMode;
        use iwbtg_sim::world::KindState;

        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("showcase.pkg");
        let p = open(&dir, false).unwrap();
        let options = SimOptions {
            start: iwbtg_sim::room::StartPoint::NewGame { difficulty: 0 },
            seed: 0,
            dll_mode: DllMode::Model,
            mode: iwbtg_sim::stripped::PlayMode::Original,
            map: None,
        };
        let mut sim = Sim::new(Arc::clone(&p.def), options).unwrap();
        // The first tick runs the start room and the kid's first-play save.
        sim.step(InputFrame::default());
        assert!(sim.world().kid(&sim.rules().tick).is_some());

        let suicide = Action::Suicide.bit();
        sim.step(InputFrame {
            held: suicide,
            pressed: suicide,
            released: 0,
        });
        sim.step(InputFrame {
            held: 0,
            pressed: 0,
            released: suicide,
        });
        assert_eq!(sim.world().engine.run.deaths, 1);
        assert!(sim.world().kid(&sim.rules().tick).is_none());
        for _ in 0..4 {
            sim.step(InputFrame::default());
        }

        // R: the key press event starts the retry; the next tick's boot load enters the saved
        // room.
        let restart = Action::Restart.bit();
        sim.step(InputFrame {
            held: restart,
            pressed: restart,
            released: 0,
        });
        sim.step(InputFrame {
            held: 0,
            pressed: 0,
            released: restart,
        });
        for _ in 0..3 {
            sim.step(InputFrame::default());
        }

        let kids: Vec<_> = sim
            .world()
            .iter_alive()
            .filter(|i| matches!(i.state, KindState::Kid(_)))
            .collect();
        assert_eq!(
            kids.len(),
            1,
            "R leaves one kid, not the start kid and the loaded one"
        );
        let slot = sim.world().save.clone().expect("the start room's save");
        assert_eq!(sim.world().room, slot.room);
        // The kid is made at the saved spot (quantized) and falls under gravity for the ticks
        // since; x does not move.
        let (kx, ky) = (kids[0].m.x().get(), kids[0].m.y().get());
        let (sx, sy) = (slot.x.get(), slot.y.get());
        assert!(
            (kx - sx).abs() <= 1.0 && (ky - sy).abs() <= 8.0,
            "the loaded kid at ({kx}, {ky}) is not at the save ({sx}, {sy})"
        );
    }

    #[test]
    fn an_iwbz_opens_like_its_folder_and_a_bad_one_is_refused() {
        let dir = std::env::temp_dir().join(format!("iwbtg-client-iwbz-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let folder = open(&demo, false).unwrap();
        let file = dir.join("demo.IWBZ");
        std::fs::write(&file, iwbtg_format::iwbz::pack(&folder.files).unwrap()).unwrap();
        let packed = open(&file, false).unwrap();
        assert_eq!(packed.gameplay_hash, folder.gameplay_hash);
        assert_eq!(packed.files.content_hash(), folder.files.content_hash());
        let bad = dir.join("bad.iwbz");
        std::fs::write(&bad, b"PK not really").unwrap();
        assert!(matches!(open(&bad, false), Err(OpenError::Files(_))));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_iwbz_preview_reads_the_key_art_and_room_but_not_the_sprites() {
        let dir = std::env::temp_dir().join(format!("iwbtg-client-preview-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/maps.pkg");
        let full = open(&folder, false).unwrap();
        let file = dir.join("maps.iwbz");
        std::fs::write(&file, iwbtg_format::iwbz::pack(&full.files).unwrap()).unwrap();
        let preview = read_iwbz(&file).unwrap();
        assert!(preview.contains("game.toml"));
        assert!(preview.contains("art/key.png"), "the key art is read");
        assert!(
            preview.contains("rooms/goal.toml"),
            "the first ranked room is read (the fixture's ranked start, #43)"
        );
        assert!(!preview.contains("sprites/kid.png"), "unrelated assets are not");
        assert!(preview.len() < full.files.len());
        let text = std::str::from_utf8(preview.get("game.toml").unwrap()).unwrap();
        let manifest = iwbtg_format::Manifest::parse(text).unwrap();
        assert_eq!(
            iwbtg_format::key_art::resolve(&preview, &manifest, None),
            iwbtg_format::key_art::resolve(&full.files, &manifest, None),
            "the preview resolves the same key art as the full package"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_iwbz_preview_reads_the_first_room_backgrounds() {
        let dir = std::env::temp_dir().join(format!("iwbtg-client-preview-bg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("showcase.pkg");
        let full = open(&folder, false).unwrap();
        let file = dir.join("showcase.iwbz");
        std::fs::write(&file, iwbtg_format::iwbz::pack(&full.files).unwrap()).unwrap();
        let preview = read_iwbz(&file).unwrap();
        assert!(
            preview.contains("rooms/courtyard.toml"),
            "the first ranked room is read"
        );
        for background in ["sky", "clouds", "hills"] {
            assert!(
                preview.contains(&format!("backgrounds/{background}.png")),
                "the {background} background is read"
            );
        }
        assert!(!preview.contains("music/night.ogg"), "music is not");
        assert!(!preview.contains("objects/fireflies.toml"), "objects are not");
        let text = std::str::from_utf8(preview.get("game.toml").unwrap()).unwrap();
        let manifest = iwbtg_format::Manifest::parse(text).unwrap();
        let art = iwbtg_format::key_art::resolve(&preview, &manifest, None);
        assert_eq!(
            art,
            iwbtg_format::key_art::resolve(&full.files, &manifest, None),
            "the preview resolves the same room-background key art"
        );
        assert!(matches!(art, iwbtg_format::key_art::KeyArt::RoomBackground(_)));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fetched_iwbz_bytes_load_like_the_file() {
        let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clear.pkg");
        let folder = open(&demo, false).unwrap();
        let bytes = iwbtg_format::iwbz::pack(&folder.files).unwrap();
        let fetched = from_iwbz_bytes(&bytes, false, &"demo.iwbz").unwrap();
        assert_eq!(fetched.gameplay_hash, folder.gameplay_hash);
        assert_eq!(fetched.files.content_hash(), folder.files.content_hash());
        assert!(matches!(
            from_iwbz_bytes(b"PK not really", false, &"bad.iwbz"),
            Err(OpenError::Files(_))
        ));
    }

    #[test]
    fn a_missing_folder_is_an_error() {
        assert!(matches!(
            open(Path::new("no/such/package"), false),
            Err(OpenError::Files(_))
        ));
    }

    #[test]
    fn a_broken_package_lists_its_errors() {
        let dir = std::env::temp_dir().join(format!("iwbtg-client-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("game.toml"), "id = \"x\"\n").unwrap();
        let Err(OpenError::Invalid(lines)) = open(&dir, false) else {
            panic!("expected errors");
        };
        assert!(lines.iter().any(|l| l.contains("game.toml")), "{lines:?}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
