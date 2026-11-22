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
use std::process::Command;

const RELEASES_LATEST: &str = "https://api.github.com/repos/Nikityyy/refined/releases/latest";
/// The portable binary asset each release publishes for this platform.
#[cfg(windows)]
const ASSET_NAME: &str = "Refined.exe";
#[cfg(windows)]
const ASSET_HASH_NAME: &str = "Refined.exe.sha256";
#[cfg(windows)]
const EXECUTABLE_MAGIC: &[u8] = b"MZ";
#[cfg(target_os = "linux")]
const ASSET_NAME: &str = "refined-linux-x86_64";
#[cfg(target_os = "linux")]
const ASSET_HASH_NAME: &str = "refined-linux-x86_64.sha256";
#[cfg(target_os = "linux")]
const EXECUTABLE_MAGIC: &[u8] = b"\x7fELF";

const MAX_EXE_BYTES: u64 = 64 * 1024 * 1024;
const RESTART_ARG: &str = "--update-restart";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    /// Download URL of the `Refined.exe` asset.
    pub url: String,
    pub size: u64,
    pub sha_url: String,
}

#[derive(Debug, Clone)]
pub enum UpdateState {
    /// A check or download is in flight (worker thread owns the work).
    Checking,
    UpToDate,
    /// Staged and verified; waiting for the user to restart.
    Ready {
        version: String,
        checksum: String,
    },
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
    let mut parts = v.trim().strip_prefix('v').unwrap_or(v.trim()).split('.');
    let version = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(version)
}

/// Asks GitHub for the latest release. Ok(None) = already up to date.
pub fn check_latest() -> Result<Option<UpdateInfo>, String> {
    #[cfg(feature = "demo")]
    let demo_url = std::env::var("REFINED_DEMO_RELEASES").ok();
    #[cfg(feature = "demo")]
    let release_url = demo_url.as_deref().unwrap_or(RELEASES_LATEST);
    #[cfg(not(feature = "demo"))]
    let release_url = RELEASES_LATEST;
    let resp: GithubRelease = http_agent()
        .get(release_url)
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
        .ok_or_else(|| format!("This release has no {ASSET_NAME} asset."))?;
    Ok(Some(UpdateInfo {
        version: resp.tag_name.trim_start_matches('v').to_string(),
        url: asset.browser_download_url.clone(),
        size: asset.size,
        sha_url: resp
            .assets
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(ASSET_HASH_NAME))
            .map(|a| a.browser_download_url.clone())
            .ok_or("This release is missing its checksum.")?,
    }))
}

/// Downloads to a fixed sibling path only after checking size and checksum.
pub fn download(info: &UpdateInfo) -> Result<String, String> {
    if !(1..=MAX_EXE_BYTES).contains(&info.size) {
        return Err("Unexpected update download size.".into());
    }
    let paths = update_paths()?;
    let expected_response = http_agent()
        .get(&info.sha_url)
        .header("User-Agent", "Refined-Updater")
        .call()
        .map_err(|e| format!("Checksum download failed: {e}"))?;
    let mut expected_bytes = Vec::new();
    expected_response
        .into_body()
        .into_reader()
        .take(1024)
        .read_to_end(&mut expected_bytes)
        .map_err(|e| format!("Checksum unreadable: {e}"))?;
    let checksum = String::from_utf8_lossy(&expected_bytes)
        .split_whitespace()
        .next()
        .filter(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or("The release checksum is invalid.")?
        .to_ascii_lowercase();

    let response = http_agent()
        .get(&info.url)
        .header("User-Agent", "Refined-Updater")
        .call()
        .map_err(|e| format!("Update download failed: {e}"))?;
    let mut binary = Vec::with_capacity(info.size as usize);
    response
        .into_body()
        .into_reader()
        .take(info.size + 1)
        .read_to_end(&mut binary)
        .map_err(|e| format!("Update download failed: {e}"))?;
    if binary.len() as u64 != info.size || !matches_checksum(&binary, &checksum) {
        return Err("The update failed its integrity check. Try downloading it again.".into());
    }
    std::fs::write(&paths.staged, binary).map_err(|e| format!("Could not stage the update: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&paths.staged, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Could not stage the update: {e}"))?;
    }
    Ok(checksum)
}

fn matches_checksum(bytes: &[u8], expected: &str) -> bool {
    bytes.starts_with(EXECUTABLE_MAGIC) && sha256_hex(bytes).eq_ignore_ascii_case(expected)
}

