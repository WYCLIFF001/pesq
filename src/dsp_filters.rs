//! Filter tables of spec 02 and the FFT-domain filter application of
//! spec 01 section 1.3.1.
//!
//! The two dB curves (2.5 and 2.6) and the 8-section input IIR cascade
//! (2.7) are transcribed from the specification tables; the coefficients
//! are used verbatim, including the exact f32 rounding of the published
//! decimals.

use crate::dsp_fft::{inverse_real_fft, real_fft};
use crate::types::Rate;

/// A dB gain curve given as a table of (frequency in Hz, gain in dB)
/// points, evaluated with the rule of spec 02 section 2.4.
#[derive(Debug, Clone, Copy)]
pub struct Curve {
    points: &'static [(f32, f32)],
}

impl Curve {
    /// Wrap a static table of (frequency in Hz, gain in dB) points,
    /// sorted by ascending frequency.
    pub const fn new(points: &'static [(f32, f32)]) -> Self {
        Self { points }
    }

    /// Evaluate the curve at a frequency in Hz.
    ///
    /// At or below the first table frequency the segment between the
    /// first and second points is extrapolated; at or above the last
    /// table frequency the segment between the second-to-last and last
    /// points is extrapolated; otherwise the bracketing segment is
    /// interpolated (spec 02, 2.4).
    pub fn value_at(&self, freq: f32) -> f32 {
        self.value_at_f64(f64::from(freq)) as f32
    }

    /// Curve evaluation in f64, per the numerical conventions of spec 01
    /// section 1.1 (curve interpolation is performed in f64).
    fn value_at_f64(&self, freq: f64) -> f64 {
        let points = self.points;
        if freq <= f64::from(points[0].0) {
            return interpolate(
                f64::from(points[0].0),
                f64::from(points[0].1),
                f64::from(points[1].0),
                f64::from(points[1].1),
                freq,
            );
        }
        let last = points.len() - 1;
        if freq >= f64::from(points[last].0) {
            return interpolate(
                f64::from(points[last - 1].0),
                f64::from(points[last - 1].1),
                f64::from(points[last].0),
                f64::from(points[last].1),
                freq,
            );
        }
        for pair in points.windows(2) {
            if freq <= f64::from(pair[1].0) {
                return interpolate(
                    f64::from(pair[0].0),
                    f64::from(pair[0].1),
                    f64::from(pair[1].0),
                    f64::from(pair[1].1),
                    freq,
                );
            }
        }
        unreachable!("curve bracketing failed for {freq} Hz");
    }

    /// Linear gain of the curve at a frequency, normalized to 0 dB at
    /// 1000 Hz: `10^((curve(freq) - curve(1000)) / 20)` (spec 02, 2.4 and
    /// spec 01, 1.3.1 step 3). The dB difference is computed in f64 and
    /// the result is rounded once to f32.
    pub fn gain_at(&self, freq: f32) -> f32 {
        let db = self.value_at_f64(f64::from(freq)) - self.value_at_f64(1000.0);
        f64::powf(10.0, db / 20.0) as f32
    }
}

/// Linear interpolation of `y` over the segment `(x0, y0)..(x1, y1)`.
fn interpolate(x0: f64, y0: f64, x1: f64, y1: f64, x: f64) -> f64 {
    y0 + (y1 - y0) * (x - x0) / (x1 - x0)
}

/// Alignment filter curve, the band-pass used for level calibration
/// (spec 02, 2.5). Flat 0 dB from 350 to 3250 Hz, suppressed below
/// 300 Hz and above 3500 Hz.
pub const ALIGNMENT_CURVE: Curve = Curve::new(&[
    (0.0, -500.0),
    (50.0, -500.0),
    (100.0, -500.0),
    (125.0, -500.0),
    (160.0, -500.0),
    (200.0, -500.0),
    (250.0, -500.0),
    (300.0, -500.0),
    (350.0, 0.0),
    (400.0, 0.0),
    (500.0, 0.0),
    (600.0, 0.0),
    (630.0, 0.0),
    (800.0, 0.0),
    (1000.0, 0.0),
    (1250.0, 0.0),
    (1600.0, 0.0),
    (2000.0, 0.0),
    (2500.0, 0.0),
    (3000.0, 0.0),
    (3250.0, 0.0),
    (3500.0, -500.0),
    (4000.0, -500.0),
    (5000.0, -500.0),
    (6300.0, -500.0),
    (8000.0, -500.0),
]);

