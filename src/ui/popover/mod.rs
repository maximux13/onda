mod app_row;
mod device_row;
mod eq_view;
mod mixer_view;
mod settings_view;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, FontWeight, Image, ImageFormat, SharedString, Window,
    WindowBackgroundAppearance, div, px, rgb,
};

use crate::audio::AudioEngine;
use crate::state::{MixerSnapshot, WindowStyle};
use crate::ui::ThemePalette;
use crate::ui::icons::lucide_svg;
use crate::updater::{UpdateController, UpdateState, relaunch_app};

/// Sidebar tab in the `Settings` view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsTab {
    General,
    Shortcuts,
    Equalizer,
    Notifications,
    About,
}

/// Glassmorphic macOS `Onda` & `Settings` popover view.
pub struct TrayPopoverView {
    pub(super) engine: Arc<Mutex<AudioEngine>>,
    pub(super) updater: Arc<Mutex<UpdateController>>,
    pub(super) update_banner_dismissed: bool,
    pub(super) snapshot: MixerSnapshot,
    pub(super) icon_cache: HashMap<String, Arc<Image>>,
    pub(super) expanded_routing_app: Option<String>,
    pub(super) is_refreshing: bool,
    pub(super) show_settings: bool,
    pub(super) settings_tab: SettingsTab,
    pub(super) is_pinned: bool,
    pub(super) advanced_mode: bool,
    pub(super) eq_collapsed: bool,
    pub(super) show_output_picker: bool,
    pub(super) show_input_picker: bool,
    pub(super) launch_at_login: bool,
    pub(super) view_transition_id: u64,
    last_window_style: Option<WindowStyle>,
    last_is_dark: Option<bool>,
}

impl TrayPopoverView {
    /// Creates a new [`TrayPopoverView`], loads the initial mixer snapshot,
    /// and starts a 150ms background timer so VU meters and app lists stay live.
    pub fn new(
        engine: Arc<Mutex<AudioEngine>>,
        updater: Arc<Mutex<UpdateController>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            engine,
            updater,
            update_banner_dismissed: false,
            snapshot: MixerSnapshot::default(),
            icon_cache: HashMap::new(),
            expanded_routing_app: None,
            is_refreshing: false,
            show_settings: false,
            settings_tab: SettingsTab::General,
            is_pinned: false,
            advanced_mode: false,
            eq_collapsed: false,
            show_output_picker: false,
            show_input_picker: false,
            launch_at_login: crate::launch_agent::is_launch_at_login_enabled(),
            view_transition_id: 0,
            last_window_style: None,
            last_is_dark: None,
        };
        view.refresh(cx);
        view.advanced_mode = view.snapshot.advanced_mode;
        view.eq_collapsed = view.snapshot.eq_collapsed;

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

    /// Returns the target popover height for the current mode.
    pub(super) fn current_popover_height(&self) -> f64 {
        if self.advanced_mode {
            crate::POPOVER_ADVANCED_HEIGHT
        } else {
            crate::POPOVER_HEIGHT
        }
    }

    /// Returns a snapshot of the current auto-updater state.
    pub(super) fn update_state(&self) -> UpdateState {
        self.updater.lock().map(|u| u.state()).unwrap_or_default()
    }

    /// Relaunches the updated `.app` bundle and quits the current instance if launch succeeds.
    pub(super) fn relaunch_and_quit(&self, app_path: &std::path::Path, cx: &mut Context<Self>) {
        if let Err(err) = relaunch_app(app_path) {
            eprintln!("{err}");
            return;
        }
        cx.quit();
    }

