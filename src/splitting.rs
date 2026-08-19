//! Utterance splitting internals (spec 01, section 1.13).
//!
//! The `best_split` helper runs the candidate machinery of steps 1 to 8
//! for one
//! utterance: the speech extent, the breakpoint grid, the per-candidate
//! coarse estimates, and the forward and backward fine passes with peak
//! spreading. [`crate::utterances`] applies an accepted split to the
//! utterance list.

use crate::alignment::coarse_correlation;
use crate::dsp::{hann_window, spectral_cross_correlate};
use crate::types::{Rate, SignalBuffer, VadData};
use crate::utterances::UtteranceWork;

/// Minimum speech length in windows for a split attempt (spec 01, 1.13
/// step 2).
const MIN_SPLIT_SPEECH_WINDOWS: usize = 200;

/// Maximum number of split candidates (spec 01, 1.13 step 3).
const SPLIT_MAX_CANDIDATES: usize = 40;

/// Breakpoint grid spacing D = A/(4*W) (spec 01, 1.13 step 3): 4 at
/// both rates.
fn split_grid_step(rate: Rate) -> usize {
    rate.align_fft_len() / (4 * rate.window_samples())
}

/// Scratch offset of the split-pass window region, 3A + 6 floats from
/// the scratch base (spec 01, 1.13.1). Step-4 correlation outputs that
/// reach past this offset overwrite the window coefficients there.
fn scratch_window_offset(rate: Rate) -> usize {
    3 * rate.align_fft_len() + 6
}

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
    let rate = reference.rate;
    let window_samples = rate.window_samples();
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
    let grid = split_grid_step(rate);
    let step = (((0.801 * speech_length as f64 + 40.0 * grid as f64 - 1.0) / (40.0 * grid as f64))
        .floor() as usize)
        * grid;
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

    // Step 4: per-candidate coarse estimates, left then right per
    // candidate, seeded with the utterance coarse estimate. The coarse
    // correlations share the scratch with the fine-pass window, so the
    // effective window of spec 01 section 1.13.1 is derived here: the
    // Hann fill runs once per split attempt, and every correlation that
    // runs overwrites the window coefficients its output reaches, in
    // call order.
    let mut effective_window = hann_window(rate.align_fft_len());
    let mut left_estimates = Vec::with_capacity(candidates.len());
    let mut right_estimates = Vec::with_capacity(candidates.len());
    for &candidate in &candidates {
        match coarse_correlation(ref_vad, deg_vad, start, candidate, coarse, rate) {
            Some((estimate, correlation)) => {
                corrupt_window(&mut effective_window, &correlation, rate);
                left_estimates.push(estimate);
            }
            None => left_estimates.push(coarse),
        }
        match coarse_correlation(ref_vad, deg_vad, candidate, end, coarse, rate) {
            Some((estimate, correlation)) => {
                corrupt_window(&mut effective_window, &correlation, rate);
                right_estimates.push(estimate);
            }
            None => right_estimates.push(coarse),
        }
    }

    // Step 5: the forward fine pass. Each pass starts fresh at the
    // first candidate whose forward confidence is not yet computed and
    // drives one accumulation from the utterance start through every
    // later breakpoint in order; when the accumulation reaches a
    // candidate's breakpoint, that candidate's own forward delay and
    // confidence are recorded from the histogram accumulated so far.
    // Candidates with a different left estimate are left uncomputed and
    // the cursors keep advancing past them, so the values recorded
    // within one pass can differ between candidates (1.13 step 5).
    let count = candidates.len();
    let mut forward_delay = vec![0i32; count];
    let mut forward_confidence = vec![0.0f32; count];
    let mut computed = vec![false; count];
    let mut i = 0;
    while i < count {
        let seed = left_estimates[i];
        let mut pass = SplitAccumulator::new(
            reference,
            degraded,
            start,
            seed,
            false,
            &effective_window,
            rate,
        );
        let mut j = i;
        while j < count {
            pass.advance_to(candidates[j] as i64 * window_samples as i64);
            if left_estimates[j] == seed {
                let (delay, confidence) = pass.record(seed);
                forward_delay[j] = delay;
                forward_confidence[j] = confidence;
                computed[j] = true;
            }
            j += 1;
        }
        while i < count && computed[i] {
            i += 1;
        }
    }

    // Step 7: the backward fine pass. Each pass starts at the last
    // candidate whose backward confidence is uncomputed and whose
    // forward confidence exceeds the utterance confidence, and drives
    // one accumulation from the utterance end downward through every
    // earlier breakpoint, recording per candidate exactly as in step 5.
    // Candidates whose forward confidence did not exceed the utterance
    // confidence keep backward confidence 0 (1.13 step 7).
    let mut backward_delay = vec![0i32; count];
    let mut backward_confidence = vec![0.0f32; count];
    let mut computed = vec![false; count];
    let mut index = count;
    loop {
        let mut start_index = index;
        let mut found = None;
        while start_index > 0 {
            start_index -= 1;
            if !computed[start_index] && forward_confidence[start_index] > utterance_confidence {
                found = Some(start_index);
                break;
            }
        }
        let Some(start_index) = found else {
            break;
        };
        let seed = right_estimates[start_index];
        let mut pass = SplitAccumulator::new(
            reference,
            degraded,
            end,
            seed,
            true,
            &effective_window,
            rate,
        );
        let mut k = start_index;
        loop {
            pass.advance_to(candidates[k] as i64 * window_samples as i64);
            if right_estimates[k] == seed
                && !computed[k]
                && forward_confidence[k] > utterance_confidence
            {
                let (delay, confidence) = pass.record(seed);
                backward_delay[k] = delay;
                backward_confidence[k] = confidence;
                computed[k] = true;
            }
            if k == 0 {
                break;
            }
            k -= 1;
        }
        index = start_index;
    }

    // Step 8: the best split by scan order with strict improvement of
    // the summed confidence.
    let mut best_index = None;
    let mut best_sum = 0.0f32;
    for i in 0..count {
        let (fd, bd) = (forward_delay[i], backward_delay[i]);
        if (fd - bd).abs() >= window_samples as i32
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

/// One incremental fine-pass accumulation with peak spreading (spec 01,
/// 1.13 steps 5 to 7). The cursors advance toward successive candidate
/// breakpoints; recording a candidate reads the histogram accumulated
/// so far, so values recorded at different breakpoints of one pass can
/// differ. One pass serves every candidate that shares its seed.
///
/// The running state of one accumulation: the two cursors, the raw
/// histogram and its running sum, and the per-frame scratch buffers.
struct SplitAccumulator<'a> {
    reference: &'a [f32],
    degraded: &'a [f32],
    degraded_nominal: usize,
    rate: Rate,
    ref_cursor: i64,
    deg_cursor: i64,
    backward: bool,
    window: Vec<f32>,
    histogram: Vec<f32>,
    hsum: f64,
    frame_ref: Vec<f32>,
    frame_deg: Vec<f32>,
}