struct UpdatePaths {
    exe: PathBuf,
    backup: PathBuf,
    staged: PathBuf,
}

fn update_paths() -> Result<UpdatePaths, String> {
    let exe = std::env::current_exe().map_err(|e| format!("Cannot locate Refined: {e}"))?;
    let dir = exe.parent().unwrap_or(Path::new("."));
    Ok(UpdatePaths {
        backup: exe.with_extension(if cfg!(windows) { "old.exe" } else { "old" }),
        staged: dir.join("Refined.update.tmp"),
        exe,
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Rechecks the staged download, then starts a new process that waits for
/// the current instance to finish shutting down before opening Studio.
pub fn apply_staged(checksum: &str) -> Result<(), String> {
    let paths = update_paths()?;
    let mut binary = Vec::new();
    std::fs::File::open(&paths.staged)
        .and_then(|file| file.take(MAX_EXE_BYTES + 1).read_to_end(&mut binary))
        .map_err(|e| format!("Could not read the staged update: {e}"))?;
    if binary.len() as u64 > MAX_EXE_BYTES || !matches_checksum(&binary, checksum) {
        return Err("The staged update failed its integrity check. Download it again.".into());
    }

    let _ = std::fs::remove_file(&paths.backup);
    std::fs::rename(&paths.exe, &paths.backup)
        .map_err(|e| format!("Could not move the running app aside: {e}"))?;
    if let Err(error) = std::fs::rename(&paths.staged, &paths.exe) {
        return Err(restore_backup(
            &paths,
            format!("Could not install the update: {error}"),
        ));
    }
    if let Err(error) = Command::new(&paths.exe)
        .args([RESTART_ARG, &std::process::id().to_string()])
        .spawn()
    {
        let _ = std::fs::rename(&paths.exe, &paths.staged);
        return Err(restore_backup(
            &paths,
            format!("Could not restart Refined: {error}"),
        ));
    }
    Ok(())
}

fn restore_backup(paths: &UpdatePaths, reason: String) -> String {
    match std::fs::rename(&paths.backup, &paths.exe) {
        Ok(()) => reason,
        Err(error) => format!("{reason}. Could not restore the previous program: {error}"),
    }
}

/// After an update restart, waits up to 10 seconds for the old process.
#[cfg(target_os = "linux")]
pub fn wait_for_previous_process() -> bool {
    let mut args = std::env::args();
    let Some(pid) = args
        .find(|arg| arg == RESTART_ARG)
        .and_then(|_| args.next())
        .and_then(|arg| arg.parse::<u32>().ok())
    else {
        return true;
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while Path::new(&format!("/proc/{pid}")).exists() {
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    true
}

#[cfg(windows)]
pub fn wait_for_previous_process() -> bool {
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };

    let mut args = std::env::args();
    let Some(pid) = args
        .find(|arg| arg == RESTART_ARG)
        .and_then(|_| args.next())
        .and_then(|arg| arg.parse().ok())
    else {
        return true;
    };
    let Ok(previous) = (unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }) else {
        return true;
    };
    unsafe {
        let stopped = WaitForSingleObject(previous, 10_000) == WAIT_OBJECT_0;
        let _ = CloseHandle(previous);
        stopped
    }
}

pub fn cleanup_after_update() {
    if let Ok(paths) = update_paths() {
        let _ = std::fs::remove_file(paths.backup);
        let _ = std::fs::remove_file(paths.staged);
    }
}

#[cfg(test)]
mod tests {
    use super::{matches_checksum, sha256_hex, version_pair};

    #[test]
    fn altered_update_is_rejected() {
        let original = [super::EXECUTABLE_MAGIC, b" verified executable"].concat();
        let altered = [super::EXECUTABLE_MAGIC, b" altered executable"].concat();
        let checksum = sha256_hex(&original);
        assert!(matches_checksum(&original, &checksum));
        assert!(!matches_checksum(&altered, &checksum));
        assert!(!matches_checksum(b"plain text", &sha256_hex(b"plain text")));
    }

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(version_pair("v0.2.0"), Some((0, 2, 0)));
        assert_eq!(version_pair("0.2.0"), Some((0, 2, 0)));
        assert!(version_pair("v0.10.0").unwrap() > version_pair("0.9.2").unwrap());
        assert_eq!(version_pair("abc"), None);
        assert_eq!(version_pair("v1.2.3-rc.1"), None);
        assert_eq!(version_pair("1.2.3.4"), None);
    }
}
