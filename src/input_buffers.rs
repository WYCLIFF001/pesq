//! Level normalization of spec 01 section 1.3.
//!
//! Each signal is calibrated independently: a scratch copy is shaped by
//! the alignment filter, its mean power over the interval
//! `[2400, N - 2400 + P)` is divided by the shared divisor
//! `Nmax - 4800 + P`, and the first N samples of the original buffer are
//! scaled so that the target mean power becomes 1e7.

use crate::dsp::{ALIGNMENT_CURVE, apply_filter_curve};
use crate::types::SignalBuffer;

/// Level normalization of spec 01 section 1.3 for both signals.
///
/// Steps 1 to 5 run independently per signal with the shared divisor of
/// step 3, `Nmax - 4800 + P`, where `Nmax` is the larger of the two
/// nominal lengths. The original signal buffers are scaled in place; the
/// padding stays zero.
///
/// An all-silent signal has zero mean power; the specification gives no
/// fallback, so the scale is literally `sqrt(1e7 / 0)` and multiplying
/// the zero samples yields NaN. Callers scoring silence should supply
/// signals with at least some content.
pub fn normalize_levels(reference: &mut SignalBuffer, degraded: &mut SignalBuffer) {
    let n_max = reference.nominal_len.max(degraded.nominal_len);
    let divisor = (n_max - 2 * reference.rate.margin_samples() + reference.rate.padding_samples())
        as f64;
    normalize_one(reference, divisor);
    normalize_one(degraded, divisor);
}

/// Level normalization for one signal buffer (spec 01, 1.3 steps 1 to 5).
pub(crate) fn normalize_one(buffer: &mut SignalBuffer, divisor: f64) {
    // Step 1: scratch copy, then step 2: the alignment filter.
    let mut scratch = buffer.samples.clone();
    apply_filter_curve(&mut scratch, &ALIGNMENT_CURVE, buffer.rate);

    // Step 3: mean power over [margin, N - margin + P) with the shared
    // divisor. The sum of squares accumulates in f64 (spec 01, 1.1).
    let interval_start = buffer.rate.margin_samples();
    let interval_end = buffer.nominal_len - interval_start + buffer.rate.padding_samples();
    let sum_squares: f64 = scratch[interval_start..interval_end]
        .iter()
        .map(|&sample| f64::from(sample * sample))
        .sum();
    let mean_power = (sum_squares / divisor) as f32;

    // Step 4: the calibration scale; step 5: scale the first N samples
    // of the original buffer (not the scratch copy).
    let scale = (1e7f64 / f64::from(mean_power)).sqrt() as f32;
    for sample in buffer.samples[..buffer.nominal_len].iter_mut() {
        *sample *= scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SAMPLE_RATE_HZ;

    /// Build a signal buffer holding `len` samples of a sine.
    fn sine_buffer(amplitude: f32, freq: f32, len: usize) -> SignalBuffer {
        let mut pcm = vec![0i16; len];
        for (i, sample) in pcm.iter_mut().enumerate() {
            let phase = std::f32::consts::TAU * freq * i as f32 / SAMPLE_RATE_HZ as f32;
            *sample = (amplitude * phase.sin()).round() as i16;
        }
        SignalBuffer::from_pcm(&pcm).unwrap()
    }

    /// Mean power of the alignment-filtered scratch, measured exactly as
    /// in 1.3 step 3: the interval `[2400, N - 2400 + P)` divided by the
    /// shared divisor `Nmax - 4800 + P`.
    fn filtered_mean_power(buffer: &SignalBuffer, n_max: usize) -> f64 {
        let mut scratch = buffer.samples.clone();
        apply_filter_curve(&mut scratch, &ALIGNMENT_CURVE, buffer.rate);
        let interval_start = buffer.rate.margin_samples();
        let interval_end = buffer.nominal_len - interval_start + buffer.rate.padding_samples();
        let sum: f64 = scratch[interval_start..interval_end]
            .iter()
            .map(|&sample| f64::from(sample * sample))
            .sum();
        let divisor = n_max - 2 * interval_start + buffer.rate.padding_samples();
        sum / divisor as f64
    }

    #[test]
    fn normalization_hits_the_1e7_target_power() {
        let mut reference = sine_buffer(1500.0, 440.0, 4000);
        let mut degraded = sine_buffer(3000.0, 880.0, 5000);
        let n_max = reference.nominal_len.max(degraded.nominal_len);
        normalize_levels(&mut reference, &mut degraded);
        for buffer in [&reference, &degraded] {
            let power = filtered_mean_power(buffer, n_max);
            assert!(
                (power - 1e7).abs() / 1e7 < 1e-3,
                "calibrated mean power {power} off the 1e7 target"
            );
        }
    }

    #[test]
    fn normalization_leaves_the_padding_zero() {
        let mut reference = sine_buffer(1500.0, 440.0, 4000);
        let mut degraded = sine_buffer(3000.0, 880.0, 4000);
        normalize_levels(&mut reference, &mut degraded);
        for buffer in [&reference, &degraded] {
            assert!(
                buffer.samples[..buffer.rate.margin_samples()]
                    .iter()
                    .all(|&s| s == 0.0)
            );
            assert!(
                buffer.samples[buffer.nominal_len..]
                    .iter()
                    .all(|&s| s == 0.0)
            );
        }
    }

    #[test]
    fn normalization_uses_the_shared_nmax_divisor() {
        // The same signal calibrated against a longer partner gets a
        // larger scale, because the divisor grows with Nmax while the
        // interval stays N - 4800 + P samples long (spec 01, 1.3 step 3).
        let signal = |len| sine_buffer(1500.0, 440.0, len);
        let probe = signal(4000).rate.margin_samples() + 2; // a nonzero sample of the 440 Hz sine
        let mut reference_a = signal(4000);
        let mut degraded_a = signal(4000);
        normalize_levels(&mut reference_a, &mut degraded_a);
        let scale_a = reference_a.samples[probe];

        let mut reference_b = signal(4000);
        let mut degraded_b = signal(8000);
        normalize_levels(&mut reference_b, &mut degraded_b);
        let scale_b = reference_b.samples[probe];

        assert!(scale_b > scale_a, "{scale_b} should exceed {scale_a}");
    }
}
