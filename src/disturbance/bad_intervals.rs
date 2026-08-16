//! Bad-interval detection, per-interval delay search, and re-alignment
//! (spec 04, section 4.5).

use crate::psychoacoustic::{
    FRAME_HOP, FRAME_LEN, NUM_BANDS, PerceptualModel, audible_power, governing_delay, local_scale,
    warp_to_bark, zwicker_loudness,
};
use crate::types::{MARGIN_SAMPLES, PADDING_SAMPLES, SignalBuffer, Utterance};

use super::norms::{asymmetric_densities, deadzone_removed, lp_norm};
use super::{
    BAD_FRAME_GATE, BAD_INTERVAL_SEARCH_SAMPLES, MAX_BAD_INTERVALS, MIN_BAD_INTERVAL_FRAMES,
    MIN_CORRELATION,
};

/// Number of power bins the band grouping consumes: bins 0..=127 of the
/// 256-point spectrum (spec 03, 3.2 and 3.3). The Nyquist bin is not
/// grouped.
const POWER_BINS: usize = FRAME_LEN / 2;

/// Run the bad-interval machinery of spec 04 section 4.5 on the
/// pre-normalization frame disturbances. Detects the bad intervals of
/// 4.5.1, builds the first re-aligned degraded signal (4.5.2), searches
/// one delay per interval (4.5.3), and re-runs the perceptual model on
/// the second re-aligned signal, replacing each frame's disturbances by
/// the minimum of the original and the recomputed values (4.5.4).
pub(crate) fn realign(
    reference: &SignalBuffer,
    degraded: &SignalBuffer,
    utterances: &[Utterance],
    model: &PerceptualModel,
    symmetric: &mut [f32],
    asymmetric: &mut [f32],
) {
    let frame_stop = model.frame_range.stop;
    let intervals = collect_intervals(&dilate(&bad_mask(symmetric)));
    if intervals.is_empty() {
        return;
    }
    // The common nominal length Nmax of spec 01 section 1.7: the T and
    // T2 buffers of 4.5.2 and 4.5.4 have Nmax + P samples, and the
    // clamps take Nmax (spec 04, 4.5.2 step 2 and 4.5.4 step 2).
    let n_max = reference.nominal_len.max(degraded.nominal_len);
    let realigned = first_realigned(degraded, utterances, n_max);

    // spec 04, 4.5.3 to 4.5.4 step 2: one delay per interval, then the
    // second re-aligned signal built from the first with that delay.
    let mut second = realigned.clone();
    let mut capped = Vec::with_capacity(intervals.len());
    for &(a, b) in &intervals {
        let start_sample = a * FRAME_HOP + MARGIN_SAMPLES;
        let stop_sample = b * FRAME_HOP + FRAME_LEN + MARGIN_SAMPLES;
        let b = b.min(frame_stop);
        let delay = interval_delay(reference, &realigned, start_sample, stop_sample, n_max);
        for (i, slot) in second[start_sample..stop_sample].iter_mut().enumerate() {
            let j = (start_sample as i64 + i as i64 + i64::from(delay)).clamp(0, n_max as i64 - 1)
                as usize;
            *slot = realigned[j];
        }
        capped.push((a, b));
    }

    // spec 04, 4.5.4 step 3: recompute over each interval's frames.
    recompute_frames(model, &capped, &second, symmetric, asymmetric);
}

/// Badness mask of spec 04 section 4.5.1 step 1: `D[frame] > 30` on the
/// post-skip frame disturbances, with frame 0 forced to be not bad.
fn bad_mask(symmetric: &[f32]) -> Vec<bool> {
    let mut mask: Vec<bool> = symmetric.iter().map(|&d| d > BAD_FRAME_GATE).collect();
    if !mask.is_empty() {
        mask[0] = false;
    }
    mask
}

/// Two-frame dilation of spec 04 section 4.5.1 step 2: for each frame in
/// `[2, frame_stop - 2]` the dilated value is the minimum of the maximum
/// of the mask over the frame and its two left neighbours and the
/// maximum over the frame and its two right neighbours. Frames outside
/// the range keep the dilated value "not bad".
pub(crate) fn dilate(mask: &[bool]) -> Vec<bool> {
    let mut dilated = vec![false; mask.len()];
    if mask.len() < 5 {
        return dilated;
    }
    for frame in 2..=mask.len() - 3 {
        let left = mask[frame - 2] || mask[frame - 1] || mask[frame];
        let right = mask[frame] || mask[frame + 1] || mask[frame + 2];
        dilated[frame] = left && right;
    }
    dilated
}

