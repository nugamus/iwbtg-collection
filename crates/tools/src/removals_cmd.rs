//! `iwbtg removals sign`: sign a removal list in the format the server serves (docs/server.md,
//! Removal list; #1479).
//!
//! Takedowns (#68) and moderation (#800) produce an unsigned list; this signs it and
//! `iwbtg-server` serves the result at `GET /removals.json` (#808). The Ed25519 seed never leaves
//! the maintainer's machine: the tool refuses a key file that group or others can read, never
//! prints the seed, and writes only the public key and signature. Clients embed the public key and
//! verify the signature before deleting anything, so a compromised server cannot mint removals.
//!
//! The signature covers canonical text, not the JSON bytes, exactly as
//! `crates/server/src/removal.rs` documents. This module repeats that rule because the CLI does not
//! link the server; [`tests::the_server_accepts_what_we_sign`] pins the two implementations
//! together through the server's own parser and `signed_bytes`.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::publish_cmd::write_atomic;
use crate::trace_cmd::{ToolError, read_text};

/// The canonical text starts here; a format change is a new prefix and a new `version`.
const SIGNED_PREFIX: &str = "iwbtg-removals-v1\n";

/// A key file is 64 hex characters; anything larger is not a key and is not read.
const MAX_KEY_FILE: u64 = 1_024;

/// The server's cap, checked here too so the tool cannot emit a list it will refuse.
const MAX_ENTRIES: usize = 4_096;

#[derive(Subcommand)]
pub enum RemovalsCommand {
    /// Sign an unsigned removal list for the catalog (docs/server.md, Removal list).
    Sign(SignArgs),
}

#[derive(Args)]
pub struct SignArgs {
    /// File holding the 32-byte Ed25519 seed as 64 hex characters, mode 600. Never logged.
    #[arg(long)]
    pub key: PathBuf,
    /// Unsigned list: `{version, key_id, generated_at, removals}`.
    #[arg(long = "in")]
    pub input: PathBuf,
    /// Where to write the signed `removals.json`.
    #[arg(long = "out")]
    pub output: PathBuf,
}

/// The unsigned input, as the takedown or moderation flow writes it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsignedList {
    /// Format version; only `1` is understood.
    pub version: u32,
    /// Which client-embedded public key verifies the signature.
    pub key_id: String,
    /// When the list was produced, UTC ISO-8601.
    pub generated_at: String,
    /// The removed packages; this tool sorts them by `package_id`.
    pub removals: Vec<Removal>,
}

/// One removed package.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Removal {
    /// The package to delete and hide.
    pub package_id: String,
    /// Why: `takedown`, `moderation`, or another lower-case code a client may ignore.
    pub reason: String,
    /// When this entry was added, UTC ISO-8601.
    pub at: String,
}

/// The signed document written to `--out`.
#[derive(Serialize)]
struct Signed<'a> {
    version: u32,
    key_id: &'a str,
    generated_at: &'a str,
    removals: &'a [Removal],
    signature: String,
}

pub fn run(command: RemovalsCommand) -> Result<bool, ToolError> {
    match command {
        RemovalsCommand::Sign(args) => sign_main(&args),
    }
}

/// Reads the unsigned list and the seed, signs, and writes the signed envelope.
pub fn sign_main(args: &SignArgs) -> Result<bool, ToolError> {
    let text = read_text(&args.input)?;
    let mut list: UnsignedList = serde_json::from_str(&text)
        .map_err(|err| ToolError::Message(format!("{}: {err}", args.input.display())))?;
    normalize(&mut list)?;
    let seed = read_key(&args.key)?;
    let signing_key = SigningKey::from_bytes(&seed);
    let canonical = canonical_bytes(&list);
    let signature = signing_key.sign(&canonical);
    let signature_hex = hex::encode(signature.to_bytes());
    let public_key = hex::encode(signing_key.verifying_key().to_bytes());
    let canonical_sha256 = hex::encode(Sha256::digest(&canonical));

    let signed = Signed {
        version: list.version,
        key_id: &list.key_id,
        generated_at: &list.generated_at,
        removals: &list.removals,
        signature: signature_hex.clone(),
    };
    let mut out = serde_json::to_string_pretty(&signed)
        .map_err(|err| ToolError::Message(format!("{}: {err}", args.output.display())))?;
    out.push('\n');
    write_atomic(&args.output, out.as_bytes())?;

    // Never the seed: the public key is what a client embeds, and the digest is the audit line.
    println!("signed {} removal(s)", list.removals.len());
    println!("public key: {public_key}");
    println!("signature: {signature_hex}");
    println!("canonical sha256: {canonical_sha256}");
    println!("wrote {}", args.output.display());
    Ok(true)
}

