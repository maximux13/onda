#![allow(dead_code)]
mod app_colors;
mod app_icons;
mod core_audio;
pub mod discovery;
pub mod master;
pub mod tap;

use std::collections::{HashMap, HashSet};

use crate::state::{
    AppRowState, AppVolumeConfig, DeviceIconKind, MixerSnapshot, Preferences, ThemeMode,
    WindowStyle,
};
use tap::AppTapHandle;

struct ActiveTap {
    handle: AppTapHandle,
    object_ids: Vec<u32>,
}

/// Coordinates Core Audio discovery, output device controls, per-app process taps,
/// real-time 10-band peaking EQ, peak metering, and persisted preferences.
pub struct AudioEngine {
    prefs: Preferences,
    taps: HashMap<String, ActiveTap>,
    configs: HashMap<String, AppVolumeConfig>,
    last_object_ids: HashMap<String, Vec<u32>>,
    last_output_uid: Option<String>,
    last_device_uids: HashSet<String>,
    selected_eq_app: Option<String>,
}

impl AudioEngine {
    pub fn new() -> Self {
        let prefs = Preferences::load();
        let configs = prefs.apps.clone();
        Self {
            prefs,
            taps: HashMap::new(),
            configs,
            last_object_ids: HashMap::new(),
            last_output_uid: None,
            last_device_uids: HashSet::new(),
            selected_eq_app: None,
        }
    }

    fn resolve_target_uid(&self, config: &AppVolumeConfig) -> Option<String> {
        if let Some(ref target_uid) = config.target_device_uid
            && self.last_device_uids.contains(target_uid)
        {
            return Some(target_uid.clone());
        }
        self.last_output_uid.clone()
    }

    fn effective_eq(&self, config: &AppVolumeConfig) -> (bool, [f32; 10]) {
        let effective_enabled = self.prefs.global_eq_enabled || config.eq_enabled;
        let mut effective_gain = [0.0f32; 10];
        for i in 0..10 {
            effective_gain[i] = ((if self.prefs.global_eq_enabled {
                self.prefs.global_eq_gains[i]
            } else {
                0.0
            }) + (if config.eq_enabled {
                config.eq_gains[i]
            } else {
                0.0
            }))
            .clamp(-12.0, 12.0);
        }
        (effective_enabled, effective_gain)
    }

    fn ensure_tap(&mut self, bundle_id: &str, config: &AppVolumeConfig) {
        let Some(uid) = self.resolve_target_uid(config) else {
            self.taps.remove(bundle_id);
            return;
        };
        let Some(object_ids) = self.last_object_ids.get(bundle_id) else {
            return;
        };
        if object_ids.is_empty() {
            return;
        }

        let max_vol = if config.boost_enabled || config.volume > 1.0 {
            self.prefs.max_volume_limit.clamp(1.0, 2.0)
        } else {
            1.0
        };
        let effective_volume = config.volume.clamp(0.0, max_vol);
        let (eq_enabled, eq_gains) = self.effective_eq(config);

        let needs_recreate = match self.taps.get(bundle_id) {
            Some(existing)
                if existing.handle.output_device_uid == uid
                    && existing.object_ids == *object_ids =>
            {
                existing.handle.set_volume(effective_volume);
                existing.handle.set_muted(config.muted);
                existing.handle.set_eq(eq_enabled, &eq_gains);
                false
            }
            _ => true,
        };

        if needs_recreate {
            self.taps.remove(bundle_id);
            match tap::create_process_tap(
                object_ids,
                &uid,
                effective_volume,
                config.muted,
                eq_enabled,
                &eq_gains,
            ) {
                Ok(handle) => {
                    self.taps.insert(
                        bundle_id.to_string(),
                        ActiveTap {
                            handle,
                            object_ids: object_ids.clone(),
                        },
                    );
                }
                Err(err) => {
                    eprintln!("Failed to create process tap for {bundle_id}: {err}");
                }
            }
        }
    }

    fn refresh_all_taps(&mut self) {
        let bundle_ids: Vec<String> = self.taps.keys().cloned().collect();
        for bundle_id in bundle_ids {
            let config = self.configs.get(&bundle_id).cloned().unwrap_or_default();
            self.ensure_tap(&bundle_id, &config);
        }
    }

