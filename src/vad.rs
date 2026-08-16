//! Voice activity detection of spec 01 section 1.8.
//!
//! The working buffer (after DC removal and the input IIR filter) is
//! split into W-sample windows over the first N samples. The energy
//! array goes through the threshold iterations, the sign encoding, the
//! short-run removal, the low-energy run removal, the gap joining, and
//! the edge smoothing, before the log-domain array is derived.

use crate::types::{SignalBuffer, VadData, WINDOW_SAMPLES};

/// Number of threshold iterations of spec 01 section 1.8 step 4.
const THRESHOLD_ITERATIONS: usize = 12;

/// Short speech runs of at most this many windows are negated
/// (spec 01, 1.8 step 8).
const MAX_SHORT_RUN: usize = 4;

/// Gaps of at most this many windows between positive runs are joined
/// (spec 01, 1.8 step 10).
const MAX_JOIN_GAP: usize = 50;

/// Voice activity detection of spec 01 section 1.8.
///
/// The analysis uses only the first N samples, split into V = N/W
/// windows; a fractional tail shorter than W samples is dropped by the
/// integer division.
pub fn voice_activity_detection(buffer: &SignalBuffer) -> VadData {
    let window_count = buffer.nominal_len / WINDOW_SAMPLES;

    // Step 1: mean of squares per window; sums accumulate in f64.
    let mut energy: Vec<f32> = (0..window_count)
        .map(|v| {
            let start = v * WINDOW_SAMPLES;
            let sum_squares: f64 = buffer.samples[start..start + WINDOW_SAMPLES]
                .iter()
                .map(|&sample| f64::from(sample * sample))
                .sum();
            (sum_squares / WINDOW_SAMPLES as f64) as f32
        })
        .collect();

    // Step 3: noise floor m and the per-window floor. The floor runs
    // before the threshold of step 2 is derived, so that the mean sits
    // at or above the floor and the empty-Q case of step 4 stays
    // unreachable (the minimum window is always at or below the mean,
    // as argued in spec 01 section 1.8 step 4).
    let maximum = energy.iter().copied().fold(0.0f32, f32::max);
    let noise_floor = if maximum > 0.0 { maximum * 1e-4 } else { 1.0 };
    for e in energy.iter_mut() {
        *e = e.max(noise_floor);
    }

    // Step 2: initial threshold is the mean of all window energies.
    let mean_all = energy.iter().map(|&e| f64::from(e)).sum::<f64>() / window_count as f64;
    let mut threshold = mean_all as f32;

    // Step 4: twelve threshold iterations over the window set Q of
    // energies at or below the threshold; the update is unconditional.
    for _ in 0..THRESHOLD_ITERATIONS {
        let (sum, count): (f64, usize) = energy
            .iter()
            .filter(|&&e| e <= threshold)
            .fold((0.0, 0), |(acc, n), &e| (acc + f64::from(e), n + 1));
        if count == 0 {
            threshold = 0.0;
        } else {
            let noise_mean = sum / count as f64;
            let mean_squared_deviation: f64 = energy
                .iter()
                .filter(|&&e| e <= threshold)
                .map(|&e| {
                    let deviation = f64::from(e) - noise_mean;
                    deviation * deviation
                })
                .sum::<f64>()
                / count as f64;
            threshold = (1.001 * (noise_mean + 2.0 * mean_squared_deviation.sqrt())) as f32;
        }
    }

    // Step 5: signal and noise levels; t becomes -1 without speech.
    let above: Vec<f32> = energy.iter().copied().filter(|&e| e > threshold).collect();
    let (signal_level, noise_level) = if above.is_empty() {
        threshold = -1.0;
        (0.0f32, 1.0f32)
    } else {
        let signal_level =
            (above.iter().map(|&e| f64::from(e)).sum::<f64>() / above.len() as f64) as f32;
        let below: Vec<f32> = energy.iter().copied().filter(|&e| e <= threshold).collect();
        let noise_level = if below.is_empty() {
            1.0f32
        } else {
            (below.iter().map(|&e| f64::from(e)).sum::<f64>() / below.len() as f64) as f32
        };
        (signal_level, noise_level)
    };

    // Step 6: sign encoding, positive = speech, negative = noise.
    for e in energy.iter_mut() {
        if *e <= threshold {
            *e = -*e;
        }
    }

    // Step 7: force the edge windows negative.
    energy[0] = -noise_floor;
    energy[window_count - 1] = -noise_floor;

    // Step 8: negate maximal positive runs of at most 4 windows.
    for (start, end) in positive_runs(&energy) {
        if end - start <= MAX_SHORT_RUN {
            for e in energy[start..end].iter_mut() {
                *e = -*e;
            }
        }
    }

    // Step 9: negate low-energy positive runs when the signal level
    // dominates the noise level.
    if signal_level >= 1000.0 * noise_level {
        for (start, end) in positive_runs(&energy) {
            let run_sum: f64 = energy[start..end].iter().map(|&e| f64::from(e)).sum();
            let limit = f64::from(3.0 * threshold * (end - start) as f32);
            if run_sum < limit {
                for e in energy[start..end].iter_mut() {
                    *e = -*e;
                }
            }
        }
    }

    // Step 10: join positive runs separated by at most 50 windows.
    let runs = positive_runs(&energy);
    for pair in runs.windows(2) {
        let (_prev_start, prev_end) = pair[0];
        let (next_start, _next_end) = pair[1];
        debug_assert!(next_start >= prev_end);
        if next_start - prev_end <= MAX_JOIN_GAP {
            for e in energy[prev_end..next_start].iter_mut() {
                *e = noise_floor;
            }
        }
    }

    // Step 11: with no positive window left, absolute values plus the
    // forced negative edges.
    if energy.iter().all(|&e| e <= 0.0) {
        for e in energy.iter_mut() {
            *e = e.abs();
        }
        energy[0] = -noise_floor;
        energy[window_count - 1] = -noise_floor;
    }

    // Step 12: edge smoothing in one pass with the specified control
    // flow per scan step.
    let mut v = 3;
    while v < window_count - 2 {
        if energy[v] > 0.0 && energy[v - 2] <= 0.0 {
            energy[v - 2] = 0.1 * energy[v];
            energy[v - 1] = 0.3 * energy[v];
            v += 1;
        }
        if energy[v] <= 0.0 && energy[v - 1] > 0.0 {
            energy[v] = 0.3 * energy[v - 1];
            energy[v + 1] = 0.1 * energy[v - 1];
            v += 3;
        }
        v += 1;
    }

    // Step 13: zero the negatives.
    for e in energy.iter_mut() {
        *e = e.max(0.0);
    }

    // Step 14: the log-domain array, with the silent-case threshold
    // restored to the noise floor m.
    let log_threshold = if threshold <= 0.0 {
        noise_floor
    } else {
        threshold
    };
    let log_vad = energy
        .iter()
        .map(|&e| {
            if e <= log_threshold {
                0.0
            } else {
                (e / log_threshold).ln()
            }
        })
        .collect();

    VadData {
        window_count,
        energy,
        log_vad,
        threshold,
        signal_level,
        noise_level,
    }
}

