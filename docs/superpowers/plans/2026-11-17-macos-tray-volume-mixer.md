# macOS Tray Per-App Volume Mixer (Rust + GPUI) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a macOS menu bar (tray) application in Rust using GPUI and Core Audio Process Taps to control individual application volumes and master output volume.

**Architecture:** The app runs as an `LSUIElement` (menu-bar-only, no Dock icon) macOS application. A background audio engine discovers active audio processes via Core Audio (`objc2-core-audio`) and intercepts/scales per-app audio using macOS 14.2+ `AudioHardwareCreateProcessTap` + `AggregateDevice` + lock-free `IOProc` callbacks. The UI uses `tray-icon` for the top-right status bar icon and a `gpui` `WindowKind::PopUp` with `WindowBackgroundAppearance::Blurred` for the floating mixer popover.

**Tech Stack:** Rust (edition 2024), `gpui`, `tray-icon`, `objc2`, `objc2-foundation`, `objc2-app-kit`, `objc2-core-audio`, `objc2-core-audio-types`, `serde` / `serde_json`.

## Global Constraints

- Target platform: macOS 14.2+ (verified on macOS 26.6.2, `rustc 1.100.0`).
- Audio callbacks (`IOProc`) MUST be real-time safe: no heap allocations, no mutex locks, no logging — communicate volume/mute updates via `AtomicU32` (f32 bits) and `AtomicBool`.
- Requires `NSAudioCaptureUsageDescription` in `Info.plist` for Core Audio Process Tap permission, and `LSUIElement = true` to hide from the Dock.
- No virtual audio drivers (BlackHole/Soundflower) are required.

---

## File Structure

```text
master/
├── Cargo.toml
├── Info.plist                  # LSUIElement + NSAudioCaptureUsageDescription
├── scripts/bundle_app.sh       # Builds .app bundle for testing permissions & tray behavior
└── src/
    ├── main.rs                 # App bootstrap, GPUI Application loop, tray-to-window wiring
    ├── state.rs                # Shared MixerState, per-app volume/mute persistence (JSON)
    ├── audio/
    │   ├── mod.rs              # AudioEngine facade & background polling/event thread
    │   ├── discovery.rs        # Enumerate active audio processes (PID, Bundle ID, name)
    │   ├── master.rs           # Default output device volume & mute control
    │   └── tap.rs              # ProcessTap + AggregateDevice + lock-free IOProc gain/mute
    ├── tray.rs                 # Tray icon setup, click event & screen position calculation
    └── ui/
        ├── mod.rs              # UI exports & theme tokens
        ├── popover.rs          # GPUI TrayPopoverView (Master row + App rows + Footer)
        └── slider.rs           # Custom interactive GPUI volume slider component
```

---

### Task 1: Project Scaffolding & macOS Bundle Configuration

**Files:**
- Modify: `Cargo.toml`
- Create: `Info.plist`
- Create: `scripts/bundle_app.sh`

**Interfaces:**
- Produces: Compilable Cargo manifest with `gpui`, `tray-icon`, `objc2-*`, `serde`, `serde_json`, and a `.app` bundle script so macOS grants `NSAudioCaptureUsageDescription` properly.

- [ ] **Step 1: Add dependencies to `Cargo.toml`**
  Configure `gpui`, `tray-icon`, `objc2`, `objc2-foundation`, `objc2-app-kit`, `objc2-core-audio`, `objc2-core-audio-types`, `serde`, `serde_json`.
- [ ] **Step 2: Create `Info.plist`**
  Include `CFBundleIdentifier`, `CFBundleName`, `LSUIElement = <true/>`, and `NSAudioCaptureUsageDescription`.
- [ ] **Step 3: Create `scripts/bundle_app.sh`**
  Build with `cargo build` and copy binary + `Info.plist` into `target/debug/VolumeMixer.app/Contents/`.
- [ ] **Step 4: Verify compilation**
  Run `cargo check` and `./scripts/bundle_app.sh` to confirm all dependencies resolve and link cleanly on macOS.

---

### Task 2: Audio Process Discovery & Master Volume Control

**Files:**
- Create: `src/audio/mod.rs`
- Create: `src/audio/discovery.rs`
- Create: `src/audio/master.rs`

**Interfaces:**
- Produces:
  - `pub struct AudioAppInfo { pub object_id: AudioObjectID, pub pid: i32, pub bundle_id: String, pub name: String, pub is_running_output: bool }`
  - `pub fn list_audio_processes() -> Vec<AudioAppInfo>`
  - `pub fn get_master_volume() -> Result<f32, String>` / `pub fn set_master_volume(vol: f32) -> Result<(), String>`
  - `pub fn get_master_mute() -> Result<bool, String>` / `pub fn set_master_mute(muted: bool) -> Result<(), String>`

- [ ] **Step 1: Write unit tests for volume clamping & filtering logic**
  Test that volume values clamp to `[0.0, 1.5]` (or `[0.0, 1.0]` for master) and system/self processes are filtered out.
- [ ] **Step 2: Implement `src/audio/discovery.rs`**
  Query `kAudioHardwarePropertyProcessObjectList` via Core Audio `AudioObjectGetPropertyData`, read `kAudioProcessPropertyPID`, `kAudioProcessPropertyBundleID`, and `kAudioProcessPropertyIsRunningOutput`, and resolve human-readable app names via `NSRunningApplication`.
- [ ] **Step 3: Implement `src/audio/master.rs`**
  Query `kAudioHardwarePropertyDefaultOutputDevice` and read/write `kAudioHardwareServiceDeviceProperty_VirtualMainVolume` and `kAudioDevicePropertyMute`.
