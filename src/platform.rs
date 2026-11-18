use gpui::{App, Bounds, DisplayId, Pixels, Point, Size, px};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSEvent, NSScreen};
use objc2_foundation::NSPoint;

const EDGE_MARGIN: f64 = 8.0;
const MENU_BAR_GAP: f64 = 6.0;
const MIN_MENU_BAR_HEIGHT: f64 = 24.0;

/// Computed GPUI window bounds, target display, and AppKit coordinates for
/// placing the tray popover below the menu bar.
pub struct PopoverPlacement {
    pub bounds: Bounds<Pixels>,
    pub display_id: Option<DisplayId>,
    pub ns_x: f64,
    pub top_y_appkit: f64,
}

struct ScreenGeometry {
    mouse_x: f64,
    disp_ox: f64,
    disp_w: f64,
    menu_bar_h: f64,
    top_y_appkit: f64,
}

impl Default for ScreenGeometry {
    fn default() -> Self {
        Self {
            mouse_x: 1300.0,
            disp_ox: 0.0,
            disp_w: 1440.0,
            menu_bar_h: 28.0,
            top_y_appkit: 860.0,
        }
    }
}

/// Queries AppKit for the screen containing the current mouse cursor and
/// derives display metrics needed for popover placement.
fn query_active_screen_geometry() -> ScreenGeometry {
    MainThreadMarker::new()
        .and_then(|mtm| {
            let mouse_loc = NSEvent::mouseLocation();
            let screens = NSScreen::screens(mtm);
            screens
                .iter()
                .find(|s| {
                    let f = s.frame();
                    mouse_loc.x >= f.origin.x
                        && mouse_loc.x < f.origin.x + f.size.width
                        && mouse_loc.y >= f.origin.y
                        && mouse_loc.y < f.origin.y + f.size.height
                })
                .map(|s| {
                    let f = s.frame();
                    let vf = s.visibleFrame();
                    let menu_bar = (f.origin.y + f.size.height) - (vf.origin.y + vf.size.height);
                    let top_y_appkit = vf.origin.y + vf.size.height - MENU_BAR_GAP;
                    ScreenGeometry {
                        mouse_x: mouse_loc.x,
                        disp_ox: f.origin.x,
                        disp_w: f.size.width,
                        menu_bar_h: menu_bar.max(MIN_MENU_BAR_HEIGHT),
                        top_y_appkit,
                    }
                })
        })
        .unwrap_or_default()
}

/// Computes the display-relative horizontal coordinate for centering a popover
/// of `width` under `mouse_x`, clamped within [`EDGE_MARGIN`] of display edges.
fn compute_clamped_x(mouse_x: f64, disp_ox: f64, disp_w: f64, width: f64) -> f64 {
    let rel_mouse_x = mouse_x - disp_ox;
    let max_x = (disp_w - width - EDGE_MARGIN).max(EDGE_MARGIN);
    (rel_mouse_x - width / 2.0).clamp(EDGE_MARGIN, max_x)
}

/// Computes GPUI window placement and AppKit top-left coordinates for a tray
/// popover of size `(width, height)` anchored near the current mouse location.
pub fn compute_popover_placement(cx: &App, width: f64, height: f64) -> PopoverPlacement {
    let screen = query_active_screen_geometry();

    let target_display = cx
        .displays()
        .into_iter()
        .find(|d| {
            let b = d.bounds();
            let ox = f64::from(b.origin.x);
            let w = f64::from(b.size.width);
            screen.mouse_x >= ox && screen.mouse_x < ox + w
        })
        .or_else(|| cx.primary_display());

    let display_id = target_display.map(|d| d.id());

    let x = compute_clamped_x(screen.mouse_x, screen.disp_ox, screen.disp_w, width);

    // GPUI's macOS WindowOptions passes `window_rect.origin` to
    // `NSWindow::setFrameTopLeftPoint_`, so `bounds.origin.y` is the
    // distance from the top of the screen to the TOP of the popover.
    let y = screen.menu_bar_h + MENU_BAR_GAP;

    let bounds = Bounds::new(
        Point::new(px(x as f32), px(y as f32)),
        Size::new(px(width as f32), px(height as f32)),
    );

    PopoverPlacement {
        bounds,
        display_id,
        ns_x: screen.disp_ox + x,
        top_y_appkit: screen.top_y_appkit,
    }
}

/// Explicitly pins any open AppKit window matching `(width, height)` to
/// `(ns_x, top_y_appkit)` in AppKit coordinates right below the menu bar.
pub fn pin_popover_top_left(width: f64, height: f64, ns_x: f64, top_y_appkit: f64) {
    if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApplication::sharedApplication(mtm);
        for win in app.windows().iter() {
            let frame = win.frame();
            if (frame.size.width - width).abs() < 2.0 && (frame.size.height - height).abs() < 2.0 {
                win.setFrameTopLeftPoint(NSPoint::new(ns_x, top_y_appkit));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamped_x_centers_under_mouse_on_primary_display() {
        let x = compute_clamped_x(720.0, 0.0, 1440.0, 460.0);
        assert!((x - 490.0).abs() < f64::EPSILON);
    }

    #[test]
    fn clamped_x_clamps_to_left_margin() {
        let x = compute_clamped_x(50.0, 0.0, 1440.0, 460.0);
        assert!((x - EDGE_MARGIN).abs() < f64::EPSILON);
    }

    #[test]
    fn clamped_x_clamps_to_right_margin() {
        let x = compute_clamped_x(1430.0, 0.0, 1440.0, 460.0);
        assert!((x - (1440.0 - 460.0 - EDGE_MARGIN)).abs() < f64::EPSILON);
    }

    #[test]
    fn clamped_x_handles_secondary_display_origin() {
        let x = compute_clamped_x(2160.0, 1440.0, 1920.0, 460.0);
        assert!((x - (720.0 - 230.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn clamped_x_handles_narrow_display() {
        let x = compute_clamped_x(100.0, 0.0, 400.0, 460.0);
        assert!((x - EDGE_MARGIN).abs() < f64::EPSILON);
    }
}
