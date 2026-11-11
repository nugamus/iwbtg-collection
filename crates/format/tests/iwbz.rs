//! The `.iwbz` container (docs/format/iwbz.md, #36): packing round-trips, and the reader refuses
//! hostile archives: path traversal, files the format does not allow, links and zip bombs.

use std::io::Write;
use std::path::Path;

use iwbtg_format::PackageFiles;
use iwbtg_format::iwbz::{IwbzError, MAX_FILES, MAX_PACKAGE_BYTES, check_entry, pack, unpack};

fn demo() -> PackageFiles {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../client/demo-clear.pkg");
    PackageFiles::load_dir(dir).unwrap()
}

#[test]
fn a_package_round_trips_with_the_same_content_hash() {
    let files = demo();
    let packed = pack(&files).unwrap();
    let back = unpack(&packed).unwrap();
    assert_eq!(back.content_hash(), files.content_hash());
    assert_eq!(back.len(), files.len());
    assert_eq!(pack(&back).unwrap(), packed, "packing is reproducible");
}

#[test]
fn tests_are_left_out_and_crlf_packs_like_lf() {
    let files = demo();
    let mut other = PackageFiles::new();
    for (path, bytes) in files.iter() {
        let bytes = if path.ends_with(".toml") {
            String::from_utf8(bytes.to_vec())
                .unwrap()
                .replace("\r\n", "\n")
                .replace('\n', "\r\n")
                .into_bytes()
        } else {
            bytes.to_vec()
        };
        other.insert(path, bytes).unwrap();
    }
    other.insert("tests/inputs/a.inputs", b"x".to_vec()).unwrap();
    let packed = pack(&other).unwrap();
    assert_eq!(packed, pack(&files).unwrap());
    assert!(!unpack(&packed).unwrap().iter().any(|(p, _)| p.starts_with("tests/")));
}

#[test]
fn pack_refuses_what_a_package_may_not_hold() {
    let mut files = demo();
    files.insert("scripts/boss.dll", b"MZ".to_vec()).unwrap();
    assert!(matches!(pack(&files), Err(IwbzError::NotAllowed { .. })));
    let mut files = PackageFiles::new();
    files.insert("rooms/a.toml", b"".to_vec()).unwrap();
    assert_eq!(pack(&files), Err(IwbzError::NoManifest));
}

#[test]
fn the_allow_list() {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n...";
    let ok = [
        ("game.toml", &b""[..]),
        ("physics.toml", b""),
        ("CREDITS.md", b""),
        ("rooms/stage1_01.toml", b""),
        ("rooms/stage1_01.tiles.toml", b""),
        ("scripts/boss1.rhai", b""),
        ("sprites/kid.png", PNG),
        ("sprites/kid.toml", b""),
        ("music/stage.ogg", b"OggS..."),
        ("engine/a1/engine.toml", b""),
        ("engine/a1/sprites/kid.png", PNG),
    ];
    for (p, b) in ok {
        assert_eq!(check_entry(p, b), Ok(()), "{p}");
    }
    let bad = [
        ("../game.toml", &b""[..]),
        ("/game.toml", b""),
        ("C:/game.toml", b""),
        ("rooms\\a.toml", b""),
        ("tests/replays/a.iwbr", b""),
        ("run.exe", b"MZ"),
        ("scripts/a.dll", b"MZ"),
        ("scripts/a.wasm", b""),
        ("scripts/a.lua", b""),
        ("rooms/a.rhai", b""),
        ("rooms/sub/a.toml", b""),
        ("sprites/kid.png", b"MZ not a png"),
        ("sounds/a.ogg", b"RIFF"),
        ("sprites/Kid.png", PNG),
        ("rooms/con.toml", b""),
        ("CON.md", b""),
        ("game", b""),
        ("other/a.toml", b""),
        ("engine/a1/x/a.toml", b""),
        ("engine/a1.toml", b""),
        ("engine/../a.toml", b""),
    ];
    for (p, b) in bad {
        assert!(check_entry(p, b).is_err(), "{p} accepted");
    }
}

/// One file of a hand-made zip.
struct Raw {
    name: &'static str,
    data: Vec<u8>,
    method: u16,
    size: u32,
    crc: u32,
    flags: u16,
    /// "Version made by" and external attributes (Unix mode in the top 16 bits).
    made_by: u16,
    external: u32,
}

fn raw(name: &'static str, data: &[u8]) -> Raw {
    Raw {
        name,
        data: data.to_vec(),
        method: 0,
        size: data.len() as u32,
        crc: crc32fast::hash(data),
        flags: 0,
        made_by: 20,
        external: 0,
    }
}

fn deflated(name: &'static str, data: &[u8]) -> Raw {
    let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    e.write_all(data).unwrap();
    Raw {
        data: e.finish().unwrap(),
        method: 8,
        ..raw(name, data)
    }
}

