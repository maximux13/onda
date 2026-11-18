#![allow(dead_code)]

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

/// Walks up from `exe_path` to find the enclosing `.app` bundle directory, if any.
pub fn find_enclosing_app_bundle(exe_path: &Path) -> Option<PathBuf> {
    exe_path
        .ancestors()
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("app"))
        .map(Path::to_path_buf)
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
