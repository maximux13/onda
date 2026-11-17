use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    App, Bounds, ElementId, MouseButton, Pixels, Window, div, px, relative, rgb, rgba,
};

use crate::state::{ThemeMode, WindowStyle};
use crate::ui::ThemePalette;

/// Glassmorphic horizontal volume slider component for GPUI with per-app dominant accent color
/// and an always-visible circular white thumb matching the AudioMixer mockup.
#[derive(IntoElement)]
pub struct VolumeSlider {
    id: ElementId,
    value: f32,
    max: f32,
    muted: bool,
    accent_color: u32,
    theme: ThemeMode,
    on_change: Rc<dyn Fn(f32, &mut Window, &mut App)>,
}

impl VolumeSlider {
    /// Creates a new [`VolumeSlider`] with range `[0.0, max]`, custom `accent_color` (`0xRRGGBB`),
    /// and theme-aware frosted track styling.
    pub fn new(
        id: impl Into<ElementId>,
        value: f32,
        max: f32,
        muted: bool,
        accent_color: u32,
        theme: ThemeMode,
        on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            value,
            max,
            muted,
            accent_color,
            theme,
            on_change: Rc::new(on_change),
        }
    }

    fn value_from_x(x: Pixels, bounds: Bounds<Pixels>, max: f32) -> Option<f32> {
        if bounds.size.width <= px(0.0) {
            return None;
        }
        let ratio = ((x - bounds.left()) / bounds.size.width).clamp(0.0, 1.0);
        Some(ratio * max)
    }
}

impl RenderOnce for VolumeSlider {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let palette = ThemePalette::for_theme(self.theme, WindowStyle::Transparent);
        let max = self.max.max(0.01);
        let fraction = (self.value / max).clamp(0.0, 1.0);

        let track_bounds: Rc<Cell<Bounds<Pixels>>> = Rc::new(Cell::new(Bounds::default()));
        let bounds_prepaint = Rc::clone(&track_bounds);
        let bounds_down = Rc::clone(&track_bounds);
        let bounds_move = Rc::clone(&track_bounds);

        let on_change_down = Rc::clone(&self.on_change);
        let on_change_move = Rc::clone(&self.on_change);

        let track_color = if palette.is_dark {
            rgba(0xffffff24)
        } else {
            rgba(0x00000018)
        };

        let fill_color = if self.muted {
            palette.slider_muted_fill
        } else {
            rgb(self.accent_color)
        };

        let thumb_color = if self.muted {
            rgba(0xe2e8f0dd)
        } else {
            rgb(0xffffff)
        };

        let mut track = div()
            .relative()
            .w_full()
            .h(px(6.0))
            .rounded_full()
            .bg(track_color)
            .child(
                div()
                    .h_full()
                    .w(relative(fraction))
                    .min_w(px(6.0))
                    .rounded_full()
                    .bg(fill_color),
            );

        // Subtle unity-gain (100%) marker when slider supports boost amplification (> 1.0).
        if max > 1.01 {
            let unity_fraction = (1.0 / max).clamp(0.0, 1.0);
            let tick_color = if palette.is_dark {
                rgba(0xffffff44)
            } else {
                rgba(0x00000033)
            };
            track = track.child(
                div()
                    .absolute()
                    .top(px(-1.0))
                    .left(relative(unity_fraction))
                    .w(px(1.5))
                    .h(px(8.0))
                    .rounded_full()
                    .bg(tick_color),
            );
        }

        // Always-visible circular white thumb matching the AudioMixer mockup.
        track = track.child(
            div()
                .absolute()
                .top(px(-5.0))
                .left(relative(fraction))
                .ml(px(-8.0))
                .size(px(16.0))
                .rounded_full()
                .bg(thumb_color)
                .shadow_md(),
        );

        let _ = &self.id;

        div()
            .h(px(22.0))
            .w_full()
            .px(px(6.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .on_children_prepainted(move |children_bounds, _, _| {
                if let Some(bounds) = children_bounds.first() {
                    bounds_prepaint.set(*bounds);
                }
            })
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                if let Some(val) = Self::value_from_x(event.position.x, bounds_down.get(), max) {
                    (on_change_down)(val, window, cx);
                }
            })
            .on_mouse_move(move |event, window, cx| {
                if event.dragging() {
                    if let Some(val) =
                        Self::value_from_x(event.position.x, bounds_move.get(), max)
                    {
                        (on_change_move)(val, window, cx);
                    }
                }
            })
            .child(track)
    }
}

