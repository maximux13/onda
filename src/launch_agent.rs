use std::fs;
use std::path::PathBuf;

const LAUNCH_AGENT_LABEL: &str = "com.onda.mixer";

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
pub fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    let plist_path = launch_agent_plist_path()
        .ok_or_else(|| String::from("Could not resolve HOME directory"))?;

    if !enabled {
        if plist_path.exists() {
            fs::remove_file(&plist_path)
                .map_err(|e| format!("Failed to remove LaunchAgent plist: {e}"))?;
        }
        return Ok(());
    }

    let exe_path =
        std::env::current_exe().map_err(|e| format!("Failed to determine executable path: {e}"))?;
    let exe_str = exe_path.to_string_lossy();

    if let Some(parent) = plist_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create LaunchAgents directory: {e}"))?;
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

    fs::write(&plist_path, plist_contents)
        .map_err(|e| format!("Failed to write LaunchAgent plist: {e}"))
}