/// Checks the fields the canonical text relies on, then sorts the entries.
///
/// Everything here mirrors `crates/server/src/removal.rs`: a restricted ASCII alphabet per field
/// (so no tab or newline can split the signed text), unique ids sorted by `package_id`, and the
/// server's caps. The round-trip test fails if the two drift.
fn normalize(list: &mut UnsignedList) -> Result<(), ToolError> {
    if list.version != 1 {
        return Err(ToolError::Message(
            "version must be 1; only that format is understood".to_string(),
        ));
    }
    if !is_bounded(&list.key_id, 32, is_id_byte) {
        return Err(ToolError::Message(
            "key_id must be 1-32 letters, digits, '.', '_' or '-'".to_string(),
        ));
    }
    if !is_bounded(&list.generated_at, 64, is_timestamp_byte) {
        return Err(ToolError::Message(
            "generated_at must be 1-64 printable characters without spaces".to_string(),
        ));
    }
    if list.removals.len() > MAX_ENTRIES {
        return Err(ToolError::Message(format!(
            "{} removals is more than the server's cap of {MAX_ENTRIES}",
            list.removals.len()
        )));
    }
    for removal in &list.removals {
        if !is_bounded(&removal.package_id, 64, is_id_byte) {
            return Err(ToolError::Message(format!(
                "package_id {:?} must be 1-64 letters, digits, '.', '_' or '-'",
                removal.package_id
            )));
        }
        if !is_bounded(&removal.reason, 32, is_reason_byte) {
            return Err(ToolError::Message(format!(
                "reason {:?} for {} must be 1-32 lower-case letters, digits or '-'",
                removal.reason, removal.package_id
            )));
        }
        if !is_bounded(&removal.at, 64, is_timestamp_byte) {
            return Err(ToolError::Message(format!(
                "the timestamp for {} must be 1-64 printable characters without spaces",
                removal.package_id
            )));
        }
    }
    list.removals
        .sort_by(|left, right| left.package_id.cmp(&right.package_id));
    if let Some(pair) = list
        .removals
        .windows(2)
        .find(|pair| pair[0].package_id == pair[1].package_id)
    {
        return Err(ToolError::Message(format!(
            "{} is listed twice; a package is removed once",
            pair[0].package_id
        )));
    }
    Ok(())
}

/// The exact bytes the signature covers (docs/server.md, Removal list).
fn canonical_bytes(list: &UnsignedList) -> Vec<u8> {
    let mut text = String::from(SIGNED_PREFIX);
    text.push_str(&list.key_id);
    text.push('\n');
    text.push_str(&list.generated_at);
    text.push('\n');
    for removal in &list.removals {
        text.push_str(&removal.package_id);
        text.push('\t');
        text.push_str(&removal.reason);
        text.push('\t');
        text.push_str(&removal.at);
        text.push('\n');
    }
    text.into_bytes()
}

