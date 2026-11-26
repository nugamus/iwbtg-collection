//! The `.iwbz` package file (docs/format/iwbz.md): one zip holding a whole built package.
//!
//! [`pack`] writes it, [`unpack`] reads it back into [`PackageFiles`], and
//! [`unpack_selected`] reads only named entries for a preview. [`Archive`] does the same over a
//! `Read + Seek` source, so a preview seeks to the few entries it wants instead of reading the
//! whole archive into memory (#1591). All go through
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
//!   path, no backslash, no drive letter, no Windows device name), and appears once, also
//!   ignoring ASCII case (Windows and macOS would unpack `a.md` and `A.md` to one file);
//! - the archive is at most [`MAX_ARCHIVE_BYTES`], has at most [`MAX_FILES`] files and the files
//!   add up to at most [`MAX_PACKAGE_BYTES`] unpacked, checked from the directory before
//!   anything is inflated, and each file inflates to exactly its stated size (zip bombs);
//! - file data may not overlap (the "overlapping files" zip bomb) and must sit before the
//!   central directory;
//! - every file's CRC-32 matches, and PNG and Ogg files start with their signatures.

use std::io::{Read, Seek, SeekFrom, Write};

use crate::files::{PackageFiles, check_path, lf_normalized};
use crate::ids::check_id;

/// The file extension, without the dot.
pub const EXTENSION: &str = "iwbz";

/// Largest package, unpacked: the format's package size limit (docs/format/assets.md).
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
    /// The bytes are not a zip this reader accepts, or the source could not be read.
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
        // `physics/` holds the optional room profiles (docs/format/physics.md, Room profiles);
        // without it every medley package was refused (#1486).
        "objects" | "timelines" | "paths" | "rooms" | "particles" | "physics" => &["toml"],
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
/// - one level down, by folder: `.toml` in `objects/`, `timelines/`, `paths/`, `rooms/`,
///   `particles/` and `physics/` (also `rooms/<id>.tiles.toml`), `.rhai` in `scripts/`, `.png` and `.toml` in
///   `sprites/`, `backgrounds/`, `fonts/` and `art/`, `.ogg` and `.toml` in `sounds/` and `music/`;
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
    let files = unpack_selected(bytes, |_| true)?;
    if !files.contains("game.toml") {
        return Err(IwbzError::NoManifest);
    }
    Ok(files)
}

/// Reads only the entries `keep` accepts, for a preview that must not inflate a whole package
/// (#1022). The central directory, the archive-wide limits and every local header and data range
/// are still checked; only a kept entry is inflated, CRC-checked and passed to [`check_entry`].
/// A damaged entry that `keep` rejects is therefore accepted here, while [`unpack`] still refuses
/// it. [`unpack`] is this with `keep` accepting everything, plus the `game.toml` requirement.
pub fn unpack_selected(bytes: &[u8], keep: impl Fn(&str) -> bool) -> Result<PackageFiles, IwbzError> {
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
        if !keep(&e.name) {
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
            .map_err(|err| match err {
                crate::files::LoadError::CaseClash { .. } => IwbzError::NotAllowed {
                    path: e.name.clone(),
                    reason: err.to_string(),
                },
                err => corrupt(err.to_string()),
            })?
            .is_some()
        {
            return Err(corrupt(format!("`{}` is in the archive twice", e.name)));
        }
    }
    Ok(files)
}

