use std::fs;
use std::path::PathBuf;

const LAUNCH_AGENT_LABEL: &str = "com.onda.mixer";

/// Errors returned when reading or updating the macOS LaunchAgent plist.
#[derive(Debug, thiserror::Error)]
pub enum LaunchAgentError {
    #[error("could not resolve HOME directory")]
    MissingHomeDir,
    #[error("failed to remove LaunchAgent plist: {0}")]
    RemovePlist(#[source] std::io::Error),
    #[error("failed to determine executable path: {0}")]
    ExecutablePath(#[source] std::io::Error),
    #[error("failed to create LaunchAgents directory: {0}")]
    CreateDir(#[source] std::io::Error),
    #[error("failed to write LaunchAgent plist: {0}")]
    WritePlist(#[source] std::io::Error),
}

fn launch_agent_plist_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{LAUNCH_AGENT_LABEL}.plist")),
    )
}

/// Checks whether the macOS LaunchAgent plist for starting VolumeMixer at login exists.
pub fn is_launch_at_login_enabled() -> bool {
    launch_agent_plist_path()
        .map(|p| p.exists())
        .unwrap_or(false)
}

/// Enables or disables starting VolumeMixer automatically at user login via `~/Library/LaunchAgents`.
pub fn set_launch_at_login(enabled: bool) -> Result<(), LaunchAgentError> {
    let plist_path = launch_agent_plist_path().ok_or(LaunchAgentError::MissingHomeDir)?;

    if !enabled {
        if plist_path.exists() {
            fs::remove_file(&plist_path).map_err(LaunchAgentError::RemovePlist)?;
        }
        return Ok(());
    }

    let exe_path = std::env::current_exe().map_err(LaunchAgentError::ExecutablePath)?;
    let exe_str = exe_path.to_string_lossy();

    if let Some(parent) = plist_path.parent() {
        fs::create_dir_all(parent).map_err(LaunchAgentError::CreateDir)?;
    }

    let plist_contents = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LAUNCH_AGENT_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe_str}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#
    );

    fs::write(&plist_path, plist_contents).map_err(LaunchAgentError::WritePlist)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn launch_agent_error_display_messages_are_clear() {
        assert_eq!(
            LaunchAgentError::MissingHomeDir.to_string(),
            "could not resolve HOME directory"
        );
        let io_err = || io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        assert!(
            LaunchAgentError::RemovePlist(io_err())
                .to_string()
                .contains("failed to remove LaunchAgent plist: denied")
        );
        assert!(
            LaunchAgentError::ExecutablePath(io_err())
                .to_string()
                .contains("failed to determine executable path: denied")
        );
        assert!(
            LaunchAgentError::CreateDir(io_err())
                .to_string()
                .contains("failed to create LaunchAgents directory: denied")
        );
        assert!(
            LaunchAgentError::WritePlist(io_err())
                .to_string()
                .contains("failed to write LaunchAgent plist: denied")
        );
    }
}
