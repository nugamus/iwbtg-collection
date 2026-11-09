//! Loading a package folder for the client: files, engine pack, the game definition and the
//! sprite images. No Bevy in here, so it is tested like any other code.

use std::path::Path;
use std::sync::Arc;

use iwbtg_format::{LoadMode, PackageFiles, load_game};
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
}

/// One decoded sprite strip.
pub struct SpriteImage {
    /// Strip width and height in pixels.
    pub size: (u32, u32),
    /// RGBA8 pixels, row by row.
    pub rgba: Vec<u8>,
}

/// Opens the package folder `dir`: reads it, bundles its engine pack from `../_engines` and
/// loads it. `trace_mode` is for research converter output.
/// The package command-line arguments.
#[derive(clap::Args, Debug)]
pub struct PackageArgs {
    /// Package folder (a folder with a game.toml). Optional only with `--bind`.
    pub package: Option<std::path::PathBuf>,
    /// Load converter output: unmapped script kinds become inert.
    #[arg(long)]
    pub trace_mode: bool,
}

pub fn open(dir: &Path, trace_mode: bool) -> Result<Package, OpenError> {
    let mut files =
        PackageFiles::load_dir(dir).map_err(|e| OpenError::Files(format!("{}: {e}", dir.display())))?;
    iwbtg_format::engine_pack::bundle_sibling(dir, &mut files)
        .map_err(|e| OpenError::Files(format!("{}: {e}", dir.display())))?;
    let mode = if trace_mode {
        LoadMode::Trace {
            unmapped_kinds_inert: true,
        }
    } else {
        LoadMode::Strict
    };
    let def = load_game(&files, mode)
        .map_err(|diags| OpenError::Invalid(diags.iter().map(ToString::to_string).collect()))?;
    Ok(Package {
        files,
        def: Arc::new(def),
    })
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
