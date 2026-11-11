//! Loading a package folder or `.iwbz` file for the client: files, engine pack, the game
//! definition and the sprite images. No Bevy in here, so it is tested like any other code.

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
    /// Every file, with the engine pack bundled.
    pub files: PackageFiles,
    /// The loaded game.
    pub def: Arc<GameDef>,
    /// The package's `gameplay_hash`, for the replay header (`replays.rs`).
    pub gameplay_hash: [u8; 32],
}

/// One decoded sprite strip.
pub struct SpriteImage {
    /// Strip width and height in pixels.
    pub size: (u32, u32),
    /// RGBA8 pixels, row by row.
    pub rgba: Vec<u8>,
}

/// The package command-line arguments.
#[derive(clap::Args, Debug)]
pub struct PackageArgs {
    /// Package folder (a folder with a game.toml) or `.iwbz` file. Not needed with
    /// `--ui-gallery`, `--bind` or `--show-bindings`.
    #[arg(required_unless_present_any = ["ui_gallery", "card_preview", "bind", "show_bindings"])]
    pub package: Option<std::path::PathBuf>,
    /// Load converter output: unmapped script kinds become inert.
    #[arg(long)]
    pub trace_mode: bool,
}

/// Opens the package at `dir` and loads it. A folder is read and gets its engine pack bundled
/// from `../_engines`; an `.iwbz` file (docs/format/iwbz.md) is unpacked, refusing anything a
/// package may not hold, and already carries its pack. `trace_mode` is for research converter
/// output.
pub fn open(dir: &Path, trace_mode: bool) -> Result<Package, OpenError> {
    let files = if is_iwbz(dir) {
        read_iwbz(dir)?
    } else {
        let mut files =
            PackageFiles::load_dir(dir).map_err(|e| OpenError::Files(format!("{}: {e}", dir.display())))?;
        iwbtg_format::engine_pack::bundle_sibling(dir, &mut files)
            .map_err(|e| OpenError::Files(format!("{}: {e}", dir.display())))?;
        files
    };
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
    Ok(Package {
        files,
        def: Arc::new(def),
        gameplay_hash,
    })
}

/// True when `path` is an `.iwbz` file (by its extension) rather than a package folder.
pub fn is_iwbz(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(iwbtg_format::iwbz::EXTENSION))
        && !path.is_dir()
}

/// Reads and unpacks an `.iwbz` file; one over the size limit is refused before it is read.
pub fn read_iwbz(path: &Path) -> Result<PackageFiles, OpenError> {
    use iwbtg_format::iwbz;
    let err = |e: String| OpenError::Files(format!("{}: {e}", path.display()));
    let len = std::fs::metadata(path).map_err(|e| err(e.to_string()))?.len();
    if len > iwbz::MAX_ARCHIVE_BYTES {
        return Err(err(format!("over {} bytes", iwbz::MAX_ARCHIVE_BYTES)));
    }
    let bytes = std::fs::read(path).map_err(|e| err(e.to_string()))?;
    iwbz::unpack(&bytes).map_err(|e| err(e.to_string()))
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
        let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("demo.pkg");
        let p = open(&demo, false).unwrap();
        let images = sprite_images(&p.files);
        assert_eq!(images.len(), p.def.sprites.len());
        assert!(images.iter().all(Option::is_some));
    }

    #[test]
    fn an_iwbz_opens_like_its_folder_and_a_bad_one_is_refused() {
        let dir = std::env::temp_dir().join(format!("iwbtg-client-iwbz-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("demo-clear.pkg");
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
