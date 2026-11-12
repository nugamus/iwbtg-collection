//! The `.iwbz` package file (docs/format/iwbz.md): one zip holding a whole built package.
//!
//! [`pack`] writes it, [`unpack`] reads it back into [`PackageFiles`]. Both go through
//! [`check_entry`], the list of what a package may hold: the folders and file types of the
//! package layout (docs/format/v0.md), nothing else. So an `.iwbz` can never carry code, a
//! `tests/` folder, a link or a path outside the package.
//!
//! The zip code is our own (stored and deflate only, no zip64, no encryption) instead of a zip
//! crate: the reader stays small enough to check by eye, every limit is in one place, and the
//! only dependencies are `flate2` and `crc32fast`, which `png` already brings.
//!
//! The reader is written for files from strangers:
//! - every name passes [`check_entry`] and [`crate::files::check_path`] (no `..`, no absolute
//!   path, no backslash, no drive letter, no Windows device name), and appears once;
//! - the archive is at most [`MAX_ARCHIVE_BYTES`], has at most [`MAX_FILES`] files and the files
//!   add up to at most [`MAX_PACKAGE_BYTES`] unpacked, checked from the directory before
//!   anything is inflated, and each file inflates to exactly its stated size (zip bombs);
//! - file data may not overlap (the "overlapping files" zip bomb) and must sit before the
//!   central directory;
//! - every file's CRC-32 matches, and PNG and Ogg files start with their signatures.

use std::io::{Read, Write};

use crate::files::{PackageFiles, check_path, lf_normalized};
use crate::ids::check_id;

/// The file extension, without the dot.
pub const EXTENSION: &str = "iwbz";

/// Largest package, unpacked: the format's package size limit (docs/format/v0.md, Assets).
pub const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;
/// Largest `.iwbz` file. Packed is never larger than unpacked plus the zip headers.
pub const MAX_ARCHIVE_BYTES: u64 = MAX_PACKAGE_BYTES + 16 * 1024 * 1024;
/// Most files in one package. The largest converted game has under 2,000.
pub const MAX_FILES: usize = 20_000;

/// Why a package could not be packed or unpacked.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IwbzError {
    /// A file the package format does not allow (see [`check_entry`]).
    #[error("`{path}` is not allowed in a package: {reason}")]
    NotAllowed {
        /// The file's path inside the package.
        path: String,
        /// Why.
        reason: String,
    },
    /// A limit of [`MAX_ARCHIVE_BYTES`], [`MAX_PACKAGE_BYTES`] or [`MAX_FILES`] is passed.
    #[error("too large: {0}")]
    TooLarge(String),
    /// The bytes are not a zip this reader accepts.
    #[error("not a valid .iwbz file: {0}")]
    Corrupt(String),
    /// The package has no `game.toml`.
    #[error("the package has no game.toml")]
    NoManifest,
}

fn corrupt(msg: impl Into<String>) -> IwbzError {
    IwbzError::Corrupt(msg.into())
}

/// File names Windows reserves in every folder, with or without an extension. A package with
/// one could not be unpacked there.
const WINDOWS_DEVICES: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9",
    "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// The file types each package folder may hold (docs/format/v0.md, Package layout).
fn folder_extensions(folder: &str) -> Option<&'static [&'static str]> {
    Some(match folder {
        "objects" | "timelines" | "paths" | "rooms" => &["toml"],
        "scripts" => &["rhai"],
        "sprites" | "backgrounds" | "fonts" | "art" => &["png", "toml"],
        "sounds" | "music" => &["ogg", "toml"],
        _ => return None,
    })
}

