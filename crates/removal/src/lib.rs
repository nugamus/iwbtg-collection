//! The signed removal-list format (docs/server.md, Removal list; #808, #1479, #1537).
//!
//! `iwbtg-server` serves a published list at `GET /removals.json`; `iwbtg removals sign` writes
//! it. Both use this crate, so the canonical text the signature covers cannot drift between the
//! producer and the validator. The signing key never reaches the server: it serves bytes a
//! publisher already signed, and a client verifies them with the public key it ships.
//!
//! The signature covers canonical text rather than the JSON bytes, so formatting is free:
//!
//! ```text
//! iwbtg-removals-v1\n
//! <key_id>\n
//! <generated_at>\n
//! <package_id>\t<reason>\t<at>\n   one line per entry, sorted by package_id
//! ```
//!
//! The checks below make that text unambiguous: every field is a restricted ASCII alphabet with
//! no tab, newline or space, ids are unique and sorted, and unknown JSON fields are rejected, so
//! nothing unsigned can ride along.

use std::fmt;

use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The canonical text starts here; a format change is a new prefix and a new `version`.
const SIGNED_PREFIX: &str = "iwbtg-removals-v1\n";

/// What one request may pull into memory; the server checks a file's size before reading it.
pub const MAX_BYTES: usize = 1 << 20;

/// A list past this is a publish mistake, not a real takedown set.
const MAX_ENTRIES: usize = 4_096;

/// The published list, already signed (docs/server.md, Removal list).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedList {
    /// Format version; only `1` is understood.
    pub version: u32,
    /// Which client-embedded public key verifies `signature`.
    pub key_id: String,
    /// When the list was produced, UTC ISO-8601.
    pub generated_at: String,
    /// Removed packages, sorted by `package_id`.
    pub removals: Vec<Removal>,
    /// Ed25519 signature over [`signed_bytes`], 128 lowercase hex characters.
    pub signature: String,
}

/// The unsigned input `iwbtg removals sign` reads; [`sign`] sorts its entries.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsignedList {
    /// Format version; only `1` is understood.
    pub version: u32,
    /// Which client-embedded public key will verify the signature.
    pub key_id: String,
    /// When the list was produced, UTC ISO-8601.
    pub generated_at: String,
    /// The removed packages, in any order.
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

/// A list the publisher or the server must refuse. Every message is safe to log or print.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// More than [`MAX_BYTES`].
    TooLarge,
    /// Not the strict JSON shape above.
    Json,
    /// A version other than `1`.
    Version,
    /// `key_id` is not a short id.
    KeyId,
    /// `generated_at` is not a timestamp.
    GeneratedAt,
    /// More than 4,096 entries.
    TooManyEntries(usize),
    /// A `package_id` is not a short id.
    PackageId(String),
    /// A `reason` is not a lower-case code.
    Reason { package_id: String, reason: String },
    /// An entry's `at` is not a timestamp.
    EntryTimestamp(String),
    /// The same package is listed twice.
    Duplicate(String),
    /// Entries are not strictly sorted by `package_id`.
    Unsorted,
    /// `signature` is not 128 lowercase hex characters.
    Signature,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => write!(formatter, "larger than 1 MiB"),
            Self::Json => write!(formatter, "not a valid removal list"),
            Self::Version => write!(formatter, "version must be 1; only that format is understood"),
            Self::KeyId => write!(formatter, "key_id must be 1-32 letters, digits, '.', '_' or '-'"),
            Self::GeneratedAt => write!(
                formatter,
                "generated_at must be 1-64 printable characters without spaces"
            ),
            Self::TooManyEntries(count) => write!(
                formatter,
                "{count} removals is more than the cap of {MAX_ENTRIES}"
            ),
            Self::PackageId(id) => write!(
                formatter,
                "package_id {id:?} must be 1-64 letters, digits, '.', '_' or '-'"
            ),
            Self::Reason { package_id, reason } => write!(
                formatter,
                "reason {reason:?} for {package_id} must be 1-32 lower-case letters, digits or '-'"
            ),
            Self::EntryTimestamp(id) => write!(
                formatter,
                "the timestamp for {id} must be 1-64 printable characters without spaces"
            ),
            Self::Duplicate(id) => {
                write!(formatter, "{id} is listed twice; a package is removed once")
            }
            Self::Unsorted => write!(formatter, "entries are not unique and sorted by package_id"),
            Self::Signature => {
                write!(formatter, "signature is not 128 lowercase hex characters")
            }
        }
    }
}

