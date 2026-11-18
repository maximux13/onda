use std::borrow::Cow;

use gpui::prelude::*;
use gpui::{AssetSource, Hsla, Result, SharedString, Svg, px, svg};

macro_rules! lucide_icons {
    ($( $name:ident => $file:literal ),+ $(,)?) => {
        /// Official Lucide icon asset paths registered with [`LucideAssets`].
        #[expect(
            non_snake_case,
            reason = "PascalCase module name acts as an icon namespace"
        )]
        pub mod IconName {
            $(
                pub const $name: &str = concat!("icons/", $file);
            )+
        }

        /// Embedded [`AssetSource`] providing official Lucide `.svg` vector icons.
        pub struct LucideAssets;

        impl AssetSource for LucideAssets {
            fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
                let bytes: &'static [u8] = match path {
                    $(
                        IconName::$name => include_bytes!(concat!("../../assets/icons/", $file)),
                    )+
                    _ => return Ok(None),
                };
                Ok(Some(Cow::Borrowed(bytes)))
            }

            fn list(&self, path: &str) -> Result<Vec<SharedString>> {
                let all = [$(IconName::$name,)+];
                Ok(all
                    .into_iter()
                    .filter(|p| p.starts_with(path))
                    .map(SharedString::from)
                    .collect())
            }
        }
    };
}

lucide_icons! {
    AUDIO_LINES => "audio-lines.svg",
    VOLUME_2 => "volume-2.svg",
    VOLUME_X => "volume-x.svg",
    SETTINGS => "settings.svg",
    PIN => "pin.svg",
    HEADPHONES => "headphones.svg",
    SPARKLES => "sparkles.svg",
    CHEVRON_DOWN => "chevron-down.svg",
    KEYBOARD => "keyboard.svg",
    BELL => "bell.svg",
    INFO => "info.svg",
    SUN => "sun.svg",
    ARROW_LEFT => "arrow-left.svg",
}

/// Renders an embedded Lucide SVG vector icon at `size_px` tinted with `color`.
pub fn lucide_svg(path: &'static str, size_px: f32, color: impl Into<Hsla>) -> Svg {
    svg()
        .path(path)
        .size(px(size_px))
        .text_color(color)
        .flex_shrink_0()
}
