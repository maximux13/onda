# Onda SOLID & Architecture Refactoring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor `onda` into cohesive, single-responsibility modules with decoupled Core Audio FFI, testable audio/DSP business logic, typed errors, and a modularized GPUI popover UI.

**Architecture:** Extract pure domain/DSP logic and Core Audio FFI utilities into focused submodules, introduce static-dispatch traits (`AudioBackend`, `PreferencesStore`) so `AudioEngine` can be unit-tested with in-memory doubles, and split the 2,464-line `src/ui/popover.rs` into per-view/per-component files.

**Tech Stack:** Rust (Edition 2024), Cargo, GPUI `0.2.2`, `objc2` / `objc2-core-audio` / `objc2-app-kit`, `serde` / `serde_json`, `thiserror`.

## Global Constraints

- Rust Edition 2024 (`Cargo.toml`); maintain `cargo fmt --check`, `cargo clippy --all-targets --all-features --locked -- -D warnings`, and `cargo test` green after every task.
- Respect `[lints.rust]` and `[lints.clippy]` in `Cargo.toml`; never silence warnings with `#[allow(...)]` without justification (prefer `#[expect(..., reason = "...")]`).
- Document every `unsafe` block and `unsafe impl` with a `// SAFETY: ...` comment.
- Prefer static dispatch (generics) over `dyn Trait` for performance-critical or engine-level abstractions.
- Name tests descriptively (`<unit>_should_<behavior>_when_<condition>` or grouped `mod <unit>`).
- macOS-only FFI (`objc2-*`) must continue compiling cleanly on macOS.

---

### Task 1: Centralize Core Audio FFI Property Helpers (`src/audio/core_audio.rs`)

**Files:**
- Create: `src/audio/core_audio.rs`
- Modify: `src/audio/mod.rs:1-5`
- Modify: `src/audio/discovery.rs:31-147`
- Modify: `src/audio/master.rs:26-83`

**Interfaces:**
- Produces in `src/audio/core_audio.rs`:
  - `pub(super) fn system_object_id() -> AudioObjectID`
  - `pub(super) fn global_address(selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress`
  - `pub(super) fn get_property_u32(object_id: AudioObjectID, address: &AudioObjectPropertyAddress) -> Option<u32>`
  - `pub(super) fn get_property_i32(object_id: AudioObjectID, address: &AudioObjectPropertyAddress) -> Option<i32>`
  - `pub(super) fn get_property_f32(object_id: AudioObjectID, address: &AudioObjectPropertyAddress) -> Option<f32>`
  - `pub(super) fn get_property_cfstring(object_id: AudioObjectID, address: &AudioObjectPropertyAddress) -> Option<String>`

- [ ] **Step 1: Write unit test for `global_address` in `src/audio/core_audio.rs`**

```rust
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

pub(super) fn get_property_u32(
    object_id: AudioObjectID,
    address: &AudioObjectPropertyAddress,
) -> Option<u32> {
    let mut value: u32 = 0;
    let mut data_size = size_of::<u32>() as u32;
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

pub(super) fn get_property_i32(
    object_id: AudioObjectID,
    address: &AudioObjectPropertyAddress,
) -> Option<i32> {
    let mut value: i32 = 0;
    let mut data_size = size_of::<i32>() as u32;
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

pub(super) fn get_property_f32(
    object_id: AudioObjectID,
    address: &AudioObjectPropertyAddress,
) -> Option<f32> {
    let mut value: f32 = 0.0;
    let mut data_size = size_of::<f32>() as u32;
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
}
```

- [ ] **Step 2: Register `mod core_audio;` in `src/audio/mod.rs` and run tests**

Add `mod core_audio;` at line 1 of `src/audio/mod.rs`, then run:
`cargo test audio::core_audio::tests::global_address_sets_global_scope_and_main_element`
Expected: PASS

- [ ] **Step 3: Replace duplicated helpers in `src/audio/discovery.rs` and `src/audio/master.rs`**

