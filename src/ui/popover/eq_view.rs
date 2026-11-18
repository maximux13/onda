use gpui::prelude::*;
use gpui::{Context, FontWeight, SharedString, div, px, rgb};

use crate::state::EQ_LABELS;
use crate::ui::ThemePalette;
use crate::ui::icons::{IconName, lucide_svg};
use crate::ui::slider::EqVerticalSlider;

use super::TrayPopoverView;

impl TrayPopoverView {
    pub(super) fn select_eq_app(&mut self, bundle_id: Option<String>, cx: &mut Context<Self>) {
        self.snapshot.selected_eq_app = bundle_id.clone();
        if let Ok(mut engine) = self.engine.lock() {
            engine.select_eq_app(bundle_id);
            self.snapshot = engine.snapshot();
        }
        cx.notify();
    }

    pub(super) fn toggle_active_eq(&mut self, cx: &mut Context<Self>) {
        let current_enabled = self.active_eq_state().1;
        let next = !current_enabled;
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_active_eq_enabled(next);
            self.snapshot = engine.snapshot();
        }
        cx.notify();
    }

    pub(super) fn set_eq_band(&mut self, band_idx: usize, gain_db: f32, cx: &mut Context<Self>) {
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

    pub(super) fn apply_eq_preset(&mut self, gains: [f32; 10], cx: &mut Context<Self>) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_active_eq_enabled(true);
            for (idx, gain) in gains.iter().copied().enumerate() {
                engine.set_active_eq_band(idx, gain);
            }
            self.snapshot = engine.snapshot();
        }
        cx.notify();
    }

    pub(super) fn reset_active_eq(&mut self, cx: &mut Context<Self>) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.reset_active_eq();
            self.snapshot = engine.snapshot();
        }
        cx.notify();
    }

    pub(super) fn active_eq_state(&self) -> (String, bool, [f32; 10], bool) {
        if let Some(ref bundle_id) = self.snapshot.selected_eq_app
            && let Some(row) = self
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
        (
            String::from("Global"),
            self.snapshot.global_eq_enabled,
            self.snapshot.global_eq_gains,
            false,
        )
    }

    /// Renders the 10-band Equalizer section (Global or per-app).
    pub(super) fn render_equalizer_section(
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
            let entity = cx.entity();

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

        let mut header_left = div().flex().items_center().gap(px(8.0)).min_w_0().child(
            div()
                .id("eq-collapse-toggle")
                .flex()
                .items_center()
                .gap(px(6.0))
                .cursor_pointer()
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.eq_collapsed = !this.eq_collapsed;
                    if let Ok(mut engine) = this.engine.lock() {
                        engine.set_eq_collapsed(this.eq_collapsed);
                    }
                    cx.notify();
                }))
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
                )
                .child(
                    div()
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded_full()
                        .bg(palette.pill_bg)
                        .hover(move |s| s.bg(palette.pill_hover_bg))
                        .flex()
                        .items_center()
                        .gap(px(3.0))
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(palette.text_secondary)
                        .child(if self.eq_collapsed { "Show" } else { "Hide" }),
                ),
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

        let mut section = div()
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
            );

        if !self.eq_collapsed {
            section = section.child(sliders_row);
        }

        section
    }
}