/// Builds a zip. `local_at` overrides each entry's local header offset (overlap tests).
fn zip(entries: &[Raw], local_at: Option<&[u32]>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut offsets = Vec::new();
    for e in entries {
        offsets.push(out.len() as u32);
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        for v in [20u16, e.flags, e.method, 0, 0x21] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in [e.crc, e.data.len() as u32, e.size] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(e.name.as_bytes());
        out.extend_from_slice(&e.data);
    }
    let offsets = local_at.map(<[u32]>::to_vec).unwrap_or(offsets);
    let cd = out.len();
    for (e, off) in entries.iter().zip(offsets) {
        out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        for v in [e.made_by, 20, e.flags, e.method, 0, 0x21] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in [e.crc, e.data.len() as u32, e.size] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in [e.name.len() as u16, 0, 0, 0, 0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&e.external.to_le_bytes());
        out.extend_from_slice(&off.to_le_bytes());
        out.extend_from_slice(e.name.as_bytes());
    }
    let cd_len = out.len() - cd;
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    for v in [0u16, 0, entries.len() as u16, entries.len() as u16] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&(cd_len as u32).to_le_bytes());
    out.extend_from_slice(&(cd as u32).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn manifest() -> Raw {
    raw("game.toml", b"id = \"x\"\n")
}

#[test]
fn a_hand_made_zip_with_folders_and_deflate_unpacks() {
    let files = unpack(&zip(
        &[
            manifest(),
            raw("rooms/", b""),
            deflated("rooms/a.toml", &b"size = [800, 608]\n".repeat(50)),
        ],
        None,
    ))
    .unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files.get("rooms/a.toml").unwrap().len(), 18 * 50);
}

#[test]
fn path_traversal_is_refused() {
    for name in ["../evil.toml", "/evil.toml", "C:/evil.toml", "rooms\\..\\..\\evil.toml", "rooms/../../evil.toml"] {
        let err = unpack(&zip(&[manifest(), raw(name, b"")], None)).unwrap_err();
        assert!(matches!(err, IwbzError::NotAllowed { .. }), "{name}: {err}");
    }
}

#[test]
fn code_tests_and_unknown_files_are_refused() {
    for name in ["scripts/a.dll", "game.exe", "tests/inputs/a.toml", "sprites/a.png"] {
        let err = unpack(&zip(&[manifest(), raw(name, b"MZ")], None)).unwrap_err();
        assert!(matches!(err, IwbzError::NotAllowed { .. }), "{name}: {err}");
    }
}

#[test]
fn links_are_refused() {
    let mut link = raw("rooms/a.toml", b"/etc/passwd");
    link.made_by = 3 << 8 | 20;
    link.external = 0o120_777 << 16;
    let err = unpack(&zip(&[manifest(), link], None)).unwrap_err();
    assert!(matches!(err, IwbzError::NotAllowed { .. }), "{err}");
    let mut plain = raw("rooms/a.toml", b"");
    plain.made_by = 3 << 8 | 20;
    plain.external = 0o100_644 << 16;
    assert!(unpack(&zip(&[manifest(), plain], None)).is_ok());
}

#[test]
fn a_file_stated_larger_than_the_package_limit_is_refused_before_inflating() {
    let mut bomb = deflated("rooms/a.toml", b"");
    bomb.size = (MAX_PACKAGE_BYTES + 1) as u32;
    let err = unpack(&zip(&[manifest(), bomb], None)).unwrap_err();
    assert!(matches!(err, IwbzError::TooLarge(_)), "{err}");
}

#[test]
fn a_file_that_inflates_past_its_stated_size_is_refused() {
    // 10 MB of zeros deflate to about 10 KB; the directory claims 100 bytes.
    let mut bomb = deflated("rooms/a.toml", &vec![b' '; 10_000_000]);
    bomb.size = 100;
    let err = unpack(&zip(&[manifest(), bomb], None)).unwrap_err();
    assert!(matches!(err, IwbzError::Corrupt(ref m) if m.contains("not the stated")), "{err}");
}

#[test]
fn overlapping_files_are_refused() {
    // Both directory entries point at the same local header: the classic overlap bomb.
    let entries = [manifest(), raw("game.toml", b"id = \"x\"\n")];
    let err = unpack(&zip(&entries, Some(&[0, 0]))).unwrap_err();
    assert!(matches!(err, IwbzError::Corrupt(_)), "{err}");
}

#[test]
fn a_name_twice_is_refused() {
    let err = unpack(&zip(&[manifest(), manifest()], None)).unwrap_err();
    assert!(matches!(err, IwbzError::Corrupt(ref m) if m.contains("twice")), "{err}");
}

#[test]
fn too_many_files_are_refused() {
    let names: Vec<&'static str> = (0..=MAX_FILES)
        .map(|i| &*Box::leak(format!("rooms/r{i}.toml").into_boxed_str()))
        .collect();
    let mut entries = vec![manifest()];
    entries.extend(names.iter().map(|n| raw(n, b"")));
    let err = unpack(&zip(&entries, None)).unwrap_err();
    assert!(matches!(err, IwbzError::TooLarge(_)), "{err}");
}

#[test]
fn bad_archives_are_refused() {
    let mut bad_crc = raw("rooms/a.toml", b"x = 1\n");
    bad_crc.crc ^= 1;
    let mut encrypted = raw("rooms/a.toml", b"");
    encrypted.flags = 1;
    let mut bzip2 = raw("rooms/a.toml", b"");
    bzip2.method = 12;
    for (what, entries) in [("crc", bad_crc), ("encrypted", encrypted), ("method", bzip2)] {
        let err = unpack(&zip(&[manifest(), entries], None)).unwrap_err();
        assert!(matches!(err, IwbzError::Corrupt(_)), "{what}: {err}");
    }
    let good = zip(&[manifest()], None);
    for cut in [0, 10, good.len() - 1] {
        assert!(unpack(&good[..cut]).is_err(), "cut at {cut}");
    }
    assert!(unpack(b"not a zip at all").is_err());
    assert_eq!(unpack(&zip(&[raw("rooms/a.toml", b"")], None)), Err(IwbzError::NoManifest));
}
