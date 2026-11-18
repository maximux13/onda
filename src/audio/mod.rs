#![allow(dead_code)]
mod app_colors;
mod app_icons;
mod core_audio;
pub mod discovery;
mod dsp;
pub mod master;
pub mod tap;

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::state::{
    AppRowState, AppVolumeConfig, AudioAppInfo, AutoDuckingConfig, DeviceIconKind, InputDeviceInfo,
    MixerSnapshot, OutputDeviceInfo, Preferences, ThemeMode, WindowStyle,
};
use crate::storage::{JsonFileStore, PreferencesStore};
use tap::AppTapHandle;

/// Controls an active per-application audio process tap.
pub trait TapController {
    fn set_volume(&self, volume: f32);
    fn set_muted(&self, muted: bool);
    fn set_eq(&self, enabled: bool, gains_db: &[f32; 10]);
    fn peak_level(&self) -> f32;
    fn output_device_uid(&self) -> &str;
}

impl TapController for AppTapHandle {
    fn set_volume(&self, volume: f32) {
        AppTapHandle::set_volume(self, volume);
    }

    fn set_muted(&self, muted: bool) {
        AppTapHandle::set_muted(self, muted);
    }

    fn set_eq(&self, enabled: bool, gains_db: &[f32; 10]) {
        AppTapHandle::set_eq(self, enabled, gains_db);
    }

    fn peak_level(&self) -> f32 {
        AppTapHandle::peak_level(self)
    }

    fn output_device_uid(&self) -> &str {
        &self.output_device_uid
    }
}

/// Hardware/OS audio backend abstraction used by [`AudioEngine`].
pub trait AudioBackend {
    type Tap: TapController;

    fn list_output_devices(&self) -> Vec<OutputDeviceInfo>;
    fn list_input_devices(&self) -> Vec<InputDeviceInfo>;
    fn default_input_device_name(&self) -> String;
    fn default_output_device_uid(&self) -> Option<String>;
    fn fallback_master_name(&self) -> String;
    fn set_default_output_device(&self, device_id: u32);
    fn set_device_volume(&self, device_id: u32, volume: f32);
    fn set_device_mute(&self, device_id: u32, muted: bool);
    fn set_default_input_device(&self, device_id: u32);
    fn set_input_device_volume(&self, device_id: u32, volume: f32);
    fn set_input_device_mute(&self, device_id: u32, muted: bool);
    fn list_audio_processes(&self, include_inactive: bool) -> Vec<AudioAppInfo>;
    fn create_process_tap(
        &self,
        object_ids: &[u32],
        output_device_uid: &str,
        volume: f32,
        muted: bool,
        eq_enabled: bool,
        eq_gains: &[f32; 10],
    ) -> Result<Self::Tap, String>;
}

/// Production [`AudioBackend`] backed by macOS Core Audio.
#[derive(Debug, Default, Clone, Copy)]
pub struct CoreAudioBackend;

impl AudioBackend for CoreAudioBackend {
    type Tap = AppTapHandle;

    fn list_output_devices(&self) -> Vec<OutputDeviceInfo> {
        master::list_output_devices()
    }

    fn list_input_devices(&self) -> Vec<InputDeviceInfo> {
        master::list_input_devices()
    }

    fn default_input_device_name(&self) -> String {
        master::get_default_input_device_name()
    }

    fn default_output_device_uid(&self) -> Option<String> {
        master::get_default_output_device().and_then(master::get_device_uid)
    }

    fn fallback_master_name(&self) -> String {
        master::get_master_state().device_name
    }

    fn set_default_output_device(&self, device_id: u32) {
        let _ = master::set_default_output_device(device_id);
    }

    fn set_device_volume(&self, device_id: u32, volume: f32) {
        let _ = master::set_device_volume(device_id, volume);
    }

    fn set_device_mute(&self, device_id: u32, muted: bool) {
        let _ = master::set_device_mute(device_id, muted);
    }

    fn set_default_input_device(&self, device_id: u32) {
        let _ = master::set_default_input_device(device_id);
    }

    fn set_input_device_volume(&self, device_id: u32, volume: f32) {
        let _ = master::set_input_device_volume(device_id, volume);
    }

    fn set_input_device_mute(&self, device_id: u32, muted: bool) {
        let _ = master::set_input_device_mute(device_id, muted);
    }

    fn list_audio_processes(&self, include_inactive: bool) -> Vec<AudioAppInfo> {
        discovery::list_audio_processes(include_inactive)
    }

