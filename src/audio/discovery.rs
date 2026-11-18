use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{self, NonNull};
use std::sync::{Mutex, OnceLock};

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSBitmapImageRepPropertyKey,
    NSCalibratedRGBColorSpace, NSCompositingOperation, NSGraphicsContext, NSImage,
    NSRunningApplication,
};
use objc2_core_audio::{
    AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectID,
    AudioObjectPropertyAddress, kAudioHardwarePropertyProcessObjectList,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
    kAudioProcessPropertyBundleID, kAudioProcessPropertyIsRunningOutput, kAudioProcessPropertyPID,
};
use objc2_core_foundation::{CFRetained, CFString};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize};

use crate::state::AudioAppInfo;

type IconCacheMap = HashMap<String, (Vec<u8>, u32)>;

static ICON_CACHE: OnceLock<Mutex<IconCacheMap>> = OnceLock::new();

fn icon_cache() -> &'static Mutex<IconCacheMap> {
    ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn system_object_id() -> AudioObjectID {
    kAudioObjectSystemObject as AudioObjectID
}

fn get_process_object_ids() -> Vec<AudioObjectID> {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioHardwarePropertyProcessObjectList,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };

    let mut data_size: u32 = 0;
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

fn get_property_u32(object_id: AudioObjectID, selector: u32) -> Option<u32> {
    let address = AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut value: u32 = 0;
    let mut data_size = size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object_id,
            NonNull::from(&address),
            0,
            ptr::null(),
            NonNull::from(&mut data_size),
            NonNull::from(&mut value).cast::<c_void>(),
        )
    };
    if status == 0 { Some(value) } else { None }
}

fn get_property_i32(object_id: AudioObjectID, selector: u32) -> Option<i32> {
    let address = AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut value: i32 = 0;
    let mut data_size = size_of::<i32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object_id,
            NonNull::from(&address),
            0,
            ptr::null(),
            NonNull::from(&mut data_size),
            NonNull::from(&mut value).cast::<c_void>(),
        )
    };
    if status == 0 { Some(value) } else { None }
}

fn get_property_cfstring(object_id: AudioObjectID, selector: u32) -> Option<String> {
    let address = AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut cf_ptr: *mut CFString = ptr::null_mut();
    let mut data_size = size_of::<*mut CFString>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object_id,
            NonNull::from(&address),
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
    let retained: CFRetained<CFString> = unsafe { CFRetained::from_raw(non_null) };
    Some(retained.to_string())
}

const DEFAULT_DOMINANT_COLOR: u32 = 0x2b8eff;

fn known_app_color(bundle_id: &str, name: &str) -> Option<u32> {
    let bid = bundle_id.to_ascii_lowercase();
    let nm = name.to_ascii_lowercase();
    if bid.contains("spotify") || nm.contains("spotify") {
        Some(0x1ed760)
    } else if bid.contains("discord") || nm.contains("discord") {
        Some(0x5865f2)
    } else if bid.contains("brave") || nm.contains("brave") {
        Some(0xff5500)
    } else if bid.contains("chrome") || nm.contains("chrome") {
        Some(0x2b8eff)
    } else if bid.contains("vscode")
        || bid.contains("microsoft.vscode")
        || nm.contains("visual studio code")
    {
        Some(0x0098ff)
    } else if bid.contains("com.apple.finder") || nm == "finder" {
        Some(0xa8b3c8)
    } else if bid.contains("com.apple.music") || nm == "music" {
        Some(0xfa2d48)
    } else if bid.contains("firefox") || nm.contains("firefox") {
        Some(0xff7139)
    } else if bid.contains("safari") || nm.contains("safari") {
        Some(0x007aff)
    } else {
        None
    }
}

fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let h = if delta <= 1e-5 {
        0.0
    } else if (max - r).abs() <= 1e-5 {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if (max - g).abs() <= 1e-5 {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    let s = if max <= 1e-5 { 0.0 } else { delta / max };
    (h, s, max)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let h_prime = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());
    let (r1, g1, b1) = if h_prime < 1.0 {
        (c, x, 0.0)
    } else if h_prime < 2.0 {
        (x, c, 0.0)
    } else if h_prime < 3.0 {
        (0.0, c, x)
    } else if h_prime < 4.0 {
        (0.0, x, c)
    } else if h_prime < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let m = v - c;
    let r = ((r1 + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = ((g1 + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = ((b1 + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    (r, g, b)
}

fn extract_dominant_color(rep: &NSBitmapImageRep) -> u32 {
    let data_ptr = rep.bitmapData();
    if data_ptr.is_null() {
        return DEFAULT_DOMINANT_COLOR;
    }
    let bytes_per_row = rep.bytesPerRow().max(64 * 4) as usize;

    // 12 hue buckets (30 degrees each) to find the most vibrant dominant color family
    let mut bucket_weight = [0.0f32; 12];
    let mut bucket_r = [0.0f32; 12];
    let mut bucket_g = [0.0f32; 12];
    let mut bucket_b = [0.0f32; 12];

    for y in 0..64usize {
        for x in 0..64usize {
            let offset = y * bytes_per_row + x * 4;
            let (raw_r, raw_g, raw_b, a) = unsafe {
                (
                    *data_ptr.add(offset),
                    *data_ptr.add(offset + 1),
                    *data_ptr.add(offset + 2),
                    *data_ptr.add(offset + 3),
                )
            };

            if a < 128 {
                continue;
            }

            // Un-premultiply alpha
            let r = ((u32::from(raw_r) * 255) / u32::from(a)).min(255) as u8;
            let g = ((u32::from(raw_g) * 255) / u32::from(a)).min(255) as u8;
            let b = ((u32::from(raw_b) * 255) / u32::from(a)).min(255) as u8;

            // Ignore near-white and near-black pixels
            if (r > 235 && g > 235 && b > 235) || (r < 30 && g < 30 && b < 30) {
                continue;
            }

            let rf = f32::from(r) / 255.0;
            let gf = f32::from(g) / 255.0;
            let bf = f32::from(b) / 255.0;
            let (h, s, v) = rgb_to_hsv(rf, gf, bf);
            if s < 0.12 {
                continue;
            }

            // Vibrancy weight favors saturated, bright pixels
            let weight = (s * s) * v * (f32::from(a) / 255.0);
            let bin = ((h / 30.0).floor() as usize).min(11);
            bucket_weight[bin] += weight;
            bucket_r[bin] += rf * weight;
            bucket_g[bin] += gf * weight;
            bucket_b[bin] += bf * weight;
        }
    }

    let mut best_bin = 0;
    let mut best_weight = 0.0f32;
    for (i, &w) in bucket_weight.iter().enumerate() {
        if w > best_weight {
            best_weight = w;
            best_bin = i;
        }
    }

    if best_weight <= 1e-4 {
        return DEFAULT_DOMINANT_COLOR;
    }

    let avg_r = bucket_r[best_bin] / best_weight;
    let avg_g = bucket_g[best_bin] / best_weight;
    let avg_b = bucket_b[best_bin] / best_weight;
    let (h, s, v) = rgb_to_hsv(avg_r, avg_g, avg_b);
    let boosted_s = s.clamp(0.60, 0.95);
    let boosted_v = v.clamp(0.75, 0.98);
    let (r, g, b) = hsv_to_rgb(h, boosted_s, boosted_v);
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

fn nsimage_to_png_64(image: &NSImage) -> Option<(Vec<u8>, u32)> {
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            ptr::null_mut(),
            64,
            64,
            8,
            4,
            true,
            false,
            NSCalibratedRGBColorSpace,
            0,
            0,
        )
    }?;

    let ctx = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&ctx));

    let dest = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(64.0, 64.0));
    let zero = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0));
    image.drawInRect_fromRect_operation_fraction(
        dest,
        zero,
        NSCompositingOperation::SourceOver,
        1.0,
    );

    NSGraphicsContext::restoreGraphicsState_class();

    let dominant_color = extract_dominant_color(&rep);

    let empty_props: Retained<NSDictionary<NSBitmapImageRepPropertyKey, AnyObject>> =
        NSDictionary::new();
    let data = unsafe {
        rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &empty_props)
    }?;
    Some((data.to_vec(), dominant_color))
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
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
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

