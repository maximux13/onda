use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, FontWeight, Image, ImageFormat, SharedString, Window, div, img, px, rgb,
    rgba,
};

use crate::audio::AudioEngine;
use crate::state::{
    AppRowState, EQ_LABELS, MixerSnapshot, OutputDeviceInfo, ThemeMode, WindowStyle,
};
use crate::ui::ThemePalette;
use crate::ui::icons::{IconName, lucide_svg};
use crate::ui::slider::{EqVerticalSlider, VolumeSlider};

/// Sidebar tab in the `Settings` view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsTab {
    General,
    Shortcuts,
    Equalizer,
    Notifications,
    About,
}

/// Glassmorphic macOS `Onda` & `Settings` popover view.
pub struct TrayPopoverView {
    engine: Arc<Mutex<AudioEngine>>,
    snapshot: MixerSnapshot,
    icon_cache: HashMap<String, Arc<Image>>,
    expanded_routing_app: Option<String>,
    is_refreshing: bool,
    show_settings: bool,
    settings_tab: SettingsTab,
    is_pinned: bool,
    advanced_mode: bool,
    show_output_picker: bool,
    launch_at_login: bool,
}

impl TrayPopoverView {
    /// Creates a new [`TrayPopoverView`], loads the initial mixer snapshot,
    /// and starts a 150ms background timer so VU meters and app lists stay live.
    pub fn new(engine: Arc<Mutex<AudioEngine>>, cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            engine,
            snapshot: MixerSnapshot::default(),
            icon_cache: HashMap::new(),
            expanded_routing_app: None,
            is_refreshing: false,
            show_settings: false,
            settings_tab: SettingsTab::General,
            is_pinned: false,
            advanced_mode: false,
            show_output_picker: false,
            launch_at_login: crate::launch_agent::is_launch_at_login_enabled(),
        };
        view.refresh(cx);

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                if this.update(cx, |view, cx| view.refresh(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();

        view
    }

    /// Returns whether the popover is pinned by the user (preventing auto-close on blur).
    pub fn is_pinned(&self) -> bool {
        self.is_pinned
    }

    /// Re-reads the current audio state from [`AudioEngine`] and updates the UI.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.is_refreshing = true;
        if let Ok(mut engine) = self.engine.lock() {
            self.snapshot = engine.snapshot();
        }
        self.sync_icon_cache();
        self.is_refreshing = false;
        cx.notify();
    }

    fn sync_icon_cache(&mut self) {
        for app in &self.snapshot.apps {
            if !self.icon_cache.contains_key(&app.info.bundle_id) {
                if let Some(bytes) = &app.info.icon_png {
                    if !bytes.is_empty() {
                        let image = Arc::new(Image::from_bytes(ImageFormat::Png, bytes.clone()));
                        self.icon_cache.insert(app.info.bundle_id.clone(), image);
                    }
                }
            }
        }
    }

