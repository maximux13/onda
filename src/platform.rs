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

#[repr(C)]
#[derive(Clone, Copy)]
struct CaTransform3D {
    m11: f64,
    m12: f64,
    m13: f64,
    m14: f64,
    m21: f64,
    m22: f64,
    m23: f64,
    m24: f64,
    m31: f64,
    m32: f64,
    m33: f64,
    m34: f64,
    m41: f64,
    m42: f64,
    m43: f64,
    m44: f64,
}

// SAFETY: `CaTransform3D` is `#[repr(C)]` with 16 `f64` (`CGFloat` on 64-bit macOS) fields,
// matching Objective-C's `CATransform3D` ABI layout and encoding.
unsafe impl objc2::Encode for CaTransform3D {
    const ENCODING: objc2::Encoding =
        objc2::Encoding::Struct("CATransform3D", &[f64::ENCODING; 16]);
}

impl CaTransform3D {
    fn scale_from_top_center(scale: f64, width: f64, height: f64) -> Self {
        let tx = width * (1.0 - scale) * 0.5;
        let ty = height * (1.0 - scale);
        Self {
            m11: scale,
            m12: 0.0,
            m13: 0.0,
            m14: 0.0,
            m21: 0.0,
            m22: scale,
            m23: 0.0,
            m24: 0.0,
            m31: 0.0,
            m32: 0.0,
            m33: 1.0,
            m34: 0.0,
            m41: tx,
            m42: ty,
            m43: 0.0,
            m44: 1.0,
        }
    }
}

/// Updates any open AppKit window matching `(width, height)` to top-left
/// `(ns_x, top_y_appkit)` in AppKit coordinates, opacity `alpha` (`0.0..=1.0`),
/// and Core Animation layer scale `scale` anchored at top-center.
pub fn set_popover_window_state(
    width: f64,
    height: f64,
    ns_x: f64,
    top_y_appkit: f64,
    alpha: f64,
    scale: f64,
) {
    if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApplication::sharedApplication(mtm);
        for win in app.windows().iter() {
            let frame = win.frame();
            if (frame.size.width - width).abs() < 2.0 && (frame.size.height - height).abs() < 2.0 {
                win.setFrameTopLeftPoint(NSPoint::new(ns_x, top_y_appkit));
                win.setAlphaValue(alpha.clamp(0.0, 1.0));
                if let Some(content_view) = win.contentView() {
                    content_view.setWantsLayer(true);
                    if let Some(layer) = content_view.layer() {
                        let transform = CaTransform3D::scale_from_top_center(
                            scale.clamp(0.5, 1.0),
                            width,
                            height,
                        );
                        // SAFETY: `layer` is a valid `CALayer` on the main thread and `setTransform:`
                        // takes a `CATransform3D` (`16 x f64`) by value.
                        unsafe {
                            let _: () = objc2::msg_send![&*layer, setTransform: transform];
                        }
                    }
                }
            }
        }
    }
}

/// Computes `(top_y_appkit, alpha, scale)` for a snappy ease-out-quint window
/// slide-down from `42px` above `target_top_y` with a `0.92 -> 1.0` scale-up.
pub fn popover_slide_down_step(target_top_y: f64, progress: f32) -> (f64, f64, f64) {
    let t = f64::from(progress.clamp(0.0, 1.0));
    let eased = 1.0 - (1.0 - t).powi(5);
    let y = target_top_y + (1.0 - eased) * 42.0;
    let scale = 0.92 + 0.08 * eased;
    (y, eased, scale)
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
        assert_eq!(compute_clamped_x(100.0, 0.0, 400.0, 460.0), EDGE_MARGIN);
    }

    #[test]
    fn popover_slide_down_step_should_start_above_and_transparent_when_progress_zero() {
        let (y, alpha, scale) = popover_slide_down_step(800.0, 0.0);
        assert_eq!(y, 842.0);
        assert_eq!(alpha, 0.0);
        assert!((scale - 0.92).abs() < f64::EPSILON);
    }

    #[test]
    fn popover_slide_down_step_should_settle_at_target_when_progress_one() {
        let (y, alpha, scale) = popover_slide_down_step(800.0, 1.0);
        assert_eq!(y, 800.0);
        assert_eq!(alpha, 1.0);
        assert!((scale - 1.0).abs() < f64::EPSILON);
    }
}
