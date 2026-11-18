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
    pub(super) snapshot: MixerSnapshot,
    pub(super) icon_cache: HashMap<String, Arc<Image>>,
    pub(super) expanded_routing_app: Option<String>,
    pub(super) is_refreshing: bool,
    pub(super) show_settings: bool,
    pub(super) settings_tab: SettingsTab,
    pub(super) is_pinned: bool,
    pub(super) advanced_mode: bool,
    pub(super) show_output_picker: bool,
    pub(super) launch_at_login: bool,
    pub(super) view_transition_id: u64,
    last_window_style: Option<WindowStyle>,
    last_is_dark: Option<bool>,
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
            view_transition_id: 0,
            last_window_style: None,
            last_is_dark: None,
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
                crate::POPOVER_HEIGHT,
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

        div()
            .w_full()
            .h_full()
            .flex()
            .flex_col()
            .p(px(16.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(palette.panel_border)
            .bg(palette.panel_bg)
            .text_color(palette.text_primary)
            .child(div().flex_1().min_h_0().w_full().h_full().child(content))
    }
}