/// Contiguous bad intervals of spec 04 section 4.5.1 step 3: runs of
/// dilated bad frames from a to b exclusive, recorded only when
/// `b - a >= 5`, at most [`MAX_BAD_INTERVALS`] intervals.
pub(crate) fn collect_intervals(dilated: &[bool]) -> Vec<(usize, usize)> {
    let mut intervals = Vec::new();
    let mut i = 0;
    while i < dilated.len() {
        if !dilated[i] {
            i += 1;
            continue;
        }
        let a = i;
        while i < dilated.len() && dilated[i] {
            i += 1;
        }
        if i - a >= MIN_BAD_INTERVAL_FRAMES && intervals.len() < MAX_BAD_INTERVALS {
            intervals.push((a, i));
        }
    }
    intervals
}

/// First re-aligned degraded signal T of spec 04 section 4.5.2: length
/// `Nmax + P`, all zeros, and for each sample position i in
/// `[2400, Nmax + P - 2400)` the degraded sample at
/// `i + delay`, clamped to `[2400, Nmax + P - 2400 - 1]`, where delay is
/// the governing utterance's delay at i.
fn first_realigned(degraded: &SignalBuffer, utterances: &[Utterance], n_max: usize) -> Vec<f32> {
    let len = n_max + PADDING_SAMPLES;
    let mut realigned = vec![0.0f32; len];
    let lo = MARGIN_SAMPLES as i64;
    let hi = len as i64 - MARGIN_SAMPLES as i64 - 1;
    for (i, slot) in realigned[MARGIN_SAMPLES..len - MARGIN_SAMPLES]
        .iter_mut()
        .enumerate()
    {
        let position = MARGIN_SAMPLES + i;
        let delay = i64::from(governing_delay(position, utterances));
        let j = (position as i64 + delay).clamp(lo, hi) as usize;
        *slot = degraded.samples[j];
    }
    realigned
}

/// Normalized circular cross-correlation heights h(tau) of spec 04
/// section 4.5.3 step 5, at period `r = x.len()`.
///
/// The buffers hold the absolute values of the first m segment samples
/// and zeros beyond. `p1 = S1/r` and `p2 = S2/r` where S1 and S2 are the
/// plain sums of the first m squared samples; when either is at most
/// 1e-6 every height is 0. The correlation follows spec 04 step 5
/// literally: forward real FFT of both buffers, every bin of the first
/// spectrum divided by r, the binwise conjugate-first product, and the
/// inverse real FFT whose 1/r is part of the transform (spec 02, 2.1).
/// With the un-normalized forward transform the inverse of the product
/// carries an extra factor r, so output position k is `c(k)/r` and
/// `h = |output| / sqrt(p1 * p2) = |c| / sqrt(S1 * S2)`, the equality
/// stated in spec 04.
pub(crate) fn correlation_heights(x: &[f32], y: &[f32], m: usize) -> Vec<f64> {
    debug_assert_eq!(x.len(), y.len());
    let r = x.len();
    let s1: f64 = x[..m].iter().map(|&v| f64::from(v) * f64::from(v)).sum();
    let s2: f64 = y[..m].iter().map(|&v| f64::from(v) * f64::from(v)).sum();
    let p1 = s1 / r as f64;
    let p2 = s2 / r as f64;
    if p1 <= 1e-6 || p2 <= 1e-6 {
        return vec![0.0; r];
    }
    let mut spectrum_x = crate::dsp::real_fft(x);
    for bin in spectrum_x.iter_mut() {
        *bin /= r as f32;
    }
    let spectrum_y = crate::dsp::real_fft(y);
    let correlation = crate::dsp::inverse_real_fft(&conjugate_product(&spectrum_x, &spectrum_y));
    let denominator = (p1 * p2).sqrt();
    correlation
        .iter()
        .map(|&c| f64::from(c.abs()) / denominator)
        .collect()
}

/// Binwise conjugate-first product `conj(x) * y` of two packed
/// half-spectra (spec 01, 1.10 step 4b and spec 02, 2.8).
fn conjugate_product(x: &[f32], y: &[f32]) -> Vec<f32> {
    let mut product = vec![0.0f32; x.len()];
    for k in 0..x.len() / 2 {
        let (x_re, x_im) = (x[2 * k], x[2 * k + 1]);
        let (y_re, y_im) = (y[2 * k], y[2 * k + 1]);
        product[2 * k] = x_re * y_re + x_im * y_im;
        product[2 * k + 1] = x_re * y_im - x_im * y_re;
    }
    product
}

