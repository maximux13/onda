# GitHub Releases Auto-Updater Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a GitHub Releases auto-update system to Onda with background version checking, an in-popover update notification banner + Settings badge, user-triggered download with `.app` bundle replacement and relaunch, and a full local testing harness.

**Architecture:** A dedicated `src/updater.rs` module wraps `self_update` (`github` backend with `rustls` and `archive-zip`) to check for newer SemVer releases and atomically replace `Onda.app` via `bundle_path_in_archive("Onda.app")`. A thread-safe `UpdateController` runs checks/downloads on background threads so the GPUI UI never blocks, surfacing state (`Checking`, `Available`, `Downloading`, `ReadyToRestart`, `Error`) in both a dismissible popover banner and `Settings -> About`. Environment overrides (`ONDA_UPDATE_API_URL`, `ONDA_UPDATE_CURRENT_VERSION`, `ONDA_UPDATE_INSTALL_PATH`) allow end-to-end local testing against a local mock HTTP server (`scripts/test_updater_local.sh`) without touching GitHub.

**Tech Stack:** Rust 2024, `self_update` (`archive-zip`, `compression-zip-deflate`, `rustls`), `gpui` 0.2.2, macOS `open` / `xattr`, Python 3 (`http.server` for local testing script).

## Global Constraints

- Repository: `maximux13/onda`; binary name: `onda`; macOS bundle name: `Onda.app`; release asset: `Onda-macos-arm64.zip`.
- Minimum macOS version: `14.2` (`Info.plist`).
- Keep `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, and `cargo test` green after every task.
- Never block the GPUI main thread with network I/O or archive extraction; always run `self_update` calls in `std::thread::spawn`.
- Preserve all existing comments and docstrings unrelated to changes.

---

## File Structure

- Modify: `Cargo.toml` — add `self_update` dependency with `archive-zip`, `compression-zip-deflate`, `rustls`.
- Create: `src/updater.rs` — `UpdateConfig`, `UpdateInfo`, `UpdateState`, `UpdateController`, bundle path discovery, SemVer check, `self_update` check/install, quarantine removal, relaunch helper, and unit/mock-HTTP integration tests.
- Modify: `src/main.rs` — declare `mod updater;`, instantiate shared `Arc<Mutex<UpdateController>>`, trigger startup background check (after 3s) and 6-hour periodic checks, pass controller into `TrayPopoverView`.
- Modify: `src/ui/popover/mod.rs` — store `updater: Arc<Mutex<UpdateController>>`, `update_banner_dismissed: bool`, and render the update banner above the main content when an update is available/downloading/ready/errored.
- Modify: `src/ui/popover/mixer_view.rs` — add an accent notification dot on the Settings gear button when an update is `Available` or `ReadyToRestart`.
- Modify: `src/ui/popover/settings_view.rs` — use dynamic `env!("CARGO_PKG_VERSION")` in `Settings -> About` and add an "Updates" card with status, "Check for Updates", "Update Now", and "Restart to Update" actions.
- Create: `scripts/test_updater_local.sh` — local end-to-end test script that packages `Onda.app`, spins up a local mock GitHub Releases API server, and launches Onda pointing at it.

---

### Task 1: Add `self_update` dependency and scaffold `src/updater.rs` with config & bundle discovery

**Files:**
- Modify: `Cargo.toml`
- Create: `src/updater.rs`
- Modify: `src/main.rs:1-8`

**Interfaces:**
- Produces: `crate::updater::{UpdateConfig, UpdateInfo, UpdateState, find_enclosing_app_bundle, is_newer_version}`

- [ ] **Step 1: Add `self_update` to `Cargo.toml`**

Run:
```bash
cargo add self_update --no-default-features --features "archive-zip,compression-zip-deflate,rustls"
```
Verify `Cargo.toml` now includes `self_update` with `default-features = false` and `features = ["archive-zip", "compression-zip-deflate", "rustls"]`.

- [ ] **Step 2: Write the failing unit tests in `src/updater.rs`**

Create `src/updater.rs` with types and tests first (stubs returning dummy values so tests compile and fail):

```rust
use std::path::{Path, PathBuf};

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
            current_version: std::env::var("ONDA_UPDATE_CURRENT_VERSION")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string()),
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
    Downloading { version: String },
    ReadyToRestart { version: String, app_path: PathBuf },
    Error(String),
}

/// Returns `true` if `candidate` is strictly newer than `current` according to SemVer.
pub fn is_newer_version(_current: &str, _candidate: &str) -> bool {
    false
}

