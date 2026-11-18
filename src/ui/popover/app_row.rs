use std::sync::Arc;

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, div, img, px, rgb, rgba};

use crate::state::AppRowState;
use crate::ui::ThemePalette;
use crate::ui::icons::{IconName, lucide_svg};
use crate::ui::slider::VolumeSlider;

use super::TrayPopoverView;

impl TrayPopoverView {
    pub(super) fn set_app_volume(&mut self, bundle_id: &str, volume: f32, cx: &mut Context<Self>) {
        let max_limit = self.snapshot.max_volume_limit.clamp(1.0, 2.0);
        let mut clamped = volume.clamp(0.0, max_limit);
        if let Some(row) = self
            .snapshot
            .apps
            .iter_mut()
            .find(|r| r.info.bundle_id == bundle_id)
        {
            let max = if row.config.boost_enabled {
                max_limit
            } else {
                1.0
            };
            clamped = volume.clamp(0.0, max);
            row.config.volume = clamped;
            if clamped > 0.001 && row.config.muted {
                row.config.muted = false;
            }
        }
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_app_volume(bundle_id, clamped);
        }
        cx.notify();
    }

    pub(super) fn toggle_app_mute(&mut self, bundle_id: &str, cx: &mut Context<Self>) {
        let mut next_muted = false;
        if let Some(row) = self
            .snapshot
            .apps
            .iter_mut()
            .find(|r| r.info.bundle_id == bundle_id)
        {
            next_muted = !row.config.muted;
            row.config.muted = next_muted;
        }
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_app_mute(bundle_id, next_muted);
        }
        cx.notify();
    }

    pub(super) fn toggle_app_boost(&mut self, bundle_id: &str, cx: &mut Context<Self>) {
        let mut next_boost = false;
        if let Some(row) = self
            .snapshot
            .apps
            .iter_mut()
            .find(|r| r.info.bundle_id == bundle_id)
        {
            next_boost = !row.config.boost_enabled;
            row.config.boost_enabled = next_boost;
            if !next_boost && row.config.volume > 1.0 {
                row.config.volume = 1.0;
            }
        }
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_app_boost(bundle_id, next_boost);
        }
        cx.notify();
    }

    pub(super) fn set_app_target_device(
        &mut self,
        bundle_id: &str,
        target_device_uid: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_app_target_device(bundle_id, target_device_uid);
            self.snapshot = engine.snapshot();
        }
        cx.notify();
    }

    pub(super) fn toggle_routing_picker(&mut self, bundle_id: &str, cx: &mut Context<Self>) {
        if self.expanded_routing_app.as_deref() == Some(bundle_id) {
            self.expanded_routing_app = None;
        } else {
            self.expanded_routing_app = Some(bundle_id.to_string());
        }
        cx.notify();
    }

    fn render_vu_meter(&self, row: &AppRowState, palette: ThemePalette) -> impl IntoElement {
        let effective_level = if row.config.muted {
            0.0
        } else {
            (row.peak_level * row.config.volume).clamp(0.0, 1.0)
        };

        let thresholds: [f32; 5] = [0.05, 0.22, 0.42, 0.64, 0.84];
        let active_colors = [
            rgb(0x30d158),
            rgb(0x30d158),
            rgb(0x30d158),
            rgb(0xffd60a),
            rgb(0xff453a),
        ];

        let mut meter = div()
            .w(px(5.0))
            .h(px(28.0))
            .flex_shrink_0()
            .flex()
            .flex_col_reverse()
            .justify_center()
            .gap(px(2.0));

        for i in 0..5 {
            let color = if effective_level >= thresholds[i] {
                active_colors[i]
            } else {
                palette.vu_unlit
            };
            meter = meter.child(div().w_full().h(px(4.0)).rounded_full().bg(color));
        }

        meter
    }

    fn render_app_icon(&self, row: &AppRowState) -> AnyElement {
        if let Some(image) = self.icon_cache.get(&row.info.bundle_id) {
            return img(Arc::clone(image))
                .size(px(32.0))
                .rounded(px(8.0))
                .flex_shrink_0()
                .into_any_element();
        }

        let monogram = row
            .info
            .name
            .chars()
            .find(|c| c.is_alphanumeric())
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| String::from("A"));

        div()
            .size(px(32.0))
            .rounded(px(8.0))
            .flex_shrink_0()
            .bg(rgb(row.info.dominant_color))
            .flex()
            .items_center()
            .justify_center()
            .text_xs()
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(0xffffff))
            .child(monogram)
            .into_any_element()
    }

    fn render_routing_option(
        &self,
        id: SharedString,
        label: SharedString,
        is_selected: bool,
        palette: ThemePalette,
        on_click: impl Fn(&mut Self, &gpui::ClickEvent, &mut gpui::Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(id)
            .w_full()
            .px(px(8.0))
            .py(px(5.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(if is_selected {
                palette.row_selected_border
            } else {
                rgba(0x00000000)
            })
            .bg(if is_selected {
                palette.row_selected_bg
            } else {
                rgba(0x00000000)
            })
            .hover(move |s| {
                if is_selected {
                    s.bg(palette.row_selected_bg)
                } else {
                    s.bg(palette.row_hover_bg)
                }
            })
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .on_click(cx.listener(on_click))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .min_w_0()
                    .child(lucide_svg(IconName::HEADPHONES, 13.0, palette.text_primary))
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .font_weight(if is_selected {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(palette.text_primary)
                            .child(label),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(palette.accent_blue)
                    .child(if is_selected { "Active" } else { "" }),
            )
    }

    fn render_routing_picker(
        &self,
        row: &AppRowState,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bundle_id: Arc<str> = Arc::from(row.info.bundle_id.as_str());
        let bundle_sys = Arc::clone(&bundle_id);
        let current_target = row.config.target_device_uid.clone();
        let is_system_default = current_target.is_none();

        let mut list = div()
            .w_full()
            .mt(px(6.0))
            .p(px(8.0))
            .rounded(px(12.0))
            .bg(palette.card_bg)
            .border_1()
            .border_color(palette.card_border)
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .w_full()
                    .pb(px(4.0))
                    .mb(px(2.0))
                    .border_b_1()
                    .border_color(palette.divider)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.text_secondary)
                            .child("Application Audio Output"),
                    ),
            )
            .child(self.render_routing_option(
                SharedString::from(format!("route-sys-{bundle_id}")),
                SharedString::from("System Default Output"),
                is_system_default,
                palette,
                move |this, _event, _window, cx| {
                    this.set_app_target_device(&bundle_sys, None, cx);
                    this.expanded_routing_app = None;
                },
                cx,
            ));

        for dev in &self.snapshot.devices {
            let dev_uid = dev.uid.clone();
            let is_selected = current_target.as_deref() == Some(dev_uid.as_str());
            let bundle_dev = Arc::clone(&bundle_id);

            list = list.child(self.render_routing_option(
                SharedString::from(format!("route-dev-{}-{}", bundle_id, dev.id)),
                SharedString::from(dev.name.clone()),
                is_selected,
                palette,
                move |this, _event, _window, cx| {
                    this.set_app_target_device(&bundle_dev, Some(dev_uid.clone()), cx);
                    this.expanded_routing_app = None;
                },
                cx,
            ));
        }

        list
    }

    pub(super) fn render_app_row(
        &self,
        row: &AppRowState,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bundle_id: Arc<str> = Arc::from(row.info.bundle_id.as_str());
        let volume_pct = if row.config.muted {
            0
        } else {
            (row.config.volume * 100.0).round() as i32
        };
        let max_limit = self.snapshot.max_volume_limit.clamp(1.0, 2.0);
        let max_vol = if row.config.boost_enabled {
            max_limit
        } else {
            1.0
        };
        let theme = self.snapshot.theme;

        let entity_slider = cx.entity();
        let bundle_for_slider = Arc::clone(&bundle_id);
        let bundle_for_mute = Arc::clone(&bundle_id);
        let bundle_for_boost = Arc::clone(&bundle_id);
        let bundle_for_select = Arc::clone(&bundle_id);

        let is_eq_selected = self.snapshot.selected_eq_app.as_deref() == Some(bundle_id.as_ref());
        let is_expanded =
            is_eq_selected || self.expanded_routing_app.as_deref() == Some(bundle_id.as_ref());

        let (speaker_icon, speaker_color) = if row.config.muted {
            (IconName::VOLUME_X, palette.mute_red)
        } else {
            (IconName::VOLUME_2, palette.text_primary)
        };

        let mut name_col = div()
            .w(px(130.0))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .justify_center()
            .child(
                div()
                    .truncate()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(palette.text_primary)
                    .child(row.info.name.clone()),
            );

        let boost_enabled = row.config.boost_enabled;

        if self.advanced_mode {
            let route_label = if row.info.is_running_output {
                row.routed_device_name.clone()
            } else {
                format!("{} · Idle", row.routed_device_name)
            };
            name_col = name_col.child(
                div()
                    .truncate()
                    .text_xs()
                    .text_color(if row.config.target_device_uid.is_some() {
                        palette.accent_blue
                    } else {
                        palette.text_secondary
                    })
                    .child(route_label),
            );
        } else if !row.info.is_running_output {
            name_col = name_col.child(
                div()
                    .truncate()
                    .text_xs()
                    .text_color(palette.text_secondary)
                    .child("Idle"),
            );
        }

        let mut left_group = div()
            .id(SharedString::from(format!("app-eq-select-{bundle_id}")))
            .flex()
            .items_center()
            .gap(px(10.0))
            .flex_shrink_0()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _event, _window, cx| {
                let next = if this.snapshot.selected_eq_app.as_deref()
                    == Some(bundle_for_select.as_ref())
                {
                    None
                } else {
                    Some(bundle_for_select.to_string())
                };
                if this.advanced_mode {
                    this.expanded_routing_app = next.clone();
                }
                this.select_eq_app(next, cx);
            }));

        if self.advanced_mode {
            left_group = left_group.child(self.render_vu_meter(row, palette));
        }

        left_group = left_group.child(self.render_app_icon(row)).child(name_col);

        let mut container = div()
            .w_full()
            .flex()
            .flex_col()
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(12.0))
            .bg(if is_eq_selected {
                palette.row_selected_bg
            } else {
                rgba(0x00000000)
            })
            .border_1()
            .border_color(if is_eq_selected {
                palette.row_selected_border
            } else {
                rgba(0x00000000)
            })
            .hover(move |s| {
                if is_eq_selected {
                    s.bg(palette.row_selected_bg)
                } else {
                    s.bg(palette.row_hover_bg)
                }
            })
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    // Left: App Icon + Name (and VU meter + routing in advanced mode)
                    .child(left_group)
                    // Middle: Dominant-color VolumeSlider
                    .child(div().flex_1().min_w(px(96.0)).child(VolumeSlider::new(
                        SharedString::from(format!("app-slider-{bundle_id}")),
                        row.config.volume,
                        max_vol,
                        row.config.muted,
                        row.info.dominant_color,
                        theme,
                        move |val, _window, cx| {
                            entity_slider.update(cx, |this, cx| {
                                this.set_app_volume(&bundle_for_slider, val, cx);
                            });
                        },
                    )))
                    // Right: Frosted percentage pill + speaker mute button
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .id(SharedString::from(format!("app-pct-pill-{bundle_id}")))
                                    .w(px(44.0))
                                    .py(px(3.0))
                                    .rounded(px(8.0))
                                    .border_1()
                                    .border_color(if boost_enabled {
                                        palette.row_selected_border
                                    } else {
                                        rgba(0x00000000)
                                    })
                                    .bg(if boost_enabled {
                                        rgba(0x2582ff38)
                                    } else {
                                        palette.pill_bg
                                    })
                                    .hover(move |s| {
                                        if boost_enabled {
                                            s.bg(rgba(0x2582ff52))
                                        } else {
                                            s.bg(palette.pill_hover_bg)
                                        }
                                    })
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(palette.text_primary)
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        this.toggle_app_boost(&bundle_for_boost, cx);
                                    }))
                                    .child(format!("{volume_pct}%")),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("app-mute-{bundle_id}")))
                                    .size(px(26.0))
                                    .rounded(px(8.0))
                                    .hover(move |s| s.bg(palette.pill_hover_bg))
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        this.toggle_app_mute(&bundle_for_mute, cx);
                                    }))
                                    .child(lucide_svg(speaker_icon, 16.0, speaker_color)),
                            ),
                    ),
            );

        if is_expanded && self.advanced_mode {
            container = container.child(self.render_routing_picker(row, palette, cx));
        }

        container
    }
}