/// IRS receive filter curve (spec 02, 2.6). The curve value at 1000 Hz
/// is 12 dB, so after the normalization of spec 02 section 2.4 the
/// passband gain at 1000 Hz is exactly 0 dB.
pub const IRS_RECEIVE_CURVE: Curve = Curve::new(&[
    (0.0, -200.0),
    (50.0, -40.0),
    (100.0, -20.0),
    (125.0, -12.0),
    (160.0, -6.0),
    (200.0, 0.0),
    (250.0, 4.0),
    (300.0, 6.0),
    (350.0, 8.0),
    (400.0, 10.0),
    (500.0, 11.0),
    (600.0, 12.0),
    (700.0, 12.0),
    (800.0, 12.0),
    (1000.0, 12.0),
    (1300.0, 12.0),
    (1600.0, 12.0),
    (2000.0, 12.0),
    (2500.0, 12.0),
    (3000.0, 12.0),
    (3250.0, 12.0),
    (3500.0, 4.0),
    (4000.0, -200.0),
    (5000.0, -200.0),
    (6300.0, -200.0),
    (8000.0, -200.0),
]);

/// Coefficients `(b0, b1, b2, a1, a2)` of the 8-section input IIR cascade
/// (spec 02, 2.7), in application order.
///
/// The decimal digits are transcribed verbatim from the specification
/// table; the extra precision beyond f32 is intentional.
#[allow(clippy::excessive_precision)]
pub const IIR_SECTIONS: [[f32; 5]; 8] = [
    // Section 0
    [0.885_535_424, -0.885_535_424, 0.0, -0.771_070_709, 0.0],
    // Section 1
    [
        0.895_092_588,
        1.292_907_193,
        0.449_260_174,
        1.268_869_037,
        0.442_025_372,
    ],
    // Section 2
    [
        4.049_527_940,
        -7.865_190_042,
        3.815_662_102,
        -1.746_859_852,
        0.786_305_963,
    ],
    // Section 3: a pure scaling of 0.5 on the first difference; reproduce
    // the coefficients exactly, do not simplify.
    [0.500_002_353, -0.500_002_353, 0.0, 0.0, 0.0],
    // Section 4
    [
        0.565_002_834,
        -0.241_585_934,
        -0.306_009_671,
        0.259_688_659,
        0.249_979_657,
    ],
    // Section 5
    [
        2.115_237_288,
        0.919_935_084,
        1.141_240_051,
        -1.587_313_419,
        0.665_935_315,
    ],
    // Section 6
    [
        0.912_224_584,
        -0.224_397_719,
        -0.641_121_413,
        -0.246_029_464,
        -0.556_720_590,
    ],
    // Section 7
    [
        0.444_617_727,
        -0.307_589_321,
        0.141_638_062,
        -0.996_391_149,
        0.502_251_622,
    ],
];

/// Coefficients `(b0, b1, b2, a1, a2)` of the 12-section input IIR
/// cascade for 16 kHz (spec 06, 6.4.2), in application order. Same
/// recurrence, ordering, and application rule as [`IIR_SECTIONS`]
/// (spec 02, 2.7).
///
/// The decimal digits are transcribed verbatim from the specification
/// table; the extra precision beyond f32 is intentional.
#[allow(clippy::excessive_precision)]
pub const IIR_SECTIONS_16K: [[f32; 5]; 12] = [
    // Section 0
    [
        0.325_631_521,
        -0.086_782_860,
        -0.238_848_661,
        -1.079_416_490,
        0.434_583_902,
    ],
    // Section 1
    [
        0.403_961_804,
        -0.556_985_881,
        0.153_024_077,
        -0.415_115_835,
        0.696_590_244,
    ],
    // Section 2
    [
        4.736_162_769,
        3.287_251_046,
        1.753_289_019,
        -1.859_599_046,
        0.876_284_034,
    ],
    // Section 3: a pure scaling of 0.365373469.
    [
        0.365_373_469,
        0.000_000_000,
        0.000_000_000,
        -0.634_626_531,
        0.000_000_000,
    ],
    // Section 4
    [
        0.884_811_506,
        0.000_000_000,
        0.000_000_000,
        -0.256_725_271,
        0.141_536_777,
    ],
    // Section 5
    [
        0.723_593_055,
        -1.447_186_099,
        0.723_593_044,
        -1.129_587_469,
        0.657_232_737,
    ],
    // Section 6
    [
        1.644_910_855,
        -1.817_280_902,
        1.249_658_063,
        -1.778_403_899,
        0.801_724_355,
    ],
    // Section 7: an FIR section (a1 = a2 = 0).
    [
        0.633_692_689,
        -0.284_644_314,
        -0.319_789_663,
        0.000_000_000,
        0.000_000_000,
    ],
    // Section 8: an FIR section (a1 = a2 = 0).
    [
        1.032_763_031,
        0.268_428_979,
        0.602_913_323,
        0.000_000_000,
        0.000_000_000,
    ],
    // Section 9
    [
        1.001_616_361,
        -0.823_749_013,
        0.439_731_942,
        -0.885_778_255,
        0.000_000_000,
    ],
    // Section 10
    [
        0.752_472_096,
        -0.375_388_990,
        0.188_977_609,
        -0.077_258_216,
        0.247_230_734,
    ],
    // Section 11
    [
        1.023_700_575,
        0.001_661_628,
        0.521_284_240,
        -0.183_867_259,
        0.354_324_187,
    ],
];