impl std::error::Error for Error {}

/// The signed envelope and the values `iwbtg removals sign` prints.
#[derive(Debug)]
pub struct Signed {
    /// The pretty-printed signed JSON, newline-terminated.
    pub json: String,
    /// The public key a client embeds, to verify [`json`](Self::json).
    pub public_key: [u8; 32],
    /// The Ed25519 signature over the canonical text.
    pub signature: [u8; 64],
    /// SHA-256 of the canonical text, for an audit log.
    pub canonical_sha256: [u8; 32],
    /// How many entries were signed.
    pub count: usize,
}

/// Parses and validates a published list. The reason is safe to log; it never includes content.
pub fn parse(bytes: &[u8]) -> Result<SignedList, Error> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::TooLarge);
    }
    let list: SignedList = serde_json::from_slice(bytes).map_err(|_| Error::Json)?;
    validate(&list)?;
    Ok(list)
}

/// Checks everything [`signed_bytes`] relies on: the field alphabets, unique sorted ids, and the
/// signature's shape. Verification is the client's job; the server holds no key.
pub fn validate(list: &SignedList) -> Result<(), Error> {
    check_fields(list.version, &list.key_id, &list.generated_at, &list.removals)?;
    if !list
        .removals
        .windows(2)
        .all(|pair| pair[0].package_id < pair[1].package_id)
    {
        return Err(Error::Unsorted);
    }
    if list.signature.len() != 128
        || !list
            .signature
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(Error::Signature);
    }
    Ok(())
}

/// Sorts the entries, validates them, and signs the canonical text.
pub fn sign(seed: &[u8; 32], list: &mut UnsignedList) -> Result<Signed, Error> {
    if list.version != 1 {
        return Err(Error::Version);
    }
    check_fields(list.version, &list.key_id, &list.generated_at, &list.removals)?;
    list.removals
        .sort_by(|left, right| left.package_id.cmp(&right.package_id));
    if let Some(pair) = list
        .removals
        .windows(2)
        .find(|pair| pair[0].package_id == pair[1].package_id)
    {
        return Err(Error::Duplicate(pair[0].package_id.clone()));
    }

    let signing_key = SigningKey::from_bytes(seed);
    let canonical = canonical_text(&list.key_id, &list.generated_at, &list.removals);
    let signature = signing_key.sign(canonical.as_bytes());
    let document = Document {
        version: list.version,
        key_id: &list.key_id,
        generated_at: &list.generated_at,
        removals: &list.removals,
        signature: hex::encode(signature.to_bytes()),
    };
    let mut json = serde_json::to_string_pretty(&document).map_err(|_| Error::Json)?;
    json.push('\n');

    let mut canonical_sha256 = [0u8; 32];
    canonical_sha256.copy_from_slice(&Sha256::digest(canonical.as_bytes()));
    Ok(Signed {
        json,
        public_key: signing_key.verifying_key().to_bytes(),
        signature: signature.to_bytes(),
        canonical_sha256,
        count: list.removals.len(),
    })
}

/// The exact bytes `signature` covers (docs/server.md, Removal list). It is text, not JSON, so a
/// publisher may reformat the file freely.
pub fn signed_bytes(list: &SignedList) -> Vec<u8> {
    canonical_text(&list.key_id, &list.generated_at, &list.removals).into_bytes()
}

/// The signed document written to `--out`.
#[derive(Serialize)]
struct Document<'a> {
    version: u32,
    key_id: &'a str,
    generated_at: &'a str,
    removals: &'a [Removal],
    signature: String,
}

/// The field checks shared by [`validate`] and [`sign`].
fn check_fields(version: u32, key_id: &str, generated_at: &str, removals: &[Removal]) -> Result<(), Error> {
    if version != 1 {
        return Err(Error::Version);
    }
    if !is_bounded(key_id, 32, is_id_byte) {
        return Err(Error::KeyId);
    }
    if !is_bounded(generated_at, 64, is_timestamp_byte) {
        return Err(Error::GeneratedAt);
    }
    if removals.len() > MAX_ENTRIES {
        return Err(Error::TooManyEntries(removals.len()));
    }
    for removal in removals {
        if !is_bounded(&removal.package_id, 64, is_id_byte) {
            return Err(Error::PackageId(removal.package_id.clone()));
        }
        if !is_bounded(&removal.reason, 32, is_reason_byte) {
            return Err(Error::Reason {
                package_id: removal.package_id.clone(),
                reason: removal.reason.clone(),
            });
        }
        if !is_bounded(&removal.at, 64, is_timestamp_byte) {
            return Err(Error::EntryTimestamp(removal.package_id.clone()));
        }
    }
    Ok(())
}