/// Interactive vertical slider for a single band of the 10-band Equalizer (`[-12.0, +12.0]` dB).
#[derive(IntoElement)]
pub struct EqVerticalSlider {
    id: ElementId,
    gain_db: f32,
    enabled: bool,
    theme: ThemeMode,
    on_change: Rc<dyn Fn(f32, &mut Window, &mut App)>,
}

impl EqVerticalSlider {
    /// Creates a new vertical EQ slider for `gain_db` in `[-12.0, 12.0]`.
    pub fn new(
        id: impl Into<ElementId>,
        gain_db: f32,
        enabled: bool,
        theme: ThemeMode,
        on_change: impl Fn(f32, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            gain_db: gain_db.clamp(-12.0, 12.0),
            enabled,
            theme,
            on_change: Rc::new(on_change),
        }
    }

    fn gain_from_y(y: Pixels, bounds: Bounds<Pixels>) -> Option<f32> {
        if bounds.size.height <= px(0.0) {
            return None;
        }
        let ratio = 1.0 - ((y - bounds.top()) / bounds.size.height).clamp(0.0, 1.0);
        Some((-12.0 + ratio * 24.0).clamp(-12.0, 12.0))
    }
}

impl RenderOnce for EqVerticalSlider {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let is_dark = ThemePalette::resolve_is_dark(self.theme);
        let ratio = ((self.gain_db + 12.0) / 24.0).clamp(0.0, 1.0);

        let track_bounds: Rc<Cell<Bounds<Pixels>>> = Rc::new(Cell::new(Bounds::default()));
        let bounds_prepaint = Rc::clone(&track_bounds);
        let bounds_down = Rc::clone(&track_bounds);
        let bounds_move = Rc::clone(&track_bounds);

        let on_change_down = Rc::clone(&self.on_change);
        let on_change_move = Rc::clone(&self.on_change);

        let track_color = if is_dark {
            rgba(0xffffff24)
        } else {
            rgba(0x00000018)
        };

        let fill_color = if self.enabled {
            rgb(0x2986ff)
        } else if is_dark {
            rgba(0xffffff38)
        } else {
            rgba(0x8e8e9366)
        };

        let thumb_color = if self.enabled {
            rgb(0xffffff)
        } else {
            rgba(0xe2e8f0cc)
        };

        let track = div()
            .relative()
            .w(px(5.0))
            .h_full()
            .rounded_full()
            .bg(track_color)
            // Bottom-up blue fill
            .child(
                div()
                    .absolute()
                    .bottom(px(0.0))
                    .left(px(0.0))
                    .w_full()
                    .h(relative(ratio))
                    .min_h(px(4.0))
                    .rounded_full()
                    .bg(fill_color),
            )
            // Always-visible 13px white circular thumb
            .child(
                div()
                    .absolute()
                    .left(px(-4.0))
                    .top(relative(1.0 - ratio))
                    .mt(px(-6.5))
                    .size(px(13.0))
                    .rounded_full()
                    .bg(thumb_color)
                    .shadow_md(),
            );

        let _ = &self.id;

        div()
            .w(px(28.0))
            .h(px(92.0))
            .py(px(6.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .on_children_prepainted(move |children_bounds, _, _| {
                if let Some(bounds) = children_bounds.first() {
                    bounds_prepaint.set(*bounds);
                }
            })
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                if event.click_count >= 2 {
                    (on_change_down)(0.0, window, cx);
                    return;
                }
                if let Some(db) = Self::gain_from_y(event.position.y, bounds_down.get()) {
                    (on_change_down)(db, window, cx);
                }
            })
            .on_mouse_move(move |event, window, cx| {
                if event.dragging() {
                    if let Some(db) = Self::gain_from_y(event.position.y, bounds_move.get()) {
                        (on_change_move)(db, window, cx);
                    }
                }
            })
            .child(track)
    }
}
