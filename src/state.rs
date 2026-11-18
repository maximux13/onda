#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Center frequencies (Hz) for the 10-band equalizer.
pub const EQ_FREQUENCIES: [f32; 10] = [
    32.0, 64.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// Display labels for the 10-band equalizer.
pub const EQ_LABELS: [&str; 10] = [
    "32", "64", "125", "250", "500", "1k", "2k", "4k", "8k", "16k",
];

/// UI color theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeMode {
    #[default]
    Auto,
    Dark,
    Light,
}

/// Window background style (Transparent glassmorphism vs Solid).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WindowStyle {
    #[default]
    Transparent,
    Solid,
}

/// Icon classification for an audio output device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DeviceIconKind {
    #[default]
    Speaker,
    Headphones,
    AirPods,
    Display,
    Virtual,
}

/// State of an individual macOS audio output device.
#[derive(Clone, Debug)]
pub struct OutputDeviceInfo {
    pub id: u32,
    pub uid: String,
    pub name: String,
    pub subtitle: Option<String>,
    pub icon_kind: DeviceIconKind,
    pub is_default: bool,
    pub volume: f32,
    pub muted: bool,
}

/// State of an individual macOS audio input device (microphone).
#[derive(Clone, Debug, PartialEq)]
pub struct InputDeviceInfo {
    pub id: u32,
    pub uid: String,
    pub name: String,
    pub is_default: bool,
    pub volume: f32,
    pub muted: bool,
}

/// Metadata for a running macOS application (with or without active audio output).
#[derive(Clone, Debug)]
pub struct AudioAppInfo {
    pub object_ids: Vec<u32>,
    pub bundle_id: String,
    pub name: String,
    pub is_running_output: bool,
    pub icon_png: Option<Vec<u8>>,
    /// Dominant vibrant RGB color (`0xRRGGBB`) extracted from the application's icon.
    pub dominant_color: u32,
}

fn default_eq_gains() -> [f32; 10] {
    [0.0; 10]
}

/// Default normalized glass opacity slider value (`0.50`, mapped to `80%` alpha in `[0.60, 1.00]`).
pub const DEFAULT_GLASS_OPACITY: f32 = 0.50;

fn default_true() -> bool {
    true
}

fn default_max_volume() -> f32 {
    2.0
}

fn default_glass_opacity() -> f32 {
    DEFAULT_GLASS_OPACITY
}

/// Persisted or current volume/mute/EQ/routing setting for a single application.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppVolumeConfig {
    pub volume: f32,
    pub muted: bool,
    #[serde(default)]
    pub boost_enabled: bool,
    #[serde(default)]
    pub target_device_uid: Option<String>,
    #[serde(default = "default_true")]
    pub eq_enabled: bool,
    #[serde(default = "default_eq_gains")]
    pub eq_gains: [f32; 10],
}

impl Default for AppVolumeConfig {
    fn default() -> Self {
        Self {
            volume: 1.0,
            muted: false,
            boost_enabled: false,
            target_device_uid: None,
            eq_enabled: true,
            eq_gains: [0.0; 10],
        }
    }
}

/// Configuration for automatic music ducking when voice apps output audio.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoDuckingConfig {
    pub enabled: bool,
    pub duck_amount: f32,
    pub threshold: f32,
    pub release_ms: u64,
    pub voice_bundle_ids: Vec<String>,
    pub music_bundle_ids: Vec<String>,
}

impl Default for AutoDuckingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            duck_amount: 0.30,
            threshold: 0.02,
            release_ms: 1200,
            voice_bundle_ids: vec![
                "com.hnc.Discord".to_string(),
                "us.zoom.xos".to_string(),
                "com.tinyspeck.slackmacgap".to_string(),
                "com.microsoft.teams2".to_string(),
                "com.apple.FaceTime".to_string(),
            ],
            music_bundle_ids: vec![
                "com.spotify.client".to_string(),
                "com.apple.Music".to_string(),
                "com.tidal.desktop".to_string(),
                "org.videolan.vlc".to_string(),
            ],
        }
    }
}

/// Global keyboard shortcut actions supported by Onda.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShortcutAction {
    TogglePopover,
    MuteActiveApp,
    CycleOutputDevice,
}

/// Persisted configuration for a single global keyboard shortcut.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutBinding {
    pub enabled: bool,
    pub accelerator: String,
}

/// Global keyboard shortcut bindings for quick actions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutsConfig {
    #[serde(default = "default_toggle_popover_binding")]
    pub toggle_popover: ShortcutBinding,
    #[serde(default = "default_mute_active_app_binding")]
    pub mute_active_app: ShortcutBinding,
    #[serde(default = "default_cycle_output_device_binding")]
    pub cycle_output_device: ShortcutBinding,
}

