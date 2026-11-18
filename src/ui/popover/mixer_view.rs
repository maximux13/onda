use gpui::prelude::*;
use gpui::{Context, FontWeight, SharedString, div, px, rgb, rgba};

use crate::ui::ThemePalette;
use crate::ui::icons::{IconName, lucide_svg};
use crate::ui::slider::VolumeSlider;
use crate::updater::UpdateState;

use super::TrayPopoverView;

impl TrayPopoverView {
    /// Renders the top header for View 1 (`Onda`).
    fn render_mixer_header(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let pin_color = if self.is_pinned {
            rgb(0xffffff)
        } else {
            palette.text_primary
        };

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(10.0))
            .pb(px(6.0))
            // Left: Waveform icon badge + Onda title & subtitle
            .child(self.render_header_title(
                IconName::AUDIO_LINES,
                "Onda",
                "Control audio for each application",
                palette,
            ))
            // Right: Circular Settings button + Pin button
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_shrink_0()
                    .child(
                        div()
                            .id("mixer-open-settings-btn")
                            .relative()
                            .size(px(32.0))
                            .rounded_full()
                            .bg(palette.pill_bg)
                            .border_1()
                            .border_color(palette.card_border)
                            .hover(move |s| s.bg(palette.pill_hover_bg))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                if !this.show_settings {
                                    this.show_settings = true;
                                    this.launch_at_login =
                                        crate::launch_agent::is_launch_at_login_enabled();
                                    this.bump_view_transition();
                                    cx.notify();
                                }
                            }))
                            .child(lucide_svg(IconName::SETTINGS, 16.0, palette.text_primary))
                            .when(
                                matches!(
                                    self.update_state(),
                                    UpdateState::Available(_) | UpdateState::ReadyToRestart { .. }
                                ),
                                |el| {
                                    el.child(
                                        div()
                                            .absolute()
                                            .top(px(5.0))
                                            .right(px(5.0))
                                            .size(px(6.0))
                                            .rounded_full()
                                            .bg(palette.accent_blue),
                                    )
                                },
                            ),
                    )
                    .child(
                        div()
                            .id("mixer-pin-btn")
                            .size(px(32.0))
                            .rounded_full()
                            .bg(if self.is_pinned {
                                palette.accent_blue
                            } else {
                                palette.pill_bg
                            })
                            .border_1()
                            .border_color(if self.is_pinned {
                                palette.accent_blue
                            } else {
                                palette.card_border
                            })
                            .hover(move |s| {
                                if self.is_pinned {
                                    s.bg(palette.accent_blue)
                                } else {
                                    s.bg(palette.pill_hover_bg)
                                }
                            })
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.is_pinned = !this.is_pinned;
                                cx.notify();
                            }))
                            .child(lucide_svg(IconName::PIN, 15.0, pin_color)),
                    ),
            )
    }

    /// Renders a floating output device & master volume popover above the footer `Output: ...` button.
    fn render_output_picker_popover(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let surface_bg = if palette.is_dark {
            rgba(0x1b2236f5)
        } else {
            rgba(0xf7f9fdf7)
        };

        let mut devices_list = div()
            .id("output-picker-devices-scroll")
            .w_full()
            .max_h(px(260.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(6.0));

        for device in &self.snapshot.devices {
            devices_list = devices_list.child(self.render_device_row(device, palette, cx));
        }

        div()
            .id("output-picker-popover")
            .w_full()
            .p(px(10.0))
            .rounded(px(12.0))
            .bg(surface_bg)
            .border_1()
            .border_color(palette.panel_border)
            .shadow_lg()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(2.0))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(palette.text_secondary)
                            .child("Output Devices & Master Volume"),
                    ),
            )
            .child(devices_list)
    }

    /// Renders the bottom footer bar matching the `Onda` mockup.
    fn render_mixer_footer(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let sparkles_color = if self.advanced_mode {
            rgb(0xffffff)
        } else {
            palette.text_primary
        };

        let out_name = if self.snapshot.default_output_name.is_empty() {
            "MacBook Pro Speakers"
        } else {
            self.snapshot.default_output_name.as_str()
        };

        div()
            .w_full()
            .pt(px(8.0))
            .border_t_1()
            .border_color(palette.divider)
            .flex()
            .items_center()
            .justify_between()
            .gap(px(10.0))
            // Left: Output: <default_output_name> pill dropdown button
            .child(
                div()
                    .id("footer-output-picker-btn")
                    .flex_1()
                    .min_w_0()
                    .px(px(12.0))
                    .py(px(7.0))
                    .rounded(px(10.0))
                    .bg(if self.show_output_picker {
                        palette.row_selected_bg
                    } else {
                        palette.card_bg
                    })
                    .border_1()
                    .border_color(if self.show_output_picker {
                        palette.row_selected_border
                    } else {
                        palette.card_border
                    })
                    .hover(move |s| {
                        if self.show_output_picker {
                            s.bg(palette.row_selected_bg)
                        } else {
                            s.bg(palette.pill_hover_bg)
                        }
                    })
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.show_output_picker = !this.show_output_picker;
                        if this.show_output_picker {
                            this.show_input_picker = false;
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .min_w_0()
                            .child(lucide_svg(IconName::HEADPHONES, 15.0, palette.text_primary))
                            .child(
                                div()
                                    .truncate()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(palette.text_primary)
                                    .child(format!("Output: {out_name}")),
                            ),
                    )
                    .child(lucide_svg(
                        IconName::CHEVRON_DOWN,
                        12.0,
                        palette.text_secondary,
                    )),
            )
            // Right: Advanced Mode pill button
            .child(
                div()
                    .id("footer-advanced-mode-btn")
                    .flex_shrink_0()
                    .px(px(12.0))
                    .py(px(7.0))
                    .rounded(px(10.0))
                    .bg(if self.advanced_mode {
                        palette.accent_blue
                    } else {
                        palette.card_bg
                    })
                    .border_1()
                    .border_color(if self.advanced_mode {
                        palette.accent_blue
                    } else {
                        palette.card_border
                    })
                    .hover(move |s| {
                        if self.advanced_mode {
                            s.bg(palette.accent_blue)
                        } else {
                            s.bg(palette.pill_hover_bg)
                        }
                    })
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        let old_height = this.current_popover_height();
                        this.advanced_mode = !this.advanced_mode;
                        if let Ok(mut engine) = this.engine.lock() {
                            engine.set_advanced_mode(this.advanced_mode);
                        }
                        let new_height = this.current_popover_height();
                        if (old_height - new_height).abs() > 0.5 {
                            cx.spawn(async move |_, cx| {
                                cx.background_executor()
                                    .timer(std::time::Duration::from_millis(1))
                                    .await;
                                crate::platform::resize_popover_window(
                                    crate::POPOVER_WIDTH,
                                    old_height,
                                    crate::POPOVER_WIDTH,
                                    new_height,
                                );
                            })
                            .detach();
                        }
                        cx.notify();
                    }))
                    .child(lucide_svg(IconName::SPARKLES, 14.0, sparkles_color))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if self.advanced_mode {
                                rgb(0xffffff)
                            } else {
                                palette.text_primary
                            })
                            .child("Advanced Mode"),
                    ),
            )
    }

    fn render_apps_filter_bar(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let show_all = self.snapshot.show_all_apps;
        div()
            .w_full()
            .px(px(2.0))
            .py(px(2.0))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(palette.text_secondary)
                    .child("Show all open apps"),
            )
            .child(self.render_ios_toggle(
                "mixer-show-all-apps-toggle",
                show_all,
                palette,
                |this, _event, _window, cx| {
                    let next = !this.snapshot.show_all_apps;
                    if let Ok(mut engine) = this.engine.lock() {
                        engine.set_show_all_apps(next);
                        this.snapshot = engine.snapshot();
                    }
                    this.sync_icon_cache();
                    cx.notify();
                },
                cx,
            ))
    }

    fn set_input_device_volume(&mut self, device_id: u32, volume: f32, cx: &mut Context<Self>) {
        let clamped = volume.clamp(0.0, 1.0);
        let mut should_unmute = false;
        if let Some(dev) = self
            .snapshot
            .input_devices
            .iter_mut()
            .find(|d| d.id == device_id)
        {
            dev.volume = clamped;
            if clamped > 0.001 && dev.muted {
                dev.muted = false;
                should_unmute = true;
            }
        }
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_input_device_volume(device_id, clamped);
            if should_unmute {
                engine.set_input_device_mute(device_id, false);
            }
        }
        cx.notify();
    }

    fn set_input_device_mute(&mut self, device_id: u32, muted: bool, cx: &mut Context<Self>) {
        if let Some(dev) = self
            .snapshot
            .input_devices
            .iter_mut()
            .find(|d| d.id == device_id)
        {
            dev.muted = muted;
        }
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_input_device_mute(device_id, muted);
        }
        cx.notify();
    }

    fn set_default_input_device(&mut self, device_id: u32, cx: &mut Context<Self>) {
        for dev in &mut self.snapshot.input_devices {
            dev.is_default = dev.id == device_id;
        }
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_default_input_device(device_id);
            self.snapshot = engine.snapshot();
        }
        self.show_input_picker = false;
        cx.notify();
    }

    /// Renders the compact top-bar microphone gain, mute, and input device selector strip.
    fn render_mic_control_bar(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active_mic = self
            .snapshot
            .input_devices
            .iter()
            .find(|d| d.is_default)
            .or_else(|| self.snapshot.input_devices.first());

        let Some(dev) = active_mic else {
            return div()
                .w_full()
                .py(px(4.0))
                .px(px(2.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .size(px(28.0))
                        .rounded_full()
                        .bg(palette.pill_bg)
                        .border_1()
                        .border_color(palette.card_border)
                        .flex()
                        .items_center()
                        .justify_center()
                        .flex_shrink_0()
                        .child(lucide_svg(IconName::MIC_OFF, 14.0, palette.text_secondary)),
                )
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(palette.text_secondary)
                        .child("No input device"),
                );
        };

        let dev_id = dev.id;
        let dev_name = dev.name.clone();
        let dev_volume = dev.volume;
        let dev_muted = dev.muted;
        let pct = (dev_volume * 100.0).round() as u32;
        let theme = self.snapshot.theme;
        let entity_slider = cx.entity();

        let (mic_icon, mic_color) = if dev_muted {
            (IconName::MIC_OFF, palette.mute_red)
        } else {
            (IconName::MIC, palette.text_primary)
        };

        div()
            .w_full()
            .py(px(2.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            // Left: Mic mute/unmute circular icon button
            .child(
                div()
                    .id("mic-mute-btn")
                    .size(px(28.0))
                    .rounded_full()
                    .bg(palette.pill_bg)
                    .border_1()
                    .border_color(palette.card_border)
                    .hover(move |s| s.bg(palette.pill_hover_bg))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.set_input_device_mute(dev_id, !dev_muted, cx);
                    }))
                    .child(lucide_svg(mic_icon, 14.0, mic_color)),
            )
            // Center-left: Clickable input device name pill
            .child(
                div()
                    .id("mic-input-picker-btn")
                    .max_w(px(165.0))
                    .min_w(px(0.0))
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(px(8.0))
                    .bg(if self.show_input_picker {
                        palette.row_selected_bg
                    } else {
                        palette.card_bg
                    })
                    .border_1()
                    .border_color(if self.show_input_picker {
                        palette.row_selected_border
                    } else {
                        palette.card_border
                    })
                    .hover(move |s| {
                        if self.show_input_picker {
                            s.bg(palette.row_selected_bg)
                        } else {
                            s.bg(palette.pill_hover_bg)
                        }
                    })
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(6.0))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.show_input_picker = !this.show_input_picker;
                        if this.show_input_picker {
                            this.show_output_picker = false;
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.text_primary)
                            .child(dev_name),
                    )
                    .child(lucide_svg(
                        IconName::CHEVRON_DOWN,
                        12.0,
                        palette.text_secondary,
                    )),
            )
            // Right: Horizontal gain slider + percentage readout
            .child(div().flex_1().min_w(px(80.0)).child(VolumeSlider::new(
                "slider-mic-gain",
                dev_volume,
                1.0,
                dev_muted,
                0x2582ff,
                theme,
                move |val, _window, cx| {
                    entity_slider.update(cx, |this, cx| {
                        this.set_input_device_volume(dev_id, val, cx);
                    });
                },
            )))
            .child(
                div()
                    .w(px(40.0))
                    .py(px(2.0))
                    .rounded(px(7.0))
                    .bg(palette.pill_bg)
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(palette.text_primary)
                    .child(format!("{pct}%")),
            )
    }

    /// Renders a floating input device picker popover below the top-bar mic control strip.
    fn render_input_picker_popover(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let surface_bg = if palette.is_dark {
            rgba(0x1b2236f5)
        } else {
            rgba(0xf7f9fdf7)
        };

        let mut devices_list = div()
            .id("input-picker-devices-scroll")
            .w_full()
            .max_h(px(220.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(4.0));

        for device in &self.snapshot.input_devices {
            let dev_id = device.id;
            let is_default = device.is_default;
            let icon_color = if is_default {
                rgb(0xffffff)
            } else {
                palette.text_primary
            };

            devices_list = devices_list.child(
                div()
                    .id(SharedString::from(format!("input-dev-select-{dev_id}")))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(if is_default {
                        palette.row_selected_border
                    } else {
                        rgba(0x00000000)
                    })
                    .bg(if is_default {
                        palette.row_selected_bg
                    } else {
                        rgba(0x00000000)
                    })
                    .hover(move |s| {
                        if is_default {
                            s.bg(palette.row_selected_bg)
                        } else {
                            s.bg(palette.row_hover_bg)
                        }
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.set_default_input_device(dev_id, cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .size(px(24.0))
                                    .rounded_full()
                                    .bg(if is_default {
                                        palette.accent_blue
                                    } else {
                                        palette.pill_bg
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .flex_shrink_0()
                                    .child(lucide_svg(IconName::MIC, 13.0, icon_color)),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_xs()
                                    .font_weight(if is_default {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::MEDIUM
                                    })
                                    .text_color(palette.text_primary)
                                    .child(device.name.clone()),
                            ),
                    )
                    .when(is_default, |el| {
                        el.child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(palette.accent_blue)
                                .flex_shrink_0()
                                .child("Active"),
                        )
                    }),
            );
        }

        div()
            .id("input-picker-popover")
            .w_full()
            .p(px(10.0))
            .rounded(px(12.0))
            .bg(surface_bg)
            .border_1()
            .border_color(palette.panel_border)
            .shadow_lg()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(2.0))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(palette.text_secondary)
                            .child("Input Devices"),
                    ),
            )
            .child(devices_list)
    }

    /// Renders View 1: Main `Onda` view.
    pub(super) fn render_mixer_view(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut apps_list = div().w_full().flex().flex_col().gap(px(4.0));

        if self.snapshot.apps.is_empty() {
            let empty_msg = if self.snapshot.show_all_apps {
                "No open applications found"
            } else {
                "No applications are currently playing audio"
            };
            apps_list = apps_list.child(
                div()
                    .w_full()
                    .py(px(24.0))
                    .rounded(px(12.0))
                    .bg(palette.card_bg)
                    .border_1()
                    .border_color(palette.card_border)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(palette.text_secondary)
                    .child(empty_msg),
            );
        } else {
            let has_active = self.snapshot.apps.iter().any(|r| r.info.is_running_output);
            let mut rendered_idle_header = false;
            for row in &self.snapshot.apps {
                if has_active && !row.info.is_running_output && !rendered_idle_header {
                    rendered_idle_header = true;
                    apps_list = apps_list.child(
                        div()
                            .w_full()
                            .px(px(2.0))
                            .pt(px(6.0))
                            .pb(px(2.0))
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.text_secondary)
                            .child("Other open apps"),
                    );
                }
                apps_list = apps_list.child(self.render_app_row(row, palette, cx));
            }
        }

        let scroll_area = div()
            .id("mixer-apps-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(apps_list);

        let mut root = div()
            .relative()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(self.render_mixer_header(palette, cx))
            .child(div().w_full().h(px(1.0)).bg(palette.divider))
            .child(self.render_mic_control_bar(palette, cx))
            .child(div().w_full().h(px(1.0)).bg(palette.divider))
            .child(self.render_apps_filter_bar(palette, cx))
            .child(scroll_area);

        if self.advanced_mode {
            root = root
                .child(div().w_full().h(px(1.0)).bg(palette.divider))
                .child(self.render_equalizer_section(palette, cx));
        }

        root = root.child(self.render_mixer_footer(palette, cx));

        if self.show_input_picker {
            root = root
                .child(
                    div()
                        .id("input-picker-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.show_input_picker = false;
                            cx.notify();
                            cx.stop_propagation();
                        })),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(88.0))
                        .left(px(0.0))
                        .right(px(0.0))
                        .child(self.render_input_picker_popover(palette, cx)),
                );
        }

        if self.show_output_picker {
            root = root
                .child(
                    div()
                        .id("output-picker-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .bottom(px(46.0))
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.show_output_picker = false;
                            cx.notify();
                            cx.stop_propagation();
                        })),
                )
                .child(
                    div()
                        .absolute()
                        .bottom(px(48.0))
                        .left(px(0.0))
                        .right(px(0.0))
                        .child(self.render_output_picker_popover(palette, cx)),
                );
        }

        root
    }
}