Replace `system_object_id`, `get_property_u32`, `get_property_i32`, `get_property_cfstring` in `src/audio/discovery.rs` and `system_object_id`, `get_cfstring_property`, `get_f32_property` in `src/audio/master.rs` with calls to `super::core_audio::*`.

- [ ] **Step 4: Run full verification**

Run: `cargo fmt && cargo clippy --all-targets --all-features --locked -- -D warnings && cargo test`
Expected: PASS (0 warnings, all tests pass)

- [ ] **Step 5: Commit**

```bash
git add src/audio/core_audio.rs src/audio/mod.rs src/audio/discovery.rs src/audio/master.rs
git commit -m "refactor(audio): centralize Core Audio FFI property helpers"
```

---

### Task 2: Extract App Color & Icon Extraction from `src/audio/discovery.rs` (SRP + OCP)

**Files:**
- Create: `src/audio/app_colors.rs`
- Create: `src/audio/app_icons.rs`
- Modify: `src/audio/mod.rs`
- Modify: `src/audio/discovery.rs`

**Interfaces:**
- Produces in `src/audio/app_colors.rs`:
  - `pub(super) fn known_app_color(bundle_id: &str, name: &str) -> Option<u32>`
  - `pub(super) fn extract_dominant_color_from_rgba(rgba: &[u8], width: usize, height: usize) -> u32` (or wrapping the existing `NSBitmapImageRep` dominant color logic)
- Produces in `src/audio/app_icons.rs`:
  - `pub(super) fn get_or_extract_app_icon(bundle_id: &str, name: &str, pid: i32) -> (Option<Vec<u8>>, u32)`

- [ ] **Step 1: Move `known_app_color` and RGB/HSV helpers into `src/audio/app_colors.rs` using a lookup table**

```rust
const KNOWN_APP_COLORS: &[(&[&str], u32)] = &[
    (&["spotify"], 0x1ed760),
    (&["discord"], 0x5865f2),
    (&["brave"], 0xff5500),
    (&["chrome"], 0x2b8eff),
    (&["vscode", "microsoft.vscode", "visual studio code"], 0x0098ff),
    (&["com.apple.finder"], 0xa8b3c8),
    (&["safari"], 0x2582ff),
    (&["apple.music", "com.apple.music"], 0xfc3c44),
    (&["arc"], 0xff6b8b),
    (&["firefox"], 0xff7139),
    (&["slack"], 0xe01e5a),
    (&["zoom"], 0x2d8cff),
    (&["teams"], 0x6264a7),
    (&["vlc"], 0xff8800),
];

pub(super) fn known_app_color(bundle_id: &str, name: &str) -> Option<u32> {
    let bid = bundle_id.to_ascii_lowercase();
    let nm = name.to_ascii_lowercase();
    KNOWN_APP_COLORS.iter().find_map(|(patterns, color)| {
        patterns
            .iter()
            .any(|pat| bid.contains(pat) || nm.contains(pat))
            .then_some(*color)
    })
}
```
Include the existing `rgb_to_hsv` / `hsv_to_rgb` functions and tests (`known_app_color` + `rgb_hsv_roundtrip`).

- [ ] **Step 2: Move `ICON_CACHE` and AppKit `NSRunningApplication`/`NSImage` extraction into `src/audio/app_icons.rs`**

Move `IconCacheMap`, `ICON_CACHE`, `icon_cache()`, and the `NSBitmapImageRep` icon extraction code from `src/audio/discovery.rs` into `src/audio/app_icons.rs`, adding `// SAFETY:` comments on the AppKit FFI calls.

- [ ] **Step 3: Wire `mod app_colors;` and `mod app_icons;` in `src/audio/mod.rs` and update `src/audio/discovery.rs`**

`src/audio/discovery.rs` now focuses solely on querying Core Audio process objects and `libc::proc_bsdinfo` (for process names), delegating icon/color lookup to `super::app_icons`.

- [ ] **Step 4: Run full verification**

Run: `cargo fmt && cargo clippy --all-targets --all-features --locked -- -D warnings && cargo test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/audio/app_colors.rs src/audio/app_icons.rs src/audio/discovery.rs src/audio/mod.rs
git commit -m "refactor(audio): separate app icon and color extraction from process discovery"
```

