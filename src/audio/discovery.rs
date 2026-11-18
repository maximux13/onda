use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{self, NonNull};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};
use objc2_core_audio::{
    AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectID,
    kAudioHardwarePropertyProcessObjectList, kAudioProcessPropertyBundleID,
    kAudioProcessPropertyIsRunningOutput, kAudioProcessPropertyPID,
};

use super::app_icons::{get_or_extract_app_icon, resolve_owning_app};
use super::core_audio::{
    get_property_cfstring, get_property_i32, get_property_u32, global_address, system_object_id,
};
use crate::state::AudioAppInfo;

fn get_process_object_ids() -> Vec<AudioObjectID> {
    let address = global_address(kAudioHardwarePropertyProcessObjectList);

    let mut data_size: u32 = 0;
    // SAFETY: `address` and `data_size` are valid stack references for querying the list byte length.
    let status = unsafe {
        AudioObjectGetPropertyDataSize(
            system_object_id(),
            NonNull::from(&address),
            0,
            ptr::null(),
            NonNull::from(&mut data_size),
        )
    };
    if status != 0 || data_size == 0 {
        return Vec::new();
    }

    let count = data_size as usize / size_of::<AudioObjectID>();
    let mut ids = vec![0 as AudioObjectID; count];
    if let Some(out_ptr) = NonNull::new(ids.as_mut_ptr().cast::<c_void>()) {
        // SAFETY: `ids` has capacity for `count` `AudioObjectID` elements matching `data_size`.
        let status = unsafe {
            AudioObjectGetPropertyData(
                system_object_id(),
                NonNull::from(&address),
                0,
                ptr::null(),
                NonNull::from(&mut data_size),
                out_ptr,
            )
        };
        if status != 0 {
            return Vec::new();
        }
        let actual_count = data_size as usize / size_of::<AudioObjectID>();
        ids.truncate(actual_count);
    }
    ids
}

fn fallback_name_from_bundle_id(bundle_id: &str) -> String {
    bundle_id
        .rsplit('.')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(bundle_id)
        .to_string()
}

fn parent_pid(pid: i32) -> Option<i32> {
    // SAFETY: `libc::proc_bsdinfo` is a plain C struct where all-zero initialization is valid.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = size_of::<libc::proc_bsdinfo>() as i32;
    // SAFETY: `info` is a stack buffer of `size` bytes matching `libc::PROC_PIDTBSDINFO`.
    let ret = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast::<c_void>(),
            size,
        )
    };
    if ret == size && info.pbi_ppid > 1 {
        Some(info.pbi_ppid as i32)
    } else {
        None
    }
}