    fn create_process_tap(
        &self,
        object_ids: &[u32],
        output_device_uid: &str,
        volume: f32,
        muted: bool,
        eq_enabled: bool,
        eq_gains: &[f32; 10],
    ) -> Result<Self::Tap, String> {
        tap::create_process_tap(
            object_ids,
            output_device_uid,
            volume,
            muted,
            eq_enabled,
            eq_gains,
        )
        .map_err(|err| err.to_string())
    }
}

struct ActiveTap<T: TapController> {
    handle: T,
    object_ids: Vec<u32>,
}

#[derive(Debug, Default)]
struct DuckingTracker {
    last_voice_active_at: Option<Instant>,
    is_active: bool,
}

impl DuckingTracker {
    fn update(
        &mut self,
        voice_peak_active: bool,
        config: &AutoDuckingConfig,
        now: Instant,
    ) -> bool {
        if !config.enabled {
            self.is_active = false;
            self.last_voice_active_at = None;
            return false;
        }
        if voice_peak_active {
            self.last_voice_active_at = Some(now);
            self.is_active = true;
            return true;
        }
        if let Some(last) = self.last_voice_active_at {
            self.is_active = now.duration_since(last).as_millis() < config.release_ms as u128;
            if !self.is_active {
                self.last_voice_active_at = None;
            }
        } else {
            self.is_active = false;
        }
        self.is_active
    }
}

/// Coordinates Core Audio discovery, output device controls, per-app process taps,
/// real-time 10-band peaking EQ, peak metering, and persisted preferences.
pub struct AudioEngine<B: AudioBackend = CoreAudioBackend, S: PreferencesStore = JsonFileStore> {
    backend: B,
    store: S,
    prefs: Preferences,
    taps: HashMap<String, ActiveTap<B::Tap>>,
    configs: HashMap<String, AppVolumeConfig>,
    last_object_ids: HashMap<String, Vec<u32>>,
    last_output_uid: Option<String>,
    last_device_uids: HashSet<String>,
    selected_eq_app: Option<String>,
    ducking: DuckingTracker,
}

/// Default [`AudioEngine`] specialization wired to Core Audio and JSON file persistence.
pub type DefaultAudioEngine = AudioEngine<CoreAudioBackend, JsonFileStore>;

impl AudioEngine<CoreAudioBackend, JsonFileStore> {
    pub fn new() -> Self {
        Self::with_backend_and_store(CoreAudioBackend, JsonFileStore::default())
    }
}

impl Default for AudioEngine<CoreAudioBackend, JsonFileStore> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: AudioBackend, S: PreferencesStore> AudioEngine<B, S> {
    /// Creates an [`AudioEngine`] with explicit backend and preferences store implementations.
    pub fn with_backend_and_store(backend: B, store: S) -> Self {
        let prefs = store.load();
        let configs = prefs.apps.clone();
        Self {
            backend,
            store,
            prefs,
            taps: HashMap::new(),
            configs,
            last_object_ids: HashMap::new(),
            last_output_uid: None,
            last_device_uids: HashSet::new(),
            selected_eq_app: None,
            ducking: DuckingTracker::default(),
        }
    }

