use std::f32::consts::PI;

const SAMPLE_RATE: f32 = 48_000.0;
const BIQUAD_Q: f32 = 1.414;

/// Second-order IIR peaking EQ filter with per-channel stereo delay state.
#[derive(Clone, Copy)]
pub(super) struct BiquadBand {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: [f32; 2],
    z2: [f32; 2],
    pub(super) last_gain_db: f32,
    pub(super) active: bool,
}

impl BiquadBand {
    pub(super) const fn new() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: [0.0; 2],
            z2: [0.0; 2],
            last_gain_db: 0.0,
            active: false,
        }
    }

    #[inline]
    pub(super) fn update_peaking(&mut self, f0: f32, gain_db: f32) {
        self.last_gain_db = gain_db;
        if gain_db.abs() < 0.05 {
            self.b0 = 1.0;
            self.b1 = 0.0;
            self.b2 = 0.0;
            self.a1 = 0.0;
            self.a2 = 0.0;
            self.active = false;
            return;
        }

        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * f0 / SAMPLE_RATE;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * BIQUAD_Q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        let inv_a0 = 1.0 / a0;
        self.b0 = b0 * inv_a0;
        self.b1 = b1 * inv_a0;
        self.b2 = b2 * inv_a0;
        self.a1 = a1 * inv_a0;
        self.a2 = a2 * inv_a0;
        self.active = true;
    }

    #[inline]
    fn process(&mut self, x: f32, ch: usize) -> f32 {
        let y = self.b0 * x + self.z1[ch];
        self.z1[ch] = self.b1 * x - self.a1 * y + self.z2[ch];
        self.z2[ch] = self.b2 * x - self.a2 * y;
        y
    }
}

#[inline]
fn process_samples<const USE_EQ: bool>(
    input: &[f32],
    output: &mut [f32],
    channel_index: usize,
    n_channels: usize,
    scale: f32,
    bands: &mut [BiquadBand; 10],
) -> f32 {
    let n_channels = n_channels.max(1);
    let mut max_abs = 0.0f32;
    for (idx, (dst, &src)) in output.iter_mut().zip(input.iter()).enumerate() {
        let mut s = src * scale;
        if !s.is_finite() {
            s = 0.0;
        }
        if USE_EQ {
            let ch = if n_channels == 1 {
                channel_index.min(1)
            } else {
                (idx % n_channels).min(1)
            };
            for band in bands.iter_mut() {
                if band.active {
                    s = band.process(s, ch);
                }
            }
            if !s.is_finite() {
                s = 0.0;
            }
        }
        let abs = s.abs();
        if abs > max_abs {
            max_abs = abs;
        }
        *dst = s;
    }
    max_abs
}

/// Processes interleaved float32 PCM samples from `input` into `output`,
/// applying linear `scale`, optional 10-band biquad EQ, and non-finite sample sanitization.
/// Any tail in `output` beyond `input.len()` is zero-filled.
/// Returns the peak absolute output amplitude (`max_abs`).
pub(super) fn process_interleaved_f32(
    input: &[f32],
    output: &mut [f32],
    channel_index: usize,
    n_channels: usize,
    scale: f32,
    use_eq: bool,
    bands: &mut [BiquadBand; 10],
) -> f32 {
    if scale == 0.0 {
        output.fill(0.0);
        return 0.0;
    }

    let count = input.len().min(output.len());
    let max_abs = if use_eq {
        process_samples::<true>(
            &input[..count],
            &mut output[..count],
            channel_index,
            n_channels,
            scale,
            bands,
        )
    } else {
        process_samples::<false>(
            &input[..count],
            &mut output[..count],
            channel_index,
            n_channels,
            scale,
            bands,
        )
    };

    if count < output.len() {
        output[count..].fill(0.0);
    }

    max_abs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_biquad_flat_is_identity() {
        let mut band = BiquadBand::new();
        band.update_peaking(1000.0, 0.0);

        for i in 0..32 {
            let input = ((i as f32) * 0.2).sin();
            let output = band.process(input, 0);
            assert!((output - input).abs() < 1e-6);
        }
    }

    #[test]
    fn test_biquad_boost_and_cut_modify_signal() {
        let mut boost_band = BiquadBand::new();
        boost_band.update_peaking(1000.0, 6.0);

        let mut cut_band = BiquadBand::new();
        cut_band.update_peaking(1000.0, -6.0);

        let mut boost_energy = 0.0f32;
        let mut cut_energy = 0.0f32;

        // 1 kHz sine wave at 48 kHz sample rate
        for n in 0..480 {
            let sample = (2.0 * PI * 1000.0 * (n as f32) / 48_000.0).sin();
            let b_out = boost_band.process(sample, 0);
            let c_out = cut_band.process(sample, 0);
            if n > 48 {
                boost_energy += b_out * b_out;
                cut_energy += c_out * c_out;
            }
        }

        assert!(boost_energy > cut_energy * 2.0);
    }

    #[test]
    fn process_interleaved_f32_should_zero_output_when_scale_is_zero() {
        let input = [0.5, -0.75, 0.25, -1.0];
        let mut output = [9.0; 4];
        let mut bands = [BiquadBand::new(); 10];

        let peak = process_interleaved_f32(&input, &mut output, 0, 2, 0.0, false, &mut bands);

        assert_eq!(peak, 0.0);
        assert_eq!(output, [0.0; 4]);
    }

    #[test]
    fn process_interleaved_f32_should_scale_samples_and_return_peak_when_eq_disabled() {
        let input = [0.2, -0.5, 0.3, 0.1];
        let mut output = [0.0; 6];
        let mut bands = [BiquadBand::new(); 10];

        let peak = process_interleaved_f32(&input, &mut output, 0, 2, 1.5, false, &mut bands);

        assert!((peak - 0.75).abs() < 1e-6);
        assert!((output[0] - 0.3).abs() < 1e-6);
        assert!((output[1] - (-0.75)).abs() < 1e-6);
        assert!((output[2] - 0.45).abs() < 1e-6);
        assert!((output[3] - 0.15).abs() < 1e-6);
        assert_eq!(output[4], 0.0);
        assert_eq!(output[5], 0.0);
    }

    #[test]
    fn process_interleaved_f32_should_sanitize_non_finite_samples() {
        let input = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.4];
        let mut output = [1.0; 4];
        let mut bands = [BiquadBand::new(); 10];

        let peak = process_interleaved_f32(&input, &mut output, 0, 2, 0.5, false, &mut bands);

        assert!((peak - 0.2).abs() < 1e-6);
        assert_eq!(output[0], 0.0);
        assert_eq!(output[1], 0.0);
        assert_eq!(output[2], 0.0);
        assert!((output[3] - (-0.2)).abs() < 1e-6);
    }

    #[test]
    fn process_interleaved_f32_should_apply_eq_and_sanitize_when_eq_enabled() {
        let mut bands = [BiquadBand::new(); 10];
        bands[5].update_peaking(1000.0, 6.0);

        let input = [f32::NAN, 0.5, 0.25, -0.25];
        let mut output = [0.0; 4];

        let peak = process_interleaved_f32(&input, &mut output, 0, 2, 1.0, true, &mut bands);

        assert_eq!(output[0], 0.0);
        assert!(output[1].is_finite() && output[1] > 0.5);
        assert!(peak >= output[1].abs());
    }
}
