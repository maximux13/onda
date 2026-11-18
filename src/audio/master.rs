#![allow(dead_code)]
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{self, NonNull};

use objc2_core_audio::{
    AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectHasProperty,
    AudioObjectID, AudioObjectIsPropertySettable, AudioObjectPropertyAddress,
    AudioObjectPropertyScope, AudioObjectPropertySelector, AudioObjectSetPropertyData,
    kAudioDevicePropertyDeviceNameCFString, kAudioDevicePropertyDeviceUID,
    kAudioDevicePropertyMute, kAudioDevicePropertyStreamConfiguration,
    kAudioDevicePropertyVolumeScalar, kAudioHardwarePropertyDefaultInputDevice,
    kAudioHardwarePropertyDefaultOutputDevice, kAudioHardwarePropertyDevices,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeInput,
    kAudioObjectPropertyScopeOutput,
};
use objc2_core_audio_types::{AudioBuffer, AudioBufferList};

use super::core_audio::{
    get_property_cfstring, get_property_f32, get_property_u32, global_address, system_object_id,
};
use crate::state::{DeviceIconKind, InputDeviceInfo, MasterState, OutputDeviceInfo};

/// `kAudioHardwareServiceDeviceProperty_VirtualMainVolume` (`'vmvc'`) from AudioToolbox.
const K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME: AudioObjectPropertySelector =
    0x766d7663;

fn has_settable_property(object_id: AudioObjectID, address: &AudioObjectPropertyAddress) -> bool {
    // SAFETY: `address` is a valid reference to an initialized `AudioObjectPropertyAddress`.
    if !unsafe { AudioObjectHasProperty(object_id, NonNull::from(address)) } {
        return false;
    }
    let mut settable = 0u8;
    // SAFETY: `address` and `settable` are valid, aligned, and non-null for the duration of the call.
    let status = unsafe {
        AudioObjectIsPropertySettable(
            object_id,
            NonNull::from(address),
            NonNull::from(&mut settable),
        )
    };
    status == 0 && settable != 0
}

fn set_f32_property(
    object_id: AudioObjectID,
    address: &AudioObjectPropertyAddress,
    value: f32,
) -> Result<(), String> {
    // SAFETY: `address` and `value` are valid, and `size_of::<f32>()` matches the pointed-to `f32`.
    let status = unsafe {
        AudioObjectSetPropertyData(
            object_id,
            NonNull::from(address),
            0,
            ptr::null(),
            size_of::<f32>() as u32,
            NonNull::from(&value).cast::<c_void>(),
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(format!(
            "AudioObjectSetPropertyData failed with OSStatus {status}"
        ))
    }
}

fn set_u32_property(
    object_id: AudioObjectID,
    address: &AudioObjectPropertyAddress,
    value: u32,
) -> Result<(), String> {
    // SAFETY: `address` and `value` are valid, and `size_of::<u32>()` matches the pointed-to `u32`.
    let status = unsafe {
        AudioObjectSetPropertyData(
            object_id,
            NonNull::from(address),
            0,
            ptr::null(),
            size_of::<u32>() as u32,
            NonNull::from(&value).cast::<c_void>(),
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(format!(
            "AudioObjectSetPropertyData failed with OSStatus {status}"
        ))
    }
}

fn get_device_name(device_id: u32) -> Option<String> {
    let name_addr = global_address(kAudioDevicePropertyDeviceNameCFString);
    get_property_cfstring(device_id, &name_addr).filter(|s| !s.is_empty())
}

fn scoped_address(
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
    element: u32,
) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: element,
    }
}

fn get_scoped_device_volume(device_id: u32, scope: AudioObjectPropertyScope) -> f32 {
    let virtual_vol_addr = scoped_address(
        K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME,
        scope,
        kAudioObjectPropertyElementMain,
    );
    let ch1_vol_addr = scoped_address(kAudioDevicePropertyVolumeScalar, scope, 1);
    let ch0_vol_addr = scoped_address(
        kAudioDevicePropertyVolumeScalar,
        scope,
        kAudioObjectPropertyElementMain,
    );
    get_property_f32(device_id, &virtual_vol_addr)
        .or_else(|| get_property_f32(device_id, &ch1_vol_addr))
        .or_else(|| get_property_f32(device_id, &ch0_vol_addr))
        .unwrap_or(1.0)
        .clamp(0.0, 1.0)
}