    fn save_prefs(&self) {
        let _ = self.store.save(&self.prefs);
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

    fn is_bundle_ducked(&self, bundle_id: &str) -> bool {
        self.ducking.is_active
            && self
                .prefs
                .auto_ducking
                .music_bundle_ids
                .iter()
                .any(|id| id == bundle_id)
    }

    fn effective_volume(&self, bundle_id: &str, config: &AppVolumeConfig) -> f32 {
        let max_vol = if config.boost_enabled || config.volume > 1.0 {
            self.prefs.max_volume_limit.clamp(1.0, 2.0)
        } else {
            1.0
        };
        let base_volume = config.volume.clamp(0.0, max_vol);
        if self.is_bundle_ducked(bundle_id) {
            base_volume * self.prefs.auto_ducking.duck_amount.clamp(0.05, 1.0)
        } else {
            base_volume
        }
    }

    fn ensure_tap(&mut self, bundle_id: &str, config: &AppVolumeConfig) {
        self.ensure_tap_with_apply(bundle_id, config, true);
    }

    fn ensure_tap_with_apply(
        &mut self,
        bundle_id: &str,
        config: &AppVolumeConfig,
        update_existing: bool,
    ) {
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

        let effective_volume = self.effective_volume(bundle_id, config);
        let (eq_enabled, eq_gains) = self.effective_eq(config);

        let needs_recreate = match self.taps.get(bundle_id) {
            Some(existing)
                if existing.handle.output_device_uid() == uid
                    && existing.object_ids == *object_ids =>
            {
                if update_existing {
                    existing.handle.set_volume(effective_volume);
                    existing.handle.set_muted(config.muted);
                    existing.handle.set_eq(eq_enabled, &eq_gains);
                }
                false
            }
            _ => true,
        };

        if needs_recreate {
            self.taps.remove(bundle_id);
            match self.backend.create_process_tap(
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
        let devices = self.backend.list_output_devices();
        let input_devices = self.backend.list_input_devices();
        let default_input_name = self.backend.default_input_device_name();
        let processes = self.backend.list_audio_processes(self.prefs.show_all_apps);

        let default_dev = devices
            .iter()
            .find(|d| d.is_default)
            .or_else(|| devices.first());

        let default_output_name = default_dev
            .map(|d| d.name.clone())
            .unwrap_or_else(|| self.backend.fallback_master_name());

        self.last_output_uid = default_dev
            .map(|d| d.uid.clone())
            .or_else(|| self.backend.default_output_device_uid());

        self.last_device_uids = devices.iter().map(|d| d.uid.clone()).collect();

        self.last_object_ids.clear();
        for proc in &processes {
            if proc.is_running_output {
                self.last_object_ids
                    .insert(proc.bundle_id.clone(), proc.object_ids.clone());
            }
        }

        let active_bundle_ids: HashSet<&str> = processes
            .iter()
            .filter(|p| p.is_running_output)
            .map(|p| p.bundle_id.as_str())
            .collect();
        self.taps
            .retain(|bundle_id, _| active_bundle_ids.contains(bundle_id.as_str()));

        // Ensure taps exist for active processes before sampling voice peaks for auto-ducking.
        for proc in &processes {
            if proc.is_running_output {
                let config = self
                    .configs
                    .get(&proc.bundle_id)
                    .cloned()
                    .unwrap_or_default();
                self.ensure_tap_with_apply(&proc.bundle_id, &config, false);
            }
        }

        let threshold = self.prefs.auto_ducking.threshold;
        let voice_peak_active = self.taps.iter().any(|(bundle_id, tap)| {
            self.prefs
                .auto_ducking
                .voice_bundle_ids
                .iter()
                .any(|id| id == bundle_id)
                && tap.handle.peak_level() >= threshold
        });
        let ducking_active =
            self.ducking
                .update(voice_peak_active, &self.prefs.auto_ducking, Instant::now());

        let mut apps = Vec::with_capacity(processes.len());
        for proc in processes {
            let config = self
                .configs
                .get(&proc.bundle_id)
                .cloned()
                .unwrap_or_default();

            if proc.is_running_output {
                self.ensure_tap(&proc.bundle_id, &config);
            }

            let peak_level = self
                .taps
                .get(&proc.bundle_id)
                .map(|t| t.handle.peak_level())
                .unwrap_or(0.0);

            let is_ducked = self.is_bundle_ducked(&proc.bundle_id);

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
                is_ducked,
            });
        }

        MixerSnapshot {
            default_output_name,
            default_input_name,
            devices,
            input_devices,
            apps,
            theme: self.prefs.theme,
            window_style: self.prefs.window_style,
            glass_opacity: self.prefs.glass_opacity,
            max_volume_limit: self.prefs.max_volume_limit,
            restore_on_close: self.prefs.restore_on_close,
            show_in_menu_bar: self.prefs.show_in_menu_bar,
            show_all_apps: self.prefs.show_all_apps,
            advanced_mode: self.prefs.advanced_mode,
            eq_collapsed: self.prefs.eq_collapsed,
            global_eq_enabled: self.prefs.global_eq_enabled,
            global_eq_gains: self.prefs.global_eq_gains,
            selected_eq_app: self.selected_eq_app.clone(),
            ducking_active,
            auto_ducking: self.prefs.auto_ducking.clone(),
        }
    }

    pub fn set_device_volume(&mut self, device_id: u32, volume: f32) {
        self.backend.set_device_volume(device_id, volume);
    }

    pub fn set_device_mute(&mut self, device_id: u32, muted: bool) {
        self.backend.set_device_mute(device_id, muted);
    }

    pub fn set_default_input_device(&mut self, device_id: u32) {
        self.backend.set_default_input_device(device_id);
    }

    pub fn set_input_device_volume(&mut self, device_id: u32, volume: f32) {
        self.backend.set_input_device_volume(device_id, volume);
    }

    pub fn set_input_device_mute(&mut self, device_id: u32, muted: bool) {
        self.backend.set_input_device_mute(device_id, muted);
    }

    fn update_app_config(&mut self, bundle_id: &str, update: impl FnOnce(&mut AppVolumeConfig)) {
        let config = self.configs.entry(bundle_id.to_string()).or_default();
        update(config);
        let updated = config.clone();

        self.prefs
            .apps
            .insert(bundle_id.to_string(), updated.clone());
        self.save_prefs();
        self.ensure_tap(bundle_id, &updated);
    }

    pub fn set_default_output_device(&mut self, device_id: u32) {
        self.backend.set_default_output_device(device_id);
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
            self.save_prefs();
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
            self.save_prefs();
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
            self.save_prefs();
            self.refresh_all_taps();
        }
    }