fn sort_audio_apps(apps: &mut [AudioAppInfo]) {
    apps.sort_by(|a, b| {
        b.is_running_output
            .cmp(&a.is_running_output)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
}

const OPEN_APPS_CACHE_TTL: Duration = Duration::from_millis(1500);

const IGNORED_IDLE_BUNDLE_IDS: &[&str] = &[
    "com.apple.finder",
    "com.apple.systempreferences",
    "com.apple.ActivityMonitor",
];

fn is_ignored_idle_bundle_id(bundle_id: &str) -> bool {
    IGNORED_IDLE_BUNDLE_IDS.contains(&bundle_id)
}

type OpenAppsCache = Mutex<Option<(Instant, Vec<AudioAppInfo>)>>;

fn open_gui_apps_cache() -> &'static OpenAppsCache {
    static CACHE: OnceLock<OpenAppsCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn collect_open_gui_apps(self_pid: i32) -> Vec<AudioAppInfo> {
    if let Ok(guard) = open_gui_apps_cache().lock()
        && let Some((captured_at, cached)) = guard.as_ref()
        && captured_at.elapsed() < OPEN_APPS_CACHE_TTL
    {
        return cached.clone();
    }

    let workspace = NSWorkspace::sharedWorkspace();
    let running_apps = workspace.runningApplications();
    let mut apps = Vec::new();

    for app in running_apps {
        if app.isTerminated() {
            continue;
        }
        let pid = app.processIdentifier();
        if pid == self_pid || pid <= 0 {
            continue;
        }
        if app.activationPolicy() != NSApplicationActivationPolicy::Regular {
            continue;
        }
        let Some(bundle_id) = app
            .bundleIdentifier()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        if is_ignored_idle_bundle_id(&bundle_id) {
            continue;
        }

        let name = app
            .localizedName()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| fallback_name_from_bundle_id(&bundle_id));
        let (icon_png, dominant_color) = get_or_extract_app_icon(&bundle_id, &name, pid);

        apps.push(AudioAppInfo {
            object_ids: Vec::new(),
            bundle_id,
            name,
            is_running_output: false,
            icon_png,
            dominant_color,
        });
    }

    if let Ok(mut guard) = open_gui_apps_cache().lock() {
        *guard = Some((Instant::now(), apps.clone()));
    }

    apps
}

fn append_open_gui_apps(grouped: &mut HashMap<String, AudioAppInfo>, self_pid: i32) {
    for app in collect_open_gui_apps(self_pid) {
        grouped.entry(app.bundle_id.clone()).or_insert(app);
    }
}

/// Lists running macOS applications currently outputting audio, grouping helper
/// subprocesses (such as Chromium/Brave audio helpers) under their parent app.
/// When `include_inactive` is `true`, also includes other open GUI applications
/// (`NSApplicationActivationPolicy::Regular`) so their audio settings can be pre-configured.
pub fn list_audio_processes(include_inactive: bool) -> Vec<AudioAppInfo> {
    let self_pid = std::process::id() as i32;
    let object_ids = get_process_object_ids();
    let mut grouped: HashMap<String, AudioAppInfo> = HashMap::new();

    for object_id in object_ids {
        let is_running_output = get_property_u32(
            object_id,
            &global_address(kAudioProcessPropertyIsRunningOutput),
        )
        .map(|v| v != 0)
        .unwrap_or(false);
        if !is_running_output {
            continue;
        }

        let Some(pid) = get_property_i32(object_id, &global_address(kAudioProcessPropertyPID))
        else {
            continue;
        };
        if pid == self_pid {
            continue;
        }

        let raw_bundle_id =
            get_property_cfstring(object_id, &global_address(kAudioProcessPropertyBundleID));
        let Some((owning_pid, bundle_id, localized_name)) =
            resolve_owning_app(pid, raw_bundle_id.as_deref(), parent_pid)
        else {
            continue;
        };

        if let Some(entry) = grouped.get_mut(&bundle_id) {
            entry.object_ids.push(object_id);
            continue;
        }

        let name = localized_name.unwrap_or_else(|| fallback_name_from_bundle_id(&bundle_id));
        let (icon_png, dominant_color) = get_or_extract_app_icon(&bundle_id, &name, owning_pid);

        grouped.insert(
            bundle_id.clone(),
            AudioAppInfo {
                object_ids: vec![object_id],
                bundle_id,
                name,
                is_running_output: true,
                icon_png,
                dominant_color,
            },
        );
    }

    if include_inactive {
        append_open_gui_apps(&mut grouped, self_pid);
    }

    let mut apps: Vec<AudioAppInfo> = grouped.into_values().collect();
    sort_audio_apps(&mut apps);

    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, is_running_output: bool) -> AudioAppInfo {
        AudioAppInfo {
            object_ids: if is_running_output { vec![1] } else { vec![] },
            bundle_id: format!("com.example.{name}"),
            name: name.to_string(),
            is_running_output,
            icon_png: None,
            dominant_color: 0,
        }
    }

    #[test]
    fn sort_audio_apps_should_place_active_apps_before_inactive_alphabetically() {
        let mut apps = vec![
            app("Zoom", false),
            app("Spotify", true),
            app("Finder", false),
            app("Arc", true),
        ];

        sort_audio_apps(&mut apps);

        let ordered: Vec<(&str, bool)> = apps
            .iter()
            .map(|a| (a.name.as_str(), a.is_running_output))
            .collect();
        assert_eq!(
            ordered,
            vec![
                ("Arc", true),
                ("Spotify", true),
                ("Finder", false),
                ("Zoom", false),
            ]
        );
    }

    #[test]
    fn ignored_idle_bundle_ids_filters_system_utilities() {
        assert!(is_ignored_idle_bundle_id("com.apple.finder"));
        assert!(is_ignored_idle_bundle_id("com.apple.systempreferences"));
        assert!(!is_ignored_idle_bundle_id("com.spotify.client"));
    }
}