/// Checks that a package may hold the file `path` with these `bytes`, by the package layout of
/// docs/format/v0.md:
///
/// - at the top: `<id>.toml` (`game.toml`, `physics.toml`) and Markdown (`CREDITS.md`);
/// - one level down, by folder: `.toml` in `objects/`, `timelines/`, `paths/` and `rooms/`
///   (also `rooms/<id>.tiles.toml`), `.rhai` in `scripts/`, `.png` and `.toml` in `sprites/`,
///   `backgrounds/`, `fonts/` and `art/`, `.ogg` and `.toml` in `sounds/` and `music/`;
/// - `engine/<pack>/`: `engine.toml`, Markdown, and the same folders as above;
/// - file stems are ids (`[a-z][a-z0-9_]*`), never a Windows device name;
/// - `.png` starts with the PNG signature and `.ogg` with `OggS`.
///
/// `tests/` is never shipped, so it is refused here.
pub fn check_entry(path: &str, bytes: &[u8]) -> Result<(), IwbzError> {
    let deny = |reason: &str| IwbzError::NotAllowed {
        path: path.to_owned(),
        reason: reason.to_owned(),
    };
    check_path(path).map_err(deny)?;
    let mut parts: Vec<&str> = path.split('/').collect();
    if parts[0] == "tests" {
        return Err(deny("`tests/` is never shipped"));
    }
    let in_pack = parts[0] == "engine";
    if in_pack {
        if parts.len() < 3 {
            return Err(deny("an engine pack file goes in `engine/<pack>/`"));
        }
        check_id(parts[1]).map_err(|e| deny(&format!("engine pack folder: {e}")))?;
        parts.drain(..2);
    }
    let name = parts[parts.len() - 1];
    let Some((stem, ext)) = name.split_once('.') else {
        return Err(deny("a package file has an extension"));
    };
    if WINDOWS_DEVICES.contains(&stem.to_ascii_lowercase().as_str()) {
        return Err(deny("the name is reserved on Windows"));
    }
    match parts.len() {
        1 => {
            let text_name = stem
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
            match ext {
                "md" if text_name => {}
                "toml" if check_id(stem).is_ok() => {}
                _ => {
                    return Err(deny(if in_pack {
                        "the top of an engine pack holds only engine.toml and Markdown files"
                    } else {
                        "the top of a package holds only <id>.toml and Markdown files"
                    }));
                }
            }
        }
        2 => {
            let Some(allowed) = folder_extensions(parts[0]) else {
                return Err(deny("not a package folder"));
            };
            let (stem, ext) = match name.strip_suffix(".tiles.toml") {
                Some(room) if parts[0] == "rooms" => (room, "toml"),
                _ => (stem, ext),
            };
            if !allowed.contains(&ext) {
                return Err(deny(&format!(
                    "`{}/` holds only {} files",
                    parts[0],
                    allowed.join(" and ")
                )));
            }
            check_id(stem).map_err(|e| deny(&format!("the file name is an id: {e}")))?;
            let magic: &[u8] = match ext {
                "png" => b"\x89PNG\r\n\x1a\n",
                "ogg" => b"OggS",
                _ => b"",
            };
            if !bytes.starts_with(magic) {
                return Err(deny(&format!("the file is not a .{ext} file")));
            }
        }
        _ => return Err(deny("package folders have no subfolders")),
    }
    Ok(())
}

/// Text files are packed with LF line ends, so the same package packs to the same bytes from a
/// Windows or a Unix checkout (the content hash already ignores CRLF).
fn is_text(path: &str) -> bool {
    path.ends_with(".toml") || path.ends_with(".rhai") || path.ends_with(".md")
}

const LOCAL_SIG: u32 = 0x0403_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const END_SIG: u32 = 0x0605_4b50;
const END_LEN: usize = 22;
/// Version 2.0: deflate. Also "made by" MS-DOS, so readers take no Unix mode from it.
const VERSION: u16 = 20;
/// General purpose flag bit 11: names are UTF-8.
const FLAG_UTF8: u16 = 0x0800;
/// 1980-01-01 00:00, the zip epoch: a fixed time keeps the archive reproducible.
const DOS_DATE: u16 = (1 << 5) | 1;
const STORED: u16 = 0;
const DEFLATE: u16 = 8;

