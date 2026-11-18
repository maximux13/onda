pub(super) const DEFAULT_DOMINANT_COLOR: u32 = 0x2b8eff;

const KNOWN_APP_COLORS: &[(&[&str], u32)] = &[
    (&["spotify"], 0x1ed760),
    (&["discord"], 0x5865f2),
    (&["brave"], 0xff5500),
    (&["chrome"], 0x2b8eff),
    (
        &["vscode", "microsoft.vscode", "visual studio code"],
        0x0098ff,
    ),
    (&["com.apple.finder"], 0xa8b3c8),
    (&["safari"], 0x2582ff),
    (&["apple.music", "com.apple.music"], 0xfc3c44),
    (&["arc"], 0xff6b8b),
    (&["firefox"], 0xff7139),
    (&["slack"], 0xe01e5a),
    (&["zoom"], 0x2d8cff),
    (&["teams"], 0x6264a7),
    (&["vlc"], 0xff8800),
];

/// Returns the curated brand color for well-known applications, if any.
pub(super) fn known_app_color(bundle_id: &str, name: &str) -> Option<u32> {
    let bid = bundle_id.to_ascii_lowercase();
    let nm = name.to_ascii_lowercase();
    KNOWN_APP_COLORS.iter().find_map(|(patterns, color)| {
        patterns
            .iter()
            .any(|pat| bid.contains(pat) || nm.contains(pat))
            .then_some(*color)
    })
}

fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let h = if delta <= 1e-5 {
        0.0
    } else if (max - r).abs() <= 1e-5 {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if (max - g).abs() <= 1e-5 {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    let s = if max <= 1e-5 { 0.0 } else { delta / max };
    (h, s, max)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let h_prime = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());
    let (r1, g1, b1) = if h_prime < 1.0 {
        (c, x, 0.0)
    } else if h_prime < 2.0 {
        (x, c, 0.0)
    } else if h_prime < 3.0 {
        (0.0, c, x)
    } else if h_prime < 4.0 {
        (0.0, x, c)
    } else if h_prime < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let m = v - c;
    let r = ((r1 + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = ((g1 + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = ((b1 + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    (r, g, b)
}

/// Extracts a vibrancy-weighted dominant RGB color (`0xRRGGBB`) from a premultiplied
/// 8-bit-per-channel RGBA pixel buffer, falling back to `DEFAULT_DOMINANT_COLOR`.
pub(super) fn extract_dominant_color_from_rgba(rgba: &[u8], width: usize, height: usize) -> u32 {
    let Some(expected_len) = width.checked_mul(height).and_then(|n| n.checked_mul(4)) else {
        return DEFAULT_DOMINANT_COLOR;
    };
    if width == 0 || height == 0 || rgba.len() < expected_len {
        return DEFAULT_DOMINANT_COLOR;
    }

    // 12 hue buckets (30 degrees each) to find the most vibrant dominant color family
    let mut bucket_weight = [0.0f32; 12];
    let mut bucket_r = [0.0f32; 12];
    let mut bucket_g = [0.0f32; 12];
    let mut bucket_b = [0.0f32; 12];

    let (pixels, _) = rgba[..expected_len].as_chunks::<4>();
    for &[raw_r, raw_g, raw_b, a] in pixels {
        if a < 128 {
            continue;
        }

        // Un-premultiply alpha
        let r = ((u32::from(raw_r) * 255) / u32::from(a)).min(255) as u8;
        let g = ((u32::from(raw_g) * 255) / u32::from(a)).min(255) as u8;
        let b = ((u32::from(raw_b) * 255) / u32::from(a)).min(255) as u8;

        // Ignore near-white and near-black pixels
        if (r > 235 && g > 235 && b > 235) || (r < 30 && g < 30 && b < 30) {
            continue;
        }

        let rf = f32::from(r) / 255.0;
        let gf = f32::from(g) / 255.0;
        let bf = f32::from(b) / 255.0;
        let (h, s, v) = rgb_to_hsv(rf, gf, bf);
        if s < 0.12 {
            continue;
        }

        // Vibrancy weight favors saturated, bright pixels
        let weight = (s * s) * v * (f32::from(a) / 255.0);
        let bin = ((h / 30.0).floor() as usize).min(11);
        bucket_weight[bin] += weight;
        bucket_r[bin] += rf * weight;
        bucket_g[bin] += gf * weight;
        bucket_b[bin] += bf * weight;
    }

    let mut best_bin = 0;
    let mut best_weight = 0.0f32;
    for (i, &w) in bucket_weight.iter().enumerate() {
        if w > best_weight {
            best_weight = w;
            best_bin = i;
        }
    }

    if best_weight <= 1e-4 {
        return DEFAULT_DOMINANT_COLOR;
    }

    let avg_r = bucket_r[best_bin] / best_weight;
    let avg_g = bucket_g[best_bin] / best_weight;
    let avg_b = bucket_b[best_bin] / best_weight;
    let (h, s, v) = rgb_to_hsv(avg_r, avg_g, avg_b);
    let boosted_s = s.clamp(0.60, 0.95);
    let boosted_v = v.clamp(0.75, 0.98);
    let (r, g, b) = hsv_to_rgb(h, boosted_s, boosted_v);
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_hsv_roundtrip() {
        let (h, s, v) = rgb_to_hsv(0.18, 0.84, 0.38);
        let (r, g, b) = hsv_to_rgb(h, s, v);
        assert!((r as i32 - (0.18_f32 * 255.0).round() as i32).abs() <= 2);
        assert!((g as i32 - (0.84_f32 * 255.0).round() as i32).abs() <= 2);
        assert!((b as i32 - (0.38_f32 * 255.0).round() as i32).abs() <= 2);
    }

    #[test]
    fn test_known_app_dominant_colors() {
        assert_eq!(
            known_app_color("com.spotify.client", "Spotify"),
            Some(0x1ed760)
        );
        assert_eq!(
            known_app_color("com.brave.Browser", "Brave Browser"),
            Some(0xff5500)
        );
        assert_eq!(
            known_app_color("com.google.Chrome", "Google Chrome"),
            Some(0x2b8eff)
        );
        assert_eq!(
            known_app_color("com.hnc.Discord", "Discord"),
            Some(0x5865f2)
        );
    }

    #[test]
    fn test_extract_dominant_color_from_rgba_falls_back_on_empty() {
        assert_eq!(
            extract_dominant_color_from_rgba(&[], 0, 0),
            DEFAULT_DOMINANT_COLOR
        );
        let transparent = [0u8; 4 * 4];
        assert_eq!(
            extract_dominant_color_from_rgba(&transparent, 2, 2),
            DEFAULT_DOMINANT_COLOR
        );
    }

    #[test]
    fn test_extract_dominant_color_from_rgba_selects_vibrant_hue() {
        let rgba = [30, 215, 96, 255].repeat(4);
        let color = extract_dominant_color_from_rgba(&rgba, 2, 2);
        let r = (color >> 16) & 0xff;
        let g = (color >> 8) & 0xff;
        let b = color & 0xff;
        assert!(
            g > r && g > b,
            "expected green-dominant color, got {color:#08x}"
        );
    }
}
