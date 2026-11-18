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
}

/// Full snapshot of mixer state passed from the audio engine to the UI.
#[derive(Clone, Debug)]
pub struct MixerSnapshot {
    pub default_output_name: String,
    pub default_input_name: String,
    pub devices: Vec<OutputDeviceInfo>,
    pub apps: Vec<AppRowState>,
    pub theme: ThemeMode,
    pub window_style: WindowStyle,
    pub glass_opacity: f32,
    pub max_volume_limit: f32,
    pub restore_on_close: bool,
    pub show_in_menu_bar: bool,
    pub show_all_apps: bool,
    pub global_eq_enabled: bool,
    pub global_eq_gains: [f32; 10],
    /// Selected app `bundle_id` for the equalizer (`None` = Global Equalizer).
    pub selected_eq_app: Option<String>,
}

impl Default for MixerSnapshot {
    fn default() -> Self {
        Self {
            default_output_name: String::from("MacBook Pro Speakers"),
            default_input_name: String::from("MacBook Pro Microphone"),
            devices: Vec::new(),
            apps: Vec::new(),
            theme: ThemeMode::Auto,
            window_style: WindowStyle::Transparent,
            glass_opacity: DEFAULT_GLASS_OPACITY,
            max_volume_limit: 2.0,
            restore_on_close: false,
            show_in_menu_bar: true,
            show_all_apps: false,
            global_eq_enabled: true,
            global_eq_gains: [0.0; 10],
            selected_eq_app: None,
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
    #[serde(default = "default_true")]
    pub global_eq_enabled: bool,
    #[serde(default = "default_eq_gains")]
    pub global_eq_gains: [f32; 10],
    #[serde(default)]
    pub apps: HashMap<String, AppVolumeConfig>,
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
            global_eq_enabled: true,
            global_eq_gains: [0.0; 10],
            apps: HashMap::new(),
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
}
