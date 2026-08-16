//! Coarse and fine time alignment (spec 01, sections 1.9 and 1.10) and
//! the utterance search windows (1.11).
//!
//! The coarse estimate correlates the log-domain VAD arrays with the FFT
//! procedure of spec 02 section 2.8; the fine estimate cross-correlates
//! Hann-windowed A-sample frames of the working buffers and votes lag
//! candidates into a histogram. The search windows of 1.11 delimit the
//! per-utterance alignment passes.

use crate::dsp::{circular_convolve, correlate, hann_window, spectral_cross_correlate};
use crate::types::{ALIGN_FFT_LEN, VadData, WINDOW_SAMPLES};

/// Fine-alignment frame advance, A/4 samples (spec 01, 1.10 step 4e).
const FINE_ADVANCE: usize = ALIGN_FFT_LEN / 4;

/// Histogram smoothing radius K = A/64 (spec 01, 1.10 step 5).
const SMOOTH_RADIUS: usize = ALIGN_FFT_LEN / 64;

/// Whole-signal coarse delay estimation of spec 01 section 1.9 steps 1
/// to 6: the FFT cross-correlation of the two log-VAD arrays, the strict
/// maximizer starting from value 0 at index Vr - 1, and the conversion
/// to samples.
pub fn coarse_delay_whole(reference: &VadData, degraded: &VadData) -> i32 {
    let x = &reference.log_vad;
    let y = &degraded.log_vad;
    if x.len() <= 1 || y.len() <= 1 {
        return 0;
    }
    lag_from_correlation(&correlate(x, y), x.len())
}

/// Per-utterance coarse estimate of spec 01 section 1.9 step 7, seeded
/// with the whole-signal or utterance coarse delay `seed`, restricted to
/// the search window `[s0, s1)` of the reference.
pub fn coarse_delay_search(
    reference: &VadData,
    degraded: &VadData,
    s0: usize,
    s1: usize,
    seed: i32,
) -> i32 {
    let window_divisor = WINDOW_SAMPLES as i32;
    let mut s0 = s0;
    let mut nr = s1 - s0;
    let mut sd = s0 as i32 + seed / window_divisor;
    if sd < 0 {
        s0 = ((-seed) / window_divisor) as usize;
        sd = 0;
        nr = s1 - s0;
    }
    let mut nd = nr;
    if sd as usize + nd > degraded.window_count {
        nd = degraded.window_count - sd as usize;
    }
    if nr <= 1 || nd <= 1 {
        return seed;
    }
    let x = &reference.log_vad[s0..s0 + nr];
    let y = &degraded.log_vad[sd as usize..sd as usize + nd];
    let correlation = correlate(x, y);
    let (best_index, _) = maximizer(&correlation, nr - 1);
    (best_index as i32 - nr as i32 + 1) * window_divisor + seed
}

/// Fine time alignment of spec 01 section 1.10 for one utterance search
/// window `[s0, s1]` and initial delay `d0`, returning the fine delay in
/// samples and its confidence.
///
/// `reference` and `degraded` are the working buffers (after 1.5 and
/// 1.6) and `degraded_nominal` is the degraded nominal length N.
pub fn fine_delay(
    reference: &[f32],
    degraded: &[f32],
    s0: usize,
    s1: usize,
    d0: i32,
    degraded_nominal: usize,
) -> (i32, f32) {
    let window = hann_window(ALIGN_FFT_LEN);
    let mut histogram = vec![0.0f32; ALIGN_FFT_LEN];

    // Step 3: the cursors, with the negative-degraded-cursor clamp that
    // replaces (not adds to) the reference cursor.
    let mut ref_cursor = (s0 * WINDOW_SAMPLES) as i64;
    let mut deg_cursor = ref_cursor + i64::from(d0);
    if deg_cursor < 0 {
        ref_cursor = i64::from(-d0);
        deg_cursor = 0;
    }

    // Step 4: accumulate the weighted lag histogram over A-sample frames
    // with a 75 percent overlap.
    let mut frame_ref = vec![0.0f32; ALIGN_FFT_LEN];
    let mut frame_deg = vec![0.0f32; ALIGN_FFT_LEN];
    while deg_cursor + ALIGN_FFT_LEN as i64 <= degraded_nominal as i64
        && ref_cursor + ALIGN_FFT_LEN as i64 <= (s1 * WINDOW_SAMPLES) as i64
    {
        let ref_start = ref_cursor as usize;
        let deg_start = deg_cursor as usize;
        for i in 0..ALIGN_FFT_LEN {
            frame_ref[i] = window[i] * reference[ref_start + i];
            frame_deg[i] = window[i] * degraded[deg_start + i];
        }
        let spectrum = spectral_cross_correlate(&frame_ref, &frame_deg);
        let peak = spectrum.iter().copied().fold(0.0f32, f32::max);
        let threshold = 0.99 * peak;
        let weight = threshold.powf(0.125);
        for (lag, &value) in spectrum.iter().enumerate() {
            if value > threshold {
                histogram[lag] += weight;
            }
        }
        ref_cursor += FINE_ADVANCE as i64;
        deg_cursor += FINE_ADVANCE as i64;
    }

    // Step 5: circular smoothing with the triangular kernel of radius
    // K = 8: kernel[0] = 1 and kernel[k] = kernel[A - k] = 1 - k/8 for
    // k = 1..=7.
    let mut kernel = vec![0.0f32; ALIGN_FFT_LEN];
    kernel[0] = 1.0;
    for k in 1..=SMOOTH_RADIUS - 1 {
        let value = 1.0 - k as f32 / SMOOTH_RADIUS as f32;
        kernel[k] = value;
        kernel[ALIGN_FFT_LEN - k] = value;
    }
    histogram = circular_convolve(&histogram, &kernel);

    // Step 6: normalize by the raw histogram sum.
    let sum: f64 = histogram.iter().map(|&h| f64::from(h)).sum();
    if sum > 0.0 {
        let sum = sum as f32;
        for h in histogram.iter_mut() {
            *h = (*h / sum).abs();
        }
    } else {
        histogram.fill(0.0);
    }

    // Steps 7 and 8: the peak lag, folded into [-A/2, A/2), and the
    // final delay plus confidence.
    let mut peak_index = 0usize;
    let mut peak_value = 0.0f32;
    for (lag, &value) in histogram.iter().enumerate() {
        if value > peak_value {
            peak_value = value;
            peak_index = lag;
        }
    }
    let folded = if peak_index >= ALIGN_FFT_LEN / 2 {
        peak_index as i32 - ALIGN_FFT_LEN as i32
    } else {
        peak_index as i32
    };
    (d0 + folded, peak_value)
}