/// Packs a package into `.iwbz` bytes. `tests/` is left out (it is never shipped); every other
/// file must pass [`check_entry`], and `game.toml` must be there. Files go in path order, text
/// with LF line ends, deflated when that is smaller, with a fixed date: the same package always
/// gives the same bytes. Unpacking gives the same `content_hash` as `files`.
pub fn pack(files: &PackageFiles) -> Result<Vec<u8>, IwbzError> {
    if !files.contains("game.toml") {
        return Err(IwbzError::NoManifest);
    }
    let mut out = Vec::new();
    let mut central = Vec::new();
    let mut count = 0usize;
    let mut total = 0u64;
    for (path, bytes) in files
        .iter()
        .filter(|(p, _)| !p.starts_with(crate::files::TESTS_DIR))
    {
        check_entry(path, bytes)?;
        let data: std::borrow::Cow<[u8]> = if is_text(path) {
            lf_normalized(bytes).into()
        } else {
            bytes.into()
        };
        count += 1;
        total += data.len() as u64;
        if count > MAX_FILES || total > MAX_PACKAGE_BYTES {
            return Err(IwbzError::TooLarge(format!(
                "a package holds at most {MAX_FILES} files and {MAX_PACKAGE_BYTES} bytes"
            )));
        }
        let crc = crc32fast::hash(&data);
        let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
        enc.write_all(&data).map_err(|e| corrupt(e.to_string()))?;
        let deflated = enc.finish().map_err(|e| corrupt(e.to_string()))?;
        let (method, stored): (u16, &[u8]) = if deflated.len() < data.len() {
            (DEFLATE, &deflated)
        } else {
            (STORED, &data)
        };
        let offset = u32::try_from(out.len()).map_err(|_| IwbzError::TooLarge("over 4 GB".into()))?;
        let header = |buf: &mut Vec<u8>| {
            put16(buf, VERSION);
            put16(buf, FLAG_UTF8);
            put16(buf, method);
            put16(buf, 0);
            put16(buf, DOS_DATE);
            put32(buf, crc);
            put32(buf, stored.len() as u32);
            put32(buf, data.len() as u32);
            put16(buf, path.len() as u16);
            put16(buf, 0);
        };
        put32(&mut out, LOCAL_SIG);
        header(&mut out);
        out.extend_from_slice(path.as_bytes());
        out.extend_from_slice(stored);

        put32(&mut central, CENTRAL_SIG);
        put16(&mut central, VERSION);
        header(&mut central);
        put16(&mut central, 0); // comment
        put16(&mut central, 0); // disk
        put16(&mut central, 0); // internal attributes
        put32(&mut central, 0); // external attributes
        put32(&mut central, offset);
        central.extend_from_slice(path.as_bytes());
    }
    let cd_offset = out.len();
    out.extend_from_slice(&central);
    put32(&mut out, END_SIG);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, count as u16);
    put16(&mut out, count as u16);
    put32(&mut out, central.len() as u32);
    put32(&mut out, cd_offset as u32);
    put16(&mut out, 0);
    if out.len() as u64 > MAX_ARCHIVE_BYTES {
        return Err(IwbzError::TooLarge(format!(
            "the .iwbz would pass {MAX_ARCHIVE_BYTES} bytes"
        )));
    }
    Ok(out)
}

fn put16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn put32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

/// Little-endian reads that fail on a short buffer instead of panicking.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8], at: usize) -> Self {
        Self { bytes, at }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], IwbzError> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len());
        let Some(end) = end else {
            return Err(corrupt("the file ends too early"));
        };
        let s = &self.bytes[self.at..end];
        self.at = end;
        Ok(s)
    }

    fn u16(&mut self) -> Result<u16, IwbzError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, IwbzError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// One file as the central directory describes it.
struct Entry {
    name: String,
    method: u16,
    crc: u32,
    packed: u64,
    size: u64,
    local: usize,
}

