use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    App, Bounds, Div, ElementId, MouseButton, Pixels, Point, Rgba, Window, div, px, relative, rgb,
    rgba,
};

use crate::state::{ThemeMode, WindowStyle};
use crate::ui::ThemePalette;

type SliderCallback = Rc<dyn Fn(f32, &mut Window, &mut App)>;

struct SliderColors {
    track: Rgba,
    track_hover: Rgba,
    fill: Rgba,
    thumb: Rgba,
    thumb_border: Rgba,
    thumb_hover_bg: Rgba,
    thumb_hover_border: Rgba,
}

fn resolve_slider_colors(palette: ThemePalette, active: bool, accent: Rgba) -> SliderColors {
    let (track_hover, thumb_inactive, thumb_border, thumb_hover_border_inactive) =
        if palette.is_dark {
            (
                rgba(0xffffff36),
                rgba(0xe2e8f0dd),
                rgba(0x00000038),
                rgba(0xffffff66),
            )
        } else {
            (
                rgba(0x00000024),
                rgba(0xf8fafce6),
                rgba(0x181c2e28),
                rgba(0x181c2e48),
            )
        };

    SliderColors {
        track: palette.slider_track,
        track_hover,
        fill: if active {
            accent
        } else {
            palette.slider_muted_fill
        },
        thumb: if active {
            rgb(0xffffff)
        } else {
            thumb_inactive
        },
        thumb_border,
        thumb_hover_bg: rgb(0xffffff),
        thumb_hover_border: if active {
            accent
        } else {
            thumb_hover_border_inactive
        },
    }
}

/// Normalizes a pointer coordinate along an axis within `[start, start + span]` to `[0.0, 1.0]`.
fn normalized_axis_ratio(pos: Pixels, start: Pixels, span: Pixels) -> Option<f32> {
    if span <= px(0.0) {
        return None;
    }
    Some(((pos - start) / span).clamp(0.0, 1.0))
}

/// Attaches shared track-bounds tracking, mouse-down, and drag-move handlers to a slider container.
fn bind_drag_handlers(
    element: Div,
    on_change: SliderCallback,
    double_click_value: Option<f32>,
    value_from_point: impl Fn(Point<Pixels>, Bounds<Pixels>) -> Option<f32> + 'static,
) -> Div {
    let track_bounds: Rc<Cell<Bounds<Pixels>>> = Rc::new(Cell::new(Bounds::default()));
    let bounds_prepaint = Rc::clone(&track_bounds);
    let bounds_down = Rc::clone(&track_bounds);
    let bounds_move = Rc::clone(&track_bounds);

    let on_change_down = Rc::clone(&on_change);
    let on_change_move = on_change;

    let value_from_point = Rc::new(value_from_point);
    let value_down = Rc::clone(&value_from_point);
    let value_move = value_from_point;

    element
        .on_children_prepainted(move |children_bounds, _, _| {
            if let Some(bounds) = children_bounds.first() {
                bounds_prepaint.set(*bounds);
            }
        })
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            if let Some(reset_val) = double_click_value
                && event.click_count >= 2
            {
                (on_change_down)(reset_val, window, cx);
                return;
            }
            if let Some(val) = (value_down)(event.position, bounds_down.get()) {
                (on_change_down)(val, window, cx);
            }
        })
        .on_mouse_move(move |event, window, cx| {
            if event.dragging()
                && let Some(val) = (value_move)(event.position, bounds_move.get())
            {
                (on_change_move)(val, window, cx);
            }
        })
}

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
    on_change: SliderCallback,
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
        normalized_axis_ratio(x, bounds.left(), bounds.size.width).map(|ratio| ratio * max)
    }
}

impl RenderOnce for VolumeSlider {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let palette = ThemePalette::for_theme(self.theme, WindowStyle::Transparent);
        let max = self.max.max(0.01);
        let fraction = (self.value / max).clamp(0.0, 1.0);
        let colors = resolve_slider_colors(palette, !self.muted, rgb(self.accent_color));

        let mut track = div()
            .relative()
            .w_full()
            .h(px(6.0))
            .rounded_full()
            .bg(colors.track)
            .hover(move |s| s.bg(colors.track_hover))
            .child(
                div()
                    .h_full()
                    .w(relative(fraction))
                    .min_w(px(6.0))
                    .rounded_full()
                    .bg(colors.fill),
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

        // Always-visible circular white thumb with subtle border and hover highlight.
        track = track.child(
            div()
                .absolute()
                .top(px(-5.0))
                .left(relative(fraction))
                .ml(px(-8.0))
                .size(px(16.0))
                .rounded_full()
                .bg(colors.thumb)
                .border_1()
                .border_color(colors.thumb_border)
                .shadow_md()
                .hover(move |s| {
                    s.bg(colors.thumb_hover_bg)
                        .border_color(colors.thumb_hover_border)
                        .shadow_lg()
                }),
        );

        let _ = &self.id;

        let container = div()
            .h(px(22.0))
            .w_full()
            .px(px(6.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .child(track);

        bind_drag_handlers(container, self.on_change, None, move |pos, bounds| {
            Self::value_from_x(pos.x, bounds, max)
        })
    }
}

/// Interactive vertical slider for a single band of the 10-band Equalizer (`[-12.0, +12.0]` dB).
#[derive(IntoElement)]
pub struct EqVerticalSlider {
    id: ElementId,
    gain_db: f32,
    enabled: bool,
    theme: ThemeMode,
    on_change: SliderCallback,
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
        normalized_axis_ratio(y, bounds.top(), bounds.size.height)
            .map(|ratio| (-12.0 + (1.0 - ratio) * 24.0).clamp(-12.0, 12.0))
    }
}

impl RenderOnce for EqVerticalSlider {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let palette = ThemePalette::for_theme(self.theme, WindowStyle::Transparent);
        let ratio = ((self.gain_db + 12.0) / 24.0).clamp(0.0, 1.0);
        let colors = resolve_slider_colors(palette, self.enabled, palette.accent_blue);

