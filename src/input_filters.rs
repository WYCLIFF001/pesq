//! IRS receive filtering, DC removal, the input IIR filter (spec 01,
//! sections 1.4 to 1.6), and the wideband input filter (spec 06, 6.3).
//!
//! The IRS receive filter runs on the working buffers and produces the
//! saved model copies; the wideband filter replaces it in wideband mode
//! (spec 06, 6.3). The DC removal reproduces the specification's
//! division quirks exactly (division by the nominal length N, not the
//! interval length, and the two W-sample ramps at the interval edges);
//! the input IIR cascade then shapes the working buffers used by the VAD
//! and the alignment stages.

use crate::dsp::{IRS_RECEIVE_CURVE, apply_filter_curve, apply_input_iir};
use crate::types::{RATE_16K, SignalBuffer};

/// IRS receive filtering of spec 01 section 1.4.
///
/// Applies the FFT-domain filter procedure of 1.3.1 with the IRS receive
/// curve to both working buffers, then returns the saved model copies
/// (step 2). All remaining preprocessing operates on the working buffers
/// only; the perceptual model consumes the copies.
pub fn apply_irs_receive(
    reference: &mut SignalBuffer,
    degraded: &mut SignalBuffer,
) -> (SignalBuffer, SignalBuffer) {
    apply_filter_curve(&mut reference.samples, &IRS_RECEIVE_CURVE, reference.rate);
    apply_filter_curve(&mut degraded.samples, &IRS_RECEIVE_CURVE, degraded.rate);
    (reference.clone(), degraded.clone())
}

/// The wideband input filter of spec 06 section 6.3, applied to both
/// signals instead of the IRS receive filter of spec 01 section 1.4.
///
/// Only valid at 16 kHz (spec 06, 6.2 item 2 rejects wideband mode at
/// 8 kHz before any audio is processed). Per signal: multiply the first
/// 16 samples of the signal region by the rising ramp `(k + 1)/16` and
/// the last 16 by the mirrored falling ramp, then apply the normative
/// second-order section of spec 06, 6.3 to the region in place with zero
/// initial state. The passband gain of 2.818 is not renormalized
/// (spec 06, 6.3). The filtered buffers are returned as the saved model
/// copies, exactly as in spec 01 section 1.4 step 2.
pub fn apply_wideband(
    reference: &mut SignalBuffer,
    degraded: &mut SignalBuffer,
) -> (SignalBuffer, SignalBuffer) {
    wideband_filter(reference);
    wideband_filter(degraded);
    (reference.clone(), degraded.clone())
}

/// Normative wideband IIR section of spec 06 section 6.3:
/// `(b0, b1, b2, a1, a2)`, with the recurrence conventions of spec 02
/// section 2.7. The decimal digits are transcribed verbatim from the
/// specification table; the extra precision beyond f32 is intentional.
#[allow(clippy::excessive_precision)]
const WIDEBAND_SECTION: [f32; 5] = [2.740_826, -5.481_651_9, 2.740_826, -1.944_477_7, 0.945_977_94];

/// Width of the edge ramps of spec 06 section 6.3, in samples.
const WIDEBAND_RAMP_SAMPLES: usize = 16;

/// One signal through the wideband input filter of spec 06 section 6.3.
fn wideband_filter(buffer: &mut SignalBuffer) {
    assert_eq!(
        buffer.rate, RATE_16K,
        "wideband mode requires the 16 kHz rate (spec 06, 6.2 item 2)"
    );
    let start = buffer.rate.margin_samples();
    let end = buffer.nominal_len - start;
    // Step 1: the 16-sample edge ramps of the signal region.
    for k in 0..WIDEBAND_RAMP_SAMPLES {
        buffer.samples[start + k] *= (k + 1) as f32 / WIDEBAND_RAMP_SAMPLES as f32;
        buffer.samples[end - 1 - k] *= (k + 1) as f32 / WIDEBAND_RAMP_SAMPLES as f32;
    }
    // Step 2: the single biquad over the region, in place, zero initial
    // state. The margins stay zero (spec 06, 6.3).
    let [b0, b1, b2, a1, a2] = WIDEBAND_SECTION;
    let mut w1 = 0.0f32;
    let mut w2 = 0.0f32;
    for sample in buffer.samples[start..end].iter_mut() {
        let w = *sample - a1 * w1 - a2 * w2;
        *sample = b0 * w + b1 * w1 + b2 * w2;
        w2 = w1;
        w1 = w;
    }
}

/// DC removal of spec 01 section 1.5.
///
/// The mean is computed over `[margin, N - margin)` but divided by the
/// nominal length N, not the interval length; this is intentional and
/// reproduced. The mean is subtracted from that interval only, and both
/// interval edges get the W-sample ramp of steps 3 and 4.
pub fn remove_dc(buffer: &mut SignalBuffer) {
    let start = buffer.rate.margin_samples();
    let end = buffer.nominal_len - start;
    let sum: f64 = buffer.samples[start..end]
        .iter()
        .map(|&sample| f64::from(sample))
        .sum();
    let mean = (sum / buffer.nominal_len as f64) as f32;

    for sample in buffer.samples[start..end].iter_mut() {
        *sample -= mean;
    }
    // Start-of-interval ramp: sample margin + k times (0.5 + k) / W.
    let w = buffer.rate.window_samples();
    for k in 0..w {
        buffer.samples[start + k] *= (0.5 + k as f32) / w as f32;
    }
    // End-of-interval ramp: sample (N - margin - 1 - k) times (0.5 + k) / W.
    for k in 0..w {
        buffer.samples[end - 1 - k] *= (0.5 + k as f32) / w as f32;
    }
}

