mod audio;
mod launch_agent;
mod state;
mod tray;
mod ui;

use audio::AudioEngine;
use gpui::{
    App, AppContext, Application, Bounds, Entity, KeyBinding, Point, Size,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, px,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tray::TrayCommand;
use ui::popover::TrayPopoverView;

const POPOVER_WIDTH: f64 = 460.0;
const POPOVER_HEIGHT: f64 = 560.0;

fn main() {
    Application::new()
        .with_assets(ui::icons::LucideAssets)
        .run(|cx: &mut App| {
            tray::set_accessory_activation_policy();

            let engine = Arc::new(Mutex::new(AudioEngine::new()));
            let (tray_handle, tray_rx) = match tray::init_tray() {
                Ok(res) => res,
                Err(err) => {
                    eprintln!("Failed to initialize tray icon: {err}");
                    cx.quit();
                    return;
                }
            };

            cx.bind_keys([KeyBinding::new("escape", ClosePopover, None)]);

            let mut active_window: Option<WindowHandle<TrayPopoverView>> = None;

            cx.spawn(async move |cx| {
                // Keep tray_handle alive for the lifetime of the app.
                let _tray = tray_handle;

                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;

                    while let Ok(cmd) = tray_rx.try_recv() {
                        match cmd {
                            TrayCommand::TogglePopover {
                                anchor_x,
                                anchor_y,
                                width,
                                height,
                            } => {
                                let is_open = active_window
                                    .as_ref()
                                    .map(|w| w.update(cx, |_, _, _| {}).is_ok())
                                    .unwrap_or(false);

                                if is_open {
                                    if let Some(handle) = active_window.take() {
                                        let _ = handle.update(cx, |_, window, _| {
                                            window.remove_window();
                                        });
                                    }
                                } else {
                                    let engine_clone = Arc::clone(&engine);
                                    let opened = cx.update(|cx| {
                                        let (mouse_x, disp_ox, disp_w, menu_bar_h, top_y_appkit) =
                                            objc2::MainThreadMarker::new()
                                                .and_then(|mtm| {
                                                    let mouse_loc =
                                                        objc2_app_kit::NSEvent::mouseLocation();
                                                    let screens =
                                                        objc2_app_kit::NSScreen::screens(mtm);
                                                    screens
                                                        .iter()
                                                        .find(|s| {
                                                            let f = s.frame();
                                                            mouse_loc.x >= f.origin.x
                                                                && mouse_loc.x
                                                                    < f.origin.x + f.size.width
                                                                && mouse_loc.y >= f.origin.y
                                                                && mouse_loc.y
                                                                    < f.origin.y + f.size.height
                                                        })
                                                        .map(|s| {
                                                            let f = s.frame();
                                                            let vf = s.visibleFrame();
                                                            let mb = (f.origin.y + f.size.height)
                                                                - (vf.origin.y + vf.size.height);
                                                            let top_y =
                                                                vf.origin.y + vf.size.height - 6.0;
                                                            (
                                                                mouse_loc.x,
                                                                f.origin.x,
                                                                f.size.width,
                                                                mb.max(24.0),
                                                                top_y,
                                                            )
                                                        })
                                                })
                                                .unwrap_or_else(|| {
                                                    let _ = (anchor_x, anchor_y, width, height);
                                                    (1300.0, 0.0, 1440.0, 28.0, 860.0)
                                                });

                                        let target_display = cx
                                            .displays()
                                            .into_iter()
                                            .find(|d| {
                                                let b = d.bounds();
                                                let ox = f64::from(b.origin.x);
                                                let w = f64::from(b.size.width);
                                                mouse_x >= ox && mouse_x < ox + w
                                            })
                                            .or_else(|| cx.primary_display());

                                        let display_id = target_display.map(|d| d.id());

                                        let rel_mouse_x = mouse_x - disp_ox;
                                        let x = (rel_mouse_x - POPOVER_WIDTH / 2.0)
                                            .clamp(8.0, (disp_w - POPOVER_WIDTH - 8.0).max(8.0));

                                        // GPUI's macOS WindowOptions passes `window_rect.origin` to
                                        // `NSWindow::setFrameTopLeftPoint_`, so `bounds.origin.y` is the
                                        // distance from the top of the screen to the TOP of the popover.
                                        let y = menu_bar_h + 6.0;

                                        let bounds = Bounds::new(
                                            Point::new(px(x as f32), px(y as f32)),
                                            Size::new(
                                                px(POPOVER_WIDTH as f32),
                                                px(POPOVER_HEIGHT as f32),
                                            ),
                                        );

                                        let options = WindowOptions {
                                            kind: WindowKind::PopUp,
                                            window_bounds: Some(WindowBounds::Windowed(bounds)),
                                            display_id,
                                            titlebar: None,
                                            focus: true,
                                            show: true,
                                            is_movable: false,
                                            is_resizable: false,
                                            window_background: WindowBackgroundAppearance::Blurred,
                                            ..Default::default()
                                        };

                                        let res = cx
                                            .open_window(options, |window, cx| {
                                                let view: Entity<TrayPopoverView> = cx.new(|cx| {
                                                    cx.observe_window_activation(
                                                        window,
                                                        |view: &mut TrayPopoverView, window, _| {
                                                            if !window.is_window_active()
                                                                && !view.is_pinned()
                                                            {
                                                                window.remove_window();
                                                            }
                                                        },
                                                    )
                                                    .detach();

                                                    TrayPopoverView::new(engine_clone, cx)
                                                });

                                                view
                                            })
                                            .ok();

                                        // Explicitly pin the NSPanel's top-left point in AppKit coordinates
                                        // right below the menu bar under the tray icon.
                                        if let Some(mtm) = objc2::MainThreadMarker::new() {
                                            let app =
                                                objc2_app_kit::NSApplication::sharedApplication(
                                                    mtm,
                                                );
                                            let ns_x = disp_ox + x;
                                            for win in app.windows().iter() {
                                                let frame = win.frame();
                                                if (frame.size.width - POPOVER_WIDTH).abs() < 2.0
                                                    && (frame.size.height - POPOVER_HEIGHT).abs()
                                                        < 2.0
                                                {
                                                    win.setFrameTopLeftPoint(
                                                        objc2_foundation::NSPoint::new(
                                                            ns_x,
                                                            top_y_appkit,
                                                        ),
                                                    );
                                                }
                                            }
                                        }

                                        res
                                    });

                                    if let Ok(handle) = opened {
                                        active_window = handle;
                                    }
                                }
                            }
                            TrayCommand::Quit => {
                                let _ = cx.update(|cx| cx.quit());
                                return;
                            }
                        }
                    }
                }
            })
            .detach();
        });
}

gpui::actions!(volume_mixer, [ClosePopover]);
