//! What game select lists (#149): each package's title, creators and maps from its `game.toml`,
//! its key art and its delicious fruit. A folder is read in two passes (#1593): every manifest
//! first, so ordering, search and selection work at once, then the preview strips and fruits
//! around the pick, built off the UI thread and bounded to [`PREVIEW_RADIUS`] rows each side.
//! Your PB per map and whether you cleared the game come from the stats book each time the
//! screen opens ([`Records::of`]).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use bevy::prelude::*;
use iwbtg_format::PackageFiles;
use iwbtg_format::key_art;
use iwbtg_format::manifest::Manifest;
use iwbtg_sim::mods::ModSet;

use crate::package::{self, SpriteImage, decode_png};
use crate::stats::{Book, fmt_ticks, map_key};
use crate::ui::backdrop;

/// The room size a back layer is composed over for the list's art: GM8's 800 x 608. The panel
/// shows a 72 px strip of it, so a room of another size only crops differently.
const ROOM: (u32, u32) = (800, 608);

/// The map name of a package without `[[maps]]`, as the loading card names its ranked run.
pub const SINGLE_MAP: &str = "Ranked mode";

/// One map of a game, as the list shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct MapInfo {
    pub id: String,
    pub name: String,
    /// The star rating; `None` until ratings exist (#111).
    pub stars: Option<f32>,
}

/// One package of the folder.
#[derive(Clone, Debug, PartialEq)]
pub struct GameInfo {
    /// The folder or `.iwbz` file, what Enter plays.
    pub path: PathBuf,
    /// The package id (the stats book's key); the file name when `game.toml` does not read.
    pub id: String,
    pub title: String,
    /// The authors, joined; empty when none are listed.
    pub creator: String,
    /// In `[[maps]]` order; one map for a package without `[[maps]]`, none when `game.toml`
    /// does not read.
    pub maps: Vec<MapInfo>,
}

/// The file or folder name of `path`.
pub fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

impl GameInfo {
    /// The list entry of `manifest`, read from `path`.
    pub fn new(path: &Path, manifest: &Manifest) -> GameInfo {
        let title = manifest.title.get_ref().trim();
        let named = !manifest.maps.is_empty();
        GameInfo {
            path: path.to_owned(),
            id: manifest.id.get_ref().clone(),
            title: if title.is_empty() {
                file_name(path)
            } else {
                title.to_owned()
            },
            creator: crate::loading::authors(manifest).unwrap_or_default(),
            maps: manifest
                .map_list()
                .into_iter()
                .map(|m| MapInfo {
                    id: m.id.to_owned(),
                    name: if named {
                        m.name.to_owned()
                    } else {
                        SINGLE_MAP.to_owned()
                    },
                    stars: None,
                })
                .collect(),
        }
    }

    /// A package whose `game.toml` does not read: listed by its file name, without maps (Enter
    /// still tries it and shows why it does not open).
    pub fn unreadable(path: &Path) -> GameInfo {
        GameInfo {
            path: path.to_owned(),
            id: file_name(path),
            title: file_name(path),
            creator: String::new(),
            maps: Vec::new(),
        }
    }
}

/// Your records on one game: the PB per map (ranked board with the mods set now) and whether any
/// map was cleared.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Records {
    /// By map, in the game's map order; `None`: no PB.
    pub pbs: Vec<Option<String>>,
    pub cleared: bool,
}

impl Records {
    pub fn of(game: &GameInfo, book: &Book, mods: ModSet) -> Records {
        let codes = mods.codes();
        let pbs = game
            .maps
            .iter()
            .map(|m| {
                let key = map_key(&game.id, &m.id);
                let rate = book.map(&key).map_or(50, |s| s.tick_rate);
                book.board(&key, true, &codes)
                    .and_then(|b| b.pb())
                    .map(|r| fmt_ticks(r.ticks, rate))
            })
            .collect();
        let cleared = game
            .maps
            .iter()
            .any(|m| book.map(&map_key(&game.id, &m.id)).is_some_and(|s| s.clears > 0));
        Records { pbs, cleared }
    }
}