---

### Task 3: Extract macOS Window Positioning from `src/main.rs` (SRP)

**Files:**
- Create: `src/platform.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces in `src/platform.rs`:
  - `pub struct PopoverPlacement { pub bounds: Bounds<Pixels>, pub display_id: Option<DisplayId>, pub ns_x: f64, pub top_y_appkit: f64 }`
  - `pub fn compute_popover_placement(cx: &App, width: f64, height: f64) -> PopoverPlacement`
  - `pub fn pin_popover_top_left(width: f64, height: f64, ns_x: f64, top_y_appkit: f64)`

- [ ] **Step 1: Create `src/platform.rs` with `compute_popover_placement` and `pin_popover_top_left`**

Move the `NSScreen::screens`, `NSEvent::mouseLocation`, display matching, and `NSApplication::windows().setFrameTopLeftPoint` logic out of `src/main.rs` into `src/platform.rs`.

- [ ] **Step 2: Update `src/main.rs` to call `platform::compute_popover_placement` and `platform::pin_popover_top_left`**

Declare `mod platform;` in `src/main.rs` and simplify the `TrayCommand::TogglePopover` arm.

- [ ] **Step 3: Run full verification**

Run: `cargo fmt && cargo clippy --all-targets --all-features --locked -- -D warnings && cargo test`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/platform.rs src/main.rs
git commit -m "refactor(platform): extract macOS popover positioning from main.rs"
```

---

### Task 4: Separate `PreferencesStore` from `src/state.rs` & Introduce `thiserror` (SRP + Typed Errors)

**Files:**
- Create: `src/storage.rs`
- Modify: `Cargo.toml`
- Modify: `src/state.rs`
- Modify: `src/launch_agent.rs`
- Modify: `src/tray.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces in `src/storage.rs`:
  - `#[derive(Debug, thiserror::Error)] pub enum StorageError`
  - `pub trait PreferencesStore { fn load(&self) -> Preferences; fn save(&self, prefs: &Preferences) -> Result<(), StorageError>; }`
  - `pub struct JsonFileStore { path: Option<PathBuf> }`
- Produces in `src/launch_agent.rs`:
  - `#[derive(Debug, thiserror::Error)] pub enum LaunchAgentError`
  - `pub fn set_launch_at_login(enabled: bool) -> Result<(), LaunchAgentError>`
- Produces in `src/tray.rs`:
  - `#[derive(Debug, thiserror::Error)] pub enum TrayError`
  - `pub fn init_tray() -> Result<(TrayHandle, Receiver<TrayCommand>), TrayError>`

- [ ] **Step 1: Add `thiserror = "2"` to `Cargo.toml`**

- [ ] **Step 2: Create `src/storage.rs` with `PreferencesStore`, `JsonFileStore`, and unit tests**

Write unit tests first for `JsonFileStore::load` (returns `Default` on missing/invalid JSON) and `JsonFileStore::save` + `load` roundtrip using a temporary path in `std::env::temp_dir()`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_file_store_should_return_defaults_when_file_missing() {
        let path = std::env::temp_dir().join("onda_missing_prefs_test.json");
        let _ = std::fs::remove_file(&path);
        let store = JsonFileStore::with_path(path);
        let prefs = store.load();
        assert_eq!(prefs.max_volume_limit, 2.0);
    }

    #[test]
    fn json_file_store_should_roundtrip_preferences() {
        let path = std::env::temp_dir().join("onda_roundtrip_prefs_test.json");
        let store = JsonFileStore::with_path(path.clone());
        let prefs = Preferences {
            theme: ThemeMode::Dark,
            max_volume_limit: 1.5,
            ..Default::default()
        };
        store.save(&prefs).expect("save prefs");
        let loaded = store.load();
        let _ = std::fs::remove_file(&path);
        assert_eq!(loaded.theme, ThemeMode::Dark);
        assert_eq!(loaded.max_volume_limit, 1.5);
    }
}
```

- [ ] **Step 3: Remove `load`/`save`/`config_path` from `src/state.rs`**

Keep `src/state.rs` strictly as pure data types (`ThemeMode`, `WindowStyle`, `DeviceIconKind`, `OutputDeviceInfo`, `AudioAppInfo`, `AppVolumeConfig`, `AppRowState`, `MixerSnapshot`, `Preferences`).

- [ ] **Step 4: Replace `Result<_, String>` with `thiserror` enums in `src/launch_agent.rs` and `src/tray.rs`**

Add unit tests exercising error display formatting in `launch_agent.rs`.

- [ ] **Step 5: Run full verification**

Run: `cargo fmt && cargo clippy --all-targets --all-features --locked -- -D warnings && cargo test`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/storage.rs src/state.rs src/launch_agent.rs src/tray.rs src/main.rs
git commit -m "refactor: extract PreferencesStore and add typed errors with thiserror"
```

