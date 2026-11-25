//! `iwbtg removals sign`: sign a removal list in the format the server serves (docs/server.md,
//! Removal list; #1479).
//!
//! The format, the canonical text and the signature live in `iwbtg-removal`, shared with
//! `iwbtg-server` (#1537), so the producer and the validator cannot drift. This module is the
//! CLI: it reads the unsigned list and a 32-byte Ed25519 seed from a 600 key file and writes the
//! signed envelope. The seed is never printed or written, and on Unix a key file that group or
//! others can read is refused.

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use iwbtg_removal::{UnsignedList, sign};

use crate::publish_cmd::write_atomic;
use crate::trace_cmd::{ToolError, read_text};

/// A key file is 64 hex characters; anything larger is not a key and is not read.
const MAX_KEY_FILE: u64 = 1_024;

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
    let seed = read_key(&args.key)?;
    let signed = sign(&seed, &mut list).map_err(|error| ToolError::Message(error.to_string()))?;
    write_atomic(&args.output, signed.json.as_bytes())?;

    // Never the seed: the public key is what a client embeds, and the digest is the audit line.
    println!("signed {} removal(s)", signed.count);
    println!("public key: {}", hex::encode(signed.public_key));
    println!("signature: {}", hex::encode(signed.signature));
    println!("canonical sha256: {}", hex::encode(signed.canonical_sha256));
    println!("wrote {}", args.output.display());
    Ok(true)
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