/// How many games on each side of the pick keep a preview: the list shows about 14 rows, and a
/// filter can reorder it, so this covers what is on screen and its neighbours (#1593).
pub const PREVIEW_RADIUS: usize = 12;

/// A path's cache key for same-path invalidation (#1593): the package path's own length and
/// mtime (a folder's changes when its entries do) and its `game.toml`'s.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathKey {
    path: (u64, Option<(u64, u64)>),
    manifest: (u64, Option<(u64, u64)>),
}

/// The key of `path` now. For a folder the manifest is its `game.toml`; an archive is its own.
pub fn path_key(path: &Path) -> PathKey {
    let meta = |p: &Path| {
        std::fs::metadata(p).ok().map(|m| {
            let modified = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| (d.as_secs(), u64::from(d.subsec_nanos())));
            (m.len(), modified)
        })
    };
    let manifest = if path.is_dir() {
        path.join("game.toml")
    } else {
        path.to_path_buf()
    };
    PathKey {
        path: meta(path).unwrap_or_default(),
        manifest: meta(&manifest).unwrap_or_default(),
    }
}

/// A preview to build off the UI thread.
struct Job {
    index: usize,
    path: PathBuf,
}

/// A built preview, on its way back.
struct Preview {
    index: usize,
    art: Image,
    fruit: Image,
}

/// The folder's games, kept between runs: their metadata at once, and their previews (key art
/// and fruit) incrementally around the pick (#1593). `records` is refreshed from the stats book
/// each time the screen opens.
#[derive(Resource)]
pub struct Library {
    /// The folder it was read from.
    pub folder: Option<PathBuf>,
    pub games: Vec<GameInfo>,
    /// Each game's key art strip, cut to the panel's band ([`art_image`]): `None` until its
    /// preview arrives or after it is evicted (the panel then shows its dark placeholder).
    pub art: Vec<Option<Handle<Image>>>,
    /// Each game's fruit for the Cleared mark ([`fruit_image`]); `None` as for `art`.
    pub fruit: Vec<Option<Handle<Image>>>,
    /// The fruit drawn while a game's own has not arrived.
    pub placeholder_fruit: Handle<Image>,
    pub records: Vec<Records>,
    /// Each game's path key, for same-path invalidation.
    pub keys: Vec<PathKey>,
    /// The preview worker's queue, or `None` when its thread could not start.
    jobs: Option<Sender<Job>>,
    done: Mutex<Receiver<Preview>>,
    /// Indices asked for and not answered yet.
    pending: HashSet<usize>,
    /// The games whose previews are wanted now; an answer outside this is dropped.
    wanted: HashSet<usize>,
    /// Bumped when a preview arrives or is evicted, so the list redraws those panels.
    revision: u64,
}

impl Library {
    /// Reads every package's manifest in `paths` (cheap, so the list is ordered and searchable
    /// at once) and starts the preview worker; previews are requested by [`want`](Self::want).
    pub fn load(folder: Option<&Path>, paths: &[PathBuf], images: &mut Assets<Image>) -> Library {
        let placeholder_fruit = images.add(fruit_image(None));
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (done_tx, done_rx) = mpsc::channel::<Preview>();
        let worker_tx = job_tx.clone();
        let spawned = std::thread::Builder::new()
            .name("iwbtg-preview".to_owned())
            .spawn(move || {
                for job in job_rx {
                    let files = preview_files(&job.path);
                    let preview = Preview {
                        index: job.index,
                        art: art_image(files.as_ref()),
                        fruit: fruit_image(files.as_ref()),
                    };
                    if done_tx.send(preview).is_err() {
                        break;
                    }
                }
            })
            .is_ok();
        let mut lib = Library {
            folder: folder.map(Path::to_owned),
            games: Vec::with_capacity(paths.len()),
            art: Vec::new(),
            fruit: Vec::new(),
            placeholder_fruit,
            records: Vec::new(),
            keys: Vec::new(),
            jobs: if spawned { Some(worker_tx) } else { None },
            done: Mutex::new(done_rx),
            pending: HashSet::new(),
            wanted: HashSet::new(),
            revision: 0,
        };
        for path in paths {
            let files = package::read_manifest(path).ok();
            let manifest = files.as_ref().and_then(manifest_of);
            lib.games.push(match manifest {
                Some(m) => GameInfo::new(path, &m),
                None => GameInfo::unreadable(path),
            });
            lib.art.push(None);
            lib.fruit.push(None);
            lib.records.push(Records::default());
            lib.keys.push(path_key(path));
        }
        lib
    }

