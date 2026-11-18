#![allow(dead_code)]
mod app_colors;
mod app_icons;
mod core_audio;
pub mod discovery;
mod dsp;
pub mod master;
pub mod tap;

use std::collections::{HashMap, HashSet};

use crate::state::{
    AppRowState, AppVolumeConfig, AudioAppInfo, DeviceIconKind, MixerSnapshot, OutputDeviceInfo,
    Preferences, ThemeMode, WindowStyle,
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
    fn default_input_device_name(&self) -> String;
    fn default_output_device_uid(&self) -> Option<String>;
    fn fallback_master_name(&self) -> String;
    fn set_default_output_device(&self, device_id: u32);
    fn set_device_volume(&self, device_id: u32, volume: f32);
    fn set_device_mute(&self, device_id: u32, muted: bool);
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
                if existing.handle.output_device_uid() == uid
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
            glass_opacity: self.prefs.glass_opacity,
            max_volume_limit: self.prefs.max_volume_limit,
            restore_on_close: self.prefs.restore_on_close,
            show_in_menu_bar: self.prefs.show_in_menu_bar,
            show_all_apps: self.prefs.show_all_apps,
            global_eq_enabled: self.prefs.global_eq_enabled,
            global_eq_gains: self.prefs.global_eq_gains,
            selected_eq_app: self.selected_eq_app.clone(),
        }
    }

    pub fn set_device_volume(&mut self, device_id: u32, volume: f32) {
        self.backend.set_device_volume(device_id, volume);
    }

    pub fn set_device_mute(&mut self, device_id: u32, muted: bool) {
        self.backend.set_device_mute(device_id, muted);
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
            self.state.borrow().peak_level
        }

        fn output_device_uid(&self) -> &str {
            &self.output_device_uid
        }
    }

    #[derive(Clone, Default)]
    struct FakeBackend {
        devices: Rc<RefCell<Vec<OutputDeviceInfo>>>,
        processes: Rc<RefCell<Vec<AudioAppInfo>>>,
        created_taps: Rc<RefCell<Vec<Rc<RefCell<FakeTapState>>>>>,
    }

    impl FakeBackend {
        fn with_devices_and_processes(
            devices: Vec<OutputDeviceInfo>,
            processes: Vec<AudioAppInfo>,
        ) -> Self {
            Self {
                devices: Rc::new(RefCell::new(devices)),
                processes: Rc::new(RefCell::new(processes)),
                created_taps: Rc::new(RefCell::new(Vec::new())),
            }
        }
    }

    impl AudioBackend for FakeBackend {
        type Tap = FakeTap;

        fn list_output_devices(&self) -> Vec<OutputDeviceInfo> {
            self.devices.borrow().clone()
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
            _object_ids: &[u32],
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
                peak_level: 0.25,
                dropped: false,
            }));
            self.created_taps.borrow_mut().push(state.clone());
            Ok(FakeTap {
                state,
                output_device_uid: output_device_uid.to_string(),
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
}
