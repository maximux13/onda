use std::sync::Arc;

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, div, px, rgb, rgba};

use crate::state::{ThemeMode, WindowStyle};
use crate::ui::ThemePalette;
use crate::ui::icons::{IconName, lucide_svg};
use crate::ui::slider::VolumeSlider;
use crate::updater::{UpdateController, UpdateState, relaunch_app};

use super::{SettingsTab, TrayPopoverView};

impl TrayPopoverView {
    fn toggle_launch_at_login(&mut self, cx: &mut Context<Self>) {
        let next = !self.launch_at_login;
        if crate::launch_agent::set_launch_at_login(next).is_ok() {
            self.launch_at_login = next;
        }
        cx.notify();
    }

    fn set_theme_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.snapshot.theme = mode;
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_theme(mode);
        }
        cx.notify();
    }

    fn cycle_theme_mode(&mut self, cx: &mut Context<Self>) {
        let next = match self.snapshot.theme {
            ThemeMode::Auto => ThemeMode::Dark,
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Auto,
        };
        self.set_theme_mode(next, cx);
    }

    fn set_window_style(&mut self, style: WindowStyle, cx: &mut Context<Self>) {
        self.snapshot.window_style = style;
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_window_style(style);
        }
        cx.notify();
    }

    fn set_glass_opacity(&mut self, opacity: f32, cx: &mut Context<Self>) {
        let clamped = opacity.clamp(0.0, 1.0);
        self.snapshot.glass_opacity = clamped;
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_glass_opacity(clamped);
        }
        cx.notify();
    }

    fn cycle_max_volume_limit(&mut self, cx: &mut Context<Self>) {
        let cur = self.snapshot.max_volume_limit;
        let next = if (cur - 1.0).abs() < 0.1 {
            1.5
        } else if (cur - 1.5).abs() < 0.1 {
            2.0
        } else {
            1.0
        };
        self.snapshot.max_volume_limit = next;
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_max_volume_limit(next);
            self.snapshot = engine.snapshot();
        }
        cx.notify();
    }

    fn toggle_restore_on_close(&mut self, cx: &mut Context<Self>) {
        let next = !self.snapshot.restore_on_close;
        self.snapshot.restore_on_close = next;
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_restore_on_close(next);
        }
        cx.notify();
    }

    fn toggle_show_in_menu_bar(&mut self, cx: &mut Context<Self>) {
        let next = !self.snapshot.show_in_menu_bar;
        self.snapshot.show_in_menu_bar = next;
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_show_in_menu_bar(next);
        }
        cx.notify();
    }

    fn reset_saved_volumes(&mut self, cx: &mut Context<Self>) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.reset_saved_app_configs();
            self.snapshot = engine.snapshot();
        }
        cx.notify();
    }

    /// Renders a single sidebar tab button in the `Settings` view.
    fn render_sidebar_item(
        &self,
        id: &'static str,
        tab: SettingsTab,
        label: &'static str,
        icon_path: &'static str,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = self.settings_tab == tab;
        let icon_color = if is_active {
            rgb(0xffffff)
        } else {
            palette.text_secondary
        };

        div()
            .id(id)
            .w_full()
            .px(px(10.0))
            .py(px(7.0))
            .rounded(px(8.0))
            .bg(if is_active {
                palette.accent_blue
            } else {
                rgba(0x00000000)
            })
            .hover(move |s| {
                if is_active {
                    s.bg(palette.accent_blue)
                } else {
                    s.bg(palette.row_hover_bg)
                }
            })
            .cursor_pointer()
            .flex()
            .items_center()
            .gap(px(8.0))
            .on_click(cx.listener(move |this, _event, _window, cx| {
                if this.settings_tab != tab {
                    this.settings_tab = tab;
                    this.bump_view_transition();
                    cx.notify();
                }
            }))
            .child(lucide_svg(icon_path, 15.0, icon_color))
            .child(
                div()
                    .truncate()
                    .text_xs()
                    .font_weight(if is_active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(if is_active {
                        rgb(0xffffff)
                    } else {
                        palette.text_secondary
                    })
                    .child(label),
            )
    }

    /// Renders the `General` tab content inside `Settings` (exact match to mockup).
    fn render_settings_general_tab(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_transparent = self.snapshot.window_style == WindowStyle::Transparent;
        let glass_opacity = self.snapshot.glass_opacity.clamp(0.0, 1.0);
        let glass_pct = format!("{}%", (glass_opacity * 100.0).round() as u32);
        let theme = self.snapshot.theme;
        let entity_opacity = cx.entity();
        let theme_label = match self.snapshot.theme {
            ThemeMode::Auto => "Auto",
            ThemeMode::Dark => "Dark",
            ThemeMode::Light => "Light",
        };
        let max_vol_pct = (self.snapshot.max_volume_limit * 100.0).round() as i32;

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(10.0))
            // Section 1: Startup
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(palette.text_primary)
                            .child("Startup"),
                    )
                    .child(
                        div()
                            .w_full()
                            .px(px(12.0))
                            .py(px(10.0))
                            .rounded(px(12.0))
                            .bg(palette.card_bg)
                            .border_1()
                            .border_color(palette.card_border)
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(palette.text_primary)
                                            .child("Launch at login"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(palette.text_secondary)
                                            .child("Keep the app running in the background"),
                                    ),
                            )
                            .child(self.render_ios_toggle(
                                "cfg-toggle-launch",
                                self.launch_at_login,
                                palette,
                                |this, _event, _window, cx| {
                                    this.toggle_launch_at_login(cx);
                                },
                                cx,
                            )),
                    ),
            )
            // Section 2: Interface
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(palette.text_primary)
                            .child("Interface"),
                    )
                    .child(
                        div()
                            .w_full()
                            .rounded(px(12.0))
                            .bg(palette.card_bg)
                            .border_1()
                            .border_color(palette.card_border)
                            .flex()
                            .flex_col()
                            // Row 1: Window style (Transparent | Solid)
                            .child(
                                div()
                                    .w_full()
                                    .px(px(12.0))
                                    .py(px(8.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(palette.text_primary)
                                            .child("Window style"),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .p(px(2.0))
                                            .rounded_full()
                                            .bg(palette.pill_bg)
                                            .child(
                                                div()
                                                    .id("cfg-style-transparent")
                                                    .px(px(9.0))
                                                    .py(px(3.0))
                                                    .rounded_full()
                                                    .cursor_pointer()
                                                    .bg(if is_transparent {
                                                        palette.accent_blue
                                                    } else {
                                                        rgba(0x00000000)
                                                    })
                                                    .text_xs()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(if is_transparent {
                                                        rgb(0xffffff)
                                                    } else {
                                                        palette.text_secondary
                                                    })
                                                    .on_click(cx.listener(
                                                        |this, _event, _window, cx| {
                                                            this.set_window_style(
                                                                WindowStyle::Transparent,
                                                                cx,
                                                            );
                                                        },
                                                    ))
                                                    .child("Transparent"),
                                            )
                                            .child(
                                                div()
                                                    .id("cfg-style-solid")
                                                    .px(px(9.0))
                                                    .py(px(3.0))
                                                    .rounded_full()
                                                    .cursor_pointer()
                                                    .bg(if !is_transparent {
                                                        palette.accent_blue
                                                    } else {
                                                        rgba(0x00000000)
                                                    })
                                                    .text_xs()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(if !is_transparent {
                                                        rgb(0xffffff)
                                                    } else {
                                                        palette.text_secondary
                                                    })
                                                    .on_click(cx.listener(
                                                        |this, _event, _window, cx| {
                                                            this.set_window_style(
                                                                WindowStyle::Solid,
                                                                cx,
                                                            );
                                                        },
                                                    ))
                                                    .child("Solid"),
                                            ),
                                    ),
                            )
                            .when(is_transparent, |this| {
                                this.child(div().w_full().h(px(1.0)).bg(palette.divider))
                                    .child(
                                        div()
                                            .w_full()
                                            .px(px(12.0))
                                            .py(px(8.0))
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap(px(10.0))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(palette.text_primary)
                                                    .child("Glass opacity"),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(8.0))
                                                    .child(div().w(px(96.0)).child(
                                                        VolumeSlider::new(
                                                            SharedString::from(
                                                                "cfg-glass-opacity-slider",
                                                            ),
                                                            glass_opacity,
                                                            1.0,
                                                            false,
                                                            0x1a73e8,
                                                            theme,
                                                            move |val, _window, cx| {
                                                                entity_opacity.update(
                                                                    cx,
                                                                    |this, cx| {
                                                                        this.set_glass_opacity(
                                                                            val, cx,
                                                                        );
                                                                    },
                                                                );
                                                            },
                                                        ),
                                                    ))
                                                    .child(
                                                        div()
                                                            .w(px(34.0))
                                                            .text_right()
                                                            .text_xs()
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(palette.text_secondary)
                                                            .child(glass_pct),
                                                    ),
                                            ),
                                    )
                            })
                            .child(div().w_full().h(px(1.0)).bg(palette.divider))
                            // Row 2: Theme (Auto / Dark / Light)
                            .child(
                                div()
                                    .w_full()
                                    .px(px(12.0))
                                    .py(px(8.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(palette.text_primary)
                                            .child("Theme"),
                                    )
                                    .child(
                                        div()
                                            .id("cfg-theme-cycle-btn")
                                            .px(px(10.0))
                                            .py(px(4.0))
                                            .rounded_full()
                                            .bg(palette.pill_bg)
                                            .hover(move |s| s.bg(palette.pill_hover_bg))
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .on_click(cx.listener(|this, _event, _window, cx| {
                                                this.cycle_theme_mode(cx);
                                            }))
                                            .child(lucide_svg(
                                                IconName::SUN,
                                                13.0,
                                                palette.text_primary,
                                            ))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(palette.text_primary)
                                                    .child(theme_label),
                                            )
                                            .child(lucide_svg(
                                                IconName::CHEVRON_DOWN,
                                                10.0,
                                                palette.text_secondary,
                                            )),
                                    ),
                            )
                            .child(div().w_full().h(px(1.0)).bg(palette.divider))
                            // Row 3: Show in menu bar
                            .child(
                                div()
                                    .w_full()
                                    .px(px(12.0))
                                    .py(px(8.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(palette.text_primary)
                                            .child("Show in menu bar"),
                                    )
                                    .child(self.render_ios_toggle(
                                        "cfg-toggle-menubar",
                                        self.snapshot.show_in_menu_bar,
                                        palette,
                                        |this, _event, _window, cx| {
                                            this.toggle_show_in_menu_bar(cx);
                                        },
                                        cx,
                                    )),
                            ),
                    ),
            )
            // Section 3: Audio
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(palette.text_primary)
                            .child("Audio"),
                    )
                    .child(
                        div()
                            .w_full()
                            .rounded(px(12.0))
                            .bg(palette.card_bg)
                            .border_1()
                            .border_color(palette.card_border)
                            .flex()
                            .flex_col()
                            // Row 1: Boost volume up to
                            .child(
                                div()
                                    .w_full()
                                    .px(px(12.0))
                                    .py(px(8.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(palette.text_primary)
                                            .child("Boost volume up to"),
                                    )
                                    .child(
                                        div()
                                            .id("cfg-max-vol-btn")
                                            .px(px(10.0))
                                            .py(px(4.0))
                                            .rounded_full()
                                            .bg(palette.pill_bg)
                                            .hover(move |s| s.bg(palette.pill_hover_bg))
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .on_click(cx.listener(|this, _event, _window, cx| {
                                                this.cycle_max_volume_limit(cx);
                                            }))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(palette.text_primary)
                                                    .child(format!("{max_vol_pct}%")),
                                            )
                                            .child(lucide_svg(
                                                IconName::CHEVRON_DOWN,
                                                10.0,
                                                palette.text_secondary,
                                            )),
                                    ),
                            )
                            .child(div().w_full().h(px(1.0)).bg(palette.divider))
                            // Row 2: Restore levels on quit
                            .child(
                                div()
                                    .w_full()
                                    .px(px(12.0))
                                    .py(px(8.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(palette.text_primary)
                                            .child("Restore levels on quit"),
                                    )
                                    .child(self.render_ios_toggle(
                                        "cfg-toggle-restore",
                                        self.snapshot.restore_on_close,
                                        palette,
                                        |this, _event, _window, cx| {
                                            this.toggle_restore_on_close(cx);
                                        },
                                        cx,
                                    )),
                            ),
                    ),
            )
            // Bottom Info Card: Onda v0.1.0
            .child(
                div()
                    .w_full()
                    .px(px(12.0))
                    .py(px(9.0))
                    .rounded(px(12.0))
                    .bg(palette.card_bg)
                    .border_1()
                    .border_color(palette.card_border)
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .min_w_0()
                            .child(
                                div()
                                    .size(px(28.0))
                                    .rounded_full()
                                    .bg(palette.pill_bg)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .flex_shrink_0()
                                    .child(lucide_svg(IconName::INFO, 15.0, palette.text_primary)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(palette.text_primary)
                                            .child("Onda v0.1.0"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(palette.text_secondary)
                                            .child("Made with love for your Mac"),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("cfg-quit-btn")
                            .px(px(10.0))
                            .py(px(4.0))
                            .rounded_full()
                            .bg(palette.pill_bg)
                            .hover(move |s| s.bg(palette.pill_hover_bg))
                            .cursor_pointer()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.mute_red)
                            .on_click(|_event, _window, cx| {
                                cx.quit();
                            })
                            .child("Quit"),
                    ),
            )
    }

    fn render_settings_shortcuts_tab(&self, palette: ThemePalette) -> impl IntoElement {
        let shortcuts = [
            ("Close mixer popover", "Esc"),
            ("Reset EQ band to 0 dB", "Double-click"),
            ("Per-application equalizer", "Click app"),
            ("Toggle 200% volume boost", "Click %"),
            ("Quit Onda", "Cmd + Q"),
        ];

        let mut card = div()
            .w_full()
            .rounded(px(12.0))
            .bg(palette.card_bg)
            .border_1()
            .border_color(palette.card_border)
            .flex()
            .flex_col();

        for (idx, (label, key)) in shortcuts.iter().enumerate() {
            if idx > 0 {
                card = card.child(div().w_full().h(px(1.0)).bg(palette.divider));
            }
            card = card.child(
                div()
                    .w_full()
                    .px(px(12.0))
                    .py(px(9.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.text_primary)
                            .child(*label),
                    )
                    .child(
                        div()
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(px(6.0))
                            .bg(palette.pill_bg)
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.text_secondary)
                            .child(*key),
                    ),
            );
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(palette.text_primary)
                    .child("Keyboard Shortcuts & Quick Actions"),
            )
            .child(card)
    }

    fn render_settings_eq_tab(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let presets: [(&str, &str, [f32; 10]); 5] = [
            ("eq-preset-flat", "Flat (0 dB)", [0.0; 10]),
            (
                "eq-preset-bass",
                "Bass Boost",
                [6.5, 5.5, 4.0, 1.5, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0],
            ),
            (
                "eq-preset-vocal",
                "Vocal Clarity",
                [-2.0, -1.5, 0.0, 2.0, 4.0, 4.5, 3.0, 1.5, 0.5, 0.0],
            ),
            (
                "eq-preset-electronic",
                "Electronic",
                [5.0, 4.0, 1.0, -1.5, -1.0, 1.0, 2.5, 4.0, 4.5, 5.0],
            ),
            (
                "eq-preset-acoustic",
                "Acoustic",
                [3.0, 2.5, 1.5, 0.5, 1.5, 2.0, 3.0, 3.5, 3.0, 2.0],
            ),
        ];

        let mut card = div()
            .w_full()
            .rounded(px(12.0))
            .bg(palette.card_bg)
            .border_1()
            .border_color(palette.card_border)
            .flex()
            .flex_col();

        for (idx, (id, name, gains)) in presets.into_iter().enumerate() {
            if idx > 0 {
                card = card.child(div().w_full().h(px(1.0)).bg(palette.divider));
            }
            card = card.child(
                div()
                    .id(id)
                    .w_full()
                    .px(px(12.0))
                    .py(px(8.0))
                    .hover(move |s| s.bg(palette.row_hover_bg))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.apply_eq_preset(gains, cx);
                    }))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.text_primary)
                            .child(name),
                    )
                    .child(
                        div()
                            .px(px(8.0))
                            .py(px(2.0))
                            .rounded_full()
                            .bg(palette.pill_bg)
                            .text_xs()
                            .text_color(palette.accent_blue)
                            .child("Apply"),
                    ),
            );
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(palette.text_primary)
                            .child("10-Band Equalizer Presets"),
                    )
                    .child(card),
            )
            .child(
                div()
                    .w_full()
                    .px(px(12.0))
                    .py(px(10.0))
                    .rounded(px(12.0))
                    .bg(palette.card_bg)
                    .border_1()
                    .border_color(palette.card_border)
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.text_primary)
                            .child("Reset all saved application levels"),
                    )
                    .child(
                        div()
                            .id("cfg-reset-all-btn")
                            .px(px(10.0))
                            .py(px(4.0))
                            .rounded_full()
                            .bg(palette.pill_bg)
                            .hover(move |s| s.bg(palette.pill_hover_bg))
                            .cursor_pointer()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.mute_red)
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.reset_saved_volumes(cx);
                            }))
                            .child("Reset All"),
                    ),
            )
    }

    fn render_settings_notifications_tab(&self, palette: ThemePalette) -> impl IntoElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(palette.text_primary)
                    .child("Audio Device Events"),
            )
            .child(
                div()
                    .w_full()
                    .p(px(12.0))
                    .rounded(px(12.0))
                    .bg(palette.card_bg)
                    .border_1()
                    .border_color(palette.card_border)
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.text_primary)
                            .child("Automatic Output Switching"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(palette.text_secondary)
                            .child("Onda automatically detects headphone, speaker, and display changes in macOS Core Audio without interrupting playback."),
                    ),
            )
    }

    fn render_settings_about_tab(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let update_state = self.update_state();
        let status_text = match &update_state {
            UpdateState::Idle | UpdateState::UpToDate => {
                "You're on the latest version.".to_string()
            }
            UpdateState::Checking => "Checking for updates...".to_string(),
            UpdateState::Available(info) => format!("Version v{} is available.", info.version),
            UpdateState::Downloading { version } => format!("Downloading v{version}..."),
            UpdateState::ReadyToRestart { version, .. } => {
                format!("v{version} is ready. Restart to finish updating.")
            }
            UpdateState::Error(err) => err.clone(),
        };

        let action_btn: Option<AnyElement> = match update_state {
            UpdateState::Idle | UpdateState::UpToDate | UpdateState::Error(_) => Some(
                div()
                    .id("about-check-updates-btn")
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(8.0))
                    .bg(palette.pill_bg)
                    .border_1()
                    .border_color(palette.card_border)
                    .hover(move |s| s.bg(palette.pill_hover_bg))
                    .cursor_pointer()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(palette.text_primary)
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.update_banner_dismissed = false;
                        UpdateController::trigger_check(Arc::clone(&this.updater));
                        cx.notify();
                    }))
                    .child("Check for Updates")
                    .into_any_element(),
            ),
            UpdateState::Available(_) => Some(
                div()
                    .id("about-download-update-btn")
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(8.0))
                    .bg(palette.accent_blue)
                    .cursor_pointer()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff))
                    .on_click(cx.listener(|this, _event, _window, _cx| {
                        UpdateController::trigger_download(Arc::clone(&this.updater));
                    }))
                    .child("Download Update")
                    .into_any_element(),
            ),
            UpdateState::ReadyToRestart { app_path, .. } => Some(
                div()
                    .id("about-restart-update-btn")
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(8.0))
                    .bg(palette.accent_blue)
                    .cursor_pointer()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff))
                    .on_click(cx.listener(move |_this, _event, _window, cx| {
                        let _ = relaunch_app(&app_path);
                        cx.quit();
                    }))
                    .child("Restart & Update")
                    .into_any_element(),
            ),
            UpdateState::Checking | UpdateState::Downloading { .. } => None,
        };

        let mut updates_card = div()
            .w_full()
            .p(px(12.0))
            .rounded(px(12.0))
            .bg(palette.card_bg)
            .border_1()
            .border_color(palette.card_border)
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(palette.text_primary)
                    .child("Updates"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(palette.text_secondary)
                    .child(status_text),
            );

        if let Some(btn) = action_btn {
            updates_card = updates_card.child(div().flex().items_center().child(btn));
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(palette.text_primary)
                    .child("About Onda"),
            )
            .child(
                div()
                    .w_full()
                    .p(px(12.0))
                    .rounded(px(12.0))
                    .bg(palette.card_bg)
                    .border_1()
                    .border_color(palette.card_border)
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(palette.text_primary)
                            .child(format!("Onda v{}", env!("CARGO_PKG_VERSION"))),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(palette.text_secondary)
                            .child("Per-application volume control, output device routing, and real-time 10-band parametric equalizer powered by Core Audio Process Taps and GPUI."),
                    ),
            )
            .child(updates_card)
    }

    /// Renders View 2: 2-Column `Settings` view matching the right panel of the mockup.
    pub(super) fn render_settings_view(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let right_content: AnyElement = match self.settings_tab {
            SettingsTab::General => self
                .render_settings_general_tab(palette, cx)
                .into_any_element(),
            SettingsTab::Shortcuts => self
                .render_settings_shortcuts_tab(palette)
                .into_any_element(),
            SettingsTab::Equalizer => self.render_settings_eq_tab(palette, cx).into_any_element(),
            SettingsTab::Notifications => self
                .render_settings_notifications_tab(palette)
                .into_any_element(),
            SettingsTab::About => self
                .render_settings_about_tab(palette, cx)
                .into_any_element(),
        };

        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            // Top Header: Frosted gear squircle + Settings title + Back button
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .child(self.render_header_title(
                        IconName::SETTINGS,
                        "Settings",
                        "Customize your experience",
                        palette,
                    ))
                    .child(
                        div()
                            .id("settings-back-btn")
                            .px(px(10.0))
                            .py(px(6.0))
                            .rounded_full()
                            .bg(palette.pill_bg)
                            .border_1()
                            .border_color(palette.card_border)
                            .hover(move |s| s.bg(palette.pill_hover_bg))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                if this.show_settings {
                                    this.show_settings = false;
                                    this.bump_view_transition();
                                    cx.notify();
                                }
                            }))
                            .child(lucide_svg(IconName::ARROW_LEFT, 12.0, palette.text_primary))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(palette.text_primary)
                                    .child("Back"),
                            ),
                    ),
            )
            // 2-Column Body: Left Sidebar + Right Scrollable Cards
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .flex()
                    .gap(px(12.0))
                    // Left Sidebar
                    .child(
                        div()
                            .w(px(136.0))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(self.render_sidebar_item(
                                "tab-general",
                                SettingsTab::General,
                                "General",
                                IconName::SETTINGS,
                                palette,
                                cx,
                            ))
                            .child(self.render_sidebar_item(
                                "tab-shortcuts",
                                SettingsTab::Shortcuts,
                                "Shortcuts",
                                IconName::KEYBOARD,
                                palette,
                                cx,
                            ))
                            .child(self.render_sidebar_item(
                                "tab-eq",
                                SettingsTab::Equalizer,
                                "Equalizer",
                                IconName::AUDIO_LINES,
                                palette,
                                cx,
                            ))
                            .child(self.render_sidebar_item(
                                "tab-notifications",
                                SettingsTab::Notifications,
                                "Notifications",
                                IconName::BELL,
                                palette,
                                cx,
                            ))
                            .child(self.render_sidebar_item(
                                "tab-about",
                                SettingsTab::About,
                                "About",
                                IconName::INFO,
                                palette,
                                cx,
                            )),
                    )
                    // Right Content Area
                    .child(
                        div()
                            .id("settings-right-scroll")
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .overflow_y_scroll()
                            .pr(px(2.0))
                            .child(right_content),
                    ),
            )
    }
}
