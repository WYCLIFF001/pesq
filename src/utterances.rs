//! Utterance boundaries, splitting, and negative-delay frame skipping
//! (spec 01, sections 1.11 to 1.14).
//!
//! The qualifying runs of 1.11 become utterances, aligned per search
//! window with the coarse (1.9 step 7) and fine (1.10) procedures, then
//! merged and clamped (1.12). Long utterances are probed for a split on
//! a grid of candidate breakpoints (1.13, see [`crate::splitting`]), and
//! the resulting delay jumps feed the frame skipping of 1.14.

use crate::alignment::{coarse_delay_search, fine_delay, qualifying_runs, search_windows};
use crate::splitting::best_split;
use crate::types::{MARGIN_SAMPLES, PesqError, SignalBuffer, Utterance, VadData, WINDOW_SAMPLES};

/// Cap on the utterance count during splitting (spec 01, 1.13 preamble).
const MAX_UTTERANCES: usize = 50;

/// An utterance in the working list: boundaries, the search window of
/// 1.11, and the alignment estimates.
#[derive(Debug, Clone)]
pub(crate) struct UtteranceWork {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) search_start: usize,
    pub(crate) search_end: usize,
    pub(crate) coarse: i32,
    pub(crate) fine: i32,
    pub(crate) confidence: f32,
}

/// Per-utterance alignment of spec 01 sections 1.9 step 7, 1.10, 1.11,
/// 1.12, and 1.13.
///
/// The reference and degraded working buffers (after 1.5 and 1.6) and
/// the two VAD outputs are consumed; the returned utterances carry the
/// fine delays the perceptual model applies to the saved model copies.
/// Returns [`PesqError::NoUtterancesFound`] when no run of 1.11
/// qualifies (1.11 step 5).
pub fn align_utterances(
    reference: &SignalBuffer,
    degraded: &SignalBuffer,
    ref_vad: &VadData,
    deg_vad: &VadData,
) -> Result<Vec<Utterance>, PesqError> {
    let coarse = crate::alignment::coarse_delay_whole(ref_vad, deg_vad);
    let runs = qualifying_runs(ref_vad, coarse, degraded.nominal_len);
    if runs.is_empty() {
        return Err(PesqError::NoUtterancesFound);
    }
    let windows = search_windows(ref_vad, coarse, degraded.nominal_len);

    let mut utterances: Vec<UtteranceWork> = runs
        .iter()
        .zip(windows.iter())
        .map(|(&(start, end), &(search_start, search_end))| {
            let per_utterance =
                coarse_delay_search(ref_vad, deg_vad, search_start, search_end, coarse);
            let (fine, confidence) = fine_delay(
                &reference.samples,
                &degraded.samples,
                search_start,
                search_end,
                per_utterance,
                degraded.nominal_len,
            );
            UtteranceWork {
                start,
                end,
                search_start,
                search_end,
                coarse: per_utterance,
                fine,
                confidence,
            }
        })
        .collect();

    merge_boundaries(&mut utterances, ref_vad.window_count);
    clamp_boundaries(&mut utterances, degraded.nominal_len);
    split_utterances(&mut utterances, reference, degraded, ref_vad, deg_vad);

    Ok(utterances
        .iter()
        .map(|work| Utterance {
            start_window: work.start,
            end_window: work.end,
            coarse_delay: work.coarse,
            fine_delay: work.fine,
            confidence: work.confidence,
            split_frame: None,
        })
        .collect())
}

/// Boundary merging of spec 01 section 1.12 step 2: the outer
/// boundaries move to 75 and V - 75, and adjacent utterances share the
/// integer midpoint.
fn merge_boundaries(utterances: &mut [UtteranceWork], window_count: usize) {
    if utterances.is_empty() {
        return;
    }
    let last = utterances.len() - 1;
    utterances[0].start = 75;
    utterances[last].end = window_count - 75;
    for u in 1..utterances.len() {
        let midpoint = (utterances[u].start + utterances[u - 1].end) / 2;
        utterances[u - 1].end = midpoint;
        utterances[u].start = midpoint;
    }
}

/// Delay clamps and overlap fix of spec 01 section 1.12 steps 3 to 5.
fn clamp_boundaries(utterances: &mut [UtteranceWork], degraded_nominal: usize) {
    let Some(first) = utterances.first_mut() else {
        return;
    };
    // Step 3: left edge. The tested operand is start[0]*W, not
    // (start[0] - 75)*W (spec 01, 1.12 step 3): step 2 has just set
    // start[0] = 75, so the clamp triggers only when delay[0] < 0.
    if first.start as i64 * WINDOW_SAMPLES as i64 + i64::from(first.fine) < MARGIN_SAMPLES as i64 {
        first.start =
            (75 + (WINDOW_SAMPLES as i32 - 1 - first.fine) / WINDOW_SAMPLES as i32) as usize;
    }
    // Step 4: right edge.
    if let Some(last) = utterances.last_mut() {
        if last.end as i64 * WINDOW_SAMPLES as i64 + i64::from(last.fine)
            > degraded_nominal as i64 - MARGIN_SAMPLES as i64
        {
            let end = (degraded_nominal as i64 - i64::from(last.fine)) / WINDOW_SAMPLES as i64
                - MARGIN_SAMPLES as i64 / WINDOW_SAMPLES as i64;
            last.end = end.max(0) as usize;
        }
    }
    // Step 5: overlap fix for adjacent pairs.
    for u in 1..utterances.len() {
        let a = utterances[u].start as i64 * WINDOW_SAMPLES as i64 + i64::from(utterances[u].fine);
        let b = utterances[u - 1].end as i64 * WINDOW_SAMPLES as i64
            + i64::from(utterances[u - 1].fine);
        if a < b {
            let c = (a + b) / 2;
            utterances[u].start = ((WINDOW_SAMPLES as i64 - 1 + c - i64::from(utterances[u].fine))
                / WINDOW_SAMPLES as i64)
                .max(0) as usize;
            utterances[u - 1].end =
                ((c - i64::from(utterances[u - 1].fine)) / WINDOW_SAMPLES as i64).max(0) as usize;
        }
    }
}