    pub fn set_theme(&mut self, theme: ThemeMode) {
        self.prefs.theme = theme;
        self.save_prefs();
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
        self.save_prefs();
    }

    /// Sets the normalized glass opacity slider value (`0.0..=1.0`) and persists preferences.
    pub fn set_glass_opacity(&mut self, opacity: f32) {
        self.prefs.glass_opacity = opacity.clamp(0.0, 1.0);
        self.save_prefs();
    }

    /// Toggles whether inactive open applications are included in the mixer list.
    pub fn set_show_all_apps(&mut self, show: bool) {
        self.prefs.show_all_apps = show;
        self.save_prefs();
    }

    /// Sets whether Advanced Mode is enabled and persists preferences.
    pub fn set_advanced_mode(&mut self, enabled: bool) {
        self.prefs.advanced_mode = enabled;
        self.save_prefs();
    }

    /// Sets whether the inline Equalizer section is collapsed and persists preferences.
    pub fn set_eq_collapsed(&mut self, collapsed: bool) {
        self.prefs.eq_collapsed = collapsed;
        self.save_prefs();
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
        self.save_prefs();
        self.refresh_all_taps();
    }

    pub fn set_restore_on_close(&mut self, restore: bool) {
        self.prefs.restore_on_close = restore;
        self.save_prefs();
    }

    pub fn set_show_in_menu_bar(&mut self, show: bool) {
        self.prefs.show_in_menu_bar = show;
        self.save_prefs();
    }

    /// Updates auto-ducking configuration, persists preferences, and refreshes active taps.
    pub fn set_auto_ducking_config(&mut self, config: AutoDuckingConfig) {
        self.prefs.auto_ducking = config;
        if !self.prefs.auto_ducking.enabled {
            self.ducking
                .update(false, &self.prefs.auto_ducking, Instant::now());
        }
        self.save_prefs();
        self.refresh_all_taps();
    }

