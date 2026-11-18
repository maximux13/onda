#![allow(dead_code)]
pub mod icons;
pub mod popover;
pub mod slider;

use gpui::{Rgba, rgb, rgba};

use crate::state::{DEFAULT_GLASS_OPACITY, ThemeMode, WindowStyle};

/// Complete color palette for the glassmorphic Onda & Settings UI.
#[derive(Clone, Copy, Debug)]
pub struct ThemePalette {
    pub is_dark: bool,
    pub panel_bg: Rgba,
    pub panel_border: Rgba,
    pub card_bg: Rgba,
    pub card_border: Rgba,
    pub row_hover_bg: Rgba,
    pub row_selected_bg: Rgba,
    pub row_selected_border: Rgba,
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub section_header: Rgba,
    pub slider_track: Rgba,
    pub slider_fill: Rgba,
    pub slider_muted_fill: Rgba,
    pub accent_blue: Rgba,
    pub device_badge_inactive: Rgba,
    pub device_badge_active: Rgba,
    pub divider: Rgba,
    pub pill_bg: Rgba,
    pub pill_hover_bg: Rgba,
    pub vu_unlit: Rgba,
    pub mute_red: Rgba,
}

impl ThemePalette {
    /// Resolves whether a [`ThemeMode`] renders with the dark glass palette.
    pub fn resolve_is_dark(theme: ThemeMode) -> bool {
        match theme {
            ThemeMode::Auto => crate::platform::is_system_dark_mode(),
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
        }
    }

    /// Returns the color palette corresponding to the active [`ThemeMode`] and [`WindowStyle`]
    /// using [`DEFAULT_GLASS_OPACITY`].
    pub fn for_theme(theme: ThemeMode, window_style: WindowStyle) -> Self {
        Self::for_theme_with_opacity(theme, window_style, DEFAULT_GLASS_OPACITY)
    }

    /// Returns the color palette corresponding to the active [`ThemeMode`], [`WindowStyle`],
    /// and normalized `glass_opacity` slider value (`0.0..=1.0`, mapped onto `60%..=100%`
    /// light glass alpha) for `WindowStyle::Transparent`.
    pub fn for_theme_with_opacity(
        theme: ThemeMode,
        window_style: WindowStyle,
        glass_opacity: f32,
    ) -> Self {
        let is_dark = Self::resolve_is_dark(theme);
        let t = glass_opacity.clamp(0.0, 1.0);
        let light_alpha = 0.60 + 0.40 * t;
        let dark_alpha = 0.25 + 0.36 * t;
        if is_dark {
            let panel_bg = match window_style {
                WindowStyle::Transparent => Rgba {
                    r: 0.086,
                    g: 0.114,
                    b: 0.188,
                    a: dark_alpha,
                },
                WindowStyle::Solid => rgba(0x1e243bff),
            };
            Self {
                is_dark: true,
                panel_bg,
                panel_border: rgba(0xffffff3c),
                card_bg: rgba(0xffffff16),
                card_border: rgba(0xffffff28),
                row_hover_bg: rgba(0xffffff1a),
                row_selected_bg: rgba(0xffffff22),
                row_selected_border: rgba(0xffffff3c),
                text_primary: rgb(0xffffff),
                text_secondary: rgba(0xdbe4ffaa),
                section_header: rgba(0xdbe4ff99),
                slider_track: rgba(0xffffff24),
                slider_fill: rgb(0x2582ff),
                slider_muted_fill: rgba(0xffffff38),
                accent_blue: rgb(0x2582ff),
                device_badge_inactive: rgba(0xffffff18),
                device_badge_active: rgb(0x2582ff),
                divider: rgba(0xffffff20),
                pill_bg: rgba(0xffffff20),
                pill_hover_bg: rgba(0xffffff30),
                vu_unlit: rgba(0xffffff22),
                mute_red: rgb(0xff4d4f),
            }
        } else {
            let (panel_bg, card_bg, card_border, pill_bg, pill_hover_bg) = match window_style {
                WindowStyle::Transparent => (
                    Rgba {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: light_alpha,
                    },
                    rgba(0xffffff1a),
                    rgba(0xffffffb8),
                    rgba(0xffffff26),
                    rgba(0xffffff44),
                ),
                WindowStyle::Solid => (
                    rgba(0xf4f6fcff),
                    rgba(0xffffffeb),
                    rgba(0x181c2e1c),
                    rgba(0x181c2e14),
                    rgba(0x181c2e22),
                ),
            };
            Self {
                is_dark: false,
                panel_bg,
                panel_border: rgba(0xffffffb8),
                card_bg,
                card_border,
                row_hover_bg: rgba(0xffffff58),
                row_selected_bg: rgba(0x1a73e81e),
                row_selected_border: rgba(0x1a73e852),
                text_primary: rgb(0x141828),
                text_secondary: rgba(0x1e2438dc),
                section_header: rgba(0x1e2438c0),
                slider_track: rgba(0x00000018),
                slider_fill: rgb(0x1a73e8),
                slider_muted_fill: rgba(0x8e8e9366),
                accent_blue: rgb(0x1a73e8),
                device_badge_inactive: rgba(0x8e8e9338),
                device_badge_active: rgb(0x1a73e8),
                divider: rgba(0x181c2e18),
                pill_bg,
                pill_hover_bg,
                vu_unlit: rgba(0x00000018),
                mute_red: rgb(0xff3b30),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_is_dark_respects_explicit_dark_and_light() {
        assert!(ThemePalette::resolve_is_dark(ThemeMode::Dark));
        assert!(!ThemePalette::resolve_is_dark(ThemeMode::Light));
    }

    #[test]
    fn resolve_is_dark_auto_matches_system_dark_mode() {
        assert_eq!(
            ThemePalette::resolve_is_dark(ThemeMode::Auto),
            crate::platform::is_system_dark_mode()
        );
    }

    #[test]
    fn transparent_window_style_uses_translucent_panel_bg_for_glassmorphism() {
        let dark = ThemePalette::for_theme(ThemeMode::Dark, WindowStyle::Transparent);
        let light = ThemePalette::for_theme(ThemeMode::Light, WindowStyle::Transparent);
        assert!(
            dark.panel_bg.a < 0.45,
            "dark glass panel_bg alpha was {}",
            dark.panel_bg.a
        );
        assert!(
            light.panel_bg.a < 0.85,
            "light glass panel_bg alpha was {}",
            light.panel_bg.a
        );

        let dark_solid = ThemePalette::for_theme(ThemeMode::Dark, WindowStyle::Solid);
        let light_solid = ThemePalette::for_theme(ThemeMode::Light, WindowStyle::Solid);
        assert!(dark_solid.panel_bg.a > 0.95);
        assert!(light_solid.panel_bg.a > 0.95);
    }

    #[test]
    fn glass_opacity_slider_maps_zero_to_hundred_onto_sixty_to_hundred_percent_alpha() {
        let min_light =
            ThemePalette::for_theme_with_opacity(ThemeMode::Light, WindowStyle::Transparent, 0.0);
        let mid_light =
            ThemePalette::for_theme_with_opacity(ThemeMode::Light, WindowStyle::Transparent, 0.5);
        let max_light =
            ThemePalette::for_theme_with_opacity(ThemeMode::Light, WindowStyle::Transparent, 1.0);
        assert!((min_light.panel_bg.a - 0.60).abs() < 1e-5);
        assert!((mid_light.panel_bg.a - 0.80).abs() < 1e-5);
        assert!((max_light.panel_bg.a - 1.00).abs() < 1e-5);
    }
}