/// Walks up from `exe_path` to find the enclosing `.app` bundle directory, if any.
pub fn find_enclosing_app_bundle(_exe_path: &Path) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
```

Register `mod updater;` in `src/main.rs` right after `mod ui;`.

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test updater::tests`
Expected: FAIL (`finds_enclosing_app_bundle_from_executable_path` and `compares_semver_versions_correctly` fail).

- [ ] **Step 4: Implement `is_newer_version` and `find_enclosing_app_bundle`**

Replace the two stub functions in `src/updater.rs`:

```rust
/// Returns `true` if `candidate` is strictly newer than `current` according to SemVer.
pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    let current_clean = current.trim().trim_start_matches('v');
    let candidate_clean = candidate.trim().trim_start_matches('v');
    self_update::version::bump_is_greater(current_clean, candidate_clean).unwrap_or(false)
}

/// Walks up from `exe_path` to find the enclosing `.app` bundle directory, if any.
pub fn find_enclosing_app_bundle(exe_path: &Path) -> Option<PathBuf> {
    exe_path
        .ancestors()
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("app"))
        .map(Path::to_path_buf)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test updater::tests`
Expected: PASS (3 tests pass).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/updater.rs src/main.rs
git commit -m "feat(updater): add self_update dependency and core version/bundle helpers"
```

---

### Task 2: Implement `check_for_updates`, `download_and_install_update`, and mock HTTP integration tests

**Files:**
- Modify: `src/updater.rs`

**Interfaces:**
- Consumes: `UpdateConfig`, `UpdateInfo`, `is_newer_version`, `find_enclosing_app_bundle`
- Produces: `check_for_updates(&UpdateConfig) -> Result<Option<UpdateInfo>, String>`, `download_and_install_update(&UpdateConfig) -> Result<(String, PathBuf), String>`, `relaunch_app(&Path) -> Result<(), String>`

- [ ] **Step 1: Write failing integration test with local mock GitHub API server**

Add to `mod tests` in `src/updater.rs`:

```rust
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn build_minimal_zip_with_app(app_name: &str, binary_content: &[u8]) -> Vec<u8> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .unix_permissions(0o755);
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

                if first_line.contains("/releases") {
                    let body = serde_json::json!([{
                        "tag_name": tag,
                        "name": format!("Onda {tag}"),
                        "body": "Test release notes",
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
                } else if first_line.contains("/download/") || first_line.contains("/assets/1") {
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        zip_bytes.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(&zip_bytes);
                } else {
                    let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
                }
            }
        });

        (base_url, handle)
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
```

Note: `zip` is a transitive dependency of `self_update`, or if not directly in `Cargo.toml`, add `zip = { version = "2", default-features = false, features = ["deflate"] }` under `[dev-dependencies]` in `Cargo.toml` so tests can build a real `.zip` archive cleanly.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test updater::tests`
Expected: FAIL (`check_for_updates` and `download_and_install_update` not defined).

- [ ] **Step 3: Implement `check_for_updates`, `download_and_install_update`, and `relaunch_app` in `src/updater.rs`**

```rust
fn resolve_target_app_bundle(config: &UpdateConfig) -> Result<PathBuf, String> {
    if let Some(path) = &config.bundle_install_path {
        return Ok(path.clone());
    }
    let exe = std::env::current_exe()
        .map_err(|e| format!("Failed to locate current executable: {e}"))?;
    find_enclosing_app_bundle(&exe).ok_or_else(|| {
        "Auto-update requires running from Onda.app (or setting ONDA_UPDATE_INSTALL_PATH)".to_string()
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

    let Some(latest) = releases.first() else {
        return Ok(None);
    };

    if !is_newer_version(&config.current_version, &latest.version) {
        return Ok(None);
    }

    let has_zip = latest.assets.iter().any(|a| a.name.ends_with(".zip"));
    if !has_zip {
        return Err(format!(
            "Release v{} has no macOS .zip asset",
            latest.version
        ));
    }

    Ok(Some(UpdateInfo {
        version: latest.version.trim_start_matches('v').to_string(),
        release_notes: latest.body.clone(),
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test updater::tests`
Expected: PASS (5 tests pass).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/updater.rs
git commit -m "feat(updater): implement GitHub release check, bundle install, and mock HTTP tests"
```

---

### Task 3: Add `UpdateController` for non-blocking background checks & downloads

**Files:**
- Modify: `src/updater.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `UpdateConfig`, `UpdateState`, `check_for_updates`, `download_and_install_update`
- Produces: `crate::updater::UpdateController` with `new(UpdateConfig)`, `state(&self) -> UpdateState`, `trigger_check(Arc<Mutex<Self>>)`, `trigger_download(Arc<Mutex<Self>>)`