/// Inflates `data` to exactly `size` bytes, reading at most one byte past it, so a file that
/// claims a small size cannot fill memory. `data` is the entry's packed bytes: a slice from
/// [`unpack_selected`], or a length-limited reader from [`Archive`].
fn inflate(data: impl Read, size: u64) -> Result<Vec<u8>, String> {
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

/// Where the end record's signature starts: the last 22 bytes plus a comment of up to 64 KiB.
fn find_end_record(bytes: &[u8]) -> Option<usize> {
    let lowest = bytes.len().saturating_sub(END_LEN + usize::from(u16::MAX));
    (lowest..=bytes.len().saturating_sub(END_LEN))
        .rev()
        .find(|&i| bytes[i..].starts_with(&END_SIG.to_le_bytes()))
}

/// The end record's fields, checked against the file's length and the directory's position.
struct EndRecord {
    count: u16,
    cd_len: u64,
    cd_start: u64,
}

/// Parses the end record whose signature starts at `end`; `from_end` is the file from there on
/// and `len` the whole file's length.
fn end_record(from_end: &[u8], end: u64, len: u64) -> Result<EndRecord, IwbzError> {
    let mut c = Cursor::new(from_end, 4);
    let disk = c.u16()?;
    let cd_disk = c.u16()?;
    let on_disk = c.u16()?;
    let count = c.u16()?;
    let cd_len = u64::from(c.u32()?);
    let cd_start = u64::from(c.u32()?);
    let comment = u64::from(c.u16()?);
    if disk != 0 || cd_disk != 0 || on_disk != count {
        return Err(corrupt("split archives are not supported"));
    }
    if count == u16::MAX || cd_start == u64::from(u32::MAX) {
        return Err(corrupt("zip64 is not supported"));
    }
    if end + END_LEN as u64 + comment != len {
        return Err(corrupt("bytes after the end record"));
    }
    if cd_start.checked_add(cd_len) != Some(end) {
        return Err(corrupt("the directory is not where the end record says"));
    }
    Ok(EndRecord {
        count,
        cd_len,
        cd_start,
    })
}

/// Parses `count` central directory entries starting at `c`.
fn parse_entries(c: &mut Cursor<'_>, count: u16) -> Result<Vec<Entry>, IwbzError> {
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
    Ok(entries)
}

/// The central directory's entries and where it starts.
fn central_directory(bytes: &[u8]) -> Result<(Vec<Entry>, usize), IwbzError> {
    let end = find_end_record(bytes).ok_or_else(|| corrupt("not a zip file (no end record)"))?;
    let record = end_record(&bytes[end..], end as u64, bytes.len() as u64)?;
    let cd_start = usize::try_from(record.cd_start).map_err(|_| corrupt("the file ends too early"))?;
    let mut c = Cursor::new(bytes, cd_start);
    let entries = parse_entries(&mut c, record.count)?;
    if c.at != end {
        return Err(corrupt("the directory size does not match its entries"));
    }
    Ok((entries, cd_start))
}

/// Maps a seek/read failure on a [`Archive`]'s source to the reader's error.
fn io_error(e: std::io::Error) -> IwbzError {
    corrupt(format!("cannot read the file: {e}"))
}

/// Fills `buf` from the current position.
fn read_next<R: Read>(source: &mut R, buf: &mut [u8]) -> Result<(), IwbzError> {
    source.read_exact(buf).map_err(|e| match e.kind() {
        std::io::ErrorKind::UnexpectedEof => corrupt("the file ends too early"),
        _ => io_error(e),
    })
}

/// Fills `buf` from the absolute offset `at`.
fn read_at<R: Read + Seek>(source: &mut R, at: u64, buf: &mut [u8]) -> Result<(), IwbzError> {
    source.seek(SeekFrom::Start(at)).map_err(io_error)?;
    read_next(source, buf)
}

/// One directory entry with where its file data starts, which [`Archive::open`] learns from the
/// local header.
struct LocalEntry {
    entry: Entry,
    data: u64,
}

/// A `.iwbz` opened on a `Read + Seek` source, for a preview that must not read the whole archive
/// into memory (#1591). [`open`](Archive::open) reads the end record and the central directory,
/// checks the archive-wide limits and every local header and data range, then
/// [`read_selected`](Archive::read_selected) seeks to and reads only the kept files.
///
/// The directory is validated once, so a preview that reads the manifest and then the files it
/// names (the client does up to three passes) pays for it once. Nothing else is retained: peak
/// memory is the directory, the kept files' bytes and one entry's inflate buffer, not the
/// archive.
pub struct Archive<R> {
    source: R,
    entries: Vec<LocalEntry>,
}

impl<R: Read + Seek> Archive<R> {
    /// Opens `source` and checks it: its length, end record, central directory, entry count and
    /// unpacked total, and each entry's local header, name and non-overlapping data range.
    /// Nothing is inflated. The checks are [`unpack_selected`]'s.
    pub fn open(mut source: R) -> Result<Self, IwbzError> {
        let len = source.seek(SeekFrom::End(0)).map_err(io_error)?;
        if len > MAX_ARCHIVE_BYTES {
            return Err(IwbzError::TooLarge(format!(
                "the file is over {MAX_ARCHIVE_BYTES} bytes"
            )));
        }
        // The end record is the last 22 bytes plus a comment of up to 64 KiB. Our packer writes
        // no comment, so try the last 22 bytes alone first: that keeps the common case from
        // reading any file data, which is the point of this reader.
        let widest = usize::try_from(len.min(END_LEN as u64 + u64::from(u16::MAX)))
            .map_err(|_| corrupt("the file ends too early"))?;
        let narrow = END_LEN.min(widest);
        let mut start = len - narrow as u64;
        let mut tail = vec![0u8; narrow];
        read_at(&mut source, start, &mut tail)?;
        let mut off = find_end_record(&tail);
        if off.is_none() && narrow < widest {
            start = len - widest as u64;
            tail = vec![0u8; widest];
            read_at(&mut source, start, &mut tail)?;
            off = find_end_record(&tail);
        }
        let off = off.ok_or_else(|| corrupt("not a zip file (no end record)"))?;
        let record = end_record(&tail[off..], start + off as u64, len)?;

        // The directory, whose size the end record checked against its own position. Reading it
        // is what the issue allows to scale with the archive: it is the index the preview needs.
        source.seek(SeekFrom::Start(record.cd_start)).map_err(io_error)?;
        let mut cd = Vec::with_capacity(record.cd_len.min(64 * 1024) as usize);
        (&mut source)
            .take(record.cd_len)
            .read_to_end(&mut cd)
            .map_err(io_error)?;
        if cd.len() as u64 != record.cd_len {
            return Err(corrupt("the file ends too early"));
        }
        let mut c = Cursor::new(&cd, 0);
        let mut entries = parse_entries(&mut c, record.count)?;
        if c.at as u64 != record.cd_len {
            return Err(corrupt("the directory size does not match its entries"));
        }

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

        // File data in archive order: each must start after the one before ends. Only the local
        // headers are read here; an entry's data is read only by `read_selected`.
        entries.sort_by_key(|e| e.local);
        let mut locals = Vec::with_capacity(entries.len());
        let mut end_of_last = 0u64;
        for entry in entries {
            let local = entry.local as u64;
            if local < end_of_last {
                return Err(corrupt(format!("`{}` overlaps another file", entry.name)));
            }
            let mut header = [0u8; 30];
            read_at(&mut source, local, &mut header)?;
            if u32::from_le_bytes([header[0], header[1], header[2], header[3]]) != LOCAL_SIG {
                return Err(corrupt(format!("`{}`: no local header", entry.name)));
            }
            let name_len = usize::from(u16::from_le_bytes([header[26], header[27]]));
            let extra_len = usize::from(u16::from_le_bytes([header[28], header[29]]));
            // The name follows the header; the header read left the position there.
            let mut name = vec![0u8; name_len];
            read_next(&mut source, &mut name)?;
            if name != entry.name.as_bytes() {
                return Err(corrupt(format!(
                    "`{}`: the two headers name different files",
                    entry.name
                )));
            }
            let data = local
                .checked_add(30 + name_len as u64 + extra_len as u64)
                .ok_or_else(|| corrupt("the file ends too early"))?;
            let entry_end = data
                .checked_add(entry.packed)
                .ok_or_else(|| corrupt("the file ends too early"))?;
            if entry_end > len {
                return Err(corrupt("the file ends too early"));
            }
            if entry_end > record.cd_start || data < local {
                return Err(corrupt(format!("`{}` runs into the directory", entry.name)));
            }
            if entry.name.ends_with('/') && entry.size != 0 {
                return Err(corrupt(format!("folder entry `{}` has data", entry.name)));
            }
            end_of_last = entry_end;
            locals.push(LocalEntry { entry, data });
        }
        Ok(Self {
            source,
            entries: locals,
        })
    }

    /// Reads only the entries `keep` accepts, with [`unpack_selected`]'s checks: every kept
    /// entry's CRC-32 matches and it passes [`check_entry`]. Folder entries are skipped.
    pub fn read_selected(&mut self, keep: impl Fn(&str) -> bool) -> Result<PackageFiles, IwbzError> {
        let mut files = PackageFiles::new();
        for local in &self.entries {
            let e = &local.entry;
            if e.name.ends_with('/') || !keep(&e.name) {
                continue;
            }
            let out = match e.method {
                STORED if e.packed == e.size => {
                    let n = usize::try_from(e.size).map_err(|_| corrupt("file too large"))?;
                    self.source.seek(SeekFrom::Start(local.data)).map_err(io_error)?;
                    let mut out = Vec::with_capacity(n);
                    (&mut self.source)
                        .take(e.packed)
                        .read_to_end(&mut out)
                        .map_err(io_error)?;
                    if out.len() as u64 != e.packed {
                        return Err(corrupt("the file ends too early"));
                    }
                    out
                }
                STORED => return Err(corrupt(format!("`{}`: stored sizes differ", e.name))),
                DEFLATE => {
                    self.source.seek(SeekFrom::Start(local.data)).map_err(io_error)?;
                    inflate((&mut self.source).take(e.packed), e.size)
                        .map_err(|m| corrupt(format!("`{}`: {m}", e.name)))?
                }
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
                .map_err(|err| match err {
                    crate::files::LoadError::CaseClash { .. } => IwbzError::NotAllowed {
                        path: e.name.clone(),
                        reason: err.to_string(),
                    },
                    err => corrupt(err.to_string()),
                })?
                .is_some()
            {
                return Err(corrupt(format!("`{}` is in the archive twice", e.name)));
            }
        }
        Ok(files)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::io::Cursor;
    use std::rc::Rc;

    fn package() -> PackageFiles {
        let mut files = PackageFiles::new();
        files.insert("game.toml", b"id = \"x\"\n".to_vec()).unwrap();
        files
            .insert("rooms/a.toml", b"size = [800, 608]\n".to_vec())
            .unwrap();
        // Large enough that the packer deflates it, so both methods are covered.
        files
            .insert("rooms/big.toml", b"size = [800, 608]\n".repeat(1_000))
            .unwrap();
        files
            .insert("art/key.png", b"\x89PNG\r\n\x1a\nkey".to_vec())
            .unwrap();
        files.insert("sounds/click.ogg", b"OggSclick".to_vec()).unwrap();
        files
    }

    /// Incompressible bytes, so the packer stores them and the archive is as large as the entry.
    fn incompressible(size: usize) -> Vec<u8> {
        let mut data = vec![0u8; size];
        let mut x = 0x1234_5678u32;
        for byte in &mut data[4..] {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *byte = (x >> 24) as u8;
        }
        data[..4].copy_from_slice(b"OggS");
        data
    }

    /// The seek reader gives the bytes reader's result for the same `keep`.
    #[test]
    fn a_seekable_read_matches_the_bytes_reader() {
        let bytes = pack(&package()).unwrap();
        let (entries, _) = central_directory(&bytes).unwrap();
        assert!(
            entries.iter().any(|e| e.method == DEFLATE),
            "a deflated entry is covered"
        );
        let cases: [fn(&str) -> bool; 4] = [
            |_| true,
            |name| name == "game.toml",
            |name| name.starts_with("rooms/"),
            |_| false,
        ];
        for keep in cases {
            let from_bytes = unpack_selected(&bytes, keep).unwrap();
            let mut archive = Archive::open(Cursor::new(bytes.as_slice())).unwrap();
            assert_eq!(archive.read_selected(keep).unwrap(), from_bytes);
        }
    }

    /// The seek reader also refuses what the bytes reader refuses, at every truncation.
    #[test]
    fn a_seekable_read_refuses_what_the_bytes_reader_refuses() {
        let bytes = pack(&package()).unwrap();
        for cut in [0, 10, bytes.len() / 2, bytes.len() - 1] {
            let short = &bytes[..cut];
            let from_bytes = unpack_selected(short, |_| true);
            let from_seek = Archive::open(Cursor::new(short)).and_then(|mut a| a.read_selected(|_| true));
            assert_eq!(
                from_bytes.is_ok(),
                from_seek.is_ok(),
                "cut at {cut}: bytes {from_bytes:?}, seek {from_seek:?}"
            );
        }
        assert!(Archive::open(Cursor::new(b"PK not really".as_slice())).is_err());
    }

    /// A comment before the end record needs the wider tail scan, and still reads like the bytes
    /// reader (#1591).
    #[test]
    fn an_end_record_with_a_comment_is_still_found() {
        let mut bytes = pack(&package()).unwrap();
        let comment = b"a comment after the directory";
        let end = bytes.len();
        bytes.extend_from_slice(comment);
        bytes[end - 2..end].copy_from_slice(&(comment.len() as u16).to_le_bytes());
        let mut archive = Archive::open(Cursor::new(bytes.as_slice())).unwrap();
        assert_eq!(archive.read_selected(|_| true).unwrap(), unpack(&bytes).unwrap());
    }

    /// A source that fails if the reader reads any byte of the large entry's data, and counts the
    /// bytes it does read.
    struct Guarded {
        inner: Cursor<Vec<u8>>,
        forbidden: std::ops::Range<u64>,
        read: Rc<Cell<u64>>,
    }

    impl Read for Guarded {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let at = self.inner.position();
            let end = at + buf.len() as u64;
            if at < self.forbidden.end && end > self.forbidden.start {
                return Err(std::io::Error::other("the large entry's data was read"));
            }
            let n = self.inner.read(buf)?;
            self.read.set(self.read.get() + n as u64);
            Ok(n)
        }
    }

    impl Seek for Guarded {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.inner.seek(pos)
        }
    }

    /// #1591: a preview of a package with a large unrelated entry reads the end record, the
    /// directory and the kept entry only; the large entry's bytes are never read.
    #[test]
    fn a_large_unread_entry_is_never_read() {
        let mut files = package();
        files
            .insert("sounds/big.ogg", incompressible(4 * 1024 * 1024))
            .unwrap();
        let bytes = pack(&files).unwrap();
        // Where the big entry's bytes sit, from the directory and its local header.
        let (entries, _) = central_directory(&bytes).unwrap();
        let big = entries.iter().find(|e| e.name == "sounds/big.ogg").unwrap();
        let local = big.local;
        let name_len = usize::from(u16::from_le_bytes([bytes[local + 26], bytes[local + 27]]));
        let extra_len = usize::from(u16::from_le_bytes([bytes[local + 28], bytes[local + 29]]));
        let data = (local + 30 + name_len + extra_len) as u64;
        let end = data + big.packed;

        let read = Rc::new(Cell::new(0));
        let source = Guarded {
            inner: Cursor::new(bytes.clone()),
            forbidden: data..end,
            read: Rc::clone(&read),
        };
        let mut archive = Archive::open(source).unwrap();
        let after_open = read.get();
        let preview = archive.read_selected(|name| name == "game.toml").unwrap();
        let total = read.get();
        assert!(preview.contains("game.toml"));
        assert!(!preview.contains("sounds/big.ogg"));
        assert!(
            after_open < 64 * 1024,
            "open read {after_open} bytes of a {} byte archive",
            bytes.len()
        );
        assert!(
            total < 64 * 1024 && total < bytes.len() as u64 / 8,
            "the preview read {total} bytes of a {} byte archive",
            bytes.len()
        );

        // The entry really is there: damaging its data breaks a read that selects it, while a
        // preview that does not select it is unaffected.
        let mut damaged = bytes.clone();
        damaged[data as usize + 100] ^= 0xff;
        let mut archive = Archive::open(Cursor::new(damaged.as_slice())).unwrap();
        assert!(archive.read_selected(|name| name == "game.toml").is_ok());
        assert!(archive.read_selected(|name| name == "sounds/big.ogg").is_err());
    }

    /// A source that only claims a length; `Archive::open` must refuse it before reading.
    struct Claimed(u64);

    impl Read for Claimed {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            unreachable!("the length check comes before any read")
        }
    }

    impl Seek for Claimed {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            match pos {
                SeekFrom::End(0) => Ok(self.0),
                _ => Ok(0),
            }
        }
    }

    #[test]
    fn an_archive_over_the_limit_is_refused_before_it_is_read() {
        assert!(matches!(
            Archive::open(Claimed(MAX_ARCHIVE_BYTES + 1)),
            Err(IwbzError::TooLarge(_))
        ));
    }

    /// Walks the client's preview passes over `read`, a reader that selects entries by name: the
    /// manifest, then the key art, first ranked room and cherry, then that room's backgrounds.
    fn preview_with(mut read: impl FnMut(&dyn Fn(&str) -> bool) -> PackageFiles) -> PackageFiles {
        use std::collections::BTreeSet;

        let mut files = read(&|name| name == "game.toml");
        let Some(manifest) = files
            .get("game.toml")
            .and_then(|b| std::str::from_utf8(b).ok())
            .and_then(|t| crate::Manifest::parse(t).ok())
        else {
            return files;
        };
        let room = crate::key_art::first_ranked_room(&manifest);
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
        files = read(&|name| wanted.contains(name));
        if let Ok(art) = crate::art::load_room_art(&files, room) {
            let before = wanted.len();
            wanted.extend(
                art.backgrounds
                    .iter()
                    .filter(|b| !b.foreground)
                    .map(|b| format!("backgrounds/{}.png", b.image)),
            );
            if wanted.len() > before {
                files = read(&|name| wanted.contains(name));
            }
        }
        files
    }

    /// A file that counts the bytes read through it.
    struct CountingFile(std::fs::File, Rc<Cell<u64>>);

    impl Read for CountingFile {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.0.read(buf)?;
            self.1.set(self.1.get() + n as u64);
            Ok(n)
        }
    }

    impl Seek for CountingFile {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.0.seek(pos)
        }
    }

    /// #1591 measurement, not a check. The pre-#1591 client preview read the whole compressed
    /// archive (`std::fs::read`) before selecting entries; `Archive` opens the file and seeks.
    /// Both run the client's three preview passes over hardlinked copies (warm OS cache), and the
    /// seek path's bytes read are counted. Run:
    /// `cargo test -p iwbtg-format --lib iwbz::tests::seek_preview_measurement -- --ignored --nocapture`
    #[test]
    #[ignore = "a measurement, not a check (#1591)"]
    fn seek_preview_measurement() {
        use std::time::Instant;

        // The package of PR #1589's measurement: showcase.pkg plus a 16 MB unrelated sound.
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../client/showcase.pkg");
        let mut files = PackageFiles::load_dir(&folder).unwrap();
        let mut big = vec![0u8; 16 * 1024 * 1024];
        big[..4].copy_from_slice(b"OggS");
        files.insert("sounds/big.ogg", big).unwrap();
        let unpacked: usize = files.iter().map(|(_, b)| b.len()).sum();
        let packed = pack(&files).unwrap();
        drop(files);

        let dir = std::env::temp_dir().join(format!("iwbtg-seek-measure-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("showcase.iwbz");
        std::fs::write(&source, &packed).unwrap();
        let baseline = preview_with(|keep| unpack_selected(&packed, keep).unwrap());
        let preview_bytes: usize = baseline.iter().map(|(_, b)| b.len()).sum();

        // One old preview (whole file read) and one seek preview, counting bytes read.
        let started = Instant::now();
        let whole = std::fs::read(&source).unwrap();
        let old_preview = preview_with(|keep| unpack_selected(&whole, keep).unwrap());
        let old_ms = started.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(old_preview, baseline, "the previews differ");

        let read = Rc::new(Cell::new(0));
        let counting = CountingFile(std::fs::File::open(&source).unwrap(), Rc::clone(&read));
        let started = Instant::now();
        let mut archive = Archive::open(counting).unwrap();
        let seek_preview = preview_with(|keep| archive.read_selected(keep).unwrap());
        let seek_ms = started.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(seek_preview, baseline, "the previews differ");
        let seek_read = read.get();

        let started = Instant::now();
        let full = unpack(&packed).unwrap();
        let full_ms = started.elapsed().as_secs_f64() * 1000.0;
        let full_files: usize = full.iter().map(|(_, b)| b.len()).sum();
        drop(full);
        println!(
            "archive {} bytes, {unpacked} unpacked, preview keeps {preview_bytes} bytes",
            packed.len()
        );
        println!(
            "full unpack: retains {} bytes, {full_ms:.1} ms",
            packed.len() + full_files
        );
        println!(
            "old preview: reads/retains {} bytes (whole archive + preview files), {old_ms:.1} ms",
            packed.len() + preview_bytes
        );
        println!(
            "seek preview: reads {seek_read} bytes (end record + directory + local headers + preview files), {seek_ms:.1} ms"
        );

        for count in [20usize, 100, 500] {
            let mut paths = Vec::with_capacity(count);
            for i in 0..count {
                let path = dir.join(format!("copy-{count}-{i}.iwbz"));
                if std::fs::hard_link(&source, &path).is_err() {
                    std::fs::copy(&source, &path).unwrap();
                }
                paths.push(path);
            }
            for path in &paths {
                std::fs::read(path).unwrap();
            }
            let started = Instant::now();
            for path in &paths {
                let whole = std::fs::read(path).unwrap();
                assert_eq!(
                    preview_with(|keep| unpack_selected(&whole, keep).unwrap()),
                    baseline
                );
            }
            let old_ms = started.elapsed().as_secs_f64() * 1000.0;
            let started = Instant::now();
            for path in &paths {
                let mut archive = Archive::open(std::fs::File::open(path).unwrap()).unwrap();
                assert_eq!(
                    preview_with(|keep| archive.read_selected(keep).unwrap()),
                    baseline
                );
            }
            let seek_ms = started.elapsed().as_secs_f64() * 1000.0;
            println!("{count} previews, warm: old {old_ms:.1} ms, seek {seek_ms:.1} ms");
        }

        // The case #1591 targets: a large archive the preview must not read. A 64 MiB Ogg that
        // does not compress, so the file really is large.
        let mut files = PackageFiles::load_dir(&folder).unwrap();
        files
            .insert("sounds/big.ogg", incompressible(64 * 1024 * 1024))
            .unwrap();
        let unpacked: usize = files.iter().map(|(_, b)| b.len()).sum();
        let packed = pack(&files).unwrap();
        drop(files);
        let large = dir.join("large.iwbz");
        std::fs::write(&large, &packed).unwrap();
        let baseline = preview_with(|keep| unpack_selected(&packed, keep).unwrap());
        let started = Instant::now();
        let whole = std::fs::read(&large).unwrap();
        let old_preview = preview_with(|keep| unpack_selected(&whole, keep).unwrap());
        let old_ms = started.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(old_preview, baseline, "the previews differ");
        let read = Rc::new(Cell::new(0));
        let counting = CountingFile(std::fs::File::open(&large).unwrap(), Rc::clone(&read));
        let started = Instant::now();
        let mut archive = Archive::open(counting).unwrap();
        let seek_preview = preview_with(|keep| archive.read_selected(keep).unwrap());
        let seek_ms = started.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(seek_preview, baseline, "the previews differ");
        println!(
            "large archive {} bytes ({unpacked} unpacked), one preview: old reads {} bytes in {old_ms:.1} ms, seek reads {} bytes in {seek_ms:.1} ms",
            packed.len(),
            packed.len(),
            read.get()
        );

        let mut paths = Vec::with_capacity(20);
        for i in 0..20 {
            let path = dir.join(format!("large-{i}.iwbz"));
            if std::fs::hard_link(&large, &path).is_err() {
                std::fs::copy(&large, &path).unwrap();
            }
            paths.push(path);
        }
        for path in &paths {
            std::fs::read(path).unwrap();
        }
        let started = Instant::now();
        for path in &paths {
            let whole = std::fs::read(path).unwrap();
            assert_eq!(
                preview_with(|keep| unpack_selected(&whole, keep).unwrap()),
                baseline
            );
        }
        let old_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        for path in &paths {
            let mut archive = Archive::open(std::fs::File::open(path).unwrap()).unwrap();
            assert_eq!(
                preview_with(|keep| archive.read_selected(keep).unwrap()),
                baseline
            );
        }
        let seek_ms = started.elapsed().as_secs_f64() * 1000.0;
        println!("large archive, 20 previews warm: old {old_ms:.1} ms, seek {seek_ms:.1} ms");
        std::fs::remove_dir_all(&dir).ok();
    }
}