/// Per-bin linear gains of a filter curve at FFT size `r`, computed
/// once per (curve, r, rate) triple in a [`std::sync::OnceLock`]-guarded
/// cache. The cached values are exactly the [`Curve::gain_at`] results,
/// so the cache is numerically transparent; it only removes the repeated
/// interpolation and power evaluations. The rate enters the cache key
/// because the bin frequency is `k * f / r`.
type GainCache =
    std::sync::Mutex<std::collections::HashMap<(usize, usize, usize), std::sync::Arc<Vec<f32>>>>;

fn curve_gains(curve: &Curve, r: usize, rate: Rate) -> std::sync::Arc<Vec<f32>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static GAINS: OnceLock<GainCache> = OnceLock::new();
    let key = (r, curve.points.as_ptr() as usize, rate.sample_rate());
    GAINS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .entry(key)
        .or_insert_with(|| {
            Arc::new(
                (0..=r / 2)
                    .map(|k| {
                        let freq = k as f32 * rate.sample_rate() as f32 / r as f32;
                        curve.gain_at(freq)
                    })
                    .collect(),
            )
        })
        .clone()
}

/// Apply the input IIR cascade to the whole buffer in place (spec 01,
/// 1.6): 8 sections at 8 kHz (spec 02, 2.7), 12 sections at 16 kHz
/// (spec 06, 6.4.2).
///
/// Each section uses the difference equations `w[n] = x[n] - a1*w[n-1] -
/// a2*w[n-2]` and `y[n] = b0*w[n] + b1*w[n-1] + b2*w[n-2]`, with
/// `w[-1] = w[-2] = 0`, over the entire buffer, in table order. All
/// arithmetic is f32.
pub fn apply_input_iir(buffer: &mut [f32], rate: Rate) {
    let sections: &[[f32; 5]] = match rate {
        Rate::Rate8k => &IIR_SECTIONS,
        Rate::Rate16k => &IIR_SECTIONS_16K,
    };
    for &[b0, b1, b2, a1, a2] in sections {
        let mut w1 = 0.0f32;
        let mut w2 = 0.0f32;
        for sample in buffer.iter_mut() {
            let w = *sample - a1 * w1 - a2 * w2;
            *sample = b0 * w + b1 * w1 + b2 * w2;
            w2 = w1;
            w1 = w;
        }
    }
}

