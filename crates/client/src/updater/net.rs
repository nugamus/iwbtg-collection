//! Reading the `main-latest` release over HTTPS (#1606): the GitHub API with no token (the
//! repository is public), the build manifest, then the zip and its checksum, all in a worker
//! thread so a slow download never stalls a frame.
//!
//! The only credentials this file knows about are none: `Authorization` is never set, and the
//! client's user agent is the whole of what GitHub sees.

use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

use serde::Deserialize;

use crate::settings::UpdateChannel;

use super::install;
use super::{BuildInfo, Event};

const API: &str = "https://api.github.com";
const REPO: &str = "nugamus/iwbtg-collection";
const TAG: &str = "main-latest";
/// GitHub's API rejects a request without a user agent (403).
const USER_AGENT: &str = "iwbtg-collection-client";
/// build.json and sha256.txt are a few hundred bytes; this is generous, not a policy.
const MAX_MANIFEST: u64 = 512 * 1024;
/// The release zip is about 40 MB; a bad server must not fill memory.
const MAX_ZIP: u64 = 400 * 1024 * 1024;
/// The folder a downloaded build waits in, under the system temp folder.
const STAGING: &str = "iwbtg-collection-update";

#[derive(Deserialize)]
struct Release {
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Why a check stopped. `notify` is false for a network or API problem (the Settings row says
/// what happened; a notice every half hour would be noise) and true for a bad or uninstallable
/// download, which the player must see.
struct Failure {
    message: String,
    notify: bool,
}

impl Failure {
    fn quiet(message: impl Into<String>) -> Failure {
        Failure {
            message: message.into(),
            notify: false,
        }
    }

    fn loud(message: impl Into<String>) -> Failure {
        Failure {
            message: message.into(),
            notify: true,
        }
    }
}

/// Runs one check on the calling thread and sends the outcome to the app. `running` is the commit
/// this build was compiled from; `None` is a local build, which never updates itself.
pub fn run(channel: UpdateChannel, running: Option<String>, tx: Sender<Event>) {
    let event = match check(channel, running.as_deref(), &tx) {
        Ok(None) => Event::UpToDate,
        Ok(Some((build, exe))) => Event::Ready { build, exe },
        Err(failure) => Event::Error {
            message: failure.message,
            notify: failure.notify,
        },
    };
    let _ = tx.send(event);
}

/// `Ok(None)` when the running build is the one published; `Ok(Some((manifest, exe)))` when a
/// verified exe waits in [`staging_dir`].
fn check(
    channel: UpdateChannel,
    running: Option<&str>,
    tx: &Sender<Event>,
) -> Result<Option<(BuildInfo, PathBuf)>, Failure> {
    // Only Windows is published for now (#1606); another platform must never install its zip.
    if !cfg!(target_os = "windows") {
        return Err(Failure::quiet(
            "Self-update is Windows-only for now; this build never installs one.",
        ));
    }
    let Some(running) = running else {
        return Err(Failure::quiet(
            "This build carries no commit (it was compiled locally), so it does not update itself.",
        ));
    };
    let agent = agent()?;
    let url = match channel {
        UpdateChannel::MainBuilds => format!("{API}/repos/{REPO}/releases/tags/{TAG}"),
        UpdateChannel::Releases => format!("{API}/repos/{REPO}/releases/latest"),
    };
    let release: Release = serde_json::from_str(&get_text(&agent, &url, MAX_MANIFEST)?)
        .map_err(|e| Failure::quiet(format!("the release listing did not read: {e}")))?;
    let asset = |name: &str| release.assets.iter().find(|a| a.name == name);
    let Some(build_asset) = asset("build.json") else {
        return Err(Failure::quiet(
            "That release has no build.json, so the client cannot tell what it contains.",
        ));
    };
    let build: BuildInfo = serde_json::from_str(&get_text(
        &agent,
        &build_asset.browser_download_url,
        MAX_MANIFEST,
    )?)
    .map_err(|e| Failure::quiet(format!("build.json did not read: {e}")))?;
    // Main builds follow the commit; releases follow the version, or every main build would
    // "update" backwards to the last tagged release it is ahead of.
    let newer = match channel {
        UpdateChannel::MainBuilds => !same_commit(&build.commit, running),
        UpdateChannel::Releases => newer_version(&build.version, env!("CARGO_PKG_VERSION")),
    };
    if !newer {
        return Ok(None);
    }
    // Only a zip the workflow published with a checksum is installable: an unverified download
    // must never run.
    let Some(zip_asset) = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(".zip") && a.name.contains("windows"))
    else {
        return Err(Failure::quiet(
            "That release has no Windows client zip the updater can install.",
        ));
    };
    let Some(sha_asset) = asset("sha256.txt") else {
        return Err(Failure::quiet(
            "That release publishes no sha256.txt, so the client will not install it.",
        ));
    };
    let _ = tx.send(Event::Downloading { build: build.clone() });
    let zip = get_bytes(&agent, &zip_asset.browser_download_url, MAX_ZIP)?;
    let manifest = get_text(&agent, &sha_asset.browser_download_url, MAX_MANIFEST)?;
    let dir = staging_dir();
    fs::create_dir_all(&dir)
        .map_err(|e| Failure::loud(format!("the update folder could not be made: {e}")))?;
    let download = dir.join("update.zip");
    fs::write(&download, &zip)
        .map_err(|e| Failure::loud(format!("the update could not be saved: {e}")))?;
    if let Err(message) = install::verify(&zip, &manifest) {
        // The unverified download is deleted before anything else sees it.
        cleanup();
        return Err(Failure::loud(message));
    }
    let exe = install::stage_exe(&zip, &dir).map_err(Failure::loud)?;
    let _ = fs::remove_file(&download);
    Ok(Some((build, exe)))
}

/// Whether two git SHA strings name the same commit. A short SHA is enough when it is at least
/// seven characters, as git itself accepts.
pub fn same_commit(a: &str, b: &str) -> bool {
    let a = a.trim().to_ascii_lowercase();
    let b = b.trim().to_ascii_lowercase();
    let (long, short) = if a.len() >= b.len() { (&a, &b) } else { (&b, &a) };
    !short.is_empty() && (short.len() >= 7 || long == short) && long.starts_with(short)
}

/// Whether `candidate` is a later dotted version than `running`. Numeric parts only: a pre-release
/// suffix does not make a version newer, so `0.2.0-rc1` is newer than `0.1.0` but not than
/// `0.2.0`. Missing parts count as zero.
pub fn newer_version(candidate: &str, running: &str) -> bool {
    fn parts(version: &str) -> Vec<u64> {
        version
            .trim_start_matches('v')
            .split(['.', '-', '+'])
            .map(|part| {
                let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
                digits.parse().unwrap_or(0)
            })
            .collect()
    }
    let candidate = parts(candidate);
    let running = parts(running);
    for at in 0..candidate.len().max(running.len()) {
        let a = candidate.get(at).copied().unwrap_or(0);
        let b = running.get(at).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    false
}

/// The folder a downloaded and verified exe waits in.
pub fn staging_dir() -> PathBuf {
    std::env::temp_dir().join(STAGING)
}

/// Deletes any download left from an earlier check or session.
pub fn cleanup() {
    let _ = fs::remove_dir_all(staging_dir());
}

fn agent() -> Result<ureq::Agent, Failure> {
    // The operating system's certificate store, not a bundled root list: see `Cargo.toml` for
    // why the crates' own stores are not used. Both stores can be empty on a stripped system,
    // which is a quiet failure like any other unreachable server.
    let found = rustls_native_certs::load_native_certs();
    if found.certs.is_empty() {
        return Err(Failure::quiet(
            "No system certificates were found, so the update server cannot be trusted.",
        ));
    }
    let certs: Vec<ureq::tls::Certificate<'static>> = found
        .certs
        .iter()
        .map(|der| ureq::tls::Certificate::from_der(der.as_ref()).to_owned())
        .collect();
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::new_with_certs(&certs))
        .build();
    Ok(ureq::Agent::config_builder()
        .tls_config(tls)
        // A check and its download must give up rather than hold the worker forever.
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into())
}

fn get_text(agent: &ureq::Agent, url: &str, limit: u64) -> Result<String, Failure> {
    let mut response = agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| Failure::quiet(format!("the update server could not be reached: {e}")))?;
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_string()
        .map_err(|e| Failure::quiet(format!("the update server's answer did not read: {e}")))
}