- [ ] **Step 4: Verify**
  Run `cargo test` and a quick CLI/example check printing discovered audio processes while playing audio in Chrome/Spotify.

---

### Task 3: Per-App Volume Control via Core Audio Process Tap

**Files:**
- Create: `src/audio/tap.rs`
- Modify: `src/audio/mod.rs`

**Interfaces:**
- Consumes: `AudioAppInfo` from `discovery.rs`.
- Produces:
  - `pub struct AppTapHandle` (holds `Arc<AtomicU32>` for gain, `Arc<AtomicBool>` for mute, and `Drop` cleanup for `IOProc` + `AggregateDevice` + `ProcessTap`).
  - `AudioEngine::set_app_volume(bundle_id_or_pid, volume: f32)`
  - `AudioEngine::set_app_mute(bundle_id_or_pid, muted: bool)`

- [ ] **Step 1: Write unit tests for lock-free gain calculation**
  Test `gain_from_state(volume: f32, muted: bool) -> f32` (returns `0.0` when muted, otherwise `volume` or perceptual curve `volume.powi(2)`).
- [ ] **Step 2: Implement `CATapDescription` & `AudioHardwareCreateProcessTap`**
  Create a stereo/mixdown tap targeting the process's `AudioObjectID` with `CATapMuteBehavior::Muted` so the original app output is silenced while tapped.
- [ ] **Step 3: Implement `AggregateDevice` + `IOProc` playback**
  Combine the tap with the current default output device, register `AudioDeviceCreateIOProcID`, scale float32 PCM samples by the atomic gain in the `IOProc` callback, and start the device.
- [ ] **Step 4: Implement lifecycle & device-change handling**
  Only keep a tap active when `volume != 1.0 || muted` (bypass when at 100% unmuted for zero overhead), and tear down cleanly on `Drop` or when the app exits / default output device changes.
- [ ] **Step 5: Verify**
  Run `cargo test` and manually verify changing volume of a running audio app (e.g., Spotify/Browser) changes its output level without double-audio echo.

---

### Task 4: Tray Icon & Popover Window Positioning

**Files:**
- Create: `src/tray.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces:
  - `pub fn init_tray() -> TrayHandle`
  - `TrayEvent::TogglePopover { anchor_x: f64, anchor_y: f64 }`
  - `TrayEvent::Quit`

- [ ] **Step 1: Create `src/tray.rs`**
  Initialize `tray-icon` with a monochrome template icon (so macOS adapts it automatically for light/dark menu bars).
- [ ] **Step 2: Handle tray click & coordinates**
  Forward `TrayIconEvent::Click` (left-click) with the icon rect coordinates to toggle the GPUI popover window directly below the tray icon.
- [ ] **Step 3: Configure `LSUIElement` / Activation Policy**
  Ensure the process runs as an Accessory app (no Dock icon) and hides the popover when it loses focus (`on_window_deactivate` / `Escape` key).
- [ ] **Step 4: Verify**
  Run the `.app` bundle, confirm the icon appears in the top-right menu bar, and clicking toggles an empty blurred GPUI `WindowKind::PopUp`.

---

### Task 5: GPUI Popover UI & Custom Volume Slider

**Files:**
- Create: `src/ui/mod.rs`
- Create: `src/ui/slider.rs`
- Create: `src/ui/popover.rs`

**Interfaces:**
- Consumes: `MixerState` snapshot (`MasterState`, `Vec<AppRowState>`), `AudioEngine` commands.
- Produces:
  - `VolumeSlider` GPUI element (supports click-to-jump, drag, and scroll-wheel).
  - `TrayPopoverView` implementing `gpui::Render`.

- [ ] **Step 1: Implement `src/ui/slider.rs`**
  Build an interactive GPUI slider track + filled bar + thumb on hover, emitting `on_change(f32)` callbacks during mouse drag.
- [ ] **Step 2: Implement `src/ui/popover.rs`**
  Render:
  - Header/Master section: output device name, mute button, `VolumeSlider`, percentage label.
  - Active Apps list: app icon, app name, mute toggle, `VolumeSlider`, percentage label (scrollable if many apps).
  - Footer: "Quit" button (and optional "Show inactive apps" toggle).
  - Styling: `WindowBackgroundAppearance::Blurred`, subtle dark/light translucent surface, rounded corners.
- [ ] **Step 3: Verify**
  Launch `.app`, open popover, and confirm sliders drag smoothly at 60/120fps and update audio state in real time.

---

### Task 6: App Icons, Persistence & End-to-End Polish

**Files:**
- Create: `src/state.rs`
- Modify: `src/audio/discovery.rs`
- Modify: `src/ui/popover.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces:
  - `AppIconCache` extracting PNG/RGBA bytes from `NSRunningApplication::icon()` for GPUI rendering.
  - `Preferences` saved to `~/Library/Application Support/VolumeMixer/volumes.json` (remembers last volume per `Bundle ID`).

- [ ] **Step 1: Write unit tests for `Preferences` serialization**
  Test saving/loading per-`bundle_id` volume and mute state with `serde_json`.
- [ ] **Step 2: Extract & cache app icons**
  Convert `NSImage` from `NSRunningApplication` to PNG/RGBA bytes once per `bundle_id` and display via GPUI `img(...)`.
- [ ] **Step 3: Wire persistence & periodic refresh**
  Refresh active audio apps when the popover opens and every ~1.5s while open; persist volume changes debounced to disk.
- [ ] **Step 4: End-to-end verification**
  Run `cargo test`, build `.app` via `./scripts/bundle_app.sh`, test opening/closing popover, muting/adjusting 2+ apps simultaneously, and restarting the app to verify saved volumes restore.