/// Apply an FFT-domain filter curve to the region of spec 01 section
/// 1.3.1: start `S` one margin in, length
/// `n = buffer.len() - 2 * margin`.
///
/// The region is copied into a zero-padded buffer of the smallest power
/// of two at least n, transformed, each bin k scaled by
/// [`Curve::gain_at`] at the bin frequency `k * f / R`, transformed
/// back, and the first n samples written back to the region.
pub fn apply_filter_curve(buffer: &mut [f32], curve: &Curve, rate: Rate) {
    let start = rate.margin_samples();
    let n = buffer.len() - 2 * start;
    let r = n.next_power_of_two();
    let mut scratch = vec![0.0f32; r];
    scratch[..n].copy_from_slice(&buffer[start..start + n]);
    let mut spectrum = real_fft(&scratch);
    let gains = curve_gains(curve, r, rate);
    for k in 0..=r / 2 {
        spectrum[2 * k] *= gains[k];
        spectrum[2 * k + 1] *= gains[k];
    }
    scratch = inverse_real_fft(&spectrum);
    buffer[start..start + n].copy_from_slice(&scratch[..n]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MARGIN_SAMPLES, PADDING_SAMPLES, RATE_8K, SAMPLE_RATE_HZ, WINDOW_SAMPLES};

    /// A signal buffer with `cycles` full periods of a sine whose
    /// frequency divides 8000 evenly, so the tone sits exactly on one
    /// FFT bin of the filter procedure. The burst is tapered with a
    /// Hann envelope so the rectangular burst edges do not leak energy
    /// into distant bins.
    fn sine_buffer(freq: f32, cycles: usize) -> Vec<f32> {
        assert_eq!(
            SAMPLE_RATE_HZ as f32 % freq,
            0.0,
            "frequency must divide 8000"
        );
        let signal_len = cycles * (SAMPLE_RATE_HZ as f32 / freq) as usize;
        let len = signal_len + 2 * MARGIN_SAMPLES + PADDING_SAMPLES;
        let mut buffer = vec![0.0f32; len];
        for (i, sample) in buffer[MARGIN_SAMPLES..MARGIN_SAMPLES + signal_len]
            .iter_mut()
            .enumerate()
        {
            let phase = std::f32::consts::TAU * freq * i as f32 / SAMPLE_RATE_HZ as f32;
            let taper = 0.5 * (1.0 - (std::f32::consts::TAU * i as f32 / signal_len as f32).cos());
            *sample = phase.sin() * taper;
        }
        buffer
    }

    #[test]
    fn curve_evaluation_extrapolates_below_the_first_point() {
        // Below 300 Hz the alignment curve reads -500 dB (spec 02, 2.5).
        assert_eq!(ALIGNMENT_CURVE.value_at(150.0), -500.0);
        assert_eq!(ALIGNMENT_CURVE.value_at(0.0), -500.0);
        // Above 3500 Hz likewise.
        assert_eq!(ALIGNMENT_CURVE.value_at(5000.0), -500.0);
    }

    #[test]
    fn alignment_filter_passes_the_passband_and_cuts_the_stopband() {
        // A 1000 Hz sine survives the alignment filter with unity gain;
        // a 125 Hz sine (a table point at -500 dB) is attenuated by
        // hundreds of dB. Both divide 8000, so each tone sits on a
        // single FFT bin of the filter procedure.
        let pass = sine_buffer(1000.0, 200);
        let stop = sine_buffer(125.0, 100);
        let energy = |buffer: &[f32]| buffer.iter().map(|s| f64::from(*s * *s)).sum::<f64>();
        let before_pass = energy(&pass);
        let before_stop = energy(&stop);
        let mut filtered_pass = pass;
        let mut filtered_stop = stop;
        apply_filter_curve(&mut filtered_pass, &ALIGNMENT_CURVE, RATE_8K);
        apply_filter_curve(&mut filtered_stop, &ALIGNMENT_CURVE, RATE_8K);
        assert!(energy(&filtered_pass) > 0.9 * before_pass);
        assert!(energy(&filtered_stop) < 1e-6 * before_stop);
    }

    #[test]
    fn irs_filter_is_unity_at_1_khz_and_shapes_the_edges() {
        let buffer = sine_buffer(1000.0, 40);
        let mut filtered = buffer.clone();
        apply_filter_curve(&mut filtered, &IRS_RECEIVE_CURVE, RATE_8K);
        let energy = |buffer: &[f32]| buffer.iter().map(|s| f64::from(*s * *s)).sum::<f64>();
        assert!((energy(&filtered) / energy(&buffer) - 1.0).abs() < 0.01);
    }

    #[test]
    fn input_iir_preserves_silence_and_touches_all_samples() {
        let mut buffer = vec![0.0f32; 64];
        apply_input_iir(&mut buffer, RATE_8K);
        assert!(buffer.iter().all(|&sample| sample == 0.0));
        let mut impulse = vec![0.0f32; 64];
        impulse[0] = 1.0;
        apply_input_iir(&mut impulse, RATE_8K);
        assert!(impulse.iter().any(|&sample| sample != 0.0));
    }

    #[test]
    fn window_length_is_still_the_vad_unit() {
        assert_eq!(WINDOW_SAMPLES, 32);
    }
}