/// Per-interval delay search of spec 04 section 4.5.3: build the
/// zero-padded reference and degraded segments, the powers, and the
/// normalized circular correlation heights, then scan tau from -s to -1
/// (wrapped positions) and 0 to s - 1, keeping the first tau with a
/// strictly larger height. A best height below 0.5 forces the delay
/// to 0.
pub(crate) fn interval_delay(
    reference: &SignalBuffer,
    realigned: &[f32],
    start_sample: usize,
    stop_sample: usize,
    n_max: usize,
) -> i32 {
    let s = BAD_INTERVAL_SEARCH_SAMPLES;
    let n = stop_sample - start_sample;
    let m = 2 * s + n;
    let r = (2 * m).next_power_of_two();

    // Reference segment: s zeros, the n reference samples, s zeros.
    let mut x = vec![0.0f32; r];
    for (i, value) in reference.samples[start_sample..stop_sample]
        .iter()
        .enumerate()
    {
        x[s + i] = value.abs();
    }
    // Degraded segment: |T[j]| with j = start_sample - s + i, clamped.
    let mut y = vec![0.0f32; r];
    let lo = MARGIN_SAMPLES as i64;
    let hi = n_max as i64 + PADDING_SAMPLES as i64 - MARGIN_SAMPLES as i64 - 1;
    for (i, slot) in y[..m].iter_mut().enumerate() {
        let j = (start_sample as i64 + i as i64 - s as i64).clamp(lo, hi) as usize;
        *slot = realigned[j].abs();
    }

    let heights = correlation_heights(&x, &y, m);
    let mut best_h = 0.0f64;
    let mut best_lag = 0i32;
    for tau in -(s as i32)..0 {
        let h = heights[(tau + r as i32) as usize];
        if h > best_h {
            best_h = h;
            best_lag = tau;
        }
    }
    for tau in 0..s as i32 {
        let h = heights[tau as usize];
        if h > best_h {
            best_h = h;
            best_lag = tau;
        }
    }
    if best_h < f64::from(MIN_CORRELATION) {
        0
    } else {
        best_lag
    }
}

/// Degraded short-term spectrum at reference start r0 (zero relative
/// delay): Hann window, forward FFT, `real^2 + imag^2` per bin, bin 0
/// forced to zero, warped to the Bark bands (spec 04, 4.5.4 step 3a-3b
/// with spec 03, 3.2 and 3.3).
fn degraded_density(samples: &[f32], start: usize, window: &[f32]) -> [f32; NUM_BANDS] {
    let windowed: Vec<f32> = (0..FRAME_LEN)
        .map(|i| samples[start + i] * window[i])
        .collect();
    let packed = crate::dsp::real_fft(&windowed);
    let mut power = [0.0f64; POWER_BINS];
    for (bin, pair) in packed.chunks_exact(2).take(POWER_BINS).enumerate() {
        power[bin] = f64::from(pair[0] * pair[0] + pair[1] * pair[1]);
    }
    power[0] = 0.0;
    warp_to_bark(&power)
}

/// Re-run the perceptual model on the second re-aligned degraded signal
/// for the frames of each interval (spec 04, 4.5.4 step 3): degraded
/// spectrum at zero relative delay, warping, local gain scaling with the
/// previous scale reset to 1 before each interval's frame range, scaled
/// degraded densities, loudness for both signals, and the disturbance
/// norms, each frame keeping the minimum of the existing and the
/// recomputed value.
fn recompute_frames(
    model: &PerceptualModel,
    intervals: &[(usize, usize)],
    second: &[f32],
    symmetric: &mut [f32],
    asymmetric: &mut [f32],
) {
    let window = crate::dsp::hann_window(FRAME_LEN);
    let mut d = [0.0f32; NUM_BANDS];
    let mut d_asym = [0.0f32; NUM_BANDS];
    for &(a, b) in intervals {
        let mut previous_scale = 1.0;
        for frame in a..b {
            let base = frame * NUM_BANDS;
            let r0 = MARGIN_SAMPLES + frame * FRAME_HOP;
            let mut deg_density = degraded_density(second, r0, &window);
            let a_ref = audible_power(&model.pitch_ref[base..base + NUM_BANDS], 1.0);
            let a_deg = audible_power(&deg_density, 1.0);
            let (unclamped, clamped) = local_scale(a_ref, a_deg, previous_scale, frame);
            previous_scale = unclamped;
            for band in deg_density.iter_mut() {
                *band *= clamped as f32;
            }
            let mut loudness_ref = [0.0f32; NUM_BANDS];
            let mut loudness_deg = [0.0f32; NUM_BANDS];
            for band in 0..NUM_BANDS {
                loudness_ref[band] = zwicker_loudness(model.pitch_ref[base + band], band);
                loudness_deg[band] = zwicker_loudness(deg_density[band], band);
            }
            deadzone_removed(&loudness_ref, &loudness_deg, &mut d);
            symmetric[frame] = symmetric[frame].min(lp_norm(&d, 2.0) as f32);
            asymmetric_densities(
                &d,
                &model.pitch_ref[base..base + NUM_BANDS],
                &deg_density,
                &mut d_asym,
            );
            asymmetric[frame] = asymmetric[frame].min(lp_norm(&d_asym, 1.0) as f32);
        }
    }
}