---

### Task 5: Extract Pure DSP Pipeline from `src/audio/tap.rs` (SRP + Testability)

**Files:**
- Create: `src/audio/dsp.rs`
- Modify: `src/audio/mod.rs`
- Modify: `src/audio/tap.rs`

**Interfaces:**
- Produces in `src/audio/dsp.rs`:
  - `pub(super) struct BiquadBand`
  - `pub(super) fn process_interleaved_f32(input: &[f32], output: &mut [f32], channel_index: usize, n_channels: usize, scale: f32, use_eq: bool, bands: &mut [BiquadBand; 10]) -> f32` (returns frame `max_abs`)

- [ ] **Step 1: Write unit tests for `process_interleaved_f32` in `src/audio/dsp.rs`**

Test cases:
- `process_interleaved_f32_should_zero_output_when_scale_is_zero`
- `process_interleaved_f32_should_scale_samples_and_return_peak_when_eq_disabled`
- `process_interleaved_f32_should_sanitize_non_finite_samples`

- [ ] **Step 2: Implement `src/audio/dsp.rs` and delegate from `tap_io_proc` in `src/audio/tap.rs`**

`tap_io_proc` only validates `NonNull` pointers, reads atomics, builds `&[f32]`/`&mut [f32]` slices from `AudioBufferList`, and calls `dsp::process_interleaved_f32`.

- [ ] **Step 3: Run full verification**

Run: `cargo fmt && cargo clippy --all-targets --all-features --locked -- -D warnings && cargo test`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/audio/dsp.rs src/audio/tap.rs src/audio/mod.rs
git commit -m "refactor(audio): extract pure DSP processing from Core Audio tap IOProc"
```

---

### Task 6: Decouple `AudioEngine` with `AudioBackend` Trait & Add Engine Unit Tests (DIP)

**Files:**
- Modify: `src/audio/mod.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces in `src/audio/mod.rs`:
  - `pub trait TapController { fn set_volume(&self, volume: f32); fn set_muted(&self, muted: bool); fn set_eq(&self, enabled: bool, gains_db: &[f32; 10]); fn peak_level(&self) -> f32; fn output_device_uid(&self) -> &str; }`
  - `pub trait AudioBackend { type Tap: TapController; fn list_output_devices(&self) -> Vec<OutputDeviceInfo>; fn default_input_device_name(&self) -> String; fn default_output_device_uid(&self) -> Option<String>; fn fallback_master_name(&self) -> String; fn set_default_output_device(&self, device_id: u32); fn set_device_volume(&self, device_id: u32, volume: f32); fn set_device_mute(&self, device_id: u32, muted: bool); fn list_audio_processes(&self) -> Vec<AudioAppInfo>; fn create_process_tap(&self, object_ids: &[u32], output_device_uid: &str, volume: f32, muted: bool, eq_enabled: bool, eq_gains: &[f32; 10]) -> Result<Self::Tap, String>; }`
  - `pub struct CoreAudioBackend;`
  - `pub struct AudioEngine<B: AudioBackend = CoreAudioBackend, S: PreferencesStore = JsonFileStore>`

- [ ] **Step 1: Define `TapController` and `AudioBackend` traits and implement them for `AppTapHandle` and `CoreAudioBackend`**

