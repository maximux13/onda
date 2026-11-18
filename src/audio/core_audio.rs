use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{self, NonNull};

use objc2_core_audio::{
    AudioObjectGetPropertyData, AudioObjectID, AudioObjectPropertyAddress,
    AudioObjectPropertySelector, kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal,
    kAudioObjectSystemObject,
};
use objc2_core_foundation::{CFRetained, CFString};

pub(super) fn system_object_id() -> AudioObjectID {
    kAudioObjectSystemObject as AudioObjectID
}

pub(super) fn global_address(selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}

fn get_property_scalar<T: Copy + Default>(
    object_id: AudioObjectID,
    address: &AudioObjectPropertyAddress,
) -> Option<T> {
    let mut value = T::default();
    let mut data_size = size_of::<T>() as u32;
    // SAFETY: `address` and `value` are valid stack references with matching `data_size`.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object_id,
            NonNull::from(address),
            0,
            ptr::null(),
            NonNull::from(&mut data_size),
            NonNull::from(&mut value).cast::<c_void>(),
        )
    };
    if status == 0 { Some(value) } else { None }
}

pub(super) fn get_property_u32(
    object_id: AudioObjectID,
    address: &AudioObjectPropertyAddress,
) -> Option<u32> {
    get_property_scalar(object_id, address)
}

pub(super) fn get_property_i32(
    object_id: AudioObjectID,
    address: &AudioObjectPropertyAddress,
) -> Option<i32> {
    get_property_scalar(object_id, address)
}

pub(super) fn get_property_f32(
    object_id: AudioObjectID,
    address: &AudioObjectPropertyAddress,
) -> Option<f32> {
    get_property_scalar(object_id, address)
}

pub(super) fn get_property_cfstring(
    object_id: AudioObjectID,
    address: &AudioObjectPropertyAddress,
) -> Option<String> {
    let mut cf_ptr: *mut CFString = ptr::null_mut();
    let mut data_size = size_of::<*mut CFString>() as u32;
    // SAFETY: `cf_ptr` receives an owned Core Foundation `CFStringRef` on status == 0.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object_id,
            NonNull::from(address),
            0,
            ptr::null(),
            NonNull::from(&mut data_size),
            NonNull::from(&mut cf_ptr).cast::<c_void>(),
        )
    };
    if status != 0 {
        return None;
    }
    let non_null = NonNull::new(cf_ptr)?;
    // SAFETY: Core Audio follows the Create Rule for CFString properties; `CFRetained::from_raw` takes ownership.
    let retained: CFRetained<CFString> = unsafe { CFRetained::from_raw(non_null) };
    Some(retained.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_address_sets_global_scope_and_main_element() {
        let addr = global_address(0x1234_5678);
        assert_eq!(addr.mSelector, 0x1234_5678);
        assert_eq!(addr.mScope, kAudioObjectPropertyScopeGlobal);
        assert_eq!(addr.mElement, kAudioObjectPropertyElementMain);
    }

    #[test]
    fn system_object_id_matches_core_audio_constant() {
        assert_eq!(
            system_object_id(),
            kAudioObjectSystemObject as AudioObjectID
        );
    }
}