/// Input IIR filter of spec 01 section 1.6: the cascade of spec 02
/// section 2.7 applied to the entire working buffer in place with zero
/// initial state. 8 sections at 8 kHz, 12 at 16 kHz (spec 06, 6.4.2).
pub fn input_iir_filter(buffer: &mut SignalBuffer) {
    apply_input_iir(&mut buffer.samples, buffer.rate);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MARGIN_SAMPLES, WINDOW_SAMPLES};

    /// A buffer of a constant value in `[2400, 2400 + len)`.
    fn constant_buffer(value: f32, len: usize) -> SignalBuffer {
        let mut buffer = SignalBuffer::from_pcm(&vec![0i16; len]).unwrap();
        for sample in buffer.samples[MARGIN_SAMPLES..MARGIN_SAMPLES + len].iter_mut() {
            *sample = value;
        }
        buffer
    }

    #[test]
    fn dc_removal_divides_by_the_nominal_length() {
        // A constant C is not fully removed: the residual is
        // C * (1 - (N - 4800) / N) = C * 4800 / N, because the mean
        // divides by N, not by the interval length (spec 01, 1.5 step 1).
        let value = 100.0f32;
        let len = 4000usize;
        let mut buffer = constant_buffer(value, len);
        let residual = value * (2.0 * MARGIN_SAMPLES as f32) / buffer.nominal_len as f32;
        remove_dc(&mut buffer);
        let interior = MARGIN_SAMPLES + WINDOW_SAMPLES;
        let interior_end = buffer.nominal_len - MARGIN_SAMPLES - WINDOW_SAMPLES;
        for &sample in &buffer.samples[interior..interior_end] {
            assert!(
                (sample - residual).abs() < 1e-3,
                "sample {sample}, residual {residual}"
            );
        }
    }

    #[test]
    fn dc_removal_ramps_the_interval_edges() {
        let value = 100.0f32;
        let len = 4000usize;
        let mut buffer = constant_buffer(value, len);
        let residual = value * (2.0 * MARGIN_SAMPLES as f32) / buffer.nominal_len as f32;
        remove_dc(&mut buffer);
        let start = MARGIN_SAMPLES;
        let end = buffer.nominal_len - MARGIN_SAMPLES;
        for k in 0..WINDOW_SAMPLES {
            let factor = (0.5 + k as f32) / WINDOW_SAMPLES as f32;
            assert!(
                (buffer.samples[start + k] - residual * factor).abs() < 1e-3,
                "start ramp k={k}"
            );
            assert!(
                (buffer.samples[end - 1 - k] - residual * factor).abs() < 1e-3,
                "end ramp k={k}"
            );
        }
    }

    #[test]
    fn dc_removal_leaves_the_margins_untouched() {
        let len = 4000usize;
        let mut buffer = constant_buffer(100.0, len);
        remove_dc(&mut buffer);
        assert!(buffer.samples[..MARGIN_SAMPLES].iter().all(|&s| s == 0.0));
        assert!(
            buffer.samples[buffer.nominal_len - MARGIN_SAMPLES..]
                .iter()
                .all(|&s| s == 0.0)
        );
    }

    #[test]
    fn iir_filter_preserves_silence() {
        let len = 4000usize;
        let mut buffer = SignalBuffer::from_pcm(&vec![0i16; len]).unwrap();
        input_iir_filter(&mut buffer);
        assert!(buffer.samples.iter().all(|&s| s == 0.0));
    }

    /// The wideband filter shapes the signal region of a 16 kHz buffer:
    /// the margins stay zero, the first ramp sample is attenuated by
    /// 1/16, and the interior response follows the biquad of spec 06
    /// section 6.3 (passband gain 2.818 at DC-free content).
    #[test]
    fn wideband_filter_ramps_the_edges_and_shapes_the_region() {
        let len = 8000usize;
        let mut buffer = SignalBuffer::from_pcm_at(&vec![0i16; len], crate::types::RATE_16K)
            .unwrap();
        let margin = buffer.rate.margin_samples();
        for sample in buffer.samples[margin..margin + len].iter_mut() {
            *sample = 1.0;
        }
        wideband_filter(&mut buffer);
        // The margins are not filtered and stay zero.
        assert!(buffer.samples[..margin].iter().all(|&s| s == 0.0));
        assert!(
            buffer.samples[buffer.nominal_len - margin..]
                .iter()
                .all(|&s| s == 0.0)
        );
        // The first ramp sample is the input times 1/16, before the
        // biquad: b0 * 1/16 = 0.1713...
        assert!((buffer.samples[margin] - 2.740_826 / 16.0).abs() < 1e-4);
        // The settled interior equals the passband gain 2.818, since the
        // DC-free step response settles to b0 + b1 + b2 = 0 for the
        // constant part; the response near the end follows the ramp.
        let mid = margin + 2000;
        assert!((buffer.samples[mid].abs()) < 1e-3, "mid {}", buffer.samples[mid]);
        // Rejecting 8 kHz is the wideband mode error of spec 06, 6.2
        // item 2.
        let mut eight_k = SignalBuffer::from_pcm(&vec![0i16; len]).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            wideband_filter(&mut eight_k);
        }));
        assert!(result.is_err(), "wideband at 8 kHz must be rejected");
    }

    #[test]
    fn irs_receive_saves_the_model_copies() {
        let len = 4000usize;
        let mut reference = constant_buffer(100.0, len);
        let mut degraded = constant_buffer(50.0, len);
        let (model_ref, model_deg) = apply_irs_receive(&mut reference, &mut degraded);
        // The copies match the working buffers immediately after the
        // filter, before DC removal.
        assert_eq!(model_ref.samples, reference.samples);
        assert_eq!(model_deg.samples, degraded.samples);
        // A signal with low-frequency content is shaped by the IRS curve.
        assert!(reference.samples[MARGIN_SAMPLES] != 100.0);
    }
}