/// Lag conversion of spec 01 section 1.9 step 5 for a correlation
/// output whose first sequence had `vr` elements.
fn lag_from_correlation(correlation: &[f32], vr: usize) -> i32 {
    let (best_index, _) = maximizer(correlation, vr - 1);
    (best_index as i32 - vr as i32 + 1) * WINDOW_SAMPLES as i32
}

/// Strict maximizer of spec 01 section 1.9 step 4: starts with value 0
/// at index `vr` and updates only on strict improvement.
fn maximizer(values: &[f32], vr: usize) -> (usize, f32) {
    let mut best_value = 0.0f32;
    let mut best_index = vr;
    for (index, &value) in values.iter().enumerate() {
        if value > best_value {
            best_value = value;
            best_index = index;
        }
    }
    (best_index, best_value)
}

/// Utterance search windows of spec 01 section 1.11: runs of speech
/// windows (energy above zero, step 2) qualifying as utterances yield
/// windows widened by the 75-window margin.
pub fn search_windows(
    reference: &VadData,
    coarse_delay: i32,
    degraded_nominal: usize,
) -> Vec<(usize, usize)> {
    let last = reference.window_count - 1;
    qualifying_runs(reference, coarse_delay, degraded_nominal)
        .into_iter()
        .map(|(a, c)| (a.saturating_sub(75), (c + 75).min(last)))
        .collect()
}