/// The canonical signed text, one line per entry in the given order.
fn canonical_text(key_id: &str, generated_at: &str, removals: &[Removal]) -> String {
    let mut text = String::from(SIGNED_PREFIX);
    text.push_str(key_id);
    text.push('\n');
    text.push_str(generated_at);
    text.push('\n');
    for removal in removals {
        text.push_str(&removal.package_id);
        text.push('\t');
        text.push_str(&removal.reason);
        text.push('\t');
        text.push_str(&removal.at);
        text.push('\n');
    }
    text
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

    /// The RFC 8032 test vector's seed: public, deterministic, never used to sign anything real.
    const THROWAWAY_SEED: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";

    /// 128 hex characters: the shape of a 64-byte Ed25519 signature.
    const SIGNATURE: &str = "0000000000000000000000000000000000000000000000000000000000000000\
                             0000000000000000000000000000000000000000000000000000000000000000";

    fn entry(package_id: &str, reason: &str, at: &str) -> String {
        format!(r#"{{"package_id": "{package_id}", "reason": "{reason}", "at": "{at}"}}"#)
    }

    /// A signed-shaped JSON fixture for the parser tests.
    fn list(removals: &str) -> String {
        format!(
            r#"{{
  "version": 1,
  "key_id": "2026-11",
  "generated_at": "2026-11-24T12:00:00Z",
  "removals": [{removals}],
  "signature": "{SIGNATURE}"
}}"#
        )
    }

    /// An unsigned input for the signing tests.
    fn unsigned(removals: &str) -> UnsignedList {
        serde_json::from_str(&format!(
            r#"{{
  "version": 1,
  "key_id": "2026-11",
  "generated_at": "2026-11-24T12:00:00Z",
  "removals": [{removals}]
}}"#
        ))
        .expect("a well-formed unsigned list")
    }

    fn seed() -> [u8; 32] {
        hex::decode(THROWAWAY_SEED)
            .expect("hex")
            .try_into()
            .expect("32 bytes")
    }

    #[test]
    fn a_well_formed_list_parses() {
        let text = list(&format!(
            "{},{}",
            entry("iwbtg.a", "takedown", "2026-11-20T00:00:00Z"),
            entry("iwbtg.b", "moderation", "2026-11-21T00:00:00Z")
        ));
        let parsed = parse(text.as_bytes()).expect("valid list");
        assert_eq!(parsed.removals.len(), 2);
        assert_eq!(parsed.removals[1].package_id, "iwbtg.b");
    }

    #[test]
    fn an_empty_list_is_valid() {
        parse(list("").as_bytes()).expect("a list may lift every removal");
    }

    #[test]
    fn the_signed_text_is_the_documented_canonical_form() {
        let parsed: SignedList = parse(
            list(&format!(
                "{},{}",
                entry("iwbtg.a", "takedown", "2026-11-20T00:00:00Z"),
                entry("iwbtg.b", "moderation", "2026-11-21T00:00:00Z")
            ))
            .as_bytes(),
        )
        .expect("valid list");
        assert_eq!(
            String::from_utf8(signed_bytes(&parsed)).expect("utf-8"),
            "iwbtg-removals-v1\n\
             2026-11\n\
             2026-11-24T12:00:00Z\n\
             iwbtg.a\ttakedown\t2026-11-20T00:00:00Z\n\
             iwbtg.b\tmoderation\t2026-11-21T00:00:00Z\n"
        );
    }

    #[test]
    fn signing_produces_a_list_the_parser_accepts_and_the_key_verifies() {
        let mut input = unsigned(&format!(
            "{},{}",
            entry("iwbtg.b", "moderation", "2026-11-21T00:00:00Z"),
            entry("iwbtg.a", "takedown", "2026-11-20T00:00:00Z")
        ));
        let signed = sign(&seed(), &mut input).expect("sign");
        assert_eq!(signed.count, 2);

        // Sorted by id, and the RFC 8032 seed's public key, so the seed-to-key step is pinned.
        assert_eq!(
            hex::encode(signed.public_key),
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
        );
        let parsed = parse(signed.json.as_bytes()).expect("the parser accepts what we signed");
        assert_eq!(parsed.removals[0].package_id, "iwbtg.a");

        // The signature covers the canonical text of the parsed list, and verifies with that key.
        let canonical = signed_bytes(&parsed);
        let signature = Signature::from_bytes(&signed.signature);
        VerifyingKey::from_bytes(&signed.public_key)
            .expect("the public key")
            .verify_strict(&canonical, &signature)
            .expect("the signature verifies");
    }

    #[test]
    fn a_tampered_entry_no_longer_verifies() {
        let mut input = unsigned(&entry("iwbtg.a", "takedown", "2026-11-20T00:00:00Z"));
        let signed = sign(&seed(), &mut input).expect("sign");

        // Flip a field the payload contains but the signature no longer matches. The file is still
        // structurally valid, so only the signature catches it.
        let tampered = signed.json.replace("takedown", "moderation");
        let parsed = parse(tampered.as_bytes()).expect("still well formed");
        let canonical = signed_bytes(&parsed);
        let signature = Signature::from_bytes(&signed.signature);
        assert!(
            VerifyingKey::from_bytes(&signed.public_key)
                .expect("the public key")
                .verify_strict(&canonical, &signature)
                .is_err(),
            "a changed entry must not verify"
        );
    }

    #[test]
    fn signing_rejects_a_duplicate_package() {
        let mut input = unsigned(&format!(
            "{},{}",
            entry("iwbtg.a", "takedown", "2026-11-20T00:00:00Z"),
            entry("iwbtg.a", "moderation", "2026-11-21T00:00:00Z")
        ));
        let error = sign(&seed(), &mut input).expect_err("duplicate ids are refused");
        assert!(error.to_string().contains("listed twice"), "{error}");
    }

    #[test]
    fn rejects_a_version_it_does_not_understand() {
        let text = list("").replace("\"version\": 1", "\"version\": 2");
        assert!(parse(text.as_bytes()).is_err());
    }

    #[test]
    fn rejects_an_unknown_field() {
        let text = list("").replace("\"version\": 1", "\"version\": 1, \"extra\": true");
        assert!(parse(text.as_bytes()).is_err());
    }

    #[test]
    fn rejects_an_entry_whose_id_could_split_the_signed_text() {
        let text = list(&entry("iwbtg.a\\tb", "takedown", "2026-11-20T00:00:00Z"));
        assert!(parse(text.as_bytes()).is_err());
    }

    #[test]
    fn rejects_a_reason_that_is_not_a_code() {
        let text = list(&entry("iwbtg.a", "Takedown", "2026-11-20T00:00:00Z"));
        assert!(parse(text.as_bytes()).is_err());
    }

    #[test]
    fn rejects_a_timestamp_with_a_space() {
        let text = list(&entry("iwbtg.a", "takedown", "2026-11-20 00:00:00"));
        assert!(parse(text.as_bytes()).is_err());
    }

    #[test]
    fn rejects_duplicate_or_unsorted_ids() {
        for entries in [
            format!(
                "{},{}",
                entry("iwbtg.b", "takedown", "2026-11-20T00:00:00Z"),
                entry("iwbtg.a", "takedown", "2026-11-20T00:00:00Z")
            ),
            format!(
                "{},{}",
                entry("iwbtg.a", "takedown", "2026-11-20T00:00:00Z"),
                entry("iwbtg.a", "moderation", "2026-11-21T00:00:00Z")
            ),
        ] {
            assert!(parse(list(&entries).as_bytes()).is_err(), "accepted {entries}");
        }
    }

    #[test]
    fn rejects_a_signature_that_is_not_64_bytes_of_hex() {
        for signature in ["", "abc", &"A".repeat(128), &"g".repeat(128)] {
            let text = list("").replace(SIGNATURE, signature);
            assert!(parse(text.as_bytes()).is_err(), "accepted {signature:?}");
        }
    }

    #[test]
    fn rejects_a_list_over_the_byte_cap() {
        let mut text = list("");
        text.push_str(&" ".repeat(MAX_BYTES));
        assert!(parse(text.as_bytes()).is_err());
    }
}