fn get_scoped_device_mute(device_id: u32, scope: AudioObjectPropertyScope) -> bool {
    let mute_addr = scoped_address(
        kAudioDevicePropertyMute,
        scope,
        kAudioObjectPropertyElementMain,
    );
    get_property_u32(device_id, &mute_addr)
        .map(|v| v != 0)
        .unwrap_or(false)
}

fn set_scoped_device_mute(
    device_id: u32,
    scope: AudioObjectPropertyScope,
    muted: bool,
) -> Result<(), String> {
    let mute_addr = scoped_address(
        kAudioDevicePropertyMute,
        scope,
        kAudioObjectPropertyElementMain,
    );
    set_u32_property(device_id, &mute_addr, u32::from(muted))
}

pub fn get_device_volume(device_id: u32) -> f32 {
    get_scoped_device_volume(device_id, kAudioObjectPropertyScopeOutput)
}

pub fn get_device_mute(device_id: u32) -> bool {
    get_scoped_device_mute(device_id, kAudioObjectPropertyScopeOutput)
}

pub fn get_input_device_volume(device_id: u32) -> f32 {
    get_scoped_device_volume(device_id, kAudioObjectPropertyScopeInput)
}

pub fn get_input_device_mute(device_id: u32) -> bool {
    get_scoped_device_mute(device_id, kAudioObjectPropertyScopeInput)
}

fn device_channel_count(device_id: u32, scope: AudioObjectPropertyScope) -> u32 {
    let address = scoped_address(
        kAudioDevicePropertyStreamConfiguration,
        scope,
        kAudioObjectPropertyElementMain,
    );
    let mut data_size: u32 = 0;
    // SAFETY: `address` and `data_size` are valid, aligned, and non-null for the duration of the call.
    let status = unsafe {
        AudioObjectGetPropertyDataSize(
            device_id,
            NonNull::from(&address),
            0,
            ptr::null(),
            NonNull::from(&mut data_size),
        )
    };
    if status != 0 || (data_size as usize) < size_of::<u32>() {
        return 0;
    }

    let words = (data_size as usize).div_ceil(size_of::<u64>());
    let mut storage = vec![0u64; words];
    let Some(out_ptr) = NonNull::new(storage.as_mut_ptr().cast::<c_void>()) else {
        return 0;
    };
    // SAFETY: `storage` is `u64`-aligned and sized to at least `data_size` bytes.
    let status = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            NonNull::from(&address),
            0,
            ptr::null(),
            NonNull::from(&mut data_size),
            out_ptr,
        )
    };
    if status != 0 || (data_size as usize) < size_of::<AudioBufferList>() {
        return 0;
    }

    // SAFETY: `storage` is `u64`-aligned (>= `AudioBufferList` alignment) and holds at least
    // `size_of::<AudioBufferList>()` bytes populated by `AudioObjectGetPropertyData`.
    let abl = unsafe { &*(storage.as_ptr() as *const AudioBufferList) };
    let num_buffers = abl.mNumberBuffers as usize;
    if num_buffers == 0 {
        return 0;
    }

    let max_buffers =
        1 + (data_size as usize - size_of::<AudioBufferList>()) / size_of::<AudioBuffer>();
    let count = num_buffers.min(max_buffers);
    // SAFETY: `count` is bounded by `max_buffers`, so all `count` `AudioBuffer` elements
    // lie within the `data_size` bytes written into `storage`.
    let buffers = unsafe { std::slice::from_raw_parts(abl.mBuffers.as_ptr(), count) };
    buffers.iter().map(|b| b.mNumberChannels).sum()
}

fn infer_device_icon(name: &str, uid: &str) -> DeviceIconKind {
    let haystack = format!("{} {}", name.to_lowercase(), uid.to_lowercase());
    if haystack.contains("airpods") {
        DeviceIconKind::AirPods
    } else if haystack.contains("headphone")
        || haystack.contains("wh-")
        || haystack.contains("buds")
        || haystack.contains("beats")
    {
        DeviceIconKind::Headphones
    } else if haystack.contains("blackhole")
        || haystack.contains("soundflower")
        || haystack.contains("virtual")
        || haystack.contains("multi")
    {
        DeviceIconKind::Virtual
    } else if haystack.contains("hdmi")
        || haystack.contains("display")
        || haystack.contains("monitor")
        || haystack.contains("lg")
        || haystack.contains("dell")
        || haystack.contains("kg")
    {
        DeviceIconKind::Display
    } else {
        DeviceIconKind::Speaker
    }
}

