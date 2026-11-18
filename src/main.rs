mod audio;
mod launch_agent;
mod platform;
mod state;
mod storage;
mod tray;
mod ui;

use audio::AudioEngine;
use gpui::{
    App, AppContext, Application, Entity, KeyBinding, WindowBackgroundAppearance, WindowBounds,
    WindowHandle, WindowKind, WindowOptions,
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
                            TrayCommand::TogglePopover { .. } => {
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
                                        let placement = platform::compute_popover_placement(
                                            cx,
                                            POPOVER_WIDTH,
                                            POPOVER_HEIGHT,
                                        );

                                        let options = WindowOptions {
                                            kind: WindowKind::PopUp,
                                            window_bounds: Some(WindowBounds::Windowed(
                                                placement.bounds,
                                            )),
                                            display_id: placement.display_id,
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

                                        platform::pin_popover_top_left(
                                            POPOVER_WIDTH,
                                            POPOVER_HEIGHT,
                                            placement.ns_x,
                                            placement.top_y_appkit,
                                        );

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
