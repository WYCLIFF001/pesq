//! Disturbance processing and aggregation (spec 04).
//!
//! [`frame_disturbances`] runs sections 4.1 to 4.7: the disturbance
//! densities with the deadzone (4.1), the two weighted frame norms (4.2
//! and 4.3), the frame skipping at negative delay jumps (4.4), the
//! bad-interval re-alignment (4.5), the power normalization and cap
//! (4.6), and the long-signal time weights (4.7). [`aggregate`] then
//! reduces the per-frame arrays to the two disturbance indicators of
//! section 4.8.

mod bad_intervals;
mod norms;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_realign;
mod time_weight;

use crate::psychoacoustic::run_frame_loop;
use crate::types::{Rate, SignalBuffer, Utterance};

use norms::{asymmetric_densities, deadzone_removed, lp_norm, power_normalized};

/// Per-frame symmetric and asymmetric disturbances of spec 04 sections
/// 4.2 and 4.3, indexed over `[frame_start, frame_stop]` of spec 03
/// section 3.1.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameDisturbances {
    /// Symmetric frame disturbance `D[frame]` (spec 04, 4.2).
    pub symmetric: Vec<f32>,
    /// Asymmetric frame disturbance `A[frame]` (spec 04, 4.3).
    pub asymmetric: Vec<f32>,
    /// Skip flags forced by negative delay jumps (spec 01, 1.14 and
    /// spec 04, 4.4). The flags themselves have no further effect on the
    /// score; only the forced zero values do.
    pub skipped: Vec<bool>,
    /// Time weight of each frame (spec 04, 4.7), used by the
    /// aggregation of 4.8.
    pub time_weights: Vec<f32>,
}

/// Aggregated disturbance indicators produced by spec 04 section 4.8.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisturbanceIndicators {
    /// Aggregated symmetric disturbance indicator D.
    pub symmetric: f64,
    /// Aggregated asymmetric disturbance indicator A.
    pub asymmetric: f64,
}

/// Bad-frame gate on the pre-normalization symmetric disturbance
/// (spec 04, 4.2 step 2).
pub const BAD_FRAME_GATE: f32 = 30.0;

/// Minimum length of a bad interval in frames (spec 04, 4.5.1 step 3).
pub const MIN_BAD_INTERVAL_FRAMES: usize = 5;

/// Maximum number of bad intervals processed (spec 04, 4.5.1 step 3).
pub const MAX_BAD_INTERVALS: usize = 1000;

/// Delay search range s = 4 * F in samples at 8 kHz (spec 04, 4.5.3
/// step 1); at 16 kHz it is 4 times the 16 kHz frame length.
pub const BAD_INTERVAL_SEARCH_SAMPLES: usize = 4 * crate::psychoacoustic::FRAME_LEN;

/// Delay search range s = 4 * F in samples of the given rate
/// (spec 04, 4.5.3 step 1).
pub fn bad_interval_search_samples(rate: Rate) -> usize {
    4 * rate.frame_len()
}

/// Minimum normalized correlation for an accepted interval delay
/// (spec 04, 4.5.3 step 6).
pub const MIN_CORRELATION: f32 = 0.5;

/// Hard cap on the power-normalized disturbances (spec 04, 4.6 step 3).
pub const DISTURBANCE_CAP: f32 = 45.0;

/// Frame count above which the long-signal time weighting applies
/// (spec 04, 4.7 step 1).
pub const LONG_SIGNAL_FRAME_COUNT: usize = 1000;

/// Syllable window length in frames (spec 04, 4.8 step 1).
pub const SYLLABLE_FRAMES: usize = 20;

/// Syllable window step in frames (spec 04, 4.8 step 1).
pub const SYLLABLE_STEP: usize = 10;

/// Intra-syllable exponent (spec 04, 4.8).
pub const SYLLABLE_EXPONENT: f64 = 6.0;

/// Time aggregation exponent (spec 04, 4.8).
pub const TIME_EXPONENT: f64 = 2.0;