    /// Renders a compact top banner when an update is available, downloading, ready, or errored.
    fn render_update_banner(
        &self,
        palette: ThemePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let banner_shell = |border_color| {
            div()
                .w_full()
                .px(px(10.0))
                .py(px(7.0))
                .rounded(px(10.0))
                .bg(palette.card_bg)
                .border_1()
                .border_color(border_color)
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
        };

        let dismiss_btn = |id: &'static str, cx: &mut Context<Self>| {
            div()
                .id(id)
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .hover(move |s| s.bg(palette.pill_hover_bg))
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(palette.text_secondary)
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.update_banner_dismissed = true;
                    cx.notify();
                }))
                .child("×")
        };

        match self.update_state() {
            UpdateState::Available(info) if !self.update_banner_dismissed => Some(
                banner_shell(palette.accent_blue)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.text_primary)
                            .child(format!("New version v{} available", info.version)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .flex_shrink_0()
                            .child(
                                div()
                                    .id("update-banner-download-btn")
                                    .px(px(8.0))
                                    .py(px(3.0))
                                    .rounded(px(6.0))
                                    .bg(palette.accent_blue)
                                    .cursor_pointer()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0xffffff))
                                    .on_click(cx.listener(|this, _event, _window, _cx| {
                                        UpdateController::trigger_download(Arc::clone(
                                            &this.updater,
                                        ));
                                    }))
                                    .child("Update"),
                            )
                            .child(dismiss_btn("update-banner-dismiss-available", cx)),
                    )
                    .into_any_element(),
            ),
            UpdateState::Downloading { version } => Some(
                banner_shell(palette.card_border)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.text_secondary)
                            .child(format!("Downloading Onda v{version}...")),
                    )
                    .into_any_element(),
            ),
            UpdateState::ReadyToRestart { version, app_path } => Some(
                banner_shell(palette.accent_blue)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(palette.text_primary)
                            .child(format!("Onda v{version} ready")),
                    )
                    .child(
                        div()
                            .id("update-banner-restart-btn")
                            .flex_shrink_0()
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(px(6.0))
                            .bg(palette.accent_blue)
                            .cursor_pointer()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xffffff))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.relaunch_and_quit(&app_path, cx);
                            }))
                            .child("Restart & Update"),
                    )
                    .into_any_element(),
            ),
            UpdateState::Error(msg) if !self.update_banner_dismissed => Some(
                banner_shell(palette.mute_red)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(palette.text_secondary)
                            .child(msg),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .flex_shrink_0()
                            .child(
                                div()
                                    .id("update-banner-retry-btn")
                                    .px(px(8.0))
                                    .py(px(3.0))
                                    .rounded(px(6.0))
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
                                    .child("Retry"),
                            )
                            .child(dismiss_btn("update-banner-dismiss-error", cx)),
                    )
                    .into_any_element(),
            ),
            _ => None,
        }
    }

    /// Advances the view transition ID so GPUI replays the view-switch animation.
    pub(super) fn bump_view_transition(&mut self) {
        self.view_transition_id = self.view_transition_id.wrapping_add(1);
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

    pub(super) fn sync_icon_cache(&mut self) {
        for app in &self.snapshot.apps {
            if !self.icon_cache.contains_key(&app.info.bundle_id)
                && let Some(bytes) = &app.info.icon_png
                && !bytes.is_empty()
            {
                let image = Arc::new(Image::from_bytes(ImageFormat::Png, bytes.clone()));
                self.icon_cache.insert(app.info.bundle_id.clone(), image);
            }
        }
    }

    /// Renders the shared left-side header icon badge, title, and subtitle so
    /// the mixer and settings headers match in icon size and typography.
    pub(super) fn render_header_title(
        &self,
        icon_path: &'static str,
        title: &'static str,
        subtitle: &'static str,
        palette: ThemePalette,
    ) -> impl IntoElement {
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
                    .child(lucide_svg(icon_path, 22.0, palette.text_primary)),
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
                            .child(title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(palette.text_secondary)
                            .child(subtitle),
                    ),
            )
    }

    /// Renders an iOS-style pill toggle switch matching the mockup.
    pub(super) fn render_ios_toggle(
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
}

impl Render for TrayPopoverView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = ThemePalette::for_theme_with_opacity(
            self.snapshot.theme,
            self.snapshot.window_style,
            self.snapshot.glass_opacity,
        );
        let is_transparent = self.snapshot.window_style == WindowStyle::Transparent;

        if self.last_window_style != Some(self.snapshot.window_style)
            || self.last_is_dark != Some(palette.is_dark)
        {
            window.set_background_appearance(if is_transparent {
                WindowBackgroundAppearance::Blurred
            } else {
                WindowBackgroundAppearance::Opaque
            });
            crate::platform::configure_popover_glass(
                crate::POPOVER_WIDTH,
                self.current_popover_height(),
                is_transparent,
                palette.is_dark,
            );
            self.last_window_style = Some(self.snapshot.window_style);
            self.last_is_dark = Some(palette.is_dark);
        }

        let content: AnyElement = if self.show_settings {
            self.render_settings_view(palette, cx).into_any_element()
        } else {
            self.render_mixer_view(palette, cx).into_any_element()
        };

        let mut root = div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .p(px(16.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(palette.panel_border)
            .bg(palette.panel_bg)
            .text_color(palette.text_primary);

        if let Some(banner) = self.render_update_banner(palette, cx) {
            root = root.child(div().mb(px(8.0)).child(banner));
        }

        root.child(div().flex_1().min_h_0().w_full().h_full().child(content))
    }
}
