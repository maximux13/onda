#![allow(dead_code)]
use std::sync::mpsc::{self, Receiver};

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// Commands emitted by the macOS menu bar tray icon.
#[derive(Debug, Clone, Copy)]
pub enum TrayCommand {
    TogglePopover {
        anchor_x: f64,
        anchor_y: f64,
        width: f64,
        height: f64,
    },
    Quit,
}

/// Keeps the underlying `TrayIcon` alive for the lifetime of the application.
pub struct TrayHandle {
    _tray_icon: TrayIcon,
}

/// Configures the macOS application as an accessory (menu-bar-only) app so it
/// does not appear in the Dock or Cmd+Tab switcher.
pub fn set_accessory_activation_policy() {
    if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    }
}

/// Initializes the menu bar tray icon and returns a handle plus a receiver of
/// [`TrayCommand`] events.
pub fn init_tray() -> Result<(TrayHandle, Receiver<TrayCommand>), String> {
    let (tx, rx) = mpsc::channel::<TrayCommand>();

    TrayIconEvent::set_event_handler(Some(move |event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            rect,
            ..
        } = event
        {
            let _ = tx.send(TrayCommand::TogglePopover {
                anchor_x: rect.position.x,
                anchor_y: rect.position.y,
                width: rect.size.width as f64,
                height: rect.size.height as f64,
            });
        }
    }));

    let icon = build_template_speaker_icon()?;

    let tray_icon = TrayIconBuilder::new()
        .with_icon_templated(icon)
        .with_tooltip("Onda")
        .build()
        .map_err(|e| format!("Failed to build tray icon: {e}"))?;

    Ok((
        TrayHandle {
            _tray_icon: tray_icon,
        },
        rx,
    ))
}

/// Generates a crisp 36x36 monochrome RGBA speaker template icon in memory.
///
/// macOS template images are rendered using only their alpha channel and
/// automatically tinted for light/dark menu bars.
fn build_template_speaker_icon() -> Result<Icon, String> {
    const SIZE: usize = 36;
    let mut rgba = vec![0u8; SIZE * SIZE * 4];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let alpha = speaker_icon_coverage(px, py);
            if alpha > 0.0 {
                let idx = (y * SIZE + x) * 4;
                rgba[idx] = 0;
                rgba[idx + 1] = 0;
                rgba[idx + 2] = 0;
                rgba[idx + 3] = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }

    Icon::from_rgba(rgba, SIZE as u32, SIZE as u32)
        .map_err(|e| format!("Failed to create tray icon image: {e}"))
}

fn speaker_icon_coverage(x: f32, y: f32) -> f32 {
    let cy = 18.0;
    let half_w = 1.6; // 3.2px total bar width with rounded caps
    let bars: [(f32, f32); 5] = [
        (6.0, 10.0),
        (12.0, 20.0),
        (18.0, 26.0),
        (24.0, 16.0),
        (30.0, 12.0),
    ];

    let mut cov = 0.0f32;
    for &(bx, h) in &bars {
        let half_h = h * 0.5;
        let bar_cov = signed_box_coverage(
            x,
            y,
            bx - half_w,
            cy - half_h,
            bx + half_w,
            cy + half_h,
            half_w,
        );
        cov = cov.max(bar_cov);
    }
    cov
}

fn signed_box_coverage(
    x: f32,
    y: f32,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    radius: f32,
) -> f32 {
    let cx = (left + right) * 0.5;
    let cy = (top + bottom) * 0.5;
    let hx = (right - left) * 0.5 - radius;
    let hy = (bottom - top) * 0.5 - radius;
    let qx = (x - cx).abs() - hx;
    let qy = (y - cy).abs() - hy;
    let dist = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
    (0.5 - dist).clamp(0.0, 1.0)
}