/// Compute the per-frame disturbances of spec 04 sections 4.1 to 4.7.
///
/// Re-runs the perceptual model of spec 03 on the saved model copies and
/// then applies: the deadzone and the two weighted frame norms (4.1 to
/// 4.3, with the first-pass bad-frame gate of 4.2 step 2), the frame
/// skipping at negative delay jumps (4.4), the bad-interval detection
/// and re-alignment (4.5, only when the gate was set), the power
/// normalization and cap (4.6), and the time weights (4.7).
pub fn frame_disturbances(
    reference: &SignalBuffer,
    degraded: &SignalBuffer,
    utterances: &[Utterance],
) -> FrameDisturbances {
    let model = run_frame_loop(reference, degraded, utterances);
    let rate = model.rate;
    let bands = rate.num_bands();
    let frame_start = model.frame_range.start;
    let frame_stop = model.frame_range.stop;
    let mut symmetric = vec![0.0f32; frame_stop + 1];
    let mut asymmetric = vec![0.0f32; frame_stop + 1];

    // spec 04, 4.1 to 4.3: first pass over the processed frames. The
    // gate of 4.2 step 2 is evaluated here, before the frame skipping
    // of 4.4 zeroes any values.
    let mut d = vec![0.0f32; bands];
    let mut d_asym = vec![0.0f32; bands];
    let mut bad_frames_exist = false;
    for frame in frame_start..=frame_stop {
        let base = frame * bands;
        deadzone_removed(
            &model.loudness_ref[base..base + bands],
            &model.loudness_deg[base..base + bands],
            &mut d,
        );
        let d_sym = lp_norm(&d, 2.0, rate) as f32;
        if d_sym > BAD_FRAME_GATE {
            bad_frames_exist = true;
        }
        asymmetric_densities(
            &d,
            &model.pitch_ref[base..base + bands],
            &model.pitch_deg[base..base + bands],
            &mut d_asym,
        );
        symmetric[frame] = d_sym;
        asymmetric[frame] = lp_norm(&d_asym, 1.0, rate) as f32;
    }

    // spec 04, 4.4: frame skipping at negative delay jumps.
    let skipped = crate::utterances::negative_delay_skip_flags(utterances, frame_stop, rate);
    for (frame, &skip) in skipped.iter().enumerate() {
        if skip {
            symmetric[frame] = 0.0;
            asymmetric[frame] = 0.0;
        }
    }

    // spec 04, 4.5: bad intervals and re-alignment, only when the gate
    // of 4.2 step 2 was set.
    if bad_frames_exist {
        bad_intervals::realign(
            reference,
            degraded,
            utterances,
            &model,
            &mut symmetric,
            &mut asymmetric,
        );
    }

    // spec 04, 4.6: power normalization and the cap at 45.
    for frame in frame_start..=frame_stop {
        symmetric[frame] = power_normalized(symmetric[frame], model.audible_ref[frame]);
        asymmetric[frame] = power_normalized(asymmetric[frame], model.audible_ref[frame]);
    }

    FrameDisturbances {
        symmetric,
        asymmetric,
        skipped,
        // The time weights take the common nominal length Nmax
        // (spec 04, 4.7 step 1).
        time_weights: time_weight::time_weights(
            frame_stop,
            reference.nominal_len.max(degraded.nominal_len),
            rate,
        ),
    }
}

/// Aggregate per-frame disturbances over syllables and time into the two
/// indicators of spec 04 section 4.8.
///
/// For each indicator a window of [`SYLLABLE_FRAMES`] frames sweeps from
/// `frame_start` in steps of [`SYLLABLE_STEP`]; each syllable value is
/// the L6 norm over the window with the denominator always 20 and frames
/// beyond `frame_stop` contributing zero. The squared time-weighted
/// syllables accumulate into the indicator `(S / T)^(1/2)` with the time
/// weight of 4.7 taken at the window start, indexed relative to
/// `frame_start` (spec 04, 4.8 step 1b). With an empty frame range both
/// indicators are 0.
pub fn aggregate(frames: &FrameDisturbances, frame_start: usize) -> DisturbanceIndicators {
    DisturbanceIndicators {
        symmetric: aggregate_indicator(&frames.symmetric, &frames.time_weights, frame_start),
        asymmetric: aggregate_indicator(&frames.asymmetric, &frames.time_weights, frame_start),
    }
}

/// Sweep one per-frame indicator array into the aggregated indicator of
/// spec 04 section 4.8.
fn aggregate_indicator(values: &[f32], weights: &[f32], frame_start: usize) -> f64 {
    let frame_stop = values.len() - 1;
    let mut sum = 0.0f64;
    let mut weight_sum = 0.0f64;
    let mut window_start = frame_start;
    while window_start <= frame_stop {
        let end = (window_start + SYLLABLE_FRAMES).min(frame_stop + 1);
        let power_sum: f64 = values[window_start..end]
            .iter()
            .map(|&value| f64::from(value).powi(6))
            .sum();
        let syllable = (power_sum / SYLLABLE_FRAMES as f64).powf(1.0 / SYLLABLE_EXPONENT);
        // The weight index is the window start relative to frame_start,
        // not the absolute frame index (spec 04, 4.8 step 1b).
        let weight = f64::from(weights[window_start - frame_start]);
        sum += (weight * syllable).powi(2);
        weight_sum += weight * weight;
        window_start += SYLLABLE_STEP;
    }
    if weight_sum == 0.0 {
        return 0.0;
    }
    (sum / weight_sum).powf(1.0 / TIME_EXPONENT)
}