impl<'a> SplitAccumulator<'a> {
    /// A fresh accumulation starting at `boundary` (the utterance start
    /// for a forward pass, the utterance end for a backward pass) with
    /// the initial delay `seed`. `window` is the effective split window
    /// of spec 01 section 1.13.1, the Hann fill partly overwritten by
    /// the step-4 correlation outputs of this split attempt.
    ///
    /// Forward (step 5): the reference cursor starts at `boundary * W`
    /// and the negative-degraded-cursor clamp of 1.10 step 3 applies as
    /// a replacement of the reference cursor. Backward (step 7): the
    /// reference cursor starts at `boundary * W - A`; when the degraded
    /// cursor plus A runs past the nominal length, the degraded cursor
    /// moves to `N_deg - A` and the reference cursor to that minus the
    /// seed. The backward pass has no clamp for a negative degraded
    /// cursor: the loop condition ends the accumulation with an empty
    /// histogram, giving confidence 0.
    fn new(
        reference: &'a SignalBuffer,
        degraded: &'a SignalBuffer,
        boundary: usize,
        seed: i32,
        backward: bool,
        window: &[f32],
        rate: Rate,
    ) -> Self {
        let window_samples = rate.window_samples() as i64;
        let align_fft_len = rate.align_fft_len() as i64;
        let mut ref_cursor = if backward {
            boundary as i64 * window_samples - align_fft_len
        } else {
            boundary as i64 * window_samples
        };
        let mut deg_cursor = ref_cursor + i64::from(seed);
        if backward {
            if deg_cursor + align_fft_len > degraded.nominal_len as i64 {
                deg_cursor = degraded.nominal_len as i64 - align_fft_len;
                ref_cursor = deg_cursor - i64::from(seed);
            }
        } else if deg_cursor < 0 {
            ref_cursor = i64::from(-seed);
            deg_cursor = 0;
        }
        let align = rate.align_fft_len();
        Self {
            reference: &reference.samples,
            degraded: &degraded.samples,
            degraded_nominal: degraded.nominal_len,
            rate,
            ref_cursor,
            deg_cursor,
            backward,
            window: window.to_vec(),
            histogram: vec![0.0f32; align],
            hsum: 0.0,
            frame_ref: vec![0.0f32; align],
            frame_deg: vec![0.0f32; align],
        }
    }
}