fn find_owning_running_app(pid: i32) -> Option<Retained<NSRunningApplication>> {
    let mut current = pid;
    for _ in 0..8 {
        if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(current) {
            // Prefer an app that has a localizedName or bundleIdentifier (skip headless helpers if parent is a real app).
            if app.bundleIdentifier().is_some() || app.localizedName().is_some() {
                return Some(app);
            }
        }
        current = parent_pid(current)?;
        if current <= 1 {
            break;
        }
    }
    None
}

fn resolve_app_metadata(
    pid: i32,
    raw_bundle_id: Option<&str>,
) -> Option<(String, String, Option<Vec<u8>>, u32)> {
    let running_app = find_owning_running_app(pid);

    let bundle_id = running_app
        .as_ref()
        .and_then(|app| app.bundleIdentifier())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| raw_bundle_id.map(str::to_string).filter(|s| !s.is_empty()))?;

    let cached_entry = icon_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&bundle_id).cloned());

    let name = running_app
        .as_ref()
        .and_then(|app| app.localizedName())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_name_from_bundle_id(&bundle_id));

    let signature_color = known_app_color(&bundle_id, &name);

    let (icon_png, dominant_color) = if let Some((bytes, cached_color)) = cached_entry {
        (Some(bytes), signature_color.unwrap_or(cached_color))
    } else if let Some(icon) = running_app.as_ref().and_then(|app| app.icon()) {
        if let Some((bytes, extracted_color)) = nsimage_to_png_64(&icon) {
            let final_color = signature_color.unwrap_or(extracted_color);
            if let Ok(mut cache) = icon_cache().lock() {
                cache.insert(bundle_id.clone(), (bytes.clone(), final_color));
            }
            (Some(bytes), final_color)
        } else {
            (None, signature_color.unwrap_or(DEFAULT_DOMINANT_COLOR))
        }
    } else {
        (None, signature_color.unwrap_or(DEFAULT_DOMINANT_COLOR))
    };

    Some((bundle_id, name, icon_png, dominant_color))
}

/// Lists running macOS applications currently outputting audio, grouping helper
/// subprocesses (such as Chromium/Brave audio helpers) under their parent app.
pub fn list_audio_processes() -> Vec<AudioAppInfo> {
    let self_pid = std::process::id() as i32;
    let object_ids = get_process_object_ids();
    let mut grouped: HashMap<String, AudioAppInfo> = HashMap::new();

    for object_id in object_ids {
        let is_running_output = get_property_u32(object_id, kAudioProcessPropertyIsRunningOutput)
            .map(|v| v != 0)
            .unwrap_or(false);
        if !is_running_output {
            continue;
        }

        let Some(pid) = get_property_i32(object_id, kAudioProcessPropertyPID) else {
            continue;
        };
        if pid == self_pid {
            continue;
        }

        let raw_bundle_id = get_property_cfstring(object_id, kAudioProcessPropertyBundleID);
        let Some((bundle_id, name, icon_png, dominant_color)) =
            resolve_app_metadata(pid, raw_bundle_id.as_deref())
        else {
            continue;
        };

        grouped
            .entry(bundle_id.clone())
            .and_modify(|entry| {
                entry.object_ids.push(object_id);
            })
            .or_insert_with(|| AudioAppInfo {
                object_ids: vec![object_id],
                bundle_id,
                name,
                is_running_output: true,
                icon_png,
                dominant_color,
            });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_hsv_roundtrip() {
        let (h, s, v) = rgb_to_hsv(0.18, 0.84, 0.38);
        let (r, g, b) = hsv_to_rgb(h, s, v);
        assert!((r as i32 - (0.18_f32 * 255.0).round() as i32).abs() <= 2);
        assert!((g as i32 - (0.84_f32 * 255.0).round() as i32).abs() <= 2);
        assert!((b as i32 - (0.38_f32 * 255.0).round() as i32).abs() <= 2);
    }

    #[test]
    fn test_known_app_dominant_colors() {
        assert_eq!(
            known_app_color("com.spotify.client", "Spotify"),
            Some(0x1ed760)
        );
        assert_eq!(
            known_app_color("com.brave.Browser", "Brave Browser"),
            Some(0xff5500)
        );
        assert_eq!(
            known_app_color("com.google.Chrome", "Google Chrome"),
            Some(0x2b8eff)
        );
        assert_eq!(
            known_app_color("com.hnc.Discord", "Discord"),
            Some(0x5865f2)
        );
    }
}