    /// Whether this library still describes `folder`, `paths` and `keys`: a package replaced or
    /// changed at the same path has a different key, so the caller reloads (#1593).
    pub fn matches(&self, folder: Option<&Path>, paths: &[PathBuf], keys: &[PathKey]) -> bool {
        self.folder.as_deref() == folder
            && self.games.iter().map(|g| &g.path).eq(paths.iter())
            && self.keys == keys
    }

    /// Whether a requested preview is still on its way (tests wait on it).
    pub fn busy(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Changes whenever the previews held change (an arrival or an eviction), so the list
    /// redraws only then.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Asks for the previews of `wanted` and drops every other loaded one, so the retained strips
    /// stay within the window around the pick.
    pub fn want(&mut self, wanted: &[usize]) {
        self.wanted = wanted.iter().copied().collect();
        for i in 0..self.art.len() {
            if self.art[i].is_some() && !self.wanted.contains(&i) {
                self.art[i] = None;
                self.fruit[i] = None;
                self.revision = self.revision.wrapping_add(1);
            }
        }
        for &i in wanted {
            if i >= self.art.len() || self.art[i].is_some() || self.pending.contains(&i) {
                continue;
            }
            let Some(jobs) = &self.jobs else {
                return;
            };
            let job = Job {
                index: i,
                path: self.games[i].path.clone(),
            };
            if jobs.send(job).is_ok() {
                self.pending.insert(i);
            }
        }
    }

    /// Takes every preview that arrived and adds it to `images`; true when one did (the list
    /// then redraws those panels). Answers for games no longer wanted are dropped, so a scroll
    /// does not keep them.
    pub fn poll(&mut self, images: &mut Assets<Image>) -> bool {
        let mut changed = false;
        loop {
            let next = {
                let rx = self
                    .done
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                rx.try_recv()
            };
            match next {
                Ok(preview) => {
                    self.pending.remove(&preview.index);
                    if self.wanted.contains(&preview.index) && preview.index < self.art.len() {
                        self.art[preview.index] = Some(images.add(preview.art));
                        self.fruit[preview.index] = Some(images.add(preview.fruit));
                        self.revision = self.revision.wrapping_add(1);
                        changed = true;
                    }
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        changed
    }

    /// Your PBs and clears from `book`, with the mods set now.
    pub fn refresh(&mut self, book: &Book, mods: ModSet) {
        self.records = self.games.iter().map(|g| Records::of(g, book, mods)).collect();
    }
}

/// Reads `path` for the list: its entry, and the files its key art and fruit need (`None` when
/// `game.toml` does not read).
pub fn read(path: &Path) -> (GameInfo, Option<PackageFiles>) {
    let files = preview_files(path);
    match files.as_ref().and_then(manifest_of) {
        Some(m) => (GameInfo::new(path, &m), files),
        None => (GameInfo::unreadable(path), None),
    }
}

/// The files a preview needs (`None` when the package does not read at all).
fn preview_files(path: &Path) -> Option<PackageFiles> {
    if package::is_iwbz(path) {
        package::read_iwbz(path).ok()
    } else {
        folder_files(path)
    }
}

/// The manifest in `files`, when it parses.
fn manifest_of(files: &PackageFiles) -> Option<Manifest> {
    files
        .get("game.toml")
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|t| Manifest::parse(t).ok())
}

/// From a package folder, only what the list draws: `game.toml`, the key art images, the first
/// ranked room and its back layers (the key art fallback) and the `cherry` sprite. A big game
/// is not read whole for a 72 px panel.
fn folder_files(dir: &Path) -> Option<PackageFiles> {
    let mut files = PackageFiles::new();
    let add = |files: &mut PackageFiles, rel: &str| {
        if let Ok(bytes) = std::fs::read(dir.join(rel)) {
            let _ = files.insert(rel, bytes);
        }
    };
    add(&mut files, "game.toml");
    let text = std::str::from_utf8(files.get("game.toml")?).ok()?;
    let manifest = Manifest::parse(text).ok()?;
    if let Some(section) = &manifest.key_art {
        for path in section.image.iter().chain(section.maps.values()) {
            add(&mut files, path.get_ref());
        }
    }
    let room = key_art::first_ranked_room(&manifest);
    add(&mut files, &format!("rooms/{room}.toml"));
    if let Ok(art) = iwbtg_format::art::load_room_art(&files, room) {
        for bg in art.backgrounds.iter().filter(|b| !b.foreground) {
            add(&mut files, &format!("backgrounds/{}.png", bg.image));
        }
    }
    add(&mut files, "sprites/cherry.toml");
    add(&mut files, "sprites/cherry.png");
    Some(files)
}

/// The game's key art for its panel (`key_art::resolve`, as the backdrop draws it; the client's
/// own scene when there is nothing).
pub fn art_image(files: Option<&PackageFiles>) -> Image {
    let empty = PackageFiles::new();
    let files = files.unwrap_or(&empty);
    let art = files
        .get("game.toml")
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|t| Manifest::parse(t).ok())
        .map_or(key_art::KeyArt::None, |m| key_art::resolve(files, &m, None));
    let full = backdrop::key_art_image(files, &art, ROOM, |bytes| {
        decode_png(bytes).map(|s| (s.rgba, s.size.0, s.size.1))
    });
    let (w, h) = (full.width(), full.height());
    let Some(data) = full.data.as_deref() else {
        return full;
    };
    let level0 = &data[..((w * h * 4) as usize).min(data.len())];
    let (rgba, sw, sh) = strip(level0, w, h);
    backdrop::art_image(&rgba, sw, sh)
}

/// The panel's art box (`Main.dc.html`: the art is 1060 x 72, 30 px past the panel's left edge).
const ART_BOX: (f32, f32) = (1060.0, 72.0);
/// The widest strip kept: the art box at about 1.2 times 1080p. A 20-game list keeps 20 strips
/// of about 1280 x 87 instead of 20 full key arts.
const STRIP_WIDTH: u32 = 1280;

/// The middle band of an RGBA image in the art box's shape (what covering the box shows),
/// shrunk to at most [`STRIP_WIDTH`] wide by averaging whole blocks.
pub fn strip(rgba: &[u8], w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    let ratio = ART_BOX.0 / ART_BOX.1;
    let (w, h) = (w.max(1), h.max(1));
    let (bw, bh) = if w as f32 / h as f32 > ratio {
        (((h as f32 * ratio).round() as u32).clamp(1, w), h)
    } else {
        (w, ((w as f32 / ratio).round() as u32).clamp(1, h))
    };
    let (x0, y0) = ((w - bw) / 2, (h - bh) / 2);
    let k = bw.div_ceil(STRIP_WIDTH).max(1);
    let (ow, oh) = ((bw / k).max(1), (bh / k).max(1));
    let mut out = Vec::with_capacity((ow * oh * 4) as usize);
    for oy in 0..oh {
        for ox in 0..ow {
            let mut sum = [0u32; 4];
            let mut n = 0;
            for y in (y0 + oy * k)..(y0 + (oy + 1) * k).min(y0 + bh) {
                for x in (x0 + ox * k)..(x0 + (ox + 1) * k).min(x0 + bw) {
                    let i = ((y * w + x) * 4) as usize;
                    for c in 0..4 {
                        sum[c] += u32::from(rgba[i + c]);
                    }
                    n += 1;
                }
            }
            let n = n.max(1);
            out.extend(sum.iter().map(|s| (s / n) as u8));
        }
    }
    (out, ow, oh)
}

/// The first frame of the game's `cherry` sprite, else our own fruit (the pause menu's).
pub fn fruit_image(files: Option<&PackageFiles>) -> Image {
    let empty = PackageFiles::new();
    let both: SpriteImage = crate::pause::fruit_image(files.unwrap_or(&empty));
    let frame = crate::pause::crop(&both, both.size.0 / 2, both.size.1);
    crate::pause::to_bevy(&frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{Board, MapStats, Run, board_key};

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn a_package_with_maps_lists_each_map_by_name() {
        let (info, files) = read(&fixture("maps.pkg"));
        assert_eq!(info.id, "demo_maps");
        assert_eq!(info.title, "Maps fixture");
        assert_eq!(info.creator, "IWBTG Collection");
        let names: Vec<&str> = info.maps.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["Medium", "Hard", "Hard 100%"]);
        assert!(
            info.maps.iter().all(|m| m.stars.is_none()),
            "stars come with #111"
        );
        let files = files.unwrap();
        assert!(files.contains("rooms/stage.toml") && files.contains("art/key.png"));
    }

    #[test]
    fn a_package_without_maps_has_one_ranked_map() {
        let (info, _) = read(&fixture("clear.pkg"));
        assert_eq!(info.maps.len(), 1);
        assert_eq!(info.maps[0].id, "main");
        assert_eq!(info.maps[0].name, SINGLE_MAP);
    }

    #[test]
    fn an_unreadable_package_keeps_its_file_name_and_no_maps() {
        let d = std::env::temp_dir().join(format!("iwbtg-library-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("broken")).unwrap();
        std::fs::write(d.join("broken/game.toml"), "title = 3").unwrap();
        let (info, files) = read(&d.join("broken"));
        assert_eq!(info.title, "broken");
        assert!(info.maps.is_empty() && files.is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn records_take_the_ranked_pb_and_any_clear() {
        let (info, _) = read(&fixture("maps.pkg"));
        let mut book = Book::default();
        assert_eq!(
            Records::of(&info, &book, ModSet::NONE),
            Records {
                pbs: vec![None; 3],
                cleared: false
            }
        );
        let mut hard = MapStats {
            tick_rate: 50,
            clears: 1,
            ..default()
        };
        let mut board = Board::default();
        board.add(Run {
            ticks: 50 * 75,
            deaths: 3,
            splits: vec![50 * 75],
            segment_deaths: vec![3],
            finished_at: 0,
            replay: None,
            death_log: Default::default(),
        });
        hard.boards.insert(board_key(true, "NM"), board);
        book.maps.insert(map_key("demo_maps", "hard"), hard);
        let rec = Records::of(&info, &book, ModSet::NONE);
        assert_eq!(rec.pbs, [None, Some("1:15.00".to_owned()), None]);
        assert!(rec.cleared);
        // Another mod combination is another board.
        let sd = ModSet::parse("SD").unwrap();
        assert_eq!(Records::of(&info, &book, sd).pbs, [None, None, None]);
    }

    #[test]
    fn the_strip_is_the_middle_band_shrunk() {
        // 16:9 key art: the band is the full width, 130 rows from the middle, halved.
        let (w, h) = (1920u32, 1080u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                rgba[((y * w + x) * 4) as usize] = if y < h / 2 { 10 } else { 200 };
            }
        }
        let (out, sw, sh) = strip(&rgba, w, h);
        assert_eq!((sw, sh), (960, 65));
        assert_eq!(out.len(), (sw * sh * 4) as usize);
        assert_eq!(out[0], 10, "the top of the band is the upper half");
        assert_eq!(
            out[((sh - 1) * sw * 4) as usize],
            200,
            "its bottom the lower half"
        );
        // A tiny colour image keeps one row.
        let (_, tw, th) = strip(&[1, 2, 3, 255].repeat(4), 2, 2);
        assert_eq!((tw, th), (2, 1));
    }

    /// The metadata pass lists every game at once and previews arrive afterwards, only for the
    /// wanted window; moving the window evicts the old strip (#1593).
    #[test]
    fn previews_arrive_after_the_metadata_and_only_around_the_pick() {
        use std::time::{Duration, Instant};
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let paths = vec![fixtures.join("clear.pkg"), fixtures.join("maps.pkg")];
        let mut images = Assets::<Image>::default();
        let mut lib = Library::load(Some(&fixtures), &paths, &mut images);
        assert_eq!(lib.games.len(), 2);
        assert_eq!(lib.games[0].title, "Clear fixture");
        assert!(
            lib.art.iter().all(Option::is_none),
            "the metadata pass loads no preview"
        );

        lib.want(&[0]);
        let mut settle = |lib: &mut Library| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while lib.busy() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
                lib.poll(&mut images);
            }
        };
        settle(&mut lib);
        assert!(lib.art[0].is_some(), "the wanted preview arrived");
        assert!(lib.art[1].is_none(), "the neighbour waits its turn");

        lib.want(&[1]);
        assert!(lib.art[0].is_none(), "outside the window, the strip is freed");
        settle(&mut lib);
        assert!(lib.art[1].is_some(), "the new window's preview arrived");
    }

    /// A package changed at the same path gets a new key, so the screen reloads it (#1593); a
    /// folder whose manifest does not parse keeps the unreadable fallback.
    #[test]
    fn a_changed_manifest_at_the_same_path_invalidates_the_library() {
        let d = std::env::temp_dir().join(format!("iwbtg-library-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let pkg = d.join("game");
        std::fs::create_dir_all(&pkg).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/maps.pkg/game.toml");
        let manifest = std::fs::read_to_string(&source).unwrap();
        std::fs::write(pkg.join("game.toml"), &manifest).unwrap();
        let broken = d.join("broken");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join("game.toml"), "title = 3").unwrap();
        let paths = vec![pkg.clone(), broken.clone()];

        let mut images = Assets::<Image>::default();
        let lib = Library::load(Some(&d), &paths, &mut images);
        assert_eq!(lib.games[0].title, "Maps fixture");
        assert_eq!(lib.games[1].title, "broken", "the unreadable fallback");
        let keys = vec![path_key(&pkg), path_key(&broken)];
        assert!(lib.matches(Some(&d), &paths, &keys));

        let changed = manifest.replace("Maps fixture", "Changed fixture");
        std::fs::write(pkg.join("game.toml"), changed).unwrap();
        let new_keys = vec![path_key(&pkg), path_key(&broken)];
        assert_ne!(keys, new_keys, "the changed manifest has a new key");
        assert!(
            !lib.matches(Some(&d), &paths, &new_keys),
            "so the screen reloads instead of reusing the cache"
        );
        let lib = Library::load(Some(&d), &paths, &mut images);
        assert_eq!(lib.games[0].title, "Changed fixture");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A hundred packages list from their manifests alone; no preview is read for the pass
    /// (#1593, #1022 boxes 2 and 3).
    #[test]
    fn five_hundred_packages_list_from_metadata_without_previews() {
        use std::time::{Duration, Instant};
        let d = std::env::temp_dir().join(format!("iwbtg-library-500-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/maps.pkg/game.toml");
        let manifest = std::fs::read_to_string(&source).unwrap();
        let mut paths = Vec::new();
        for i in 0..500 {
            let dir = d.join(format!("g{i:03}"));
            std::fs::create_dir_all(&dir).unwrap();
            let title = format!("Game {i}");
            let text = manifest.replace("Maps fixture", &title);
            std::fs::write(dir.join("game.toml"), text).unwrap();
            paths.push(dir);
        }
        let mut images = Assets::<Image>::default();
        let started = Instant::now();
        let lib = Library::load(Some(&d), &paths, &mut images);
        let elapsed = started.elapsed();
        assert_eq!(lib.games.len(), 500);
        assert!(lib.art.iter().all(Option::is_none), "no previews at load");
        println!("500 manifests listed in {elapsed:?}");
        assert!(
            elapsed < Duration::from_secs(10),
            "the metadata pass took {elapsed:?}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn art_and_fruit_have_images_even_without_files() {
        assert!(art_image(None).width() > 0);
        assert_eq!(fruit_image(None).width(), crate::pause::FRUIT.x);
        let (_, files) = read(&fixture("maps.pkg"));
        assert!(art_image(files.as_ref()).width() > 0);
    }
}
