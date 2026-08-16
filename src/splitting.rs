//! Utterance splitting internals (spec 01, section 1.13).
//!
//! [`best_split`] runs the candidate machinery of steps 1 to 8 for one
//! utterance: the speech extent, the breakpoint grid, the per-candidate
//! coarse estimates, and the forward and backward fine passes with peak
//! spreading. [`crate::utterances`] applies an accepted split to the
//! utterance list.

use crate::alignment::coarse_delay_search;
use crate::dsp::{circular_convolve, hann_window, spectral_cross_correlate};
use crate::types::{ALIGN_FFT_LEN, SignalBuffer, VadData, WINDOW_SAMPLES};
use crate::utterances::UtteranceWork;

/// Minimum speech length in windows for a split attempt (spec 01, 1.13
/// step 2).
const MIN_SPLIT_SPEECH_WINDOWS: usize = 200;

/// Breakpoint grid spacing D = A/(4*W) (spec 01, 1.13 step 3).
const SPLIT_GRID_STEP: usize = 4;

/// Maximum number of split candidates (spec 01, 1.13 step 3).
const SPLIT_MAX_CANDIDATES: usize = 40;

/// Histogram smoothing radius K = A/64 (spec 01, 1.10 step 5).
const SMOOTH_RADIUS: usize = ALIGN_FFT_LEN / 64;

/// A split accepted by the rules of spec 01 section 1.13 step 8.
pub(crate) struct Split {
    pub(crate) breakpoint: usize,
    pub(crate) left_estimate: i32,
    pub(crate) right_estimate: i32,
    pub(crate) forward_delay: i32,
    pub(crate) backward_delay: i32,
    pub(crate) forward_confidence: f32,
    pub(crate) backward_confidence: f32,
}

/// The split attempt of spec 01 section 1.13 steps 1 to 8 for one
/// utterance, or `None` when no candidate qualifies.
///
/// `work.start` and `work.end` are the inclusive utterance boundaries,
/// `work.coarse` the utterance coarse estimate, and `work.confidence`
/// the confidence of 1.10 step 8 that the candidates must exceed.
pub(crate) fn best_split(
    work: &UtteranceWork,
    reference: &SignalBuffer,
    degraded: &SignalBuffer,
    ref_vad: &VadData,
    deg_vad: &VadData,
) -> Option<Split> {
    let (start, end) = (work.start, work.end);
    let (coarse, utterance_confidence) = (work.coarse, work.confidence);
    // Steps 1 and 2: the speech extent, trimmed of zero-energy windows.
    let mut speech_start = start;
    let mut speech_end = end;
    while speech_start < speech_end && ref_vad.energy[speech_start] == 0.0 {
        speech_start += 1;
    }
    while speech_end > start && ref_vad.energy[speech_end] == 0.0 {
        speech_end -= 1;
    }
    speech_end += 1;
    let speech_length = speech_end - speech_start;
    if speech_length < MIN_SPLIT_SPEECH_WINDOWS {
        return None;
    }

    // Step 3: the breakpoint grid.
    let step = (((0.801 * speech_length as f64 + 40.0 * SPLIT_GRID_STEP as f64 - 1.0)
        / (40.0 * SPLIT_GRID_STEP as f64))
        .floor() as usize)
        * SPLIT_GRID_STEP;
    let pad = (speech_length / 10).max(75);
    let mut candidates = Vec::new();
    let mut breakpoint = speech_start + pad;
    while breakpoint <= speech_end.saturating_sub(pad) && candidates.len() < SPLIT_MAX_CANDIDATES {
        candidates.push(breakpoint);
        breakpoint += step;
    }
    if candidates.is_empty() {
        return None;
    }

    // Step 4: per-candidate coarse estimates, left and right, seeded
    // with the utterance coarse estimate.
    let left_estimates: Vec<i32> = candidates
        .iter()
        .map(|&c| coarse_delay_search(ref_vad, deg_vad, start, c, coarse))
        .collect();
    let right_estimates: Vec<i32> = candidates
        .iter()
        .map(|&c| coarse_delay_search(ref_vad, deg_vad, c, end, coarse))
        .collect();

    // Step 5: the forward fine pass, extended over candidates sharing
    // the same left estimate.
    let count = candidates.len();
    let mut forward_delay = vec![0i32; count];
    let mut forward_confidence = vec![0.0f32; count];
    let mut i = 0;
    while i < count {
        let seed = left_estimates[i];
        let mut j = i;
        while j + 1 < count && left_estimates[j + 1] == seed {
            j += 1;
        }
        let (delay, confidence) = directional_pass(
            reference,
            degraded,
            start,
            candidates[j],
            seed,
            degraded.nominal_len,
            false,
        );
        for estimate in forward_delay.iter_mut().take(j + 1).skip(i) {
            *estimate = delay;
        }
        for conf in forward_confidence.iter_mut().take(j + 1).skip(i) {
            *conf = confidence;
        }
        i = j + 1;
    }

    // Step 7: the backward fine pass for eligible candidates, extended
    // over candidates sharing the same right estimate.
    let mut backward_delay = vec![0i32; count];
    let mut backward_confidence = vec![0.0f32; count];
    let mut idx = count;
    while idx > 0 {
        idx -= 1;
        if forward_confidence[idx] <= utterance_confidence {
            continue;
        }
        let seed = right_estimates[idx];
        let mut j = idx;
        while j > 0 && right_estimates[j - 1] == seed {
            j -= 1;
        }
        let (delay, confidence) = directional_pass(
            reference,
            degraded,
            end,
            candidates[j],
            seed,
            degraded.nominal_len,
            true,
        );
        for estimate in backward_delay.iter_mut().take(idx + 1).skip(j) {
            *estimate = delay;
        }
        for conf in backward_confidence.iter_mut().take(idx + 1).skip(j) {
            *conf = confidence;
        }
        idx = j;
    }

    // Step 8: the best split by scan order with strict improvement of
    // the summed confidence.
    let mut best_index = None;
    let mut best_sum = 0.0f32;
    for i in 0..count {
        let (fd, bd) = (forward_delay[i], backward_delay[i]);
        if (fd - bd).abs() >= WINDOW_SAMPLES as i32
            && forward_confidence[i] > utterance_confidence
            && backward_confidence[i] > utterance_confidence
        {
            let sum = forward_confidence[i] + backward_confidence[i];
            if best_index.is_none() || sum > best_sum {
                best_index = Some(i);
                best_sum = sum;
            }
        }
    }
    let i = best_index?;
    Some(Split {
        breakpoint: candidates[i],
        left_estimate: left_estimates[i],
        right_estimate: right_estimates[i],
        forward_delay: forward_delay[i],
        backward_delay: backward_delay[i],
        forward_confidence: forward_confidence[i],
        backward_confidence: backward_confidence[i],
    })
}