fn get_bytes(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>, Failure> {
    let mut response = agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| Failure::quiet(format!("the update could not be downloaded: {e}")))?;
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|e| Failure::quiet(format!("the update download stopped: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commits_match_only_when_they_are_the_same_commit() {
        let full = "493e284f0a1b2c3d4e5f60718293a4b5c6d7e8f9";
        assert!(same_commit(full, full));
        assert!(same_commit(full, &full[..7]));
        assert!(same_commit(&full[..8], &full[..12]));
        assert!(!same_commit(full, "493e285"));
        assert!(!same_commit("", full));
        assert!(!same_commit("abc", "abcdef"));
        assert!(same_commit("ABC", "abc"), "hex case does not matter");
    }

    #[test]
    fn versions_compare_by_their_numbers() {
        assert!(newer_version("0.2.0", "0.1.0"));
        assert!(!newer_version("0.1.0", "0.1.0"));
        assert!(!newer_version("0.0.9", "0.1.0"));
        assert!(newer_version("v1.0", "0.9.9"), "a leading v is allowed");
        assert!(!newer_version("1.0.0", "1"), "missing parts are zero");
        assert!(newer_version("1.0.1", "1"), "missing parts are zero");
        assert!(!newer_version("0.2.0-rc1", "0.2.0"), "a pre-release is not newer");
        assert!(newer_version("0.2.0-rc1", "0.1.0"));
        assert!(!newer_version("", "0.1.0"));
    }
}
