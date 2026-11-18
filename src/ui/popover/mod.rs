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
    Animation, AnimationExt, AnyElement, Context, Image, ImageFormat, SharedString, Window, div,
    ease_out_quint, px, rgb,
};

use crate::audio::AudioEngine;
use crate::state::MixerSnapshot;
use crate::ui::ThemePalette;

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
            if !self.icon_cache.contains_key(&app.info.bundle_id)
                && let Some(bytes) = &app.info.icon_png
                && !bytes.is_empty()
            {
                let image = Arc::new(Image::from_bytes(ImageFormat::Png, bytes.clone()));
                self.icon_cache.insert(app.info.bundle_id.clone(), image);
            }
        }
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = ThemePalette::for_theme(self.snapshot.theme, self.snapshot.window_style);

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
            .with_animation(
                "popover-enter",
                Animation::new(Duration::from_millis(180)).with_easing(ease_out_quint()),
                |el, delta| {
                    let (opacity, offset_px) = popover_enter_metrics(delta);
                    el.opacity(opacity).mt(px(offset_px))
                },
            )
    }
}

fn popover_enter_metrics(delta: f32) -> (f32, f32) {
    let t = delta.clamp(0.0, 1.0);
    let opacity = t;
    let top_offset_px = (1.0 - t) * -8.0;
    (opacity, top_offset_px)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popover_enter_metrics_should_start_transparent_and_offset_when_delta_zero() {
        let (opacity, offset) = popover_enter_metrics(0.0);
        assert_eq!(opacity, 0.0);
        assert_eq!(offset, -8.0);
    }

    #[test]
    fn popover_enter_metrics_should_settle_opaque_and_aligned_when_delta_one() {
        let (opacity, offset) = popover_enter_metrics(1.0);
        assert_eq!(opacity, 1.0);
        assert_eq!(offset, 0.0);
    }

    #[test]
    fn popover_enter_metrics_should_clamp_out_of_range_deltas() {
        assert_eq!(popover_enter_metrics(-0.5), (0.0, -8.0));
        assert_eq!(popover_enter_metrics(1.5), (1.0, 0.0));
    }
}