/// Returns the `AudioObjectID` of the current default system output device.
pub fn get_default_output_device() -> Option<u32> {
    let address = global_address(kAudioHardwarePropertyDefaultOutputDevice);
    get_property_u32(system_object_id(), &address).filter(|&id| id != 0)
}

/// Returns the `AudioObjectID` of the current default system input device.
pub fn get_default_input_device() -> Option<u32> {
    let address = global_address(kAudioHardwarePropertyDefaultInputDevice);
    get_property_u32(system_object_id(), &address).filter(|&id| id != 0)
}

/// Returns the display name of the current default system input device.
pub fn get_default_input_device_name() -> String {
    get_default_input_device()
        .and_then(get_device_name)
        .unwrap_or_else(|| String::from("Microphone"))
}

/// Returns the persistent UID string for the given CoreAudio device ID.
pub fn get_device_uid(device_id: u32) -> Option<String> {
    let address = global_address(kAudioDevicePropertyDeviceUID);
    get_property_cfstring(device_id, &address)
}

fn list_hardware_device_ids() -> Vec<AudioObjectID> {
    let address = global_address(kAudioHardwarePropertyDevices);

    let mut data_size: u32 = 0;
    // SAFETY: `address` and `data_size` are valid, aligned, and non-null for the duration of the call.
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
    let mut device_ids = vec![0 as AudioObjectID; count];
    if let Some(out_ptr) = NonNull::new(device_ids.as_mut_ptr().cast::<c_void>()) {
        // SAFETY: `device_ids` has capacity for `count` `AudioObjectID`s (`data_size` bytes).
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
        device_ids.truncate(actual_count);
    }

    device_ids
}

/// Lists all active output audio devices on the system (excluding internal process tap aggregates).
pub fn list_output_devices() -> Vec<OutputDeviceInfo> {
    let default_output_id = get_default_output_device();
    let mut devices = Vec::new();

    for id in list_hardware_device_ids() {
        if device_channel_count(id, kAudioObjectPropertyScopeOutput) == 0 {
            continue;
        }
        let Some(name) = get_device_name(id) else {
            continue;
        };
        if name.starts_with("VolumeMixer-Tap-") {
            continue;
        }
        let Some(uid) = get_device_uid(id) else {
            continue;
        };

        let icon_kind = infer_device_icon(&name, &uid);
        let is_default = default_output_id == Some(id);
        let volume = get_device_volume(id);
        let muted = get_device_mute(id);

        devices.push(OutputDeviceInfo {
            id,
            uid,
            name,
            subtitle: None,
            icon_kind,
            is_default,
            volume,
            muted,
        });
    }

    devices.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });

    devices
}

/// Lists all active input audio devices (microphones) on the system (excluding internal process tap aggregates).
pub fn list_input_devices() -> Vec<InputDeviceInfo> {
    let default_input_id = get_default_input_device();
    let mut devices = Vec::new();

    for id in list_hardware_device_ids() {
        if device_channel_count(id, kAudioObjectPropertyScopeInput) == 0 {
            continue;
        }
        let Some(name) = get_device_name(id) else {
            continue;
        };
        if name.starts_with("VolumeMixer-Tap-") {
            continue;
        }
        let Some(uid) = get_device_uid(id) else {
            continue;
        };

        devices.push(InputDeviceInfo {
            id,
            uid,
            name,
            is_default: default_input_id == Some(id),
            volume: get_input_device_volume(id),
            muted: get_input_device_mute(id),
        });
    }

    devices.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });

    devices
}