- [ ] **Step 2: Parameterize `AudioEngine<B, S>` and add `AudioEngine::with_backend_and_store(backend, store)`**

Keep `AudioEngine::new()` as a convenience constructor for `(CoreAudioBackend, JsonFileStore::default())` so `main.rs` and `TrayPopoverView` require minimal type-alias updates (`pub type DefaultAudioEngine = AudioEngine<CoreAudioBackend, JsonFileStore>;`).

- [ ] **Step 3: Write unit tests in `src/audio/mod.rs` using `FakeBackend` and `MemoryStore`**

Test cases:
- `snapshot_should_create_tap_for_active_process_and_drop_stale_taps`
- `set_app_volume_should_clamp_and_enable_boost_above_one`
- `set_app_boost_false_should_clamp_volume_back_to_one`
- `effective_eq_should_sum_global_and_app_gains_clamped_to_twelve_db`
- `resolve_target_uid_should_fallback_to_default_when_target_device_disconnected`

- [ ] **Step 4: Run full verification**

Run: `cargo fmt && cargo clippy --all-targets --all-features --locked -- -D warnings && cargo test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/audio/mod.rs src/main.rs
git commit -m "refactor(audio): decouple AudioEngine with AudioBackend and PreferencesStore traits"
```

---

### Task 7: Modularize `src/ui/popover.rs` & Optimize UI Closures (SRP + OCP + Perf)

**Files:**
- Delete/Replace: `src/ui/popover.rs` -> `src/ui/popover/` directory:
  - Create: `src/ui/popover/mod.rs`
  - Create: `src/ui/popover/mixer_view.rs`
  - Create: `src/ui/popover/app_row.rs`
  - Create: `src/ui/popover/device_row.rs`
  - Create: `src/ui/popover/eq_view.rs`
  - Create: `src/ui/popover/settings_view.rs`
- Modify: `src/ui/icons.rs`
- Modify: `src/ui/slider.rs`

**Interfaces:**
- Preserves public API in `src/ui/popover/mod.rs`:
  - `pub struct TrayPopoverView`
  - `impl TrayPopoverView { pub fn new(engine: Arc<Mutex<AudioEngine>>, cx: &mut Context<Self>) -> Self; pub fn is_pinned(&self) -> bool; }`
  - `impl Render for TrayPopoverView`

- [ ] **Step 1: Split `src/ui/popover.rs` into `src/ui/popover/` submodules**

Move each cohesive rendering section into its own file as `impl TrayPopoverView` methods (`pub(super) fn render_...`), keeping `mod.rs` under ~350 lines (state, engine sync, `Render`, header/footer).

- [ ] **Step 2: Replace per-closure `bundle_id: String` clones in `app_row.rs` with `SharedString` or `Arc<str>`**

Avoid 5 heap-allocated `String::clone()` calls per app row on every render.

- [ ] **Step 3: Deduplicate `IconName` + `LucideAssets` in `src/ui/icons.rs` with a declarative macro**

Define a single `lucide_icons!` macro that generates `IconName` constants, `LucideAssets::load`, and `LucideAssets::list` from one source of truth.

- [ ] **Step 4: Extract shared drag/bounds helper in `src/ui/slider.rs`**

Share common mouse-down/mouse-move clamping logic between `VolumeSlider` and `EqVerticalSlider`.

- [ ] **Step 5: Run full verification**

Run: `cargo fmt && cargo clippy --all-targets --all-features --locked -- -D warnings && cargo test`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/ui/
git commit -m "refactor(ui): modularize popover into focused view components and deduplicate icon registry"
```

---

### Self-Review Checklist
- [x] Every task produces a compiling, clippy-clean, tested deliverable.
- [x] Zero placeholders (`TBD`/`TODO`); exact file paths, module names, and test names specified.
- [x] Consistent signatures across tasks (`PreferencesStore` in Task 4 matches `AudioEngine<B, S>` in Task 6; `dsp::process_interleaved_f32` in Task 5 matches `tap.rs`).
- [x] Static dispatch used for `AudioEngine<B, S>` per `rust-best-practices` Chapter 6.
