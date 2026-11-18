use gpui::prelude::*;
use gpui::{Context, FontWeight, div, px, rgb};

use crate::ui::ThemePalette;
use crate::ui::icons::{IconName, lucide_svg};

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
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .min_w_0()
                    .child(
                        div()
                            .size(px(38.0))
                            .rounded(px(11.0))
                            .bg(palette.card_bg)
                            .border_1()
                            .border_color(palette.card_border)
                            .flex()
                            .items_center()
                            .justify_center()
                            .flex_shrink_0()
                            .child(lucide_svg(
                                IconName::AUDIO_LINES,
                                22.0,
                                palette.text_primary,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .min_w_0()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(palette.text_primary)
                                    .child("Onda"),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_xs()
                                    .text_color(palette.text_secondary)
                                    .child("Control audio for each application"),
                            ),
                    ),
            )
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
                            .child(lucide_svg(IconName::SETTINGS, 16.0, palette.text_primary)),
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

    /// Renders View 1: Main `Onda` view.
    pub(super) fn render_mixer_view(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut apps_list = div().w_full().flex().flex_col().gap(px(4.0));

        if self.snapshot.apps.is_empty() {
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
                    .child("No applications are currently playing audio"),
            );
        } else {
            for row in &self.snapshot.apps {
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

        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(self.render_mixer_header(palette, cx))
            .child(div().w_full().h(px(1.0)).bg(palette.divider))
            .child(scroll_area)
            .child(div().w_full().h(px(1.0)).bg(palette.divider))
            .child(self.render_equalizer_section(palette, cx))
            .child(self.render_mixer_footer(palette, cx))
    }
}