/// Maximal runs of consecutive positive windows, as (start, end) pairs
/// with the end exclusive.
fn positive_runs(energy: &[f32]) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut i = 0;
    while i < energy.len() {
        if energy[i] > 0.0 {
            let start = i;
            while i < energy.len() && energy[i] > 0.0 {
                i += 1;
            }
            runs.push((start, i));
        } else {
            i += 1;
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MARGIN_SAMPLES, SAMPLE_RATE_HZ};

    /// A signal buffer with two sine bursts separated by silence. The
    /// bursts start at PCM offset 0, so they begin at buffer window
    /// `MARGIN_SAMPLES / WINDOW_SAMPLES` after the margin layout of
    /// spec 01 section 1.2 step 6.
    fn two_bursts(amplitude: f32, burst_windows: usize, gap_windows: usize) -> SignalBuffer {
        let burst = burst_windows * WINDOW_SAMPLES;
        let gap = gap_windows * WINDOW_SAMPLES;
        let len = 2 * burst + gap + 2 * MARGIN_SAMPLES;
        let mut pcm = vec![0i16; len];
        let mut sine = |offset: usize, count: usize| {
            for i in 0..count {
                let phase = std::f32::consts::TAU * 1000.0 * i as f32 / SAMPLE_RATE_HZ as f32;
                pcm[offset + i] = (amplitude * phase.sin()).round() as i16;
            }
        };
        sine(0, burst);
        sine(burst + gap, burst);
        SignalBuffer::from_pcm(&pcm).unwrap()
    }

    #[test]
    fn vad_on_silence_is_all_zero_in_the_log_domain() {
        let buffer = SignalBuffer::from_pcm(&vec![0i16; 4000]).unwrap();
        let vad = voice_activity_detection(&buffer);
        assert_eq!(vad.threshold, -1.0);
        assert_eq!(vad.signal_level, 0.0);
        assert_eq!(vad.noise_level, 1.0);
        assert!(vad.log_vad.iter().all(|&l| l == 0.0));
        assert!(vad.energy.iter().all(|&e| e >= 0.0));
    }

    #[test]
    fn vad_on_speech_marks_the_bursts_and_clears_the_gap() {
        let buffer = two_bursts(3000.0, 40, 60);
        let vad = voice_activity_detection(&buffer);
        assert!(vad.threshold > 0.0);
        let first = MARGIN_SAMPLES / WINDOW_SAMPLES;
        let burst = 40usize;
        let gap = 60usize;
        // Mid-burst windows carry speech energy and a positive log VAD.
        let mid_first = first + burst / 2;
        assert!(vad.energy[mid_first] > 0.0);
        assert!(vad.log_vad[mid_first] > 0.0);
        let mid_second = first + burst + gap + burst / 2;
        assert!(vad.energy[mid_second] > 0.0);
        assert!(vad.log_vad[mid_second] > 0.0);
        // Mid-gap windows are silent: zero energy, zero log VAD.
        let mid_gap = first + burst + gap / 2;
        assert_eq!(vad.energy[mid_gap], 0.0);
        assert_eq!(vad.log_vad[mid_gap], 0.0);
    }

    #[test]
    fn vad_on_noise_stays_finite_and_nonnegative() {
        // Deterministic white noise at a modest level: every processed
        // energy is non-negative and every log value is finite.
        let mut state = 42u32;
        let len = 4000usize;
        let pcm: Vec<i16> = (0..len)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((state as f32 / u32::MAX as f32) * 1000.0 - 500.0) as i16
            })
            .collect();
        let buffer = SignalBuffer::from_pcm(&pcm).unwrap();
        let vad = voice_activity_detection(&buffer);
        assert!(vad.energy.iter().all(|&e| e >= 0.0 && e.is_finite()));
        assert!(vad.log_vad.iter().all(|&l| l.is_finite()));
        assert_eq!(vad.energy.len(), buffer.nominal_len / WINDOW_SAMPLES);
    }

    #[test]
    fn vad_window_count_is_n_over_w() {
        let buffer = SignalBuffer::from_pcm(&vec![0i16; 4000]).unwrap();
        let vad = voice_activity_detection(&buffer);
        assert_eq!(vad.window_count, buffer.nominal_len / WINDOW_SAMPLES);
        assert_eq!(vad.energy.len(), vad.log_vad.len());
    }
}