/// Qualifying utterance runs of spec 01 sections 1.11 and 1.12 step 1:
/// maximal runs of speech windows (`energy[v] > 0`) that satisfy the
/// length and degradation-bound conditions of 1.11 step 3, as (start,
/// trigger) pairs. The trigger `c` is exclusive for a non-speech
/// trigger and `V - 1` (inclusive) for a run reaching the last window.
pub fn qualifying_runs(
    reference: &VadData,
    coarse_delay: i32,
    degraded_nominal: usize,
) -> Vec<(usize, usize)> {
    let window_divisor = WINDOW_SAMPLES as i32;
    let b1 = 50 - coarse_delay / window_divisor;
    let b2 = (degraded_nominal as i32 - coarse_delay) / window_divisor - 50;
    let last = reference.window_count - 1;
    let mut runs = Vec::new();
    let mut a = 0usize;
    while a < reference.window_count {
        if reference.energy[a] == 0.0 {
            a += 1;
            continue;
        }
        let start = a;
        while a < reference.window_count && reference.energy[a] > 0.0 {
            a += 1;
        }
        let (c, length) = if a == reference.window_count {
            (last, last - start)
        } else {
            (a, a - start)
        };
        if length >= 50 && (start as i64) < i64::from(b2) && (c as i64) > i64::from(b1) {
            runs.push((start, c));
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SignalBuffer;

    /// VAD data with one burst of `run` speech windows starting at
    /// `onset`, inside `total` windows.
    fn vad_with_burst(total: usize, onset: usize, run: usize) -> VadData {
        let mut energy = vec![0.0f32; total];
        for e in energy[onset..onset + run].iter_mut() {
            *e = 1.0;
        }
        let log_vad = energy
            .iter()
            .map(|&e| if e > 0.0 { 1.0 } else { 0.0 })
            .collect();
        VadData {
            window_count: total,
            energy,
            log_vad,
            threshold: 0.5,
            signal_level: 1.0,
            noise_level: 0.0,
        }
    }

    #[test]
    fn coarse_delay_finds_a_shifted_log_vad_burst() {
        let total = 400usize;
        let shift = 6usize;
        let reference = vad_with_burst(total, 100, 40);
        let degraded = vad_with_burst(total, 100 + shift, 40);
        let lag = coarse_delay_whole(&reference, &degraded);
        assert_eq!(lag, shift as i32 * WINDOW_SAMPLES as i32);
    }

    #[test]
    fn coarse_delay_is_zero_for_single_window_vad() {
        let tiny = vad_with_burst(1, 0, 1);
        assert_eq!(coarse_delay_whole(&tiny, &tiny), 0);
    }

    #[test]
    fn per_utterance_coarse_seeds_from_the_window() {
        let total = 400usize;
        let reference = vad_with_burst(total, 100, 40);
        let degraded = vad_with_burst(total, 106, 40);
        let estimate = coarse_delay_search(&reference, &degraded, 25, 235, 0);
        assert_eq!(estimate, 6 * WINDOW_SAMPLES as i32);
    }

    #[test]
    fn per_utterance_coarse_returns_the_seed_for_tiny_windows() {
        let reference = vad_with_burst(400, 100, 40);
        let degraded = vad_with_burst(400, 100, 40);
        assert_eq!(coarse_delay_search(&reference, &degraded, 100, 101, 17), 17);
    }

    /// A working-buffer pair where the degraded signal is the reference
    /// delayed by `shift` samples. The content is aperiodic noise so the
    /// cross-correlation has a single sharp peak at the true lag (a
    /// periodic tone would repeat its peaks every period).
    fn shifted_pair(shift: usize) -> (SignalBuffer, SignalBuffer) {
        let signal_len = 16000usize;
        let mut state = 99u32;
        let mut ref_pcm = vec![0i16; signal_len];
        for sample in ref_pcm[2000..12000].iter_mut() {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *sample = ((state as f32 / u32::MAX as f32) * 8000.0 - 4000.0) as i16;
        }
        let reference = SignalBuffer::from_pcm(&ref_pcm).unwrap();
        let mut deg_pcm = vec![0i16; shift];
        deg_pcm.extend_from_slice(&ref_pcm);
        let degraded = SignalBuffer::from_pcm(&deg_pcm[..signal_len]).unwrap();
        (reference, degraded)
    }

    #[test]
    fn fine_delay_recovers_a_known_sample_shift() {
        // The degraded copy is delayed by 64 samples, so the frames
        // correlate best at lag 64 and the fine delay is 64 samples.
        let (reference, degraded) = shifted_pair(64);
        let (delay, confidence) = fine_delay(
            &reference.samples,
            &degraded.samples,
            75,
            reference.nominal_len / WINDOW_SAMPLES - 75,
            0,
            degraded.nominal_len,
        );
        assert_eq!(delay, 64);
        assert!(confidence > 0.0);
    }

    #[test]
    fn fine_delay_is_zero_for_identical_buffers() {
        let (reference, _) = shifted_pair(0);
        let (delay, confidence) = fine_delay(
            &reference.samples,
            &reference.samples,
            75,
            reference.nominal_len / WINDOW_SAMPLES - 75,
            0,
            reference.nominal_len,
        );
        assert_eq!(delay, 0);
        assert!(confidence > 0.0);
    }

    #[test]
    fn fine_delay_respects_the_degraded_length_clamp() {
        // A degraded nominal length that ends before the first frame is
        // reached yields an empty histogram: the delay stays the seed
        // and the confidence is zero.
        let (reference, _) = shifted_pair(0);
        let (delay, confidence) = fine_delay(
            &reference.samples,
            &reference.samples,
            75,
            reference.nominal_len / WINDOW_SAMPLES - 75,
            0,
            WINDOW_SAMPLES,
        );
        assert_eq!(delay, 0);
        assert_eq!(confidence, 0.0);
    }

    #[test]
    fn search_windows_follow_the_run_qualification_rules() {
        // Two 60-window bursts at [100, 160) and [300, 360) in 400
        // windows, no coarse delay: both qualify and widen by 75.
        let mut vad = vad_with_burst(400, 100, 60);
        for e in vad.energy[300..360].iter_mut() {
            *e = 1.0;
        }
        let windows = search_windows(&vad, 0, 400 * WINDOW_SAMPLES);
        assert_eq!(windows, vec![(25, 235), (225, 399)]);
    }

    #[test]
    fn search_windows_reject_short_runs() {
        let vad = vad_with_burst(400, 100, 49);
        assert!(search_windows(&vad, 0, 400 * WINDOW_SAMPLES).is_empty());
    }
}
