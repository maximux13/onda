use gpui::prelude::*;
use gpui::{Context, FontWeight, SharedString, div, px, rgb, rgba};

use crate::state::OutputDeviceInfo;
use crate::ui::ThemePalette;
use crate::ui::icons::{IconName, lucide_svg};
use crate::ui::slider::VolumeSlider;

use super::TrayPopoverView;

impl TrayPopoverView {
    pub(super) fn set_device_volume(
        &mut self,
        device_id: u32,
        volume: f32,
        cx: &mut Context<Self>,
    ) {
        let clamped = volume.clamp(0.0, 1.0);
        if let Some(dev) = self.snapshot.devices.iter_mut().find(|d| d.id == device_id) {
            dev.volume = clamped;
            if clamped > 0.001 && dev.muted {
                dev.muted = false;
            }
        }
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_device_volume(device_id, clamped);
        }
        cx.notify();
    }

    pub(super) fn toggle_device_mute(&mut self, device_id: u32, cx: &mut Context<Self>) {
        let mut next_muted = false;
        if let Some(dev) = self.snapshot.devices.iter_mut().find(|d| d.id == device_id) {
            next_muted = !dev.muted;
            dev.muted = next_muted;
        }
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_device_mute(device_id, next_muted);
        }
        cx.notify();
    }

    pub(super) fn set_default_output_device(&mut self, device_id: u32, cx: &mut Context<Self>) {
        for dev in &mut self.snapshot.devices {
            dev.is_default = dev.id == device_id;
            if dev.is_default {
                self.snapshot.default_output_name = dev.name.clone();
            }
        }
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_default_output_device(device_id);
            self.snapshot = engine.snapshot();
        }
        cx.notify();
    }

    pub(super) fn render_device_row(
        &self,
        device: &OutputDeviceInfo,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let dev_id = device.id;
        let is_default = device.is_default;
        let muted = device.muted;
        let volume = device.volume;
        let pct = if muted {
            0
        } else {
            (volume * 100.0).round() as i32
        };
        let theme = self.snapshot.theme;
        let entity_slider = cx.entity();

        let (speaker_icon, speaker_color) = if muted {
            (IconName::VOLUME_X, palette.mute_red)
        } else {
            (IconName::VOLUME_2, palette.text_primary)
        };
        let hp_color = if is_default {
            rgb(0xffffff)
        } else {
            palette.text_primary
        };

        div()
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
            .child(
                div()
                    .id(SharedString::from(format!("dev-select-{dev_id}")))
                    .w(px(150.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.set_default_output_device(dev_id, cx);
                    }))
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
                            .child(lucide_svg(IconName::HEADPHONES, 13.0, hp_color)),
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
            .child(div().flex_1().min_w(px(90.0)).child(VolumeSlider::new(
                SharedString::from(format!("dev-slider-{dev_id}")),
                volume,
                1.0,
                muted,
                0x2582ff,
                theme,
                move |val, _window, cx| {
                    entity_slider.update(cx, |this, cx| {
                        this.set_device_volume(dev_id, val, cx);
                    });
                },
            )))
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .w(px(42.0))
                            .py(px(2.0))
                            .rounded(px(7.0))
                            .bg(palette.pill_bg)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.text_primary)
                            .child(format!("{pct}%")),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("dev-mute-{dev_id}")))
                            .size(px(26.0))
                            .rounded(px(7.0))
                            .hover(move |s| s.bg(palette.pill_hover_bg))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.toggle_device_mute(dev_id, cx);
                            }))
                            .child(lucide_svg(speaker_icon, 14.0, speaker_color)),
                    ),
            )
    }
}