/// Reads `.iwbz` bytes into a package, refusing anything [`check_entry`] refuses and every
/// archive that breaks a limit (see the module docs). Folder entries (`rooms/`) are skipped.
pub fn unpack(bytes: &[u8]) -> Result<PackageFiles, IwbzError> {
    if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
        return Err(IwbzError::TooLarge(format!(
            "the file is over {MAX_ARCHIVE_BYTES} bytes"
        )));
    }
    let entries = central_directory(bytes)?;
    let cd_start = entries.1;
    let mut entries = entries.0;

    // Limits from the directory, before anything is inflated.
    if entries.len() > MAX_FILES {
        return Err(IwbzError::TooLarge(format!("more than {MAX_FILES} files")));
    }
    let total: u64 = entries.iter().map(|e| e.size).sum();
    if total > MAX_PACKAGE_BYTES {
        return Err(IwbzError::TooLarge(format!(
            "the files add up to {total} bytes; a package is at most {MAX_PACKAGE_BYTES}"
        )));
    }

    // File data in archive order: each must start after the one before ends.
    entries.sort_by_key(|e| e.local);
    let mut files = PackageFiles::new();
    let mut end_of_last = 0usize;
    for e in &entries {
        if e.local < end_of_last {
            return Err(corrupt(format!("`{}` overlaps another file", e.name)));
        }
        let mut c = Cursor::new(bytes, e.local);
        if c.u32()? != LOCAL_SIG {
            return Err(corrupt(format!("`{}`: no local header", e.name)));
        }
        c.take(22)?;
        let name_len = usize::from(c.u16()?);
        let extra_len = usize::from(c.u16()?);
        if c.take(name_len)? != e.name.as_bytes() {
            return Err(corrupt(format!(
                "`{}`: the two headers name different files",
                e.name
            )));
        }
        c.take(extra_len)?;
        let packed = usize::try_from(e.packed).map_err(|_| corrupt("file too large"))?;
        let start = c.at;
        let data = c.take(packed)?;
        end_of_last = c.at;
        if end_of_last > cd_start || start < e.local {
            return Err(corrupt(format!("`{}` runs into the directory", e.name)));
        }
        if e.name.ends_with('/') {
            if e.size != 0 {
                return Err(corrupt(format!("folder entry `{}` has data", e.name)));
            }
            continue;
        }
        let out = match e.method {
            STORED if e.packed == e.size => data.to_vec(),
            STORED => return Err(corrupt(format!("`{}`: stored sizes differ", e.name))),
            DEFLATE => inflate(data, e.size).map_err(|m| corrupt(format!("`{}`: {m}", e.name)))?,
            m => {
                return Err(corrupt(format!(
                    "`{}`: compression method {m}; only stored and deflate",
                    e.name
                )));
            }
        };
        if crc32fast::hash(&out) != e.crc {
            return Err(corrupt(format!("`{}`: CRC-32 does not match", e.name)));
        }
        check_entry(&e.name, &out)?;
        if files
            .insert(e.name.clone(), out)
            .map_err(|err| corrupt(err.to_string()))?
            .is_some()
        {
            return Err(corrupt(format!("`{}` is in the archive twice", e.name)));
        }
    }
    if !files.contains("game.toml") {
        return Err(IwbzError::NoManifest);
    }
    Ok(files)
}

/// Inflates `data` to exactly `size` bytes, reading at most one byte past it, so a file that
/// claims a small size cannot fill memory.
fn inflate(data: &[u8], size: u64) -> Result<Vec<u8>, String> {
    let cap = usize::try_from(size).map_err(|_| "file too large".to_owned())?;
    let mut out = Vec::with_capacity(cap);
    flate2::read::DeflateDecoder::new(data)
        .take(size + 1)
        .read_to_end(&mut out)
        .map_err(|e| format!("bad deflate data: {e}"))?;
    if out.len() as u64 != size {
        return Err(format!("inflates to {} bytes, not the stated {size}", out.len()));
    }
    Ok(out)
}

