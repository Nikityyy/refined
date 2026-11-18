//! Self-update against the GitHub releases page of Refined.
//!
//! The flow is deliberately installer-free: the "portable" `Refined.exe`
//! asset of the latest release is downloaded next to the running exe,
//! SHA-256-verified, and swapped in place - a running exe can be renamed
//! (not overwritten), so the old binary is renamed away, the new one moved
//! into its exact path (this keeps Start-menu, desktop and autostart
//! shortcuts and the uninstaller valid), and the app restarts itself into
//! the new version. No elevation, no setup wizard, settings and history
//! are untouched.

use crate::groq::http_agent;
use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};

const RELEASES_LATEST: &str = "https://api.github.com/repos/Nikityyy/refined/releases/latest";
/// The portable binary asset each release publishes.
const ASSET_NAME: &str = "Refined.exe";
const ASSET_HASH_NAME: &str = "Refined.exe.sha256";

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    /// Download URL of the `Refined.exe` asset.
    pub url: String,
    pub size: u64,
    pub sha_url: Option<String>,
}

#[derive(Debug, Clone)]
pub enum UpdateState {
    /// A check or download is in flight (worker thread owns the work).
    Checking,
    UpToDate,
    Available(UpdateInfo),
    /// Staged and verified; waiting for the user to restart.
    Ready(UpdateInfo),
    Failed(String),
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

pub fn version_pair(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.trim().trim_start_matches('v').split('.');
    let a = it.next()?.parse().ok()?;
    let b = it.next().unwrap_or("0").parse().unwrap_or(0);
    let c = it.next().unwrap_or("0").parse().unwrap_or(0);
    Some((a, b, c))
}

/// Asks GitHub for the latest release. Ok(None) = already up to date.
pub fn check_latest() -> Result<Option<UpdateInfo>, String> {
    let resp: GithubRelease = http_agent()
        .get(RELEASES_LATEST)
        .header("User-Agent", "Refined-Updater")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Could not reach GitHub: {e}"))?
        .into_body()
        .read_json()
        .map_err(|e| format!("GitHub response unreadable: {e}"))?;

    let latest = version_pair(&resp.tag_name)
        .ok_or_else(|| format!("Unreadable version tag: {}", resp.tag_name))?;
    let current = version_pair(CURRENT_VERSION).unwrap_or((0, 0, 0));
    if latest <= current {
        return Ok(None);
    }

    let asset = resp
        .assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(ASSET_NAME))
        .ok_or("This release has no Refined.exe asset.")?;
    Ok(Some(UpdateInfo {
        version: resp.tag_name.trim_start_matches('v').to_string(),
        url: asset.browser_download_url.clone(),
        size: asset.size,
        sha_url: resp
            .assets
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(ASSET_HASH_NAME))
            .map(|a| a.browser_download_url.clone()),
    }))
}

/// Downloads the new binary to `Refined.update.tmp` next to the running exe
/// and verifies it against the release's SHA-256 checksum file.
/// Returns the staged path. Only ever called from a worker thread.
pub fn download(info: &UpdateInfo) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("Cannot locate our exe: {e}"))?;
    let dir = exe
        .parent()
        .ok_or("Our exe has no directory?")?
        .to_path_buf();
    let tmp = dir.join("Refined.update.tmp");

    let resp = http_agent()
        .get(&info.url)
        .header("User-Agent", "Refined-Updater")
        .call()
        .map_err(|e| format!("Download failed: {e}"))?;
    let mut reader = resp.into_body().into_reader();

    let mut buf = Vec::with_capacity(info.size.min(64 * 1024 * 1024) as usize);
    loop {
        let mut chunk = [0u8; 64 * 1024];
        let n = reader
            .read(&mut chunk)
            .map_err(|e| format!("Download failed: {e}"))?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if info.size > 0 && buf.len() as u64 > info.size {
            return Err("Downloaded file is larger than expected - aborted.".into());
        }
    }

    // SHA-256 against the release checksum asset; a mismatch means a
    // corrupted or tampered download and is never applied.
    if let Some(sha_url) = &info.sha_url {
        let expected_body = http_agent()
            .get(sha_url)
            .header("User-Agent", "Refined-Updater")
            .call()
            .map_err(|e| format!("Checksum download failed: {e}"))?
            .into_body()
            .into_reader()
            .take(64 * 1024)
            .bytes()
            .collect::<Result<Vec<u8>, _>>()
            .map_err(|e| format!("Checksum unreadable: {e}"))?;
        let expected = String::from_utf8_lossy(&expected_body)
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let hex = sha256_hex(&buf);
        if expected.is_empty() || !hex.eq_ignore_ascii_case(&expected) {
            return Err("Download failed its integrity check - nothing was changed.".into());
        }
    } else {
        return Err("Release is missing its checksum file - refusing to update.".into());
    }

    std::fs::write(&tmp, &buf).map_err(|e| format!("Could not write the update file: {e}"))?;
    Ok(tmp)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Swap the staged `Refined.update.tmp` in and restart into it. The staged
/// path is derived from the running exe, exactly where `download` put it.
/// Swapping the exact exe path keeps the Start-menu, desktop and autostart
/// shortcuts and the uninstaller entry valid without running any installer.
/// This function does not return on success.
pub fn apply_staged() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("Cannot locate our exe: {e}"))?;
    let staged = match exe.parent() {
        Some(dir) => dir.join("Refined.update.tmp"),
        None => return Err("Our exe has no directory?".to_string()),
    };
    let old = exe.with_extension("old.exe");

    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).map_err(|e| {
        format!("Could not set the update aside: {e}. Try running Refined from a writable folder.")
    })?;
    if let Err(e) = std::fs::rename(&staged, &exe) {
        // Put the current version back so the app starts next time.
        let _ = std::fs::rename(&old, &exe);
        return Err(format!("Could not place the new version: {e}"));
    }

    let _ = std::process::Command::new(&exe).spawn();
    std::process::exit(0);
}

/// Removes the previous version's leftovers next to the running exe.
/// The old binary cannot be deleted while it is what we are running.
pub fn cleanup_after_update() {
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap_or(Path::new("."));
        let _ = std::fs::remove_file(dir.join("Refined.old.exe"));
        let _ = std::fs::remove_file(dir.join("Refined.update.tmp"));
    }
}

#[cfg(test)]
mod tests {
    use super::version_pair;

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(version_pair("v0.2.0"), Some((0, 2, 0)));
        assert_eq!(version_pair("0.2.0"), Some((0, 2, 0)));
        assert!(version_pair("v0.10.0").unwrap() > version_pair("0.9.2").unwrap());
        assert_eq!(version_pair("abc"), None);
    }
}
