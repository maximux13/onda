mod audio;
mod hotkeys;
mod launch_agent;
mod platform;
mod state;
mod storage;
mod tray;
mod ui;
mod updater;

use audio::AudioEngine;
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use gpui::{
    App, AppContext, AsyncApp, Application, Entity, KeyBinding, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use hotkeys::HotkeyManager;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tray::TrayCommand;
use ui::popover::TrayPopoverView;
use updater::UpdateController;

use crate::state::{ShortcutAction, WindowStyle};
use crate::ui::ThemePalette;

pub(crate) const POPOVER_WIDTH: f64 = 460.0;
pub(crate) const POPOVER_HEIGHT: f64 = 560.0;
pub(crate) const POPOVER_ADVANCED_HEIGHT: f64 = 700.0;

fn main() {
    Application::new()
        .with_assets(ui::icons::LucideAssets)
        .run(|cx: &mut App| {
            tray::set_accessory_activation_policy();

            let engine = Arc::new(Mutex::new(AudioEngine::new()));
            let updater = Arc::new(Mutex::new(UpdateController::new(
                updater::UpdateConfig::from_env(),
            )));
            let initial_shortcuts = engine
                .lock()
                .map(|mut e| e.snapshot().shortcuts)
                .unwrap_or_default();
            let hotkeys = Arc::new(Mutex::new(HotkeyManager::new(&initial_shortcuts)));

            let updater_bg = Arc::clone(&updater);
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                loop {
                    updater::UpdateController::trigger_background_check(Arc::clone(&updater_bg));
                    cx.background_executor()
                        .timer(Duration::from_secs(6 * 3600))
                        .await;
                }
            })
            .detach();
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
                // Keep tray_handle and hotkeys alive for the lifetime of the app.
                let _tray = tray_handle;
                let hotkey_rx = GlobalHotKeyEvent::receiver();

                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;

                    while let Ok(cmd) = tray_rx.try_recv() {
                        match cmd {
                            TrayCommand::TogglePopover { .. } => {
                                toggle_popover_window(
                                    &mut active_window,
                                    &engine,
                                    &updater,
                                    &hotkeys,
                                    cx,
                                );
                            }
                            TrayCommand::Quit => {
                                let _ = cx.update(|cx| cx.quit());
                                return;
                            }
                        }
                    }

                    while let Ok(event) = hotkey_rx.try_recv() {
                        if event.state != HotKeyState::Pressed {
                            continue;
                        }
                        let action = hotkeys
                            .lock()
                            .ok()
                            .and_then(|mgr| mgr.action_for_id(event.id));
                        if let Some(action) = action {
                            match action {
                                ShortcutAction::TogglePopover => {
                                    toggle_popover_window(
                                        &mut active_window,
                                        &engine,
                                        &updater,
                                        &hotkeys,
                                        cx,
                                    );
                                }
                                ShortcutAction::MuteActiveApp => {
                                    let frontmost = platform::frontmost_app_bundle_id();
                                    if let Ok(mut e) = engine.lock() {
                                        e.toggle_mute_active_app(frontmost.as_deref());
                                    }
                                }
                                ShortcutAction::CycleOutputDevice => {
                                    if let Ok(mut e) = engine.lock() {
                                        e.cycle_default_output_device();
                                    }
                                }
                            }
                        }
                    }
                }
            })
            .detach();
        });
}

fn toggle_popover_window(
    active_window: &mut Option<WindowHandle<TrayPopoverView>>,
    engine: &Arc<Mutex<AudioEngine>>,
    updater: &Arc<Mutex<UpdateController>>,
    hotkeys: &Arc<Mutex<HotkeyManager>>,
    cx: &mut AsyncApp,
) {
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
        return;
    }

    let (initial_style, initial_theme, initial_advanced) = engine
        .lock()
        .map(|mut e| {
            let snap = e.snapshot();
            (snap.window_style, snap.theme, snap.advanced_mode)
        })
        .unwrap_or_default();
    let initial_height = if initial_advanced {
        POPOVER_ADVANCED_HEIGHT
    } else {
        POPOVER_HEIGHT
    };
    let is_transparent = initial_style == WindowStyle::Transparent;
    let is_dark = ThemePalette::resolve_is_dark(initial_theme);

    let engine_clone = Arc::clone(engine);
    let updater_clone = Arc::clone(updater);
    let hotkeys_clone = Arc::clone(hotkeys);
    let opened = cx.update(|cx| {
        let placement = platform::compute_popover_placement(cx, POPOVER_WIDTH, initial_height);

        let options = WindowOptions {
            kind: WindowKind::PopUp,
            window_bounds: Some(WindowBounds::Windowed(placement.bounds)),
            display_id: placement.display_id,
            titlebar: None,
            focus: true,
            show: true,
            is_movable: false,
            is_resizable: false,
            window_background: if is_transparent {
                WindowBackgroundAppearance::Blurred
            } else {
                WindowBackgroundAppearance::Opaque
            },
            ..Default::default()
        };

        let res = cx
            .open_window(options, |window, cx| {
                let view: Entity<TrayPopoverView> = cx.new(|cx| {
                    cx.observe_window_activation(window, |view: &mut TrayPopoverView, window, _| {
                        if !window.is_window_active() && !view.is_pinned() {
                            window.remove_window();
                        }
                    })
                    .detach();

                    TrayPopoverView::new(engine_clone, updater_clone, hotkeys_clone, cx)
                });

                view
            })
            .ok();

        let (start_y, start_alpha, start_scale) =
            platform::popover_slide_down_step(placement.top_y_appkit, 0.0);
        platform::configure_popover_glass(POPOVER_WIDTH, initial_height, is_transparent, is_dark);
        platform::set_popover_window_state(
            POPOVER_WIDTH,
            initial_height,
            placement.ns_x,
            start_y,
            start_alpha,
            start_scale,
        );

        if res.is_some() {
            let ns_x = placement.ns_x;
            let target_y = placement.top_y_appkit;
            cx.spawn(async move |cx| {
                let start = std::time::Instant::now();
                let duration = Duration::from_millis(140);
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(8))
                        .await;
                    let progress =
                        (start.elapsed().as_secs_f32() / duration.as_secs_f32()).min(1.0);
                    let (y, alpha, scale) =
                        platform::popover_slide_down_step(target_y, progress);
                    let _ = cx.update(|_| {
                        platform::set_popover_window_state(
                            POPOVER_WIDTH,
                            initial_height,
                            ns_x,
                            y,
                            alpha,
                            scale,
                        );
                    });
                    if progress >= 1.0 {
                        break;
                    }
                }
            })
            .detach();
        }

        res
    });

    if let Ok(handle) = opened {
        *active_window = handle;
    }
}

gpui::actions!(volume_mixer, [ClosePopover]);
