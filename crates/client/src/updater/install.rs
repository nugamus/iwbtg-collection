//! What the updater does to a file on disk (#1606): verify the downloaded zip's sha256, read the
//! one `.exe` out of it, swap that exe in beside the running one and put the old one back if any
//! step fails.
//!
//! Nothing here knows about HTTP or the UI, so the swap plan runs against any pair of paths and
//! its tests use a temp folder. Saves, settings, stats and packages are never in an [`InstallPlan`]:
//! only the client's own executable changes.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// The zip's SHA-256, lowercase hex. The manifest's first 64-hex token is taken, so the usual
/// `sha256sum` format (`<hash>  <name>`) and a bare hash both read.
pub fn expected_digest(manifest: &str) -> Option<String> {
    manifest.lines().find_map(|line| {
        let token = line.split_whitespace().next()?;
        (token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| token.to_ascii_lowercase())
    })
}

/// The bytes' SHA-256, lowercase hex.
pub fn digest(bytes: &[u8]) -> String {
    let hash = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in hash {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Checks the download against `sha256.txt` before anything is unpacked. A mismatch names no
/// hashes (they are only noise to a player) but is an error the caller reports and deletes.
pub fn verify(bytes: &[u8], manifest: &str) -> Result<(), String> {
    let Some(expected) = expected_digest(manifest) else {
        return Err("the update has no checksum, so it was not installed".into());
    };
    if digest(bytes) == expected {
        Ok(())
    } else {
        Err("the update did not match its checksum, so it was deleted".into())
    }
}

/// One entry of a zip's central directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntry {
    pub name: String,
    method: u16,
    compressed: usize,
    size: usize,
    /// Where the entry's local header starts.
    header: usize,
}

const EOCD: u32 = 0x0605_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;
/// The most the end-of-central-directory record and its comment may take.
const EOCD_SEARCH: usize = 22 + 65_535;
/// The largest exe the client accepts out of a release zip; the release is about 40 MB zipped.
pub const MAX_EXE: usize = 300 * 1024 * 1024;

/// The first entry whose name ends with `suffix` (the release's `iwbtg-client.exe`), from the zip's
/// central directory: sizes there are always filled in, unlike a local header with a data
/// descriptor.
pub fn find_entry(zip: &[u8], suffix: &str) -> Option<ZipEntry> {
    let start = zip.len().saturating_sub(EOCD_SEARCH);
    let eocd = (start..zip.len().saturating_sub(3))
        .rev()
        .find(|&at| le32(zip, at) == Some(EOCD))?;
    let count = le16(zip, eocd + 10)? as usize;
    let mut at = le32(zip, eocd + 16)? as usize;
    for _ in 0..count {
        if le32(zip, at) != Some(CENTRAL) {
            return None;
        }
        let method = le16(zip, at + 10)?;
        let compressed = le32(zip, at + 20)? as usize;
        let size = le32(zip, at + 24)? as usize;
        let name_len = le16(zip, at + 28)? as usize;
        let extra_len = le16(zip, at + 30)? as usize;
        let comment_len = le16(zip, at + 32)? as usize;
        let header = le32(zip, at + 42)? as usize;
        let name_at = at + 46;
        let name = zip.get(name_at..name_at + name_len)?;
        let name = String::from_utf8_lossy(name).into_owned();
        if name.ends_with(suffix) {
            return Some(ZipEntry {
                name,
                method,
                compressed,
                size,
                header,
            });
        }
        at = name_at + name_len + extra_len + comment_len;
    }
    None
}

/// Unpacks one entry. Stored deflates pass through; compressed ones are raw DEFLATE, which is what
/// `zipfile.ZIP_DEFLATED` writes.
pub fn extract(zip: &[u8], entry: &ZipEntry) -> Result<Vec<u8>, String> {
    if entry.size > MAX_EXE {
        return Err(format!(
            "{} is larger than the client installs",
            entry.name
        ));
    }
    if le32(zip, entry.header) != Some(LOCAL) {
        return Err(format!("{}: not a zip entry", entry.name));
    }
    let name_len = le16(zip, entry.header + 26).ok_or("truncated zip header")? as usize;
    let extra_len = le16(zip, entry.header + 28).ok_or("truncated zip header")? as usize;
    let data_at = entry.header + 30 + name_len + extra_len;
    let data = zip
        .get(data_at..data_at + entry.compressed)
        .ok_or_else(|| format!("{}: truncated zip data", entry.name))?;
    let bytes = match entry.method {
        0 => data.to_vec(),
        8 => {
            let mut out = Vec::with_capacity(entry.size);
            flate2::read::DeflateDecoder::new(data)
                .read_to_end(&mut out)
                .map_err(|e| format!("{}: {e}", entry.name))?;
            out
        }
        method => return Err(format!("{}: unsupported zip method {method}", entry.name)),
    };
    if bytes.len() != entry.size {
        return Err(format!("{}: unpacked to the wrong size", entry.name));
    }
    Ok(bytes)
}

/// Writes the exe out of `zip` to `dir/iwbtg-client.exe`, the file the update will install.
pub fn stage_exe(zip: &[u8], dir: &Path) -> Result<PathBuf, String> {
    let entry = find_entry(zip, "iwbtg-client.exe")
        .ok_or("the update zip has no iwbtg-client.exe")?;
    let bytes = extract(zip, &entry)?;
    fs::create_dir_all(dir).map_err(|e| format!("the update folder could not be made: {e}"))?;
    let exe = dir.join("iwbtg-client.exe");
    fs::write(&exe, bytes).map_err(|e| format!("the update could not be unpacked: {e}"))?;
    Ok(exe)
}

/// Where the running exe is moved while the new one is copied in.
pub fn old_path(exe: &Path) -> PathBuf {
    let mut name = exe.as_os_str().to_owned();
    name.push(".old");
    PathBuf::from(name)
}

/// The swap: move `current` aside, copy `staged` in its place. If the copy fails the old exe is
/// moved back, so a failed install leaves a client that still runs.
pub fn swap(staged: &Path, current: &Path) -> Result<PathBuf, String> {
    let old = old_path(current);
    if old.exists() {
        fs::remove_file(&old).map_err(|e| format!("the previous update could not be cleared: {e}"))?;
    }
    fs::rename(current, &old)
        .map_err(|e| format!("the running client could not be moved aside: {e}"))?;
    match fs::copy(staged, current) {
        Ok(_) => Ok(old),
        Err(e) => {
            let _ = fs::rename(&old, current);
            Err(format!("the update could not be put in place: {e}"))
        }
    }
}

/// Installs the staged exe over the running one and starts it again. The caller exits at once
/// after this returns `Ok`: two clients must never run from the same settings file.
pub fn install_and_restart(staged: &Path) -> Result<(), String> {
    let current = std::env::current_exe()
        .map_err(|e| format!("the running client's path is unknown: {e}"))?;
    let old = swap(staged, &current)?;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if let Err(e) = Command::new(&current).args(args).spawn() {
        // A new exe that cannot start is worse than the old one: put the old one back.
        let _ = fs::remove_file(&current);
        let _ = fs::rename(&old, &current);
        return Err(format!("the updated client could not start: {e}"));
    }
    Ok(())
}

/// Deletes the `.old` exe a previous update left, on the next start. A failure only costs disk
/// space, so it is reported and ignored.
pub fn remove_old_on_start() {
    let Ok(current) = std::env::current_exe() else {
        return;
    };
    let old = old_path(&current);
    if old.exists()
        && let Err(e) = fs::remove_file(&old)
    {
        crate::storage::warn(&format!("an old client build could not be deleted: {e}"));
    }
}

fn le16(bytes: &[u8], at: usize) -> Option<u16> {
    let b = bytes.get(at..at + 2)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("iwbtg-updater-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A one-entry zip, written the way `zipfile.ZIP_DEFLATED` writes one: local header, data,
    /// central directory, end record.
    fn zip_bytes(name: &str, data: &[u8], deflate: bool) -> Vec<u8> {
        let payload = if deflate {
            use std::io::Write;
            let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            enc.write_all(data).unwrap();
            enc.finish().unwrap()
        } else {
            data.to_vec()
        };
        let method: u16 = if deflate { 8 } else { 0 };
        let crc = {
            let mut crc = 0u32;
            for &b in data {
                crc = crc32(crc, b);
            }
            crc
        };
        let mut out = Vec::new();
        // Local header.
        out.extend_from_slice(&LOCAL.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&payload);
        let central_at = out.len() as u32;
        // Central directory.
        out.extend_from_slice(&CENTRAL.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        let central_size = out.len() as u32 - central_at;
        // End of central directory.
        out.extend_from_slice(&EOCD.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&central_size.to_le_bytes());
        out.extend_from_slice(&central_at.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    /// The zip test fixture's own CRC, so a hand-built central directory is valid enough.
    fn crc32(mut crc: u32, byte: u8) -> u32 {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
        crc
    }

    #[test]
    fn the_manifest_takes_the_hash_and_ignores_the_name() {
        let hash = "a".repeat(64);
        assert_eq!(expected_digest(&format!("{hash}  client.zip\n")), Some(hash.clone()));
        assert_eq!(expected_digest(&hash), Some(hash));
        assert_eq!(expected_digest("not a hash\n"), None);
        assert_eq!(expected_digest(""), None);
    }

    #[test]
    fn a_download_must_match_its_checksum() {
        let bytes = b"the new client";
        let good = format!("{}  client.zip\n", digest(bytes));
        assert!(verify(bytes, &good).is_ok());
        let bad = format!("{}  client.zip\n", digest(b"something else"));
        let err = verify(bytes, &bad).unwrap_err();
        assert!(err.contains("checksum"), "{err}");
        assert!(verify(bytes, "no digest here").is_err());
    }

    #[test]
    fn the_exe_is_read_out_of_a_stored_and_a_deflated_zip() {
        for deflate in [false, true] {
            let zip = zip_bytes("main-latest/iwbtg-client.exe", b"binary", deflate);
            let entry = find_entry(&zip, "iwbtg-client.exe").expect("the entry");
            assert_eq!(entry.name, "main-latest/iwbtg-client.exe");
            assert_eq!(extract(&zip, &entry).unwrap(), b"binary");
        }
        let zip = zip_bytes("readme.txt", b"x", false);
        assert!(find_entry(&zip, "iwbtg-client.exe").is_none());
    }

    #[test]
    fn staging_writes_the_exe_beside_the_zip() {
        let dir = temp("stage");
        let zip = zip_bytes("main-latest/iwbtg-client.exe", b"binary", true);
        let exe = stage_exe(&zip, &dir).unwrap();
        assert_eq!(exe, dir.join("iwbtg-client.exe"));
        assert_eq!(fs::read(&exe).unwrap(), b"binary");
        let _ = fs::remove_dir_all(&dir);
    }

    /// #1606: a good swap leaves the new exe running and the old one as `.old`; a failed copy puts
    /// the old exe back.
    #[test]
    fn the_swap_keeps_the_old_exe_when_the_new_one_cannot_be_copied() {
        let dir = temp("swap");
        let current = dir.join("iwbtg-client.exe");
        let staged = dir.join("staged.exe");
        fs::write(&current, b"old").unwrap();
        fs::write(&staged, b"new").unwrap();

        let old = swap(&staged, &current).unwrap();
        assert_eq!(old, dir.join("iwbtg-client.exe.old"));
        assert_eq!(fs::read(&current).unwrap(), b"new");
        assert_eq!(fs::read(&old).unwrap(), b"old");

        // The staged file is gone: the swap must fail and restore the previous exe.
        fs::remove_file(&staged).unwrap();
        let err = swap(&staged, &current).unwrap_err();
        assert!(err.contains("could not"), "{err}");
        assert_eq!(fs::read(&current).unwrap(), b"new", "the running exe stays");
        let _ = fs::remove_dir_all(&dir);
    }
}