    /// Fetches the latest output devices, input device name, and active audio processes,
    /// reconciles per-app process taps, and returns a full [`MixerSnapshot`] for the UI.
    pub fn snapshot(&mut self) -> MixerSnapshot {
        let devices = master::list_output_devices();
        let default_input_name = master::get_default_input_device_name();
        let processes = discovery::list_audio_processes();

        let default_dev = devices
            .iter()
            .find(|d| d.is_default)
            .or_else(|| devices.first());

        let default_output_name = default_dev
            .map(|d| d.name.clone())
            .unwrap_or_else(|| master::get_master_state().device_name);

        self.last_output_uid = default_dev
            .map(|d| d.uid.clone())
            .or_else(|| master::get_default_output_device().and_then(master::get_device_uid));

        self.last_device_uids = devices.iter().map(|d| d.uid.clone()).collect();

        self.last_object_ids.clear();
        for proc in &processes {
            self.last_object_ids
                .insert(proc.bundle_id.clone(), proc.object_ids.clone());
        }

        let active_bundle_ids: HashSet<&str> =
            processes.iter().map(|p| p.bundle_id.as_str()).collect();
        self.taps
            .retain(|bundle_id, _| active_bundle_ids.contains(bundle_id.as_str()));

        let mut apps = Vec::with_capacity(processes.len());
        for proc in processes {
            let config = self
                .configs
                .get(&proc.bundle_id)
                .cloned()
                .unwrap_or_default();

            self.ensure_tap(&proc.bundle_id, &config);

            let peak_level = self
                .taps
                .get(&proc.bundle_id)
                .map(|t| t.handle.peak_level())
                .unwrap_or(0.0);

            let routed_dev = config
                .target_device_uid
                .as_deref()
                .and_then(|uid| devices.iter().find(|d| d.uid == uid))
                .or(default_dev);

            let routed_device_name = routed_dev
                .map(|d| d.name.clone())
                .unwrap_or_else(|| default_output_name.clone());

            let routed_device_icon = routed_dev
                .map(|d| d.icon_kind)
                .unwrap_or(DeviceIconKind::Speaker);

            apps.push(AppRowState {
                info: proc,
                config,
                peak_level,
                routed_device_name,
                routed_device_icon,
            });
        }

        MixerSnapshot {
            default_output_name,
            default_input_name,
            devices,
            apps,
            theme: self.prefs.theme,
            window_style: self.prefs.window_style,
            max_volume_limit: self.prefs.max_volume_limit,
            restore_on_close: self.prefs.restore_on_close,
            show_in_menu_bar: self.prefs.show_in_menu_bar,
            global_eq_enabled: self.prefs.global_eq_enabled,
            global_eq_gains: self.prefs.global_eq_gains,
            selected_eq_app: self.selected_eq_app.clone(),
        }
    }

    pub fn set_device_volume(&mut self, device_id: u32, volume: f32) {
        let _ = master::set_device_volume(device_id, volume);
    }

    pub fn set_device_mute(&mut self, device_id: u32, muted: bool) {
        let _ = master::set_device_mute(device_id, muted);
    }

    fn update_app_config(&mut self, bundle_id: &str, update: impl FnOnce(&mut AppVolumeConfig)) {
        let config = self.configs.entry(bundle_id.to_string()).or_default();
        update(config);
        let updated = config.clone();

        self.prefs
            .apps
            .insert(bundle_id.to_string(), updated.clone());
        self.prefs.save();
        self.ensure_tap(bundle_id, &updated);
    }

    pub fn set_default_output_device(&mut self, device_id: u32) {
        let _ = master::set_default_output_device(device_id);
    }

    pub fn set_app_volume(&mut self, bundle_id: &str, volume: f32) {
        let max_limit = self.prefs.max_volume_limit.clamp(1.0, 2.0);
        self.update_app_config(bundle_id, |config| {
            config.volume = volume.clamp(0.0, max_limit);
            if config.volume > 1.0 {
                config.boost_enabled = true;
            }
        });
    }

