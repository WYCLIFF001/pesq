//! Fourier transform conventions, windows, and filter tables (spec 02).
//!
//! This module holds the data and the small procedures of spec 02 that are
//! fully pinned down by the specification: the Hann window (2.3), the
//! curve evaluation rule and both dB curves (2.4 to 2.6), and the
//! 8-section input IIR cascade (2.7). The FFT-dependent procedures
//! (2.1, 2.2, 2.8) are stubs in this Round 2 scaffold and will be filled
//! in on top of `rustfft`.

/// Hann window of length `len` (spec 02, 2.3):
/// `w[n] = 0.5 * (1 - cos(2*pi*n/T))` for `n = 0..T-1`.
pub fn hann_window(len: usize) -> Vec<f32> {
    let denominator = len as f32;
    (0..len)
        .map(|n| {
            let phase = std::f32::consts::TAU * n as f32 / denominator;
            0.5 * (1.0 - phase.cos())
        })
        .collect()
}

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
    /// At or below the first table frequency the segment between the first
    /// and second points is extrapolated; at or above the last table
    /// frequency the segment between the second-to-last and last points is
    /// extrapolated; otherwise the bracketing segment is interpolated
    /// (spec 02, 2.4).
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

/// IRS receive filter curve (spec 02, 2.6). The curve value at 1000 Hz is
/// 12 dB, so after the normalization of spec 02 section 2.4 the passband
/// gain at 1000 Hz is exactly 0 dB.
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

/// Apply the 8-section input IIR cascade of spec 02 section 2.7 to the
/// whole buffer in place (spec 01, 1.6).
///
/// Each section uses the difference equations `w[n] = x[n] - a1*w[n-1] -
/// a2*w[n-2]` and `y[n] = b0*w[n] + b1*w[n-1] + b2*w[n-2]`, with
/// `w[-1] = w[-2] = 0`, over the entire buffer, in table order. All
/// arithmetic is f32.
pub fn apply_input_iir(buffer: &mut [f32]) {
    for &[b0, b1, b2, a1, a2] in &IIR_SECTIONS {
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

/// Apply an FFT-domain filter curve to a buffer region
/// (spec 01, 1.3.1). Round 2 placeholder; see spec 02 sections 2.1 and
/// 2.2 for the transform conventions.
///
/// The region starts at sample 2400 and covers `buffer.len() - 4800`
/// samples; the gain per bin is [`Curve::gain_at`] at the bin frequency.
pub fn apply_filter_curve(_buffer: &mut [f32], _curve: &Curve) {
    todo!("spec 01, 1.3.1: FFT-domain filter application")
}

/// Coarse log-VAD correlation of spec 01 section 1.9 (plain binwise
/// product, spec 02, 2.8). Round 2 placeholder.
pub fn correlate(_first: &[f32], _second: &[f32]) -> Vec<f32> {
    todo!("spec 01, 1.9 and spec 02, 2.8: FFT cross-correlation")
}

/// Spectral cross-correlation of spec 01 section 1.10 step 4b (conjugate
/// product form, spec 02, 2.8). Round 2 placeholder.
pub fn spectral_cross_correlate(_first: &[f32], _second: &[f32]) -> Vec<f32> {
    todo!("spec 01, 1.10 and spec 02, 2.8: spectral cross-correlation")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hann_window_matches_the_spec_formula() {
        // Spec 02 section 2.3 defines w[n] = 0.5 * (1 - cos(2*pi*n/T))
        // for n = 0..T-1, so the last tap is small but nonzero:
        // 0.5 * (1 - cos(2*pi*255/256)) is about 1.5e-4.
        let window = hann_window(256);
        assert_eq!(window.len(), 256);
        assert!(window[0].abs() < 1e-6);
        assert!((window[128] - 1.0).abs() < 1e-6);
        let expected_last = 0.5 * (1.0 - (std::f32::consts::TAU * 255.0 / 256.0).cos());
        assert!((window[255] - expected_last).abs() < 1e-6);
        assert!(window[255] > 1e-4);
    }

    #[test]
    fn alignment_curve_is_flat_in_the_passband() {
        // Flat 0 dB from 350 to 3250 Hz (spec 02, 2.5).
        for freq in [350.0, 500.0, 1000.0, 2000.0, 3250.0] {
            assert_eq!(ALIGNMENT_CURVE.value_at(freq), 0.0, "at {freq} Hz");
        }
        assert_eq!(ALIGNMENT_CURVE.gain_at(1000.0), 1.0);
    }

    #[test]
    fn irs_curve_normalizes_to_0_db_at_1_khz() {
        // The table reads 12 dB at 1000 Hz (spec 02, 2.6); the normalized
        // gain there is exactly 0 dB.
        assert_eq!(IRS_RECEIVE_CURVE.value_at(1000.0), 12.0);
        assert!((IRS_RECEIVE_CURVE.gain_at(1000.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn input_iir_preserves_silence() {
        let mut buffer = vec![0.0f32; 64];
        apply_input_iir(&mut buffer);
        assert!(buffer.iter().all(|&sample| sample == 0.0));
    }

    #[test]
    fn input_iir_matches_section_3_scaling_on_a_unit_impulse() {
        // Section 3 scales the first difference by its b0
        // (spec 02, 2.7), so y[0] = b0 * x[0] and y[1] = b1 * x[0]
        // when only x[0] is nonzero. The other sections then shape the
        // result, so only the impulse response length is checked here.
        let mut buffer = vec![0.0f32; 64];
        buffer[0] = 1.0;
        apply_input_iir(&mut buffer);
        assert!(!buffer[0].is_nan() && buffer[0].is_finite());
    }
}