        let track = div()
            .relative()
            .w(px(5.0))
            .h_full()
            .rounded_full()
            .bg(colors.track)
            .hover(move |s| s.bg(colors.track_hover))
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
                    .bg(colors.fill),
            )
            // Always-visible 13px white circular thumb with subtle border and hover highlight
            .child(
                div()
                    .absolute()
                    .left(px(-4.0))
                    .top(relative(1.0 - ratio))
                    .mt(px(-6.5))
                    .size(px(13.0))
                    .rounded_full()
                    .bg(colors.thumb)
                    .border_1()
                    .border_color(colors.thumb_border)
                    .shadow_md()
                    .hover(move |s| {
                        s.bg(colors.thumb_hover_bg)
                            .border_color(colors.thumb_hover_border)
                            .shadow_lg()
                    }),
            );

        let _ = &self.id;

        let container = div()
            .w(px(28.0))
            .h(px(92.0))
            .py(px(6.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .child(track);

        bind_drag_handlers(container, self.on_change, Some(0.0), |pos, bounds| {
            Self::gain_from_y(pos.y, bounds)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, size};

    fn sample_bounds() -> Bounds<Pixels> {
        Bounds {
            origin: point(px(10.0), px(20.0)),
            size: size(px(200.0), px(80.0)),
        }
    }

    #[test]
    fn value_from_x_should_return_zero_when_x_at_or_before_bounds_left() {
        let bounds = sample_bounds();
        assert_eq!(VolumeSlider::value_from_x(px(10.0), bounds, 1.5), Some(0.0));
        assert_eq!(
            VolumeSlider::value_from_x(px(-25.0), bounds, 1.5),
            Some(0.0)
        );
    }

    #[test]
    fn value_from_x_should_return_max_when_x_at_or_after_bounds_right() {
        let bounds = sample_bounds();
        assert_eq!(
            VolumeSlider::value_from_x(px(210.0), bounds, 1.5),
            Some(1.5)
        );
        assert_eq!(
            VolumeSlider::value_from_x(px(500.0), bounds, 1.5),
            Some(1.5)
        );
    }

    #[test]
    fn value_from_x_should_interpolate_proportionally_when_x_within_bounds() {
        let bounds = sample_bounds();
        let mid = VolumeSlider::value_from_x(px(110.0), bounds, 2.0).unwrap();
        assert!((mid - 1.0).abs() < 1e-5);
    }

    #[test]
    fn value_from_x_should_return_none_when_bounds_width_non_positive() {
        let zero_width = Bounds {
            origin: point(px(10.0), px(20.0)),
            size: size(px(0.0), px(80.0)),
        };
        assert_eq!(VolumeSlider::value_from_x(px(10.0), zero_width, 1.0), None);
    }

    #[test]
    fn gain_from_y_should_return_plus_twelve_when_y_at_or_above_bounds_top() {
        let bounds = sample_bounds();
        assert_eq!(EqVerticalSlider::gain_from_y(px(20.0), bounds), Some(12.0));
        assert_eq!(EqVerticalSlider::gain_from_y(px(-5.0), bounds), Some(12.0));
    }

    #[test]
    fn gain_from_y_should_return_minus_twelve_when_y_at_or_below_bounds_bottom() {
        let bounds = sample_bounds();
        assert_eq!(
            EqVerticalSlider::gain_from_y(px(100.0), bounds),
            Some(-12.0)
        );
        assert_eq!(
            EqVerticalSlider::gain_from_y(px(250.0), bounds),
            Some(-12.0)
        );
    }

    #[test]
    fn gain_from_y_should_return_zero_db_when_y_at_bounds_midpoint() {
        let bounds = sample_bounds();
        let gain = EqVerticalSlider::gain_from_y(px(60.0), bounds).unwrap();
        assert!(gain.abs() < 1e-5);
    }

    #[test]
    fn gain_from_y_should_return_none_when_bounds_height_non_positive() {
        let zero_height = Bounds {
            origin: point(px(10.0), px(20.0)),
            size: size(px(200.0), px(0.0)),
        };
        assert_eq!(EqVerticalSlider::gain_from_y(px(20.0), zero_height), None);
    }
}