/// Reads the 32-byte seed. Errors never echo the file's content.
fn read_key(path: &Path) -> Result<[u8; 32], ToolError> {
    let metadata =
        std::fs::metadata(path).map_err(|err| ToolError::Message(format!("{}: {err}", path.display())))?;
    if metadata.len() > MAX_KEY_FILE {
        return Err(ToolError::Message(format!(
            "{}: larger than a key file; expected 64 hex characters",
            path.display()
        )));
    }
    // A signing key readable by another account is compromised; refuse it on Unix. Windows has no
    // mode bits, so the operator keeps the file out of shared folders there.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        if mode & 0o077 != 0 {
            return Err(ToolError::Message(format!(
                "{}: the key is readable by group or others; chmod 600 it",
                path.display()
            )));
        }
    }
    let text = read_text(path)?;
    let text = text.trim();
    if text.len() != 64 {
        return Err(ToolError::Message(format!(
            "{}: expected a 32-byte Ed25519 seed as 64 hex characters",
            path.display()
        )));
    }
    let bytes = hex::decode(text)
        .map_err(|_| ToolError::Message(format!("{}: the key is not hex", path.display())))?;
    bytes.try_into().map_err(|_| {
        ToolError::Message(format!(
            "{}: expected exactly 32 bytes after decoding",
            path.display()
        ))
    })
}

/// A non-empty value of at most `max` bytes, every byte accepted by `allowed`.
fn is_bounded(value: &str, max: usize, allowed: impl Fn(u8) -> bool) -> bool {
    !value.is_empty() && value.len() <= max && value.bytes().all(allowed)
}

/// Package ids and key ids: letters, digits, `.`, `_` and `-`.
fn is_id_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
}

/// A reason code: lower-case letters, digits and `-`.
fn is_reason_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
}