/// One fine-pass histogram with peak spreading (spec 01, 1.13 steps 5
/// to 7). The forward pass scans from `boundary` toward `candidate`; the
/// backward pass scans from `boundary` back to `candidate`.
fn directional_pass(
    reference: &SignalBuffer,
    degraded: &SignalBuffer,
    boundary: usize,
    candidate: usize,
    seed: i32,
    degraded_nominal: usize,
    backward: bool,
) -> (i32, f32) {
    let window = hann_window(ALIGN_FFT_LEN);
    let mut histogram = vec![0.0f32; ALIGN_FFT_LEN];
    let mut hsum = 0.0f64;
    let mut frame_ref = vec![0.0f32; ALIGN_FFT_LEN];
    let mut frame_deg = vec![0.0f32; ALIGN_FFT_LEN];

    let mut ref_cursor: i64 = if backward {
        boundary as i64 * WINDOW_SAMPLES as i64 - ALIGN_FFT_LEN as i64
    } else {
        boundary as i64 * WINDOW_SAMPLES as i64
    };
    let mut deg_cursor = ref_cursor + i64::from(seed);
    if deg_cursor < 0 {
        ref_cursor = i64::from(-seed);
        deg_cursor = 0;
    }
    if backward && deg_cursor + ALIGN_FFT_LEN as i64 > degraded_nominal as i64 {
        deg_cursor = degraded_nominal as i64 - ALIGN_FFT_LEN as i64;
        ref_cursor = deg_cursor - i64::from(seed);
    }
    let candidate_samples = candidate as i64 * WINDOW_SAMPLES as i64;
    loop {
        let in_range = if backward {
            deg_cursor >= 0 && ref_cursor >= candidate_samples
        } else {
            deg_cursor + ALIGN_FFT_LEN as i64 <= degraded_nominal as i64
                && ref_cursor + ALIGN_FFT_LEN as i64 <= candidate_samples
        };
        if !in_range {
            break;
        }
        let ref_start = ref_cursor as usize;
        let deg_start = deg_cursor as usize;
        for i in 0..ALIGN_FFT_LEN {
            frame_ref[i] = window[i] * reference.samples[ref_start + i];
            frame_deg[i] = window[i] * degraded.samples[deg_start + i];
        }
        let spectrum = spectral_cross_correlate(&frame_ref, &frame_deg);
        let peak = spectrum.iter().copied().fold(0.0f32, f32::max);
        let v = 0.99 * peak;
        let unit = v.powf(0.125) / 8.0;
        for (lag, &value) in spectrum.iter().enumerate() {
            if value > v {
                for k in -7..=7i32 {
                    let index = (lag as i32 + k).rem_euclid(ALIGN_FFT_LEN as i32) as usize;
                    histogram[index] += unit * (8 - k.unsigned_abs()) as f32;
                }
            }
        }
        hsum += f64::from(v.powf(0.125));
        if backward {
            ref_cursor -= ALIGN_FFT_LEN as i64 / 4;
            deg_cursor -= ALIGN_FFT_LEN as i64 / 4;
        } else {
            ref_cursor += ALIGN_FFT_LEN as i64 / 4;
            deg_cursor += ALIGN_FFT_LEN as i64 / 4;
        }
    }

    // Smoothing of 1.10 step 5, then the folded peak; the confidence is
    // peak / Hsum (0 with a non-positive Hsum).
    let mut kernel = vec![0.0f32; ALIGN_FFT_LEN];
    kernel[0] = 1.0;
    for k in 1..=SMOOTH_RADIUS - 1 {
        let value = 1.0 - k as f32 / SMOOTH_RADIUS as f32;
        kernel[k] = value;
        kernel[ALIGN_FFT_LEN - k] = value;
    }
    histogram = circular_convolve(&histogram, &kernel);
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
    let confidence = if hsum > 0.0 {
        f64::from(peak_value) / hsum
    } else {
        0.0
    } as f32;
    (seed + folded, confidence)
}