/// Sets the volume of a specific output device, clamped to `0.0..=1.0`.
pub fn set_device_volume(device_id: u32, volume: f32) -> Result<(), String> {
    let clamped = volume.clamp(0.0, 1.0);

    let virtual_vol_addr = scoped_address(
        K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME,
        kAudioObjectPropertyScopeOutput,
        kAudioObjectPropertyElementMain,
    );
    if has_settable_property(device_id, &virtual_vol_addr) {
        return set_f32_property(device_id, &virtual_vol_addr, clamped);
    }

    let mut set_any = false;
    let mut last_err = None;
    for channel in [1u32, 2u32] {
        let ch_addr = scoped_address(
            kAudioDevicePropertyVolumeScalar,
            kAudioObjectPropertyScopeOutput,
            channel,
        );
        if has_settable_property(device_id, &ch_addr) {
            match set_f32_property(device_id, &ch_addr, clamped) {
                Ok(()) => set_any = true,
                Err(err) => last_err = Some(err),
            }
        }
    }

    if set_any {
        Ok(())
    } else {
        Err(last_err
            .unwrap_or_else(|| String::from("Output device has no settable volume property")))
    }
}

/// Sets the mute state of a specific output device.
pub fn set_device_mute(device_id: u32, muted: bool) -> Result<(), String> {
    set_scoped_device_mute(device_id, kAudioObjectPropertyScopeOutput, muted)
}

/// Sets the system default output device.
pub fn set_default_output_device(device_id: u32) -> Result<(), String> {
    let address = global_address(kAudioHardwarePropertyDefaultOutputDevice);
    set_u32_property(system_object_id(), &address, device_id)
}

/// Sets the input gain/volume of a specific input device, clamped to `0.0..=1.0`.
pub fn set_input_device_volume(device_id: u32, volume: f32) -> Result<(), String> {
    let clamped = volume.clamp(0.0, 1.0);
    let virtual_vol_addr = scoped_address(
        K_AUDIO_HARDWARE_SERVICE_DEVICE_PROPERTY_VIRTUAL_MAIN_VOLUME,
        kAudioObjectPropertyScopeInput,
        kAudioObjectPropertyElementMain,
    );
    if has_settable_property(device_id, &virtual_vol_addr) {
        return set_f32_property(device_id, &virtual_vol_addr, clamped);
    }
    let ch0_addr = scoped_address(
        kAudioDevicePropertyVolumeScalar,
        kAudioObjectPropertyScopeInput,
        0,
    );
    if has_settable_property(device_id, &ch0_addr) {
        return set_f32_property(device_id, &ch0_addr, clamped);
    }
    let mut set_any = false;
    let mut last_err = None;
    for channel in [1u32, 2u32] {
        let ch_addr = scoped_address(
            kAudioDevicePropertyVolumeScalar,
            kAudioObjectPropertyScopeInput,
            channel,
        );
        if has_settable_property(device_id, &ch_addr) {
            match set_f32_property(device_id, &ch_addr, clamped) {
                Ok(()) => set_any = true,
                Err(err) => last_err = Some(err),
            }
        }
    }
    if set_any {
        Ok(())
    } else {
        Err(last_err
            .unwrap_or_else(|| String::from("Input device has no settable volume property")))
    }
}

/// Sets the mute state of a specific input device.
pub fn set_input_device_mute(device_id: u32, muted: bool) -> Result<(), String> {
    set_scoped_device_mute(device_id, kAudioObjectPropertyScopeInput, muted)
}

/// Sets the system default input device.
pub fn set_default_input_device(device_id: u32) -> Result<(), String> {
    let address = global_address(kAudioHardwarePropertyDefaultInputDevice);
    set_u32_property(system_object_id(), &address, device_id)
}

/// Reads the current default output device's name, volume, and mute state.
pub fn get_master_state() -> MasterState {
    let Some(device_id) = get_default_output_device() else {
        return MasterState::default();
    };

    let device_name =
        get_device_name(device_id).unwrap_or_else(|| MasterState::default().device_name);
    let volume = get_device_volume(device_id);
    let muted = get_device_mute(device_id);

    MasterState {
        device_name,
        volume,
        muted,
    }
}

/// Sets the default output device's master volume, clamped to `0.0..=1.0`.
pub fn set_master_volume(volume: f32) -> Result<(), String> {
    let device_id =
        get_default_output_device().ok_or_else(|| String::from("No default output device"))?;
    set_device_volume(device_id, volume)
}

/// Sets the default output device's mute state.
pub fn set_master_mute(muted: bool) -> Result<(), String> {
    let device_id =
        get_default_output_device().ok_or_else(|| String::from("No default output device"))?;
    set_device_mute(device_id, muted)
}
