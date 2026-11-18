use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const REPO_OWNER: &str = "maximux13";
pub const REPO_NAME: &str = "onda";
pub const BIN_NAME: &str = "onda";
pub const BUNDLE_NAME: &str = "Onda.app";

/// Configuration for checking and applying GitHub Release updates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateConfig {
    pub repo_owner: String,
    pub repo_name: String,
    pub bin_name: String,
    pub bundle_name: String,
    pub current_version: String,
    pub api_base_url: Option<String>,
    pub bundle_install_path: Option<PathBuf>,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

impl UpdateConfig {
    /// Builds configuration from compile-time defaults with optional environment overrides
    /// (`ONDA_UPDATE_API_URL`, `ONDA_UPDATE_CURRENT_VERSION`, `ONDA_UPDATE_INSTALL_PATH`)
    /// for local testing.
    pub fn from_env() -> Self {
        Self {
            repo_owner: REPO_OWNER.to_string(),
            repo_name: REPO_NAME.to_string(),
            bin_name: BIN_NAME.to_string(),
            bundle_name: BUNDLE_NAME.to_string(),
            current_version: installed_app_version(),
            api_base_url: std::env::var("ONDA_UPDATE_API_URL")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            bundle_install_path: std::env::var("ONDA_UPDATE_INSTALL_PATH")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .map(PathBuf::from),
        }
    }
}

/// Metadata describing an available update release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: String,
    pub release_notes: Option<String>,
}

/// Current lifecycle state of the auto-updater.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(UpdateInfo),
    Downloading {
        version: String,
    },
    ReadyToRestart {
        version: String,
        app_path: PathBuf,
    },
    Error(String),
}

/// Returns `true` if `candidate` is strictly newer than `current` according to SemVer.
pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    let current_clean = current.trim().trim_start_matches('v');
    let candidate_clean = candidate.trim().trim_start_matches('v');
    self_update::version::bump_is_greater(current_clean, candidate_clean).unwrap_or(false)
}