/// A timestamp: printable ASCII without spaces, so no tab or newline can split the signed text.
fn is_timestamp_byte(byte: u8) -> bool {
    (0x21..=0x7e).contains(&byte)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signature, VerifyingKey};

    /// The RFC 8032 test-vector seed: public, deterministic and never used to sign anything real.
    const THROWAWAY_SEED: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";

    fn unsigned(removals: &str) -> String {
        format!(
            r#"{{
  "version": 1,
  "key_id": "2026-11",
  "generated_at": "2026-11-24T12:00:00Z",
  "removals": [{removals}]
}}"#
        )
    }

    fn entry(package_id: &str, reason: &str, at: &str) -> String {
        format!(r#"{{"package_id": "{package_id}", "reason": "{reason}", "at": "{at}"}}"#)
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("iwbtg-removals-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// Writes the throwaway seed as a 600 key file and returns the three paths.
    fn fixture(tag: &str, text: &str) -> (PathBuf, SignArgs) {
        let dir = scratch(tag);
        let key = dir.join("removals.key");
        std::fs::write(&key, format!("{THROWAWAY_SEED}\n")).expect("write key");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).expect("chmod the key");
        }
        let input = dir.join("removals.unsigned.json");
        std::fs::write(&input, text).expect("write input");
        let output = dir.join("removals.json");
        (output.clone(), SignArgs { key, input, output })
    }

    fn verifying_key() -> VerifyingKey {
        let seed: [u8; 32] = hex::decode(THROWAWAY_SEED)
            .expect("hex")
            .try_into()
            .expect("32 bytes");
        SigningKey::from_bytes(&seed).verifying_key()
    }

    fn signature_from(signed: &str) -> Signature {
        let value: serde_json::Value = serde_json::from_str(signed).expect("signed JSON");
        let hex = value["signature"].as_str().expect("signature").to_string();
        Signature::from_slice(&hex::decode(hex).expect("hex")).expect("64 bytes")
    }

    #[test]
    fn the_server_accepts_what_we_sign() {
        let (output, args) = fixture(
            "round-trip",
            &unsigned(&format!(
                "{},{}",
                entry("iwbtg.b", "moderation", "2026-11-21T00:00:00Z"),
                entry("iwbtg.a", "takedown", "2026-11-20T00:00:00Z")
            )),
        );
        assert!(sign_main(&args).expect("sign"));
        let signed = std::fs::read_to_string(&output).expect("read output");

        // The RFC 8032 test vector's public key, so the seed-to-key step is pinned too.
        assert_eq!(
            hex::encode(verifying_key().to_bytes()),
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
        );

        // The server's own parser and canonicalization, not a copy of them.
        let parsed = iwbtg_server::removal::parse(signed.as_bytes()).expect("the server accepts it");
        assert_eq!(parsed.removals[0].package_id, "iwbtg.a", "sorted by id");
        let canonical = iwbtg_server::removal::signed_bytes(&parsed);
        assert_eq!(canonical, canonical_bytes(&parse_unsigned(&signed)));

        // And the signature the client checks covers those bytes.
        verifying_key()
            .verify_strict(&canonical, &signature_from(&signed))
            .expect("the signature verifies");
    }

    #[test]
    fn a_tampered_entry_no_longer_verifies() {
        let (output, args) = fixture(
            "tampered",
            &unsigned(&entry("iwbtg.a", "takedown", "2026-11-20T00:00:00Z")),
        );
        assert!(sign_main(&args).expect("sign"));
        let signed = std::fs::read_to_string(&output).expect("read output");

        // Flip a field the payload contains but the signature no longer matches. The file is still
        // structurally valid, so only the signature catches it.
        let tampered = signed.replace("takedown", "moderation");
        let parsed = iwbtg_server::removal::parse(tampered.as_bytes()).expect("still well formed");
        let canonical = iwbtg_server::removal::signed_bytes(&parsed);
        assert!(
            verifying_key()
                .verify_strict(&canonical, &signature_from(&signed))
                .is_err(),
            "a changed entry must not verify"
        );
    }

    #[test]
    fn a_duplicate_package_is_rejected() {
        let (_, args) = fixture(
            "duplicate",
            &unsigned(&format!(
                "{},{}",
                entry("iwbtg.a", "takedown", "2026-11-20T00:00:00Z"),
                entry("iwbtg.a", "moderation", "2026-11-21T00:00:00Z")
            )),
        );
        let err = sign_main(&args).expect_err("duplicate ids are refused");
        assert!(err.to_string().contains("listed twice"), "{err}");
    }

    #[test]
    fn an_entry_that_could_split_the_signed_text_is_rejected() {
        let (_, args) = fixture(
            "tab-id",
            &unsigned(&entry("iwbtg.a\\tb", "takedown", "2026-11-20T00:00:00Z")),
        );
        assert!(sign_main(&args).is_err());
    }

    #[test]
    fn a_key_file_that_is_not_a_seed_is_rejected() {
        let (_, args) = fixture("short-key", &unsigned(""));
        std::fs::write(&args.key, "abcd\n").expect("write short key");
        let err = sign_main(&args).expect_err("a short key is refused");
        assert!(err.to_string().contains("64 hex characters"), "{err}");
    }

    #[test]
    fn the_seed_never_reaches_the_output() {
        let (output, args) = fixture(
            "no-seed",
            &unsigned(&entry("iwbtg.a", "takedown", "2026-11-20T00:00:00Z")),
        );
        assert!(sign_main(&args).expect("sign"));
        let signed = std::fs::read_to_string(&output).expect("read output");
        assert!(!signed.contains(THROWAWAY_SEED), "the seed must never be written");
    }

    #[cfg(unix)]
    #[test]
    fn a_group_readable_key_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let (_, args) = fixture("loose-key", &unsigned(""));
        std::fs::set_permissions(&args.key, std::fs::Permissions::from_mode(0o640)).expect("chmod 640");
        let err = sign_main(&args).expect_err("a group-readable key is refused");
        assert!(err.to_string().contains("chmod 600"), "{err}");
    }

    /// The unsigned list inside a signed file, for the canonical-bytes comparison.
    fn parse_unsigned(signed: &str) -> UnsignedList {
        let mut value: serde_json::Value = serde_json::from_str(signed).expect("signed JSON");
        value.as_object_mut().expect("an object").remove("signature");
        serde_json::from_value(value).expect("the signed file without its signature")
    }
}