    pub fn reset_saved_app_configs(&mut self) {
        self.configs.clear();
        self.prefs.apps.clear();
        self.save_prefs();
        self.taps.clear();
        self.selected_eq_app = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StorageError;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default, Clone)]
    struct MemoryStore {
        prefs: Rc<RefCell<Preferences>>,
    }

    impl PreferencesStore for MemoryStore {
        fn load(&self) -> Preferences {
            self.prefs.borrow().clone()
        }

        fn save(&self, prefs: &Preferences) -> Result<(), StorageError> {
            *self.prefs.borrow_mut() = prefs.clone();
            Ok(())
        }
    }

    #[derive(Clone, Debug)]
    struct FakeTapState {
        output_device_uid: String,
        volume: f32,
        muted: bool,
        eq_enabled: bool,
        eq_gains: [f32; 10],
        peak_level: f32,
        dropped: bool,
    }

    struct FakeTap {
        state: Rc<RefCell<FakeTapState>>,
        output_device_uid: String,
        object_ids: Vec<u32>,
        object_peaks: Rc<RefCell<HashMap<u32, f32>>>,
    }

    impl FakeTap {
        fn last_volume(&self) -> f32 {
            self.state.borrow().volume
        }
    }

    impl Drop for FakeTap {
        fn drop(&mut self) {
            self.state.borrow_mut().dropped = true;
        }
    }

    impl TapController for FakeTap {
        fn set_volume(&self, volume: f32) {
            self.state.borrow_mut().volume = volume;
        }

        fn set_muted(&self, muted: bool) {
            self.state.borrow_mut().muted = muted;
        }

        fn set_eq(&self, enabled: bool, gains_db: &[f32; 10]) {
            let mut state = self.state.borrow_mut();
            state.eq_enabled = enabled;
            state.eq_gains = *gains_db;
        }

        fn peak_level(&self) -> f32 {
            let peaks = self.object_peaks.borrow();
            self.object_ids
                .iter()
                .filter_map(|id| peaks.get(id).copied())
                .fold(self.state.borrow().peak_level, f32::max)
        }

        fn output_device_uid(&self) -> &str {
            &self.output_device_uid
        }
    }

    #[derive(Clone, Default)]
    struct FakeBackend {
        devices: Rc<RefCell<Vec<OutputDeviceInfo>>>,
        input_devices: Vec<InputDeviceInfo>,
        processes: Rc<RefCell<Vec<AudioAppInfo>>>,
        created_taps: Rc<RefCell<Vec<Rc<RefCell<FakeTapState>>>>>,
        object_peaks: Rc<RefCell<HashMap<u32, f32>>>,
        last_input_volume: Rc<RefCell<Option<(u32, f32)>>>,
        last_input_mute: Rc<RefCell<Option<(u32, bool)>>>,
        last_default_input: Rc<RefCell<Option<u32>>>,
    }

    impl FakeBackend {
        fn with_devices_and_processes(
            devices: Vec<OutputDeviceInfo>,
            processes: Vec<AudioAppInfo>,
        ) -> Self {
            Self {
                devices: Rc::new(RefCell::new(devices)),
                processes: Rc::new(RefCell::new(processes)),
                ..Default::default()
            }
        }

        fn set_fake_peak(&self, bundle_id: &str, peak: f32) {
            let procs = self.processes.borrow();
            let mut peaks = self.object_peaks.borrow_mut();
            for proc in procs.iter().filter(|p| p.bundle_id == bundle_id) {
                for &oid in &proc.object_ids {
                    peaks.insert(oid, peak);
                }
            }
        }

        fn last_input_volume(&self) -> Option<(u32, f32)> {
            *self.last_input_volume.borrow()
        }

        fn last_input_mute(&self) -> Option<(u32, bool)> {
            *self.last_input_mute.borrow()
        }

        fn last_default_input(&self) -> Option<u32> {
            *self.last_default_input.borrow()
        }
    }

    impl AudioBackend for FakeBackend {
        type Tap = FakeTap;

        fn list_output_devices(&self) -> Vec<OutputDeviceInfo> {
            self.devices.borrow().clone()
        }

        fn list_input_devices(&self) -> Vec<InputDeviceInfo> {
            self.input_devices.clone()
        }

        fn default_input_device_name(&self) -> String {
            "Built-in Mic".to_string()
        }

        fn default_output_device_uid(&self) -> Option<String> {
            self.devices
                .borrow()
                .iter()
                .find(|d| d.is_default)
                .map(|d| d.uid.clone())
        }

        fn fallback_master_name(&self) -> String {
            "No Output".to_string()
        }

        fn set_default_output_device(&self, _device_id: u32) {}
        fn set_device_volume(&self, _device_id: u32, _volume: f32) {}
        fn set_device_mute(&self, _device_id: u32, _muted: bool) {}

        fn set_default_input_device(&self, device_id: u32) {
            *self.last_default_input.borrow_mut() = Some(device_id);
        }

        fn set_input_device_volume(&self, device_id: u32, volume: f32) {
            *self.last_input_volume.borrow_mut() = Some((device_id, volume));
        }

        fn set_input_device_mute(&self, device_id: u32, muted: bool) {
            *self.last_input_mute.borrow_mut() = Some((device_id, muted));
        }

        fn list_audio_processes(&self, include_inactive: bool) -> Vec<AudioAppInfo> {
            self.processes
                .borrow()
                .iter()
                .filter(|p| p.is_running_output || include_inactive)
                .cloned()
                .collect()
        }

        fn create_process_tap(
            &self,
            object_ids: &[u32],
            output_device_uid: &str,
            volume: f32,
            muted: bool,
            eq_enabled: bool,
            eq_gains: &[f32; 10],
        ) -> Result<Self::Tap, String> {
            let state = Rc::new(RefCell::new(FakeTapState {
                output_device_uid: output_device_uid.to_string(),
                volume,
                muted,
                eq_enabled,
                eq_gains: *eq_gains,
                peak_level: 0.0,
                dropped: false,
            }));
            self.created_taps.borrow_mut().push(state.clone());
            Ok(FakeTap {
                state,
                output_device_uid: output_device_uid.to_string(),
                object_ids: object_ids.to_vec(),
                object_peaks: self.object_peaks.clone(),
            })
        }
    }

    fn make_device(id: u32, uid: &str, name: &str, is_default: bool) -> OutputDeviceInfo {
        OutputDeviceInfo {
            id,
            uid: uid.to_string(),
            name: name.to_string(),
            subtitle: None,
            icon_kind: DeviceIconKind::Speaker,
            is_default,
            volume: 1.0,
            muted: false,
        }
    }

    fn make_app(bundle_id: &str, name: &str, object_ids: Vec<u32>) -> AudioAppInfo {
        AudioAppInfo {
            object_ids,
            bundle_id: bundle_id.to_string(),
            name: name.to_string(),
            is_running_output: true,
            icon_png: None,
            dominant_color: 0x336699,
        }
    }

    #[test]
    fn snapshot_should_create_tap_for_active_process_and_drop_stale_taps() {
        let backend = FakeBackend::with_devices_and_processes(
            vec![make_device(1, "uid-default", "Speakers", true)],
            vec![make_app("com.spotify.client", "Spotify", vec![10])],
        );
        let store = MemoryStore::default();
        let mut engine = AudioEngine::with_backend_and_store(backend.clone(), store);

        let snap1 = engine.snapshot();
        assert_eq!(snap1.apps.len(), 1);
        assert_eq!(backend.created_taps.borrow().len(), 1);
        assert!(!backend.created_taps.borrow()[0].borrow().dropped);

        backend.processes.borrow_mut().clear();
        let snap2 = engine.snapshot();
        assert!(snap2.apps.is_empty());
        assert!(backend.created_taps.borrow()[0].borrow().dropped);
    }

    #[test]
    fn snapshot_should_include_inactive_apps_without_taps_and_apply_preconfigured_volume() {
        let mut inactive = make_app("com.apple.Safari", "Safari", vec![]);
        inactive.is_running_output = false;

        let backend = FakeBackend::with_devices_and_processes(
            vec![make_device(1, "uid-default", "Speakers", true)],
            vec![
                make_app("com.spotify.client", "Spotify", vec![10]),
                inactive,
            ],
        );
        let store = MemoryStore::default();
        let mut engine = AudioEngine::with_backend_and_store(backend.clone(), store);

        // Default: only active apps are shown.
        let snap_active_only = engine.snapshot();
        assert_eq!(snap_active_only.apps.len(), 1);
        assert_eq!(
            snap_active_only.apps[0].info.bundle_id,
            "com.spotify.client"
        );
        assert!(!snap_active_only.show_all_apps);

        // Enable showing all open apps: Safari appears, but no tap is created for it yet.
        engine.set_show_all_apps(true);
        let snap_all = engine.snapshot();
        assert!(snap_all.show_all_apps);
        assert_eq!(snap_all.apps.len(), 2);
        assert_eq!(backend.created_taps.borrow().len(), 1);

        // Pre-configure Safari's volume while it is idle.
        engine.set_app_volume("com.apple.Safari", 0.45);
        assert_eq!(backend.created_taps.borrow().len(), 1);

        // Once Safari starts playing audio, its tap is created with the pre-configured volume.
        {
            let mut procs = backend.processes.borrow_mut();
            procs[1].is_running_output = true;
            procs[1].object_ids = vec![20];
        }
        let snap_playing = engine.snapshot();
        assert_eq!(snap_playing.apps.len(), 2);
        assert_eq!(backend.created_taps.borrow().len(), 2);
        assert!((backend.created_taps.borrow()[1].borrow().volume - 0.45).abs() < 1e-4);
    }

    #[test]
    fn set_app_volume_should_clamp_and_enable_boost_above_one() {
        let backend = FakeBackend::with_devices_and_processes(
            vec![make_device(1, "uid-default", "Speakers", true)],
            vec![make_app("com.spotify.client", "Spotify", vec![10])],
        );
        let store = MemoryStore::default();
        let mut engine = AudioEngine::with_backend_and_store(backend, store.clone());
        engine.snapshot();

        engine.set_app_volume("com.spotify.client", 1.5);
        let cfg = store
            .load()
            .apps
            .get("com.spotify.client")
            .cloned()
            .unwrap();
        assert_eq!(cfg.volume, 1.5);
        assert!(cfg.boost_enabled);

        engine.set_app_volume("com.spotify.client", 3.5);
        let cfg = store
            .load()
            .apps
            .get("com.spotify.client")
            .cloned()
            .unwrap();
        assert_eq!(cfg.volume, 2.0);
        assert!(cfg.boost_enabled);

        engine.set_app_volume("com.spotify.client", -0.2);
        let cfg = store
            .load()
            .apps
            .get("com.spotify.client")
            .cloned()
            .unwrap();
        assert_eq!(cfg.volume, 0.0);
    }

    #[test]
    fn set_app_boost_false_should_clamp_volume_back_to_one() {
        let backend = FakeBackend::with_devices_and_processes(
            vec![make_device(1, "uid-default", "Speakers", true)],
            vec![make_app("com.spotify.client", "Spotify", vec![10])],
        );
        let store = MemoryStore::default();
        let mut engine = AudioEngine::with_backend_and_store(backend, store.clone());
        engine.snapshot();

        engine.set_app_volume("com.spotify.client", 1.75);
        engine.set_app_boost("com.spotify.client", false);

        let cfg = store
            .load()
            .apps
            .get("com.spotify.client")
            .cloned()
            .unwrap();
        assert!(!cfg.boost_enabled);
        assert_eq!(cfg.volume, 1.0);
    }

    #[test]
    fn effective_eq_should_sum_global_and_app_gains_clamped_to_twelve_db() {
        let backend = FakeBackend::default();
        let store = MemoryStore::default();
        let mut engine = AudioEngine::with_backend_and_store(backend, store);

        engine.select_eq_app(None);
        engine.set_active_eq_enabled(true);
        engine.set_active_eq_band(0, 8.0);
        engine.set_active_eq_band(1, -9.0);

        engine.select_eq_app(Some("com.spotify.client".to_string()));
        engine.set_active_eq_enabled(true);
        engine.set_active_eq_band(0, 7.0);
        engine.set_active_eq_band(1, -6.0);

        let cfg = engine.configs.get("com.spotify.client").unwrap();
        let (enabled, gains) = engine.effective_eq(cfg);
        assert!(enabled);
        assert_eq!(gains[0], 12.0);
        assert_eq!(gains[1], -12.0);
    }

    #[test]
    fn resolve_target_uid_should_fallback_to_default_when_target_device_disconnected() {
        let backend = FakeBackend::with_devices_and_processes(
            vec![
                make_device(1, "uid-default", "Speakers", true),
                make_device(2, "uid-headphones", "Headphones", false),
            ],
            vec![make_app("com.spotify.client", "Spotify", vec![10])],
        );
        let store = MemoryStore::default();
        let mut engine = AudioEngine::with_backend_and_store(backend.clone(), store);

        engine.snapshot();
        engine.set_app_target_device("com.spotify.client", Some("uid-headphones".to_string()));
        let cfg = engine.configs.get("com.spotify.client").unwrap();
        assert_eq!(
            engine.resolve_target_uid(cfg).as_deref(),
            Some("uid-headphones")
        );

        backend
            .devices
            .borrow_mut()
            .retain(|d| d.uid != "uid-headphones");
        engine.snapshot();

        let cfg = engine.configs.get("com.spotify.client").unwrap();
        assert_eq!(
            engine.resolve_target_uid(cfg).as_deref(),
            Some("uid-default")
        );
    }

    #[test]
    fn snapshot_includes_input_devices_and_forwards_input_controls() {
        let mut backend = FakeBackend::default();
        backend.input_devices.push(InputDeviceInfo {
            id: 42,
            uid: "usb-mic-1".to_string(),
            name: "Shure MV7".to_string(),
            is_default: true,
            volume: 0.8,
            muted: false,
        });
        let mut engine = AudioEngine::with_backend_and_store(backend, MemoryStore::default());

        let snap = engine.snapshot();
        assert_eq!(snap.input_devices.len(), 1);
        assert_eq!(snap.input_devices[0].name, "Shure MV7");

        engine.set_input_device_volume(42, 0.45);
        engine.set_input_device_mute(42, true);
        engine.set_default_input_device(42);

        assert_eq!(engine.backend.last_input_volume(), Some((42, 0.45)));
        assert_eq!(engine.backend.last_input_mute(), Some((42, true)));
        assert_eq!(engine.backend.last_default_input(), Some(42));
    }

    #[test]
    fn auto_ducking_lowers_music_tap_volume_without_mutating_config() {
        let backend = FakeBackend::with_devices_and_processes(
            vec![make_device(1, "uid-default", "Speakers", true)],
            vec![
                make_app("com.hnc.Discord", "Discord", vec![10]),
                make_app("com.spotify.client", "Spotify", vec![20]),
            ],
        );
        let mut engine = AudioEngine::with_backend_and_store(backend, MemoryStore::default());
        engine.set_app_volume("com.spotify.client", 0.80);

        let duck_cfg = crate::state::AutoDuckingConfig {
            enabled: true,
            duck_amount: 0.25,
            threshold: 0.05,
            ..Default::default()
        };
        engine.set_auto_ducking_config(duck_cfg);

        // Simulate Discord outputting voice above threshold.
        engine.backend.set_fake_peak("com.hnc.Discord", 0.40);
        let snap = engine.snapshot();
        assert!(snap.ducking_active);

        let spotify_row = snap
            .apps
            .iter()
            .find(|a| a.info.bundle_id == "com.spotify.client")
            .unwrap();
        assert!(spotify_row.is_ducked);
        assert!((spotify_row.config.volume - 0.80).abs() < 1e-5);

        let spotify_tap = engine.taps.get("com.spotify.client").unwrap();
        assert!((spotify_tap.handle.last_volume() - 0.20).abs() < 1e-5);

        let discord_row = snap
            .apps
            .iter()
            .find(|a| a.info.bundle_id == "com.hnc.Discord")
            .unwrap();
        assert!(!discord_row.is_ducked);
    }

    #[test]
    fn auto_ducking_tracker_holds_during_release_window_and_resets_when_disabled() {
        use std::time::Duration;

        let mut tracker = DuckingTracker::default();
        let mut cfg = AutoDuckingConfig {
            enabled: true,
            release_ms: 1200,
            ..Default::default()
        };

        let t0 = Instant::now();
        assert!(!tracker.update(false, &cfg, t0));
        assert!(tracker.update(true, &cfg, t0));

        // Voice stops, still inside release_ms window -> stays active.
        assert!(tracker.update(false, &cfg, t0 + Duration::from_millis(500)));
        assert!(tracker.update(false, &cfg, t0 + Duration::from_millis(1199)));

        // At/after release_ms -> releases.
        assert!(!tracker.update(false, &cfg, t0 + Duration::from_millis(1200)));

        // Re-activate, then disable -> resets immediately.
        assert!(tracker.update(true, &cfg, t0 + Duration::from_millis(1300)));
        cfg.enabled = false;
        assert!(!tracker.update(true, &cfg, t0 + Duration::from_millis(1350)));
        assert!(tracker.last_voice_active_at.is_none());
    }

    #[test]
    fn auto_ducking_updates_volume_when_adjusted_while_ducked_and_restores_on_disable() {
        let backend = FakeBackend::with_devices_and_processes(
            vec![make_device(1, "uid-default", "Speakers", true)],
            vec![
                make_app("com.spotify.client", "Spotify", vec![20]),
                make_app("com.hnc.Discord", "Discord", vec![10]),
            ],
        );
        let mut engine = AudioEngine::with_backend_and_store(backend, MemoryStore::default());
        engine.set_app_volume("com.spotify.client", 0.80);

        let duck_cfg = AutoDuckingConfig {
            enabled: true,
            duck_amount: 0.25,
            threshold: 0.05,
            ..Default::default()
        };
        engine.set_auto_ducking_config(duck_cfg.clone());
        engine.backend.set_fake_peak("com.hnc.Discord", 0.30);

        // Even when Spotify appears before Discord in processes, Spotify is ducked on first snapshot.
        let snap = engine.snapshot();
        assert!(snap.ducking_active);
        assert!((engine.taps["com.spotify.client"].handle.last_volume() - 0.20).abs() < 1e-5);

        // Adjusting Spotify's volume while ducked stores unscaled config and applies scaled tap volume.
        engine.set_app_volume("com.spotify.client", 0.40);
        assert!((engine.configs["com.spotify.client"].volume - 0.40).abs() < 1e-5);
        assert!((engine.taps["com.spotify.client"].handle.last_volume() - 0.10).abs() < 1e-5);

        // Disabling auto-ducking immediately restores full tap volume.
        engine.set_auto_ducking_config(AutoDuckingConfig {
            enabled: false,
            ..duck_cfg
        });
        assert!((engine.taps["com.spotify.client"].handle.last_volume() - 0.40).abs() < 1e-5);
        let snap2 = engine.snapshot();
        assert!(!snap2.ducking_active);
        assert!(!snap2.apps[0].is_ducked);
    }
}
