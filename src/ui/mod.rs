#![allow(dead_code)]
pub mod icons;
pub mod popover;
pub mod slider;

use gpui::{Rgba, rgb, rgba};

use crate::state::{ThemeMode, WindowStyle};

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
            ThemeMode::Auto | ThemeMode::Dark => true,
            ThemeMode::Light => false,
        }
    }

    /// Returns the color palette corresponding to the active [`ThemeMode`] and [`WindowStyle`].
    pub fn for_theme(theme: ThemeMode, window_style: WindowStyle) -> Self {
        let is_dark = Self::resolve_is_dark(theme);
        if is_dark {
            let panel_bg = match window_style {
                WindowStyle::Transparent => rgba(0x283252c8),
                WindowStyle::Solid => rgba(0x1e243bf8),
            };
            Self {
                is_dark: true,
                panel_bg,
                panel_border: rgba(0xffffff44),
                card_bg: rgba(0xffffff18),
                card_border: rgba(0xffffff2c),
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
            let panel_bg = match window_style {
                WindowStyle::Transparent => rgba(0xeef2fcdb),
                WindowStyle::Solid => rgba(0xf4f6fcfc),
            };
            Self {
                is_dark: false,
                panel_bg,
                panel_border: rgba(0x181c2e2e),
                card_bg: rgba(0xffffffb8),
                card_border: rgba(0x181c2e20),
                row_hover_bg: rgba(0x181c2e12),
                row_selected_bg: rgba(0x1a73e81a),
                row_selected_border: rgba(0x1a73e848),
                text_primary: rgb(0x181c2e),
                text_secondary: rgba(0x3a4260b8),
                section_header: rgba(0x3a426099),
                slider_track: rgba(0x00000018),
                slider_fill: rgb(0x1a73e8),
                slider_muted_fill: rgba(0x8e8e9366),
                accent_blue: rgb(0x1a73e8),
                device_badge_inactive: rgba(0x00000010),
                device_badge_active: rgb(0x1a73e8),
                divider: rgba(0x181c2e18),
                pill_bg: rgba(0x181c2e16),
                pill_hover_bg: rgba(0x181c2e24),
                vu_unlit: rgba(0x00000018),
                mute_red: rgb(0xff3b30),
            }
        }
    }
}
