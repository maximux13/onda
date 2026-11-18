use std::borrow::Cow;

use gpui::prelude::*;
use gpui::{AssetSource, Hsla, Result, SharedString, Svg, px, svg};

/// Official Lucide icon asset paths registered with [`LucideAssets`].
#[expect(
    non_snake_case,
    reason = "PascalCase module name acts as an icon namespace"
)]
pub mod IconName {
    pub const AUDIO_LINES: &str = "icons/audio-lines.svg";
    pub const VOLUME_2: &str = "icons/volume-2.svg";
    pub const VOLUME_X: &str = "icons/volume-x.svg";
    pub const SETTINGS: &str = "icons/settings.svg";
    pub const PIN: &str = "icons/pin.svg";
    pub const HEADPHONES: &str = "icons/headphones.svg";
    pub const SPARKLES: &str = "icons/sparkles.svg";
    pub const CHEVRON_DOWN: &str = "icons/chevron-down.svg";
    pub const KEYBOARD: &str = "icons/keyboard.svg";
    pub const BELL: &str = "icons/bell.svg";
    pub const INFO: &str = "icons/info.svg";
    pub const SUN: &str = "icons/sun.svg";
    pub const ARROW_LEFT: &str = "icons/arrow-left.svg";
}

/// Embedded [`AssetSource`] providing official Lucide `.svg` vector icons.
pub struct LucideAssets;

impl AssetSource for LucideAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
            IconName::AUDIO_LINES => include_bytes!("../../assets/icons/audio-lines.svg"),
            IconName::VOLUME_2 => include_bytes!("../../assets/icons/volume-2.svg"),
            IconName::VOLUME_X => include_bytes!("../../assets/icons/volume-x.svg"),
            IconName::SETTINGS => include_bytes!("../../assets/icons/settings.svg"),
            IconName::PIN => include_bytes!("../../assets/icons/pin.svg"),
            IconName::HEADPHONES => include_bytes!("../../assets/icons/headphones.svg"),
            IconName::SPARKLES => include_bytes!("../../assets/icons/sparkles.svg"),
            IconName::CHEVRON_DOWN => include_bytes!("../../assets/icons/chevron-down.svg"),
            IconName::KEYBOARD => include_bytes!("../../assets/icons/keyboard.svg"),
            IconName::BELL => include_bytes!("../../assets/icons/bell.svg"),
            IconName::INFO => include_bytes!("../../assets/icons/info.svg"),
            IconName::SUN => include_bytes!("../../assets/icons/sun.svg"),
            IconName::ARROW_LEFT => include_bytes!("../../assets/icons/arrow-left.svg"),
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let all = [
            IconName::AUDIO_LINES,
            IconName::VOLUME_2,
            IconName::VOLUME_X,
            IconName::SETTINGS,
            IconName::PIN,
            IconName::HEADPHONES,
            IconName::SPARKLES,
            IconName::CHEVRON_DOWN,
            IconName::KEYBOARD,
            IconName::BELL,
            IconName::INFO,
            IconName::SUN,
            IconName::ARROW_LEFT,
        ];
        Ok(all
            .into_iter()
            .filter(|p| p.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}

/// Renders an embedded Lucide SVG vector icon at `size_px` tinted with `color`.
pub fn lucide_svg(path: &'static str, size_px: f32, color: impl Into<Hsla>) -> Svg {
    svg()
        .path(path)
        .size(px(size_px))
        .text_color(color)
        .flex_shrink_0()
}