/// Utterance splitting of spec 01 section 1.13: scan the utterance list
/// in order, attempt one split per utterance with at least 200 windows
/// of speech, and stop once the count reaches [`MAX_UTTERANCES`]. After
/// a split the scan continues at the same index, which now holds the
/// left half (1.13 step 11), so the left half's own possible second
/// split is never skipped.
fn split_utterances(
    utterances: &mut Vec<UtteranceWork>,
    reference: &SignalBuffer,
    degraded: &SignalBuffer,
    ref_vad: &VadData,
    deg_vad: &VadData,
) {
    let mut u = 0;
    while u < utterances.len() && utterances.len() < MAX_UTTERANCES {
        let work = &utterances[u];
        let Some(split) = best_split(work, reference, degraded, ref_vad, deg_vad) else {
            u += 1;
            continue;
        };
        // Step 9: the halves inherit the search window; the boundaries
        // depend on which delay is smaller, with integer division
        // truncating toward zero.
        let (left_end, right_start) = if split.backward_delay < split.forward_delay {
            (split.breakpoint, split.breakpoint)
        } else {
            let half = (split.backward_delay - split.forward_delay) / (2 * WINDOW_SAMPLES as i32);
            (
                split.breakpoint + half as usize,
                split.breakpoint.saturating_sub(half as usize),
            )
        };
        let mut left = UtteranceWork {
            start: work.start,
            end: left_end,
            search_start: work.search_start,
            search_end: work.search_end,
            coarse: split.left_estimate,
            fine: split.forward_delay,
            confidence: split.forward_confidence,
        };
        let mut right = UtteranceWork {
            start: right_start,
            end: work.end,
            search_start: work.search_start,
            search_end: work.search_end,
            coarse: split.right_estimate,
            fine: split.backward_delay,
            confidence: split.backward_confidence,
        };
        // Step 10: clamps after splitting.
        if (left.start as i64 - 75) * WINDOW_SAMPLES as i64 + i64::from(left.fine) < 0 {
            left.start =
                (75 + (WINDOW_SAMPLES as i32 - 1 - left.fine) / WINDOW_SAMPLES as i32) as usize;
        }
        if right.end as i64 * WINDOW_SAMPLES as i64 + i64::from(right.fine)
            > degraded.nominal_len as i64 - MARGIN_SAMPLES as i64
        {
            let end =
                (degraded.nominal_len as i64 - i64::from(right.fine)) / WINDOW_SAMPLES as i64 - 75;
            right.end = end.max(0) as usize;
        }
        utterances[u] = left;
        utterances.insert(u + 1, right);
        // Step 11: the utterances shifted right by the split take their
        // own current boundaries as their search windows (no 75-window
        // widening); the two halves keep the search window inherited in
        // step 9. The scan then continues at the same index, which now
        // holds the left half.
        for work in utterances.iter_mut().skip(u + 2) {
            work.search_start = work.start;
            work.search_end = work.end;
        }
    }
}

/// Frame skipping at negative delay jumps of spec 01 section 1.14.
///
/// Returns one flag per frame index 0..=frame_stop. For every adjacent
/// utterance pair whose delay jumps by less than -128 samples, frames
/// strictly below frame_stop in the range [f1, min(f2, frame_stop - 1)]
/// are marked. Both f1 and j divide with integer division truncating
/// toward zero (1.14 step 1); the reference implementation's floor()
/// around the integer quotient is a no-op. Frame_stop itself is never
/// skipped (1.14 step 3).
pub fn negative_delay_skip_flags(utterances: &[Utterance], frame_stop: usize) -> Vec<bool> {
    let mut flags = vec![false; frame_stop + 1];
    for pair in utterances.windows(2) {
        let j0 = i64::from(pair[0].fine_delay);
        let j1 = i64::from(pair[1].fine_delay);
        if j1 - j0 < -128 {
            let mut f1 = ((pair[1].start_window as i64 - 75) * WINDOW_SAMPLES as i64 + j1) / 128;
            let j = ((pair[0].end_window as i64 - 75) * WINDOW_SAMPLES as i64 + j0) / 128;
            if f1 > j {
                f1 = j;
            }
            if f1 < 0 {
                f1 = 0;
            }
            let f2 = ((pair[1].start_window as i64 - 75) * WINDOW_SAMPLES as i64
                + (j1 - j0).abs().max(0))
                / 128
                + 1;
            let last = f2.min(frame_stop as i64 - 1);
            for frame in f1..=last {
                flags[frame as usize] = true;
            }
        }
    }
    flags
}

#[cfg(test)]
mod tests;