fn default_toggle_popover_binding() -> ShortcutBinding {
    ShortcutBinding {
        enabled: false,
        accelerator: String::from("CmdOrCtrl+Shift+O"),
    }
}

fn default_mute_active_app_binding() -> ShortcutBinding {
    ShortcutBinding {
        enabled: false,
        accelerator: String::from("CmdOrCtrl+Shift+M"),
    }
}

fn default_cycle_output_device_binding() -> ShortcutBinding {
    ShortcutBinding {
        enabled: false,
        accelerator: String::from("CmdOrCtrl+Shift+D"),
    }
}

impl Default for ShortcutsConfig {
    fn default() -> Self {
        Self {
            toggle_popover: default_toggle_popover_binding(),
            mute_active_app: default_mute_active_app_binding(),
            cycle_output_device: default_cycle_output_device_binding(),
        }
    }
}

/// Legacy single-master struct kept for compatibility.
#[derive(Clone, Debug, Default)]
pub struct MasterState {
    pub device_name: String,
    pub volume: f32,
    pub muted: bool,
}

/// Combined row model rendered by the UI for each audio application.
#[derive(Clone, Debug)]
pub struct AppRowState {
    pub info: AudioAppInfo,
    pub config: AppVolumeConfig,
    /// Real-time peak audio level `[0.0, 1.0]` for the 5-bar VU meter.
    pub peak_level: f32,
    /// Resolved display name of the output device this app is routed to.
    pub routed_device_name: String,
    /// Icon kind of the output device this app is routed to.
    pub routed_device_icon: DeviceIconKind,
    /// Whether this app is currently attenuated by auto-ducking.
    pub is_ducked: bool,
}

/// Full snapshot of mixer state passed from the audio engine to the UI.
#[derive(Clone, Debug)]
pub struct MixerSnapshot {
    pub default_output_name: String,
    pub default_input_name: String,
    pub devices: Vec<OutputDeviceInfo>,
    pub input_devices: Vec<InputDeviceInfo>,
    pub apps: Vec<AppRowState>,
    pub theme: ThemeMode,
    pub window_style: WindowStyle,
    pub glass_opacity: f32,
    pub max_volume_limit: f32,
    pub restore_on_close: bool,
    pub show_in_menu_bar: bool,
    pub show_all_apps: bool,
    pub advanced_mode: bool,
    pub eq_collapsed: bool,
    pub global_eq_enabled: bool,
    pub global_eq_gains: [f32; 10],
    /// Selected app `bundle_id` for the equalizer (`None` = Global Equalizer).
    pub selected_eq_app: Option<String>,
    /// Whether auto-ducking is currently attenuating music apps.
    pub ducking_active: bool,
    pub auto_ducking: AutoDuckingConfig,
    pub shortcuts: ShortcutsConfig,
}

impl Default for MixerSnapshot {
    fn default() -> Self {
        Self {
            default_output_name: String::from("MacBook Pro Speakers"),
            default_input_name: String::from("MacBook Pro Microphone"),
            devices: Vec::new(),
            input_devices: Vec::new(),
            apps: Vec::new(),
            theme: ThemeMode::Auto,
            window_style: WindowStyle::Transparent,
            glass_opacity: DEFAULT_GLASS_OPACITY,
            max_volume_limit: 2.0,
            restore_on_close: false,
            show_in_menu_bar: true,
            show_all_apps: false,
            advanced_mode: false,
            eq_collapsed: false,
            global_eq_enabled: true,
            global_eq_gains: [0.0; 10],
            selected_eq_app: None,
            ducking_active: false,
            auto_ducking: AutoDuckingConfig::default(),
            shortcuts: ShortcutsConfig::default(),
        }
    }
}