impl SplitAccumulator<'_> {
    /// Advance the accumulation until the reference cursor passes the
    /// breakpoint sample `candidate_samples`. Forward frames accumulate
    /// while the cursors plus A stay within the degraded nominal length
    /// and at most the breakpoint; backward frames accumulate while the
    /// degraded cursor is non-negative and the reference cursor stays at
    /// least the breakpoint. Both cursors step by A/4 per frame, and
    /// each frame spreads its peaks per 1.13 step 6.
    fn advance_to(&mut self, candidate_samples: i64) {
        loop {
            let align = self.rate.align_fft_len() as i64;
            let in_range = if self.backward {
                self.deg_cursor >= 0 && self.ref_cursor >= candidate_samples
            } else {
                self.deg_cursor + align <= self.degraded_nominal as i64
                    && self.ref_cursor + align <= candidate_samples
            };
            if !in_range {
                return;
            }
            let align = self.rate.align_fft_len();
            let ref_start = self.ref_cursor as usize;
            let deg_start = self.deg_cursor as usize;
            for i in 0..align {
                self.frame_ref[i] = self.window[i] * self.reference[ref_start + i];
                self.frame_deg[i] = self.window[i] * self.degraded[deg_start + i];
            }
            let spectrum = spectral_cross_correlate(&self.frame_ref, &self.frame_deg);
            // The correlation values are absolute (1.10 step 4c); v is
            // 0.99 times the maximum of the absolute values and the
            // spread covers every lag whose absolute value exceeds v.
            // The Hsum increment applies once per exceeding lag, so the
            // recorded confidence peak/Hsum stays at most 1.
            let peak = spectrum
                .iter()
                .map(|value| value.abs())
                .fold(0.0f32, f32::max);
            // The spreading radius K = A/64 (spec 01, 1.13 step 6).
            let radius = (self.rate.align_fft_len() / 64) as i32;
            let v = 0.99 * peak;
            let unit = v.powf(0.125) / radius as f32;
            for (lag, &value) in spectrum.iter().enumerate() {
                if value.abs() > v {
                    for k in -(radius - 1)..=radius - 1 {
                        let index =
                            (lag as i32 + k).rem_euclid(self.rate.align_fft_len() as i32) as usize;
                        self.histogram[index] += unit * (radius - k.unsigned_abs() as i32) as f32;
                    }
                    self.hsum += f64::from(v.powf(0.125));
                }
            }
            let step = self.rate.align_fft_len() as i64 / 4;
            if self.backward {
                self.ref_cursor -= step;
                self.deg_cursor -= step;
            } else {
                self.ref_cursor += step;
                self.deg_cursor += step;
            }
        }
    }

    /// Record the delay and confidence at the current position: the
    /// seed plus the folded peak of the raw histogram (the spread of
    /// step 6 provides the smoothing; first position wins ties by scan
    /// order), and the peak divided by the Hsum accumulated per
    /// exceeding lag (0 with a non-positive Hsum).
    fn record(&mut self, seed: i32) -> (i32, f32) {
        let mut peak_index = 0usize;
        let mut peak_value = 0.0f32;
        for (lag, &value) in self.histogram.iter().enumerate() {
            if value > peak_value {
                peak_value = value;
                peak_index = lag;
            }
        }
        let align = self.rate.align_fft_len();
        let folded = if peak_index >= align / 2 {
            peak_index as i32 - align as i32
        } else {
            peak_index as i32
        };
        let confidence = if self.hsum > 0.0 {
            f64::from(peak_value) / self.hsum
        } else {
            0.0
        } as f32;
        (seed + folded, confidence)
    }
}

/// Apply one step-4 correlation output to the effective split window
/// (spec 01, section 1.13.1).
///
/// The correlation output of nr + nd - 1 values sits at scratch offsets
/// 0..nr + nd - 2, and the window occupies offsets 3A + 6 onwards, so
/// every output value whose offset lands inside the window region
/// replaces the Hann coefficient there. Later correlations overwrite
/// again, so the writes must be applied in call order (candidate order,
/// left estimate then right estimate per candidate).
fn corrupt_window(window: &mut [f32], correlation: &[f32], rate: Rate) {
    let offset = scratch_window_offset(rate);
    if correlation.len() <= offset {
        return;
    }
    let reach = (correlation.len() - offset).min(window.len());
    for (slot, &value) in window[..reach].iter_mut().zip(&correlation[offset..]) {
        *slot = value;
    }
}