/// The central directory's entries and where it starts.
fn central_directory(bytes: &[u8]) -> Result<(Vec<Entry>, usize), IwbzError> {
    // The end record is the last 22 bytes plus a comment of up to 64 KiB.
    let lowest = bytes.len().saturating_sub(END_LEN + usize::from(u16::MAX));
    let end = (lowest..=bytes.len().saturating_sub(END_LEN))
        .rev()
        .find(|&i| bytes[i..].starts_with(&END_SIG.to_le_bytes()))
        .ok_or_else(|| corrupt("not a zip file (no end record)"))?;
    let mut c = Cursor::new(bytes, end + 4);
    let disk = c.u16()?;
    let cd_disk = c.u16()?;
    let on_disk = c.u16()?;
    let count = c.u16()?;
    let cd_len = c.u32()? as usize;
    let cd_start = c.u32()? as usize;
    let comment = usize::from(c.u16()?);
    if disk != 0 || cd_disk != 0 || on_disk != count {
        return Err(corrupt("split archives are not supported"));
    }
    if count == u16::MAX || cd_start == u32::MAX as usize {
        return Err(corrupt("zip64 is not supported"));
    }
    if end + END_LEN + comment != bytes.len() {
        return Err(corrupt("bytes after the end record"));
    }
    if cd_start.checked_add(cd_len) != Some(end) {
        return Err(corrupt("the directory is not where the end record says"));
    }
    let mut c = Cursor::new(bytes, cd_start);
    let mut entries = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        if c.u32()? != CENTRAL_SIG {
            return Err(corrupt("bad directory entry"));
        }
        let made_by = c.u16()?;
        let _needed = c.u16()?;
        let flags = c.u16()?;
        let method = c.u16()?;
        c.take(4)?; // time, date
        let crc = c.u32()?;
        let packed = c.u32()?;
        let size = c.u32()?;
        let name_len = usize::from(c.u16()?);
        let extra_len = usize::from(c.u16()?);
        let comment_len = usize::from(c.u16()?);
        c.take(4)?; // disk, internal attributes
        let external = c.u32()?;
        let local = c.u32()?;
        let name = c.take(name_len)?;
        c.take(extra_len + comment_len)?;
        let name = std::str::from_utf8(name)
            .map_err(|_| corrupt("a file name is not UTF-8"))?
            .to_owned();
        if flags & 1 != 0 || flags & (1 << 6) != 0 {
            return Err(corrupt(format!("`{name}` is encrypted")));
        }
        if packed == u32::MAX || size == u32::MAX || local == u32::MAX {
            return Err(corrupt("zip64 is not supported"));
        }
        // Made on Unix: the top of the external attributes is the file mode; only plain files
        // and folders (no links, devices or pipes).
        const UNIX: u16 = 3;
        let mode = external >> 16;
        if made_by >> 8 == UNIX && mode != 0 && !matches!(mode & 0o170_000, 0o100_000 | 0o040_000) {
            return Err(IwbzError::NotAllowed {
                path: name,
                reason: "only plain files (no links or special files)".into(),
            });
        }
        entries.push(Entry {
            name,
            method,
            crc,
            packed: u64::from(packed),
            size: u64::from(size),
            local: local as usize,
        });
    }
    if c.at != end {
        return Err(corrupt("the directory size does not match its entries"));
    }
    Ok((entries, cd_start))
}

/// Writes unpacked files into the empty or missing folder `dir`, for `iwbtg pkg unpack`. The
/// paths have passed [`check_entry`], so each lands inside `dir`.
pub fn write_dir(files: &PackageFiles, dir: &std::path::Path) -> std::io::Result<()> {
    if dir.exists() && std::fs::read_dir(dir)?.next().is_some() {
        return Err(std::io::Error::other(format!("{} is not empty", dir.display())));
    }
    for (path, bytes) in files.iter() {
        let target = dir.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, bytes)?;
    }
    Ok(())
}