/// Persisted preferences (theme, window style, global EQ, and per-app settings).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default)]
    pub theme: ThemeMode,
    #[serde(default)]
    pub window_style: WindowStyle,
    #[serde(default = "default_glass_opacity")]
    pub glass_opacity: f32,
    #[serde(default = "default_max_volume")]
    pub max_volume_limit: f32,
    #[serde(default)]
    pub restore_on_close: bool,
    #[serde(default = "default_true")]
    pub show_in_menu_bar: bool,
    #[serde(default)]
    pub show_all_apps: bool,
    #[serde(default)]
    pub advanced_mode: bool,
    #[serde(default)]
    pub eq_collapsed: bool,
    #[serde(default = "default_true")]
    pub global_eq_enabled: bool,
    #[serde(default = "default_eq_gains")]
    pub global_eq_gains: [f32; 10],
    #[serde(default)]
    pub apps: HashMap<String, AppVolumeConfig>,
    #[serde(default)]
    pub auto_ducking: AutoDuckingConfig,
    #[serde(default)]
    pub shortcuts: ShortcutsConfig,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Auto,
            window_style: WindowStyle::Transparent,
            glass_opacity: DEFAULT_GLASS_OPACITY,
            max_volume_limit: 2.0,
            restore_on_close: false,
            show_in_menu_bar: true,
            show_all_apps: false,
            advanced_mode: false,
            eq_collapsed: false,
            global_eq_enabled: true,
            global_eq_gains: [0.0; 10],
            apps: HashMap::new(),
            auto_ducking: AutoDuckingConfig::default(),
            shortcuts: ShortcutsConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_volume_config_defaults() {
        let cfg = AppVolumeConfig::default();
        assert_eq!(cfg.volume, 1.0);
        assert!(!cfg.muted);
        assert!(!cfg.boost_enabled);
        assert!(cfg.eq_enabled);
        assert_eq!(cfg.eq_gains, [0.0; 10]);
        assert!(cfg.target_device_uid.is_none());
    }

    #[test]
    fn test_preferences_json_roundtrip() {
        let mut prefs = Preferences {
            theme: ThemeMode::Dark,
            window_style: WindowStyle::Transparent,
            max_volume_limit: 1.5,
            ..Default::default()
        };
        prefs.global_eq_gains[0] = 4.5;
        prefs.global_eq_gains[9] = -3.0;

        let mut app_cfg = AppVolumeConfig {
            volume: 0.75,
            boost_enabled: true,
            ..Default::default()
        };
        app_cfg.eq_gains[3] = 2.0;
        prefs.apps.insert("com.spotify.client".to_string(), app_cfg);

        let json = serde_json::to_string(&prefs).expect("serialize prefs");
        let restored: Preferences = serde_json::from_str(&json).expect("deserialize prefs");

        assert_eq!(restored.theme, ThemeMode::Dark);
        assert_eq!(restored.window_style, WindowStyle::Transparent);
        assert_eq!(restored.max_volume_limit, 1.5);
        assert_eq!(restored.global_eq_gains[0], 4.5);
        assert_eq!(restored.global_eq_gains[9], -3.0);

        let spotify = restored
            .apps
            .get("com.spotify.client")
            .expect("spotify config");
        assert_eq!(spotify.volume, 0.75);
        assert!(spotify.boost_enabled);
        assert_eq!(spotify.eq_gains[3], 2.0);
    }

    #[test]
    fn test_eq_constants_match_ten_bands() {
        assert_eq!(EQ_FREQUENCIES.len(), 10);
        assert_eq!(EQ_LABELS.len(), 10);
        assert_eq!(EQ_FREQUENCIES[0], 32.0);
        assert_eq!(EQ_FREQUENCIES[9], 16000.0);
    }

    #[test]
    fn test_preferences_without_auto_ducking_deserializes_with_defaults() {
        let legacy_json = r#"{"theme":"Dark"}"#;
        let prefs: Preferences = serde_json::from_str(legacy_json).expect("deserialize legacy");
        assert!(!prefs.auto_ducking.enabled);
        assert!((prefs.auto_ducking.duck_amount - 0.30).abs() < 1e-6);
        assert!((prefs.auto_ducking.threshold - 0.02).abs() < 1e-6);
        assert_eq!(prefs.auto_ducking.release_ms, 1200);
        assert!(
            prefs
                .auto_ducking
                .voice_bundle_ids
                .iter()
                .any(|id| id == "com.hnc.Discord")
        );
        assert!(
            prefs
                .auto_ducking
                .music_bundle_ids
                .iter()
                .any(|id| id == "com.spotify.client")
        );
    }

    #[test]
    fn test_preferences_without_shortcuts_deserializes_with_defaults() {
        let legacy_json = r#"{"theme":"Light"}"#;
        let prefs: Preferences = serde_json::from_str(legacy_json).expect("deserialize legacy");
        assert!(!prefs.shortcuts.toggle_popover.enabled);
        assert_eq!(
            prefs.shortcuts.toggle_popover.accelerator,
            "CmdOrCtrl+Shift+O"
        );
        assert!(!prefs.shortcuts.mute_active_app.enabled);
        assert_eq!(
            prefs.shortcuts.mute_active_app.accelerator,
            "CmdOrCtrl+Shift+M"
        );
        assert!(!prefs.shortcuts.cycle_output_device.enabled);
        assert_eq!(
            prefs.shortcuts.cycle_output_device.accelerator,
            "CmdOrCtrl+Shift+D"
        );
    }
}
