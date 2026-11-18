# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **GitHub Releases Auto-Updater (`src/updater.rs`):**
  - Periodic background release checks (3s after launch, then every 6 hours) and manual checks from **Settings → About**.
  - Popover update banner, settings badge indicator, `.zip` download and `Onda.app` bundle replacement, quarantine xattr cleanup, and one-click relaunch.
  - Typed `UpdaterError` error handling and unit/mock-server tests covering version comparison, plist parsing, release checks, and bundle replacement.
- **Local Updater Test Harness (`scripts/test_updater_local.sh`):**
  - Spins up a local mock GitHub Releases HTTP server with a staged `Onda-macos-arm64.zip` and launches `Onda.app` using `ONDA_UPDATE_API_URL`, `ONDA_UPDATE_CURRENT_VERSION`, and `ONDA_UPDATE_INSTALL_PATH`.

## [0.1.0] - 2026-11-17

### Added
- **Menu Bar Tray Application (`LSUIElement`):** Waveform template tray icon with auto-positioning popover on multi-monitor macOS setups and window pin toggle.
- **Per-Application Volume & Mute Control:** Powered by macOS 14.2+ Core Audio Process Taps (`AudioHardwareCreateProcessTap`) with zero virtual driver dependencies.
- **Subprocess / Browser Audio Grouping:** Automatic `ppid` traversal via `proc_pidinfo` to group Chromium, Brave, Safari, and Electron audio helper processes under their parent application.
- **Dominant App Icon Color Extraction:** HSV saturation-weighted pixel analysis of `.app` icons to tint each application's volume slider with its signature brand color.
- **Real-Time 10-Band Biquad Equalizer (`32Hz` – `16kHz`):** Stereo peaking biquad filter bank in the real-time `IOProc` supporting both **Global** and **Per-Application** EQ curves plus built-in presets (*Flat*, *Bass Boost*, *Vocal Clarity*, *Electronic*, *Acoustic*).
- **Output Device Management & Per-App Routing:** Switch system default output device, control individual output device volumes, and route specific applications to dedicated output devices.
- **Real-Time 5-Bar LED VU Meter:** Lock-free atomic peak level metering per active application.
- **Glassmorphic GPUI Interface + Lucide SVG Icons:** Full support for Dark, Light, and Auto themes, Transparent (frosted glass) and Solid window styles, and official vector Lucide SVG icons via `gpui::AssetSource`.
- **Settings Screen & Launch at Login:** 2-column configuration view with macOS `LaunchAgent` login item management, volume boost limits (`100%`, `150%`, `200%`), and JSON preference persistence.
