use std::collections::HashMap;
use std::ptr;
use std::sync::{Mutex, OnceLock};

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSBitmapImageRepPropertyKey,
    NSCalibratedRGBColorSpace, NSCompositingOperation, NSGraphicsContext, NSImage,
    NSRunningApplication,
};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize};

use super::app_colors::{
    DEFAULT_DOMINANT_COLOR, extract_dominant_color_from_rgba, known_app_color,
};

type IconCacheMap = HashMap<String, (Vec<u8>, u32)>;

static ICON_CACHE: OnceLock<Mutex<IconCacheMap>> = OnceLock::new();

fn icon_cache() -> &'static Mutex<IconCacheMap> {
    ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn extract_rep_dominant_color(rep: &NSBitmapImageRep) -> u32 {
    let data_ptr = rep.bitmapData();
    if data_ptr.is_null() {
        return DEFAULT_DOMINANT_COLOR;
    }
    let width = rep.pixelsWide().max(0) as usize;
    let height = rep.pixelsHigh().max(0) as usize;
    let row_bytes = width.saturating_mul(4);
    let bytes_per_row = rep.bytesPerRow().max(row_bytes as isize) as usize;
    if width == 0 || height == 0 || bytes_per_row < row_bytes {
        return DEFAULT_DOMINANT_COLOR;
    }
    // SAFETY: `bitmapData()` returns a valid pointer to at least `height * bytes_per_row`
    // bytes owned by `rep` for the lifetime of `&rep`.
    let raw = unsafe { std::slice::from_raw_parts(data_ptr, height * bytes_per_row) };
    if bytes_per_row == row_bytes {
        extract_dominant_color_from_rgba(raw, width, height)
    } else {
        let mut packed = Vec::with_capacity(height * row_bytes);
        for y in 0..height {
            let start = y * bytes_per_row;
            packed.extend_from_slice(&raw[start..start + row_bytes]);
        }
        extract_dominant_color_from_rgba(&packed, width, height)
    }
}

fn nsimage_to_png_64(image: &NSImage) -> Option<(Vec<u8>, u32)> {
    // SAFETY: `NSBitmapImageRep::alloc()` returns a freshly allocated instance and passing
    // `null_mut()` for `planes` asks AppKit to allocate a 64x64 8-bpc 4-spp RGBA buffer.
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

    let dominant_color = extract_rep_dominant_color(&rep);

    let empty_props: Retained<NSDictionary<NSBitmapImageRepPropertyKey, AnyObject>> =
        NSDictionary::new();
    // SAFETY: `empty_props` is a valid empty `NSDictionary` matching AppKit's PNG properties parameter.
    let data = unsafe {
        rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &empty_props)
    }?;
    Some((data.to_vec(), dominant_color))
}

fn find_owning_running_app(
    pid: i32,
    parent_pid: impl Fn(i32) -> Option<i32>,
) -> Option<(i32, Retained<NSRunningApplication>)> {
    let mut current = pid;
    for _ in 0..8 {
        if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(current) {
            // Prefer an app that has a localizedName or bundleIdentifier (skip headless helpers if parent is a real app).
            if app.bundleIdentifier().is_some() || app.localizedName().is_some() {
                return Some((current, app));
            }
        }
        current = parent_pid(current)?;
        if current <= 1 {
            break;
        }
    }
    None
}

/// Resolves the owning macOS application's `(owning_pid, bundle_id, localized_name)`
/// by walking parent PIDs until an `NSRunningApplication` with metadata is found.
pub(super) fn resolve_owning_app(
    pid: i32,
    raw_bundle_id: Option<&str>,
    parent_pid: impl Fn(i32) -> Option<i32>,
) -> Option<(i32, String, Option<String>)> {
    let running_app = find_owning_running_app(pid, parent_pid);

    let bundle_id = running_app
        .as_ref()
        .and_then(|(_, app)| app.bundleIdentifier())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| raw_bundle_id.map(str::to_string).filter(|s| !s.is_empty()))?;

    let localized_name = running_app
        .as_ref()
        .and_then(|(_, app)| app.localizedName())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    let owning_pid = running_app
        .as_ref()
        .map(|(owning, _)| *owning)
        .unwrap_or(pid);
    Some((owning_pid, bundle_id, localized_name))
}

/// Returns the cached or newly extracted 64x64 PNG icon bytes and dominant RGB color
/// for `bundle_id`, preferring curated brand colors from `app_colors`.
pub(super) fn get_or_extract_app_icon(
    bundle_id: &str,
    name: &str,
    pid: i32,
) -> (Option<Vec<u8>>, u32) {
    let signature_color = known_app_color(bundle_id, name);

    if let Some((bytes, cached_color)) = icon_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(bundle_id).cloned())
    {
        return (Some(bytes), signature_color.unwrap_or(cached_color));
    }

    let Some(icon) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .and_then(|app| app.icon())
    else {
        return (None, signature_color.unwrap_or(DEFAULT_DOMINANT_COLOR));
    };

    if let Some((bytes, extracted_color)) = nsimage_to_png_64(&icon) {
        let final_color = signature_color.unwrap_or(extracted_color);
        if let Ok(mut cache) = icon_cache().lock() {
            cache.insert(bundle_id.to_string(), (bytes.clone(), final_color));
        }
        (Some(bytes), final_color)
    } else {
        (None, signature_color.unwrap_or(DEFAULT_DOMINANT_COLOR))
    }
}