    pub fn set_app_mute(&mut self, bundle_id: &str, muted: bool) {
        self.update_app_config(bundle_id, |config| {
            config.muted = muted;
        });
    }

    pub fn set_app_boost(&mut self, bundle_id: &str, boost_enabled: bool) {
        self.update_app_config(bundle_id, |config| {
            config.boost_enabled = boost_enabled;
            if !boost_enabled && config.volume > 1.0 {
                config.volume = 1.0;
            }
        });
    }

    pub fn set_app_target_device(&mut self, bundle_id: &str, target_device_uid: Option<String>) {
        self.update_app_config(bundle_id, |config| {
            config.target_device_uid = target_device_uid;
        });
    }

    pub fn select_eq_app(&mut self, bundle_id: Option<String>) {
        self.selected_eq_app = bundle_id;
    }

    pub fn set_active_eq_enabled(&mut self, enabled: bool) {
        if let Some(bundle_id) = self.selected_eq_app.clone() {
            self.update_app_config(&bundle_id, |config| {
                config.eq_enabled = enabled;
            });
        } else {
            self.prefs.global_eq_enabled = enabled;
            self.prefs.save();
            self.refresh_all_taps();
        }
    }

    pub fn set_active_eq_band(&mut self, band_idx: usize, gain_db: f32) {
        if band_idx >= 10 {
            return;
        }
        let clamped = gain_db.clamp(-12.0, 12.0);
        if let Some(bundle_id) = self.selected_eq_app.clone() {
            self.update_app_config(&bundle_id, |config| {
                config.eq_gains[band_idx] = clamped;
            });
        } else {
            self.prefs.global_eq_gains[band_idx] = clamped;
            self.prefs.save();
            self.refresh_all_taps();
        }
    }

    pub fn reset_active_eq(&mut self) {
        if let Some(bundle_id) = self.selected_eq_app.clone() {
            self.update_app_config(&bundle_id, |config| {
                config.eq_gains = [0.0; 10];
            });
        } else {
            self.prefs.global_eq_gains = [0.0; 10];
            self.prefs.save();
            self.refresh_all_taps();
        }
    }

    pub fn set_theme(&mut self, theme: ThemeMode) {
        self.prefs.theme = theme;
        self.prefs.save();
    }

    pub fn toggle_theme(&mut self) -> ThemeMode {
        let next = match self.prefs.theme {
            ThemeMode::Auto | ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
        };
        self.set_theme(next);
        next
    }

    pub fn set_window_style(&mut self, style: WindowStyle) {
        self.prefs.window_style = style;
        self.prefs.save();
    }

    pub fn set_max_volume_limit(&mut self, limit: f32) {
        let clamped_limit = limit.clamp(1.0, 2.0);
        self.prefs.max_volume_limit = clamped_limit;
        for config in self.configs.values_mut() {
            if config.volume > clamped_limit {
                config.volume = clamped_limit;
            }
            if clamped_limit <= 1.0 {
                config.boost_enabled = false;
            }
        }
        for config in self.prefs.apps.values_mut() {
            if config.volume > clamped_limit {
                config.volume = clamped_limit;
            }
            if clamped_limit <= 1.0 {
                config.boost_enabled = false;
            }
        }
        self.prefs.save();
        self.refresh_all_taps();
    }

    pub fn set_restore_on_close(&mut self, restore: bool) {
        self.prefs.restore_on_close = restore;
        self.prefs.save();
    }

    pub fn set_show_in_menu_bar(&mut self, show: bool) {
        self.prefs.show_in_menu_bar = show;
        self.prefs.save();
    }

    pub fn reset_saved_app_configs(&mut self) {
        self.configs.clear();
        self.prefs.apps.clear();
        self.prefs.save();
        self.taps.clear();
        self.selected_eq_app = None;
    }

    pub fn set_master_volume(&mut self, volume: f32) {
        let _ = master::set_master_volume(volume);
    }

    pub fn set_master_mute(&mut self, muted: bool) {
        let _ = master::set_master_mute(muted);
    }
}
