//! IRS receive filtering, DC removal, and the input IIR filter
//! (spec 01, sections 1.4 to 1.6).
//!
//! The IRS receive filter runs on the working buffers and produces the
//! saved model copies; the DC removal reproduces the specification's
//! division quirks exactly (division by the nominal length N, not the
//! interval length, and the two W-sample ramps at the interval edges);
//! the input IIR cascade then shapes the working buffers used by the VAD
//! and the alignment stages.

use crate::dsp::{IRS_RECEIVE_CURVE, apply_filter_curve, apply_input_iir};
use crate::types::{MARGIN_SAMPLES, SignalBuffer, WINDOW_SAMPLES};

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
    apply_filter_curve(&mut reference.samples, &IRS_RECEIVE_CURVE);
    apply_filter_curve(&mut degraded.samples, &IRS_RECEIVE_CURVE);
    (reference.clone(), degraded.clone())
}

/// DC removal of spec 01 section 1.5.
///
/// The mean is computed over `[2400, N - 2400)` but divided by the
/// nominal length N, not the interval length; this is intentional and
/// reproduced. The mean is subtracted from that interval only, and both
/// interval edges get the W-sample ramp of steps 3 and 4.
pub fn remove_dc(buffer: &mut SignalBuffer) {
    let start = MARGIN_SAMPLES;
    let end = buffer.nominal_len - MARGIN_SAMPLES;
    let sum: f64 = buffer.samples[start..end]
        .iter()
        .map(|&sample| f64::from(sample))
        .sum();
    let mean = (sum / buffer.nominal_len as f64) as f32;

    for sample in buffer.samples[start..end].iter_mut() {
        *sample -= mean;
    }
    // Start-of-interval ramp: sample 2400 + k times (0.5 + k) / W.
    for k in 0..WINDOW_SAMPLES {
        buffer.samples[start + k] *= (0.5 + k as f32) / WINDOW_SAMPLES as f32;
    }
    // End-of-interval ramp: sample (N - 2400 - 1 - k) times (0.5 + k) / W.
    for k in 0..WINDOW_SAMPLES {
        buffer.samples[end - 1 - k] *= (0.5 + k as f32) / WINDOW_SAMPLES as f32;
    }
}

/// Input IIR filter of spec 01 section 1.6: the 8-section cascade of
/// spec 02 section 2.7 applied to the entire working buffer in place
/// with zero initial state.
pub fn input_iir_filter(buffer: &mut SignalBuffer) {
    apply_input_iir(&mut buffer.samples);
}

#[cfg(test)]
mod tests {
    use super::*;

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