    fn set_device_volume(&mut self, device_id: u32, volume: f32, cx: &mut Context<Self>) {
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

    fn toggle_device_mute(&mut self, device_id: u32, cx: &mut Context<Self>) {
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

    fn set_default_output_device(&mut self, device_id: u32, cx: &mut Context<Self>) {
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

    fn set_app_volume(&mut self, bundle_id: &str, volume: f32, cx: &mut Context<Self>) {
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

    fn toggle_app_mute(&mut self, bundle_id: &str, cx: &mut Context<Self>) {
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

    fn toggle_app_boost(&mut self, bundle_id: &str, cx: &mut Context<Self>) {
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

    fn set_app_target_device(
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

    fn toggle_routing_picker(&mut self, bundle_id: &str, cx: &mut Context<Self>) {
        if self.expanded_routing_app.as_deref() == Some(bundle_id) {
            self.expanded_routing_app = None;
        } else {
            self.expanded_routing_app = Some(bundle_id.to_string());
        }
        cx.notify();
    }

    fn select_eq_app(&mut self, bundle_id: Option<String>, cx: &mut Context<Self>) {
        self.snapshot.selected_eq_app = bundle_id.clone();
        if let Ok(mut engine) = self.engine.lock() {
            engine.select_eq_app(bundle_id);
            self.snapshot = engine.snapshot();
        }
        cx.notify();
    }

    fn toggle_active_eq(&mut self, cx: &mut Context<Self>) {
        let current_enabled = self.active_eq_state().1;
        let next = !current_enabled;
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_active_eq_enabled(next);
            self.snapshot = engine.snapshot();
        }
        cx.notify();
    }

    fn set_eq_band(&mut self, band_idx: usize, gain_db: f32, cx: &mut Context<Self>) {
        if band_idx >= 10 {
            return;
        }
        let clamped = gain_db.clamp(-12.0, 12.0);
        if let Some(ref sel_id) = self.snapshot.selected_eq_app {
            if let Some(app) = self
                .snapshot
                .apps
                .iter_mut()
                .find(|a| &a.info.bundle_id == sel_id)
            {
                app.config.eq_gains[band_idx] = clamped;
            }
        } else {
            self.snapshot.global_eq_gains[band_idx] = clamped;
        }
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_active_eq_band(band_idx, clamped);
        }
        cx.notify();
    }

    fn apply_eq_preset(&mut self, gains: [f32; 10], cx: &mut Context<Self>) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_active_eq_enabled(true);
            for (idx, gain) in gains.iter().copied().enumerate() {
                engine.set_active_eq_band(idx, gain);
            }
            self.snapshot = engine.snapshot();
        }
        cx.notify();
    }

    fn reset_active_eq(&mut self, cx: &mut Context<Self>) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.reset_active_eq();
            self.snapshot = engine.snapshot();
        }
        cx.notify();
    }

    fn active_eq_state(&self) -> (String, bool, [f32; 10], bool) {
        if let Some(ref bundle_id) = self.snapshot.selected_eq_app {
            if let Some(row) = self
                .snapshot
                .apps
                .iter()
                .find(|r| &r.info.bundle_id == bundle_id)
            {
                return (
                    row.info.name.clone(),
                    row.config.eq_enabled,
                    row.config.eq_gains,
                    true,
                );
            }
        }
        (
            String::from("Global"),
            self.snapshot.global_eq_enabled,
            self.snapshot.global_eq_gains,
            false,
        )
    }

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

    /// Renders an iOS-style pill toggle switch matching the mockup.
    fn render_ios_toggle(
        &self,
        id: impl Into<SharedString>,
        is_on: bool,
        palette: ThemePalette,
        on_click: impl Fn(&mut Self, &gpui::ClickEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(id.into())
            .w(px(40.0))
            .h(px(22.0))
            .rounded_full()
            .p(px(2.0))
            .cursor_pointer()
            .bg(if is_on {
                palette.accent_blue
            } else {
                palette.device_badge_inactive
            })
            .flex()
            .items_center()
            .justify_between()
            .on_click(cx.listener(on_click))
            .child(if is_on { div().flex_1() } else { div() })
            .child(
                div()
                    .size(px(18.0))
                    .rounded_full()
                    .bg(rgb(0xffffff))
                    .shadow_sm(),
            )
            .child(if !is_on { div().flex_1() } else { div() })
    }

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
                                this.show_settings = true;
                                this.launch_at_login =
                                    crate::launch_agent::is_launch_at_login_enabled();
                                cx.notify();
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
            .w(px(6.0))
            .h(px(22.0))
            .flex_shrink_0()
            .flex()
            .flex_col_reverse()
            .justify_between()
            .gap(px(1.5));

        for i in 0..5 {
            let color = if effective_level >= thresholds[i] {
                active_colors[i]
            } else {
                palette.vu_unlit
            };
            meter = meter.child(div().w_full().h(px(2.5)).rounded(px(1.0)).bg(color));
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

    fn render_routing_picker(
        &self,
        row: &AppRowState,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bundle_id = row.info.bundle_id.clone();
        let bundle_sys = bundle_id.clone();
        let current_target = row.config.target_device_uid.clone();
        let is_system_default = current_target.is_none();

        let mut list = div()
            .w_full()
            .mt(px(4.0))
            .p(px(8.0))
            .rounded(px(10.0))
            .bg(palette.card_bg)
            .border_1()
            .border_color(palette.card_border)
            .flex()
            .flex_col()
            .gap(px(3.0))
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
            // System Default option
            .child(
                div()
                    .id(SharedString::from(format!("route-sys-{bundle_id}")))
                    .w_full()
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(6.0))
                    .bg(if is_system_default {
                        palette.row_selected_bg
                    } else {
                        rgba(0x00000000)
                    })
                    .hover(move |s| s.bg(palette.row_hover_bg))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.set_app_target_device(&bundle_sys, None, cx);
                        this.expanded_routing_app = None;
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(lucide_svg(
                                IconName::HEADPHONES,
                                13.0,
                                palette.text_primary,
                            ))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(if is_system_default {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .text_color(palette.text_primary)
                                    .child("System Default Output"),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(palette.accent_blue)
                            .child(if is_system_default { "Activo" } else { "" }),
                    ),
            );

        for dev in &self.snapshot.devices {
            let dev_uid = dev.uid.clone();
            let is_selected = current_target.as_deref() == Some(dev_uid.as_str());
            let bundle_dev = bundle_id.clone();

            list = list.child(
                div()
                    .id(SharedString::from(format!(
                        "route-dev-{}-{}",
                        bundle_id, dev.id
                    )))
                    .w_full()
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(6.0))
                    .bg(if is_selected {
                        palette.row_selected_bg
                    } else {
                        rgba(0x00000000)
                    })
                    .hover(move |s| s.bg(palette.row_hover_bg))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.set_app_target_device(&bundle_dev, Some(dev_uid.clone()), cx);
                        this.expanded_routing_app = None;
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .min_w_0()
                            .child(lucide_svg(
                                IconName::HEADPHONES,
                                13.0,
                                palette.text_primary,
                            ))
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
                                    .child(dev.name.clone()),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(palette.accent_blue)
                            .child(if is_selected { "Activo" } else { "" }),
                    ),
            );
        }

        list
    }

    fn render_app_row(
        &self,
        row: &AppRowState,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bundle_id = row.info.bundle_id.clone();
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

        let entity_slider = cx.entity().clone();
        let bundle_for_slider = bundle_id.clone();
        let bundle_for_mute = bundle_id.clone();
        let bundle_for_boost = bundle_id.clone();
        let bundle_for_select = bundle_id.clone();
        let bundle_for_route = bundle_id.clone();

        let is_eq_selected =
            self.snapshot.selected_eq_app.as_deref() == Some(bundle_id.as_str());
        let is_expanded = self.expanded_routing_app.as_deref() == Some(bundle_id.as_str());

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

        if self.advanced_mode {
            name_col = name_col.child(
                div()
                    .id(SharedString::from(format!("app-route-sub-{bundle_id}")))
                    .truncate()
                    .text_xs()
                    .text_color(if row.config.target_device_uid.is_some() {
                        palette.accent_blue
                    } else {
                        palette.text_secondary
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.toggle_routing_picker(&bundle_for_route, cx);
                    }))
                    .child(row.routed_device_name.clone()),
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
                    == Some(bundle_for_select.as_str())
                {
                    None
                } else {
                    Some(bundle_for_select.clone())
                };
                this.select_eq_app(next, cx);
            }));

