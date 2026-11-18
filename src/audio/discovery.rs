use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{self, NonNull};

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

/// Lists running macOS applications currently outputting audio, grouping helper
/// subprocesses (such as Chromium/Brave audio helpers) under their parent app.
pub fn list_audio_processes() -> Vec<AudioAppInfo> {
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

    let mut apps: Vec<AudioAppInfo> = grouped.into_values().collect();
    apps.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });

    apps
}