/// Reads `CFBundleShortVersionString` from `<app_bundle>/Contents/Info.plist`, if present.
pub fn bundle_short_version(app_bundle: &Path) -> Option<String> {
    let plist = std::fs::read_to_string(app_bundle.join("Contents/Info.plist")).ok()?;
    let (_, after_key) = plist.split_once("<key>CFBundleShortVersionString</key>")?;
    let (_, after_open) = after_key.split_once("<string>")?;
    let (version, _) = after_open.split_once("</string>")?;
    let trimmed = version.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Returns the active app version, checking `ONDA_UPDATE_CURRENT_VERSION`, then the
/// enclosing `.app` bundle's `Info.plist`, then the compile-time `CARGO_PKG_VERSION`.
pub fn installed_app_version() -> String {
    if let Ok(v) = std::env::var("ONDA_UPDATE_CURRENT_VERSION") {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(app_bundle) = find_enclosing_app_bundle(&exe)
        && let Some(version) = bundle_short_version(&app_bundle)
    {
        return version;
    }
    env!("CARGO_PKG_VERSION").to_string()
}

/// Walks up from `exe_path` to find the enclosing `.app` bundle directory, if any.
pub fn find_enclosing_app_bundle(exe_path: &Path) -> Option<PathBuf> {
    exe_path
        .ancestors()
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("app"))
        .map(Path::to_path_buf)
}

fn resolve_target_app_bundle(config: &UpdateConfig) -> Result<PathBuf, String> {
    if let Some(path) = &config.bundle_install_path {
        return Ok(path.clone());
    }
    let exe =
        std::env::current_exe().map_err(|e| format!("Failed to locate current executable: {e}"))?;
    find_enclosing_app_bundle(&exe).ok_or_else(|| {
        "Auto-update requires running from Onda.app (or setting ONDA_UPDATE_INSTALL_PATH)"
            .to_string()
    })
}

/// Queries GitHub Releases (or `config.api_base_url`) for a version newer than `config.current_version`.
pub fn check_for_updates(config: &UpdateConfig) -> Result<Option<UpdateInfo>, String> {
    let mut builder = self_update::backends::github::ReleaseList::configure();
    builder
        .repo_owner(&config.repo_owner)
        .repo_name(&config.repo_name);

    if let Some(base_url) = &config.api_base_url {
        builder.api_base_url(base_url);
    }

    let releases = builder
        .build()
        .map_err(|e| format!("Failed to configure release check: {e}"))?
        .fetch()
        .map_err(|e| format!("Failed to fetch releases: {e}"))?;

    let Some(latest) = releases.latest() else {
        return Ok(None);
    };

    if !is_newer_version(&config.current_version, latest.version()) {
        return Ok(None);
    }

    let has_zip = latest.assets().iter().any(|a| a.name().ends_with(".zip"));
    if !has_zip {
        return Err(format!(
            "Release v{} has no macOS .zip asset",
            latest.version()
        ));
    }

    Ok(Some(UpdateInfo {
        version: latest.version().trim_start_matches('v').to_string(),
        release_notes: latest.body().map(str::to_string),
    }))
}

/// Downloads the latest release `.zip` and replaces the `Onda.app` bundle on disk.
pub fn download_and_install_update(config: &UpdateConfig) -> Result<(String, PathBuf), String> {
    let app_path = resolve_target_app_bundle(config)?;

    let mut builder = self_update::backends::github::Update::configure();
    builder
        .repo_owner(&config.repo_owner)
        .repo_name(&config.repo_name)
        .bin_name(&config.bin_name)
        .current_version(config.current_version.trim_start_matches('v'))
        .bundle_path_in_archive(&config.bundle_name)
        .bundle_install_path(&app_path)
        .target("macos-arm64")
        .no_confirm(true)
        .show_download_progress(false);

    if let Some(base_url) = &config.api_base_url {
        builder.api_base_url(base_url);
    }

    let status = builder
        .build()
        .map_err(|e| format!("Failed to configure updater: {e}"))?
        .update()
        .map_err(|e| format!("Update failed: {e}"))?;

    // Clear macOS quarantine xattr if present so Gatekeeper does not block relaunch.
    let _ = std::process::Command::new("xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(&app_path)
        .output();

    Ok((
        status.version().trim_start_matches('v').to_string(),
        app_path,
    ))
}

/// Launches the updated `.app` bundle after a short delay so the current process can exit cleanly.
pub fn relaunch_app(app_path: &Path) -> Result<(), String> {
    let escaped = app_path.to_string_lossy().replace('\'', "'\\''");
    let cmd = format!("sleep 0.4 && open -n '{escaped}'");
    std::process::Command::new("/bin/sh")
        .args(["-c", &cmd])
        .spawn()
        .map_err(|e| format!("Failed to relaunch Onda: {e}"))?;
    Ok(())
}

/// Thread-safe coordinator for background update checks and downloads.
pub struct UpdateController {
    config: UpdateConfig,
    state: UpdateState,
}

impl UpdateController {
    pub fn new(config: UpdateConfig) -> Self {
        Self {
            config,
            state: UpdateState::Idle,
        }
    }

    pub fn state(&self) -> UpdateState {
        self.state.clone()
    }

    /// Spawns a manual check on a background thread (surfaces errors in `UpdateState::Error`).
    pub fn trigger_check(controller: Arc<Mutex<Self>>) {
        Self::trigger_check_inner(controller, false);
    }

    /// Spawns a background check that stays silent (`UpdateState::Idle`) if the network is
    /// unavailable or the release endpoint cannot be reached.
    pub fn trigger_background_check(controller: Arc<Mutex<Self>>) {
        Self::trigger_check_inner(controller, true);
    }

    fn trigger_check_inner(controller: Arc<Mutex<Self>>, silent_on_error: bool) {
        let config = {
            let Ok(mut guard) = controller.lock() else {
                return;
            };
            if matches!(
                guard.state,
                UpdateState::Checking | UpdateState::Downloading { .. }
            ) {
                return;
            }
            guard.state = UpdateState::Checking;
            guard.config.clone()
        };

        std::thread::spawn(move || {
            let next_state = match check_for_updates(&config) {
                Ok(Some(info)) => UpdateState::Available(info),
                Ok(None) => UpdateState::UpToDate,
                Err(_) if silent_on_error => UpdateState::Idle,
                Err(err) => UpdateState::Error(err),
            };
            if let Ok(mut guard) = controller.lock() {
                guard.state = next_state;
            }
        });
    }

    /// Spawns a background thread to download and replace `Onda.app`.
    pub fn trigger_download(controller: Arc<Mutex<Self>>) {
        let (config, target_version) = {
            let Ok(mut guard) = controller.lock() else {
                return;
            };
            let version = match &guard.state {
                UpdateState::Available(info) => info.version.clone(),
                _ => return,
            };
            guard.state = UpdateState::Downloading {
                version: version.clone(),
            };
            (guard.config.clone(), version)
        };

        std::thread::spawn(move || {
            let next_state = match download_and_install_update(&config) {
                Ok((installed_version, app_path)) => UpdateState::ReadyToRestart {
                    version: if installed_version.is_empty() {
                        target_version
                    } else {
                        installed_version
                    },
                    app_path,
                },
                Err(err) => UpdateState::Error(err),
            };
            if let Ok(mut guard) = controller.lock() {
                guard.state = next_state;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn build_minimal_zip_with_app(app_name: &str, binary_content: &[u8]) -> Vec<u8> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        zip.start_file(format!("{app_name}/Contents/MacOS/onda"), options)
            .unwrap();
        zip.write_all(binary_content).unwrap();
        zip.finish().unwrap().into_inner()
    }

    fn spawn_mock_github_server(
        tag: &str,
        zip_bytes: Vec<u8>,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{addr}");
        let tag = tag.to_string();

        let handle = std::thread::spawn(move || {
            // Handle up to 4 requests (release list + asset download)
            for _ in 0..4 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let first_line = req.lines().next().unwrap_or("");

                if first_line.contains("/download/") || first_line.contains("/assets/1") {
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        zip_bytes.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(&zip_bytes);
                } else if first_line.contains("/releases") {
                    let body = serde_json::json!([{
                        "tag_name": tag,
                        "name": format!("Onda {tag}"),
                        "body": "Test release notes",
                        "created_at": "2026-11-18T00:00:00Z",
                        "assets": [{
                            "name": "Onda-macos-arm64.zip",
                            "browser_download_url": format!("http://{addr}/download/Onda-macos-arm64.zip"),
                            "url": format!("http://{addr}/repos/maximux13/onda/releases/assets/1")
                        }]
                    }])
                    .to_string();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(resp.as_bytes());
                } else {
                    let _ =
                        stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
                }
            }
        });

        (base_url, handle)
    }

    #[test]
    fn finds_enclosing_app_bundle_from_executable_path() {
        let exe = Path::new("/Applications/Onda.app/Contents/MacOS/onda");
        assert_eq!(
            find_enclosing_app_bundle(exe),
            Some(PathBuf::from("/Applications/Onda.app"))
        );
    }

    #[test]
    fn returns_none_when_not_inside_app_bundle() {
        let exe = Path::new("/Users/dev/workspace/target/debug/onda");
        assert_eq!(find_enclosing_app_bundle(exe), None);
    }

    #[test]
    fn compares_semver_versions_correctly() {
        assert!(is_newer_version("0.1.0", "0.2.0"));
        assert!(is_newer_version("0.1.0", "v0.1.1"));
        assert!(!is_newer_version("0.2.0", "0.2.0"));
        assert!(!is_newer_version("0.2.0", "0.1.9"));
    }

    #[test]
    fn check_for_updates_detects_newer_release_from_mock_server() {
        let zip_bytes = build_minimal_zip_with_app("Onda.app", b"new-binary");
        let (base_url, _server) = spawn_mock_github_server("v0.2.0", zip_bytes);

        let config = UpdateConfig {
            current_version: "0.1.0".to_string(),
            api_base_url: Some(base_url),
            ..UpdateConfig::default()
        };

        let found = check_for_updates(&config).expect("check should succeed");
        assert_eq!(
            found,
            Some(UpdateInfo {
                version: "0.2.0".to_string(),
                release_notes: Some("Test release notes".to_string()),
            })
        );
    }

    #[test]
    fn download_and_install_replaces_app_bundle_in_temp_dir() {
        let temp_dir = std::env::temp_dir().join(format!(
            "onda-updater-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let app_dir = temp_dir.join("Onda.app");
        std::fs::create_dir_all(app_dir.join("Contents/MacOS")).unwrap();
        std::fs::write(app_dir.join("Contents/MacOS/onda"), b"old-binary").unwrap();

        let zip_bytes = build_minimal_zip_with_app("Onda.app", b"new-binary-v0.2.0");
        let (base_url, _server) = spawn_mock_github_server("v0.2.0", zip_bytes);

        let config = UpdateConfig {
            current_version: "0.1.0".to_string(),
            api_base_url: Some(base_url),
            bundle_install_path: Some(app_dir.clone()),
            ..UpdateConfig::default()
        };

        let (version, installed_path) =
            download_and_install_update(&config).expect("install should succeed");
        assert_eq!(version, "0.2.0");
        assert_eq!(installed_path, app_dir);
        let updated_bin = std::fs::read(app_dir.join("Contents/MacOS/onda")).unwrap();
        assert_eq!(updated_bin, b"new-binary-v0.2.0");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn controller_transitions_from_idle_to_available_on_check() {
        let zip_bytes = build_minimal_zip_with_app("Onda.app", b"bin");
        let (base_url, _server) = spawn_mock_github_server("v0.3.0", zip_bytes);

        let controller = Arc::new(Mutex::new(UpdateController::new(UpdateConfig {
            current_version: "0.1.0".to_string(),
            api_base_url: Some(base_url),
            ..UpdateConfig::default()
        })));

        UpdateController::trigger_check(Arc::clone(&controller));

        // Poll briefly for background thread completion in test
        for _ in 0..50 {
            if let UpdateState::Available(info) = controller.lock().unwrap().state() {
                assert_eq!(info.version, "0.3.0");
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!(
            "UpdateController did not reach Available state: {:?}",
            controller.lock().unwrap().state()
        );
    }
}