- [ ] **Step 1: Write failing unit test for `UpdateController` state transitions**

Add to `mod tests` in `src/updater.rs`:

```rust
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
        panic!("UpdateController did not reach Available state: {:?}", controller.lock().unwrap().state());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test controller_transitions`
Expected: FAIL (`UpdateController` not defined).

- [ ] **Step 3: Implement `UpdateController` in `src/updater.rs` and wire startup/periodic checks in `src/main.rs`**

Add to `src/updater.rs`:

```rust
use std::sync::{Arc, Mutex};

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

    /// Spawns a background thread to check GitHub Releases if not already checking/downloading.
    pub fn trigger_check(controller: Arc<Mutex<Self>>) {
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
```

In `src/main.rs`:
- Create `let updater = Arc::new(Mutex::new(updater::UpdateController::new(updater::UpdateConfig::from_env())));`
- Spawn a background task on `cx` that waits 3 seconds, calls `UpdateController::trigger_check(Arc::clone(&updater))`, and repeats every `6 * 3600` seconds.
- Pass `Arc::clone(&updater)` into `TrayPopoverView::new(engine_clone, updater_clone, cx)`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test updater::tests`
Expected: PASS (6 tests pass).

- [ ] **Step 5: Commit**

```bash
git add src/updater.rs src/main.rs
git commit -m "feat(updater): add UpdateController and background startup/periodic checks"
```

---

### Task 4: Add Update Banner in Popover, Badge on Settings Icon, and Update Controls in `Settings -> About`

**Files:**
- Modify: `src/ui/popover/mod.rs`
- Modify: `src/ui/popover/mixer_view.rs`
- Modify: `src/ui/popover/settings_view.rs`

**Interfaces:**
- Consumes: `crate::updater::{UpdateController, UpdateState, relaunch_app}`

- [ ] **Step 1: Add `updater` and `update_banner_dismissed` to `TrayPopoverView` in `src/ui/popover/mod.rs`**

Update `TrayPopoverView` struct and `TrayPopoverView::new(engine: Arc<Mutex<AudioEngine>>, updater: Arc<Mutex<UpdateController>>, cx: &mut Context<Self>)`:
- Store `pub(super) updater: Arc<Mutex<UpdateController>>`
- Store `pub(super) update_banner_dismissed: bool` (initialized to `false`)
- Add `pub(super) fn update_state(&self) -> UpdateState` helper reading `self.updater.lock().map(|u| u.state()).unwrap_or_default()`.
- Add `render_update_banner(&self, palette: ThemePalette, cx: &mut Context<Self>) -> Option<AnyElement>` in `src/ui/popover/mod.rs`:
  - `UpdateState::Available(info)` (when `!self.update_banner_dismissed`): shows `"Nueva versión v{version} disponible"`, `"Actualizar"` button (calls `UpdateController::trigger_download(Arc::clone(&this.updater))`), and `"×"` dismiss button (`this.update_banner_dismissed = true`).
  - `UpdateState::Downloading { version }`: shows `"Descargando Onda v{version}..."`.
  - `UpdateState::ReadyToRestart { version, app_path }`: shows `"v{version} lista"` and `"Reiniciar y actualizar"` button (calls `let _ = crate::updater::relaunch_app(&app_path); cx.quit();`).
  - `UpdateState::Error(msg)` (when `!self.update_banner_dismissed`): shows compact error text + `"Reintentar"` button (`UpdateController::trigger_check`) + `"×"` dismiss button.
- Render `self.render_update_banner(palette, cx)` at the top of the popover content when `Some(banner)`.

- [ ] **Step 2: Add notification dot on the Settings button in `src/ui/popover/mixer_view.rs`**

In `render_mixer_header`, when `matches!(self.update_state(), UpdateState::Available(_) | UpdateState::ReadyToRestart { .. })`, render a `6px × 6px` `palette.accent_blue` circle positioned at top-right (`absolute().top(px(5.0)).right(px(5.0))`) inside `mixer-open-settings-btn` (with `.relative()` on the button).

- [ ] **Step 3: Update `render_settings_about_tab` in `src/ui/popover/settings_view.rs`**

- Replace `"Onda v0.1.0"` with `format!("Onda v{}", env!("CARGO_PKG_VERSION"))`.
- Add an **Updates** card below the About card showing:
  - Current status text (`Idle`/`UpToDate`: `"You're on the latest version."`, `Checking`: `"Checking for updates..."`, `Available(info)`: `format!("Version v{} is available.", info.version)`, `Downloading { version }`: `format!("Downloading v{version}...")`, `ReadyToRestart { version, .. }`: `format!("v{version} is ready. Restart to finish updating.")`, `Error(err)`: `err`).
  - Action button:
    - `Idle` / `UpToDate` / `Error`: `"Check for Updates"` -> `UpdateController::trigger_check(Arc::clone(&this.updater))`
    - `Available`: `"Download Update"` -> `UpdateController::trigger_download(Arc::clone(&this.updater))`
    - `ReadyToRestart`: `"Restart & Update"` -> `relaunch_app(&app_path)` + `cx.quit()`

- [ ] **Step 4: Run quality gate**

Run:
```bash
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test
```
Expected: All pass with zero warnings.

- [ ] **Step 5: Commit**

```bash
git add src/ui/popover/mod.rs src/ui/popover/mixer_view.rs src/ui/popover/settings_view.rs
git commit -m "feat(ui): add update notification banner, settings badge, and About update controls"
```

---

### Task 5: Create local end-to-end testing script (`scripts/test_updater_local.sh`)

**Files:**
- Create: `scripts/test_updater_local.sh`

- [ ] **Step 1: Write `scripts/test_updater_local.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail

PORT="${ONDA_TEST_PORT:-8787}"
TEST_ROOT="/tmp/onda-update-test"
INSTALLED_DIR="$TEST_ROOT/Installed"
MOCK_API_DIR="$TEST_ROOT/api"
FAKE_VERSION="${1:-99.0.0}"

echo "==> Building Onda.app (debug)..."
./scripts/bundle_app.sh debug

rm -rf "$TEST_ROOT"
mkdir -p "$INSTALLED_DIR" "$MOCK_API_DIR/repos/maximux13/onda" "$MOCK_API_DIR/download"

echo "==> Copying Onda.app to $INSTALLED_DIR/Onda.app..."
cp -R "target/debug/Onda.app" "$INSTALLED_DIR/Onda.app"

echo "==> Creating mock release archive Onda-macos-arm64.zip (v$FAKE_VERSION)..."
ditto -c -k --sequesterRsrc --keepParent "target/debug/Onda.app" "$MOCK_API_DIR/download/Onda-macos-arm64.zip"

cat > "$MOCK_API_DIR/repos/maximux13/onda/releases" <<EOF
[
  {
    "tag_name": "v$FAKE_VERSION",
    "name": "Onda v$FAKE_VERSION (Local Test)",
    "body": "Local mock release for testing Onda auto-updater.",
    "assets": [
      {
        "name": "Onda-macos-arm64.zip",
        "browser_download_url": "http://127.0.0.1:$PORT/download/Onda-macos-arm64.zip",
        "url": "http://127.0.0.1:$PORT/download/Onda-macos-arm64.zip"
      }
    ]
  }
]
EOF

echo "==> Starting local mock GitHub API server on http://127.0.0.1:$PORT..."
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$MOCK_API_DIR" >/tmp/onda-mock-server.log 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT

sleep 0.5
echo "==> Launching Onda.app with local updater env overrides..."
echo "    ONDA_UPDATE_API_URL=http://127.0.0.1:$PORT"
echo "    ONDA_UPDATE_CURRENT_VERSION=0.1.0"
echo "    ONDA_UPDATE_INSTALL_PATH=$INSTALLED_DIR/Onda.app"

ONDA_UPDATE_API_URL="http://127.0.0.1:$PORT" \
ONDA_UPDATE_CURRENT_VERSION="0.1.0" \
ONDA_UPDATE_INSTALL_PATH="$INSTALLED_DIR/Onda.app" \
"$INSTALLED_DIR/Onda.app/Contents/MacOS/onda"
```

- [ ] **Step 2: Make script executable and verify syntax**

Run:
```bash
chmod +x scripts/test_updater_local.sh
bash -n scripts/test_updater_local.sh
```
Expected: Exit code 0.

- [ ] **Step 3: Commit**

```bash
git add scripts/test_updater_local.sh
git commit -m "chore(updater): add local end-to-end updater test script"
```

---

## Self-Review Checklist
- **Spec coverage:** GitHub Releases auto-update (`self_update` + `Onda.app` bundle replacement), notification in popover + Settings badge + `Settings -> About`, and local testing (mock HTTP unit/integration tests + `scripts/test_updater_local.sh`) are all covered.
- **No placeholders:** Every task has exact file paths, complete code blocks, and exact commands.
- **Type consistency:** `UpdateConfig`, `UpdateInfo`, `UpdateState`, `UpdateController`, `check_for_updates`, `download_and_install_update`, and `relaunch_app` match across all tasks.
