use gpui::prelude::*;
use gpui::{Context, FontWeight, div, px, rgb};

use crate::ui::ThemePalette;
use crate::ui::icons::{IconName, lucide_svg};
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

    /// Renders an inline output device & master volume drawer when the user clicks `Output: ...`.
    fn render_output_picker_drawer(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut drawer = div()
            .w_full()
            .p(px(10.0))
            .rounded(px(12.0))
            .bg(palette.card_bg)
            .border_1()
            .border_color(palette.card_border)
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(palette.text_secondary)
                            .child("Output Devices & Master Volume"),
                    ),
            );

        for device in &self.snapshot.devices {
            drawer = drawer.child(self.render_device_row(device, palette, cx));
        }

        drawer
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
                        this.advanced_mode = !this.advanced_mode;
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

        let mut scroll_area = div()
            .id("mixer-apps-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(apps_list);

        if self.show_output_picker {
            scroll_area = scroll_area.child(self.render_output_picker_drawer(palette, cx));
        }

        let mut root = div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(self.render_mixer_header(palette, cx))
            .child(div().w_full().h(px(1.0)).bg(palette.divider))
            .child(self.render_apps_filter_bar(palette, cx))
            .child(scroll_area);

        if self.advanced_mode {
            root = root
                .child(div().w_full().h(px(1.0)).bg(palette.divider))
                .child(self.render_equalizer_section(palette, cx));
        }

        root.child(self.render_mixer_footer(palette, cx))
    }
}