        if self.advanced_mode {
            left_group = left_group.child(self.render_vu_meter(row, palette));
        }

        left_group = left_group
            .child(self.render_app_icon(row))
            .child(name_col);

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
                    .child(
                        div().flex_1().min_w(px(96.0)).child(VolumeSlider::new(
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
                        )),
                    )
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
                                    .rounded(px(7.0))
                                    .bg(if row.config.boost_enabled {
                                        rgba(0x2582ff38)
                                    } else {
                                        palette.pill_bg
                                    })
                                    .hover(move |s| s.bg(palette.pill_hover_bg))
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
                                    .rounded(px(7.0))
                                    .hover(move |s| s.bg(palette.pill_bg))
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

    /// Renders the 10-band Equalizer section (Global or per-app).
    fn render_equalizer_section(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (target_label, eq_enabled, eq_gains, is_app_target) = self.active_eq_state();
        let has_non_zero = eq_gains.iter().any(|g| g.abs() > 0.05);
        let theme = self.snapshot.theme;

        let mut sliders_row = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(px(4.0))
            .pt(px(2.0));

        for (idx, label) in EQ_LABELS.iter().copied().enumerate() {
            let gain = eq_gains[idx];
            let entity = cx.entity().clone();

            sliders_row = sliders_row.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(1.0))
                    .child(EqVerticalSlider::new(
                        SharedString::from(format!("eq-band-{idx}")),
                        gain,
                        eq_enabled,
                        theme,
                        move |new_db, _window, cx| {
                            entity.update(cx, |this, cx| {
                                this.set_eq_band(idx, new_db, cx);
                            });
                        },
                    ))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.text_primary)
                            .child(label),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(palette.text_secondary)
                            .child("Hz"),
                    ),
            );
        }

        let mut header_left = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .min_w_0()
            .child(lucide_svg(
                IconName::AUDIO_LINES,
                16.0,
                palette.text_primary,
            ))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(palette.text_primary)
                    .child("Equalizer"),
            );

        if is_app_target {
            header_left = header_left.child(
                div()
                    .id("eq-target-badge")
                    .px(px(8.0))
                    .py(px(2.0))
                    .rounded_full()
                    .bg(palette.accent_blue)
                    .cursor_pointer()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0xffffff))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.select_eq_app(None, cx);
                    }))
                    .child(target_label),
            );
        } else if self.advanced_mode {
            header_left = header_left.child(
                div()
                    .px(px(8.0))
                    .py(px(2.0))
                    .rounded_full()
                    .bg(palette.pill_bg)
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(palette.text_secondary)
                    .child("Global"),
            );
        }

        if has_non_zero {
            header_left = header_left.child(
                div()
                    .id("eq-reset-inline-btn")
                    .px(px(7.0))
                    .py(px(2.0))
                    .rounded_full()
                    .bg(palette.pill_bg)
                    .hover(move |s| s.bg(palette.pill_hover_bg))
                    .cursor_pointer()
                    .text_xs()
                    .text_color(palette.text_secondary)
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.reset_active_eq(cx);
                    }))
                    .child("Reset"),
            );
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .pt(px(4.0))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(4.0))
                    .child(header_left)
                    .child(self.render_ios_toggle(
                        "eq-enable-toggle",
                        eq_enabled,
                        palette,
                        |this, _event, _window, cx| {
                            this.toggle_active_eq(cx);
                        },
                        cx,
                    )),
            )
            .child(sliders_row)
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

    fn render_device_row(
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
        let entity_slider = cx.entity().clone();

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
            .py(px(5.0))
            .rounded(px(9.0))
            .bg(if is_default {
                palette.row_selected_bg
            } else {
                rgba(0x00000000)
            })
            .hover(move |s| s.bg(palette.row_hover_bg))
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
            .child(
                div().flex_1().min_w(px(90.0)).child(VolumeSlider::new(
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
                )),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .w(px(40.0))
                            .py(px(2.0))
                            .rounded(px(6.0))
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
                            .size(px(24.0))
                            .rounded(px(6.0))
                            .hover(move |s| s.bg(palette.pill_bg))
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
                    .hover(move |s| s.bg(palette.pill_hover_bg))
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
                            .child(lucide_svg(
                                IconName::HEADPHONES,
                                15.0,
                                palette.text_primary,
                            ))
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
    fn render_mixer_view(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut apps_list = div().w_full().flex().flex_col().gap(px(2.0));

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
            .gap(px(4.0))
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
                this.settings_tab = tab;
                cx.notify();
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
            // Section 1: Inicio
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
            // Section 2: Interfaz
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
                            // Row 1: Aumentar volumen hasta (200% v)
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
                            // Row 2: Restaurar niveles al cerrar
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
                                    .child(lucide_svg(
                                        IconName::INFO,
                                        15.0,
                                        palette.text_primary,
                                    )),
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

    fn render_settings_about_tab(&self, palette: ThemePalette) -> impl IntoElement {
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
                            .child("Onda v0.1.0"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(palette.text_secondary)
                            .child("Per-application volume control, output device routing, and real-time 10-band parametric equalizer powered by Core Audio Process Taps and GPUI."),
                    ),
            )
    }

    /// Renders View 2: 2-Column `Settings` view matching the right panel of the mockup.
    fn render_settings_view(
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
            SettingsTab::Equalizer => self
                .render_settings_eq_tab(palette, cx)
                .into_any_element(),
            SettingsTab::Notifications => self
                .render_settings_notifications_tab(palette)
                .into_any_element(),
            SettingsTab::About => self.render_settings_about_tab(palette).into_any_element(),
        };

        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            // Top Header: Large frosted gear squircle + Settings title + Back button
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .min_w_0()
                            .child(
                                div()
                                    .size(px(46.0))
                                    .rounded(px(12.0))
                                    .bg(palette.card_bg)
                                    .border_1()
                                    .border_color(palette.card_border)
                                    .shadow_sm()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .flex_shrink_0()
                                    .child(lucide_svg(
                                        IconName::SETTINGS,
                                        26.0,
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
                                            .child("Settings"),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_xs()
                                            .text_color(palette.text_secondary)
                                            .child("Customize your experience"),
                                    ),
                            ),
                    )
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
                                this.show_settings = false;
                                cx.notify();
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

impl Render for TrayPopoverView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette =
            ThemePalette::for_theme(self.snapshot.theme, self.snapshot.window_style);

        let content: AnyElement = if self.show_settings {
            self.render_settings_view(palette, cx).into_any_element()
        } else {
            self.render_mixer_view(palette, cx).into_any_element()
        };

        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .p(px(16.0))
            .rounded(px(20.0))
            .bg(palette.panel_bg)
            .border_1()
            .border_color(palette.panel_border)
            .shadow_lg()
            .text_color(palette.text_primary)
            .child(content)
    }
}
