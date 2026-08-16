//! Disturbance processing and aggregation (spec 04).
//!
//! This Round 2 scaffold holds the constants, the per-frame disturbance
//! arrays, and the aggregated indicator pair of spec 04. The per-frame
//! loop and the bad-interval re-alignment are stubbed.

use crate::types::{SignalBuffer, Utterance};

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

/// Delay search range s = 4 * F in samples (spec 04, 4.5.3 step 1).
pub const BAD_INTERVAL_SEARCH_SAMPLES: usize = 1024;

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

/// Compute the per-frame disturbances of spec 04 sections 4.1 to 4.7
/// (stub, Round 2): disturbance densities and deadzone (4.1), the two
/// frame norms (4.2 and 4.3), bad intervals and re-alignment (4.5), and
/// power normalization (4.6).
pub fn frame_disturbances(
    _reference: &SignalBuffer,
    _degraded: &SignalBuffer,
    _utterances: &[Utterance],
) -> FrameDisturbances {
    todo!("spec 04, 4.1 to 4.7: per-frame disturbance computation")
}

/// Aggregate per-frame disturbances over syllables and time into the two
/// indicators of spec 04 section 4.8 (stub, Round 2).
pub fn aggregate(_frames: &FrameDisturbances, _frame_start: usize) -> DisturbanceIndicators {
    todo!("spec 04, 4.8: disturbance aggregation")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indicators_are_plain_pairs() {
        let indicators = DisturbanceIndicators {
            symmetric: 1.0,
            asymmetric: 2.0,
        };
        assert_eq!(indicators.symmetric, 1.0);
        assert_eq!(indicators.asymmetric, 2.0);
    }

    #[test]
    fn constants_match_spec_04() {
        assert_eq!(BAD_FRAME_GATE, 30.0);
        assert_eq!(MIN_BAD_INTERVAL_FRAMES, 5);
        assert_eq!(MAX_BAD_INTERVALS, 1000);
        assert_eq!(BAD_INTERVAL_SEARCH_SAMPLES, 1024);
        assert_eq!(MIN_CORRELATION, 0.5);
        assert_eq!(DISTURBANCE_CAP, 45.0);
        assert_eq!(LONG_SIGNAL_FRAME_COUNT, 1000);
        assert_eq!(SYLLABLE_FRAMES, 20);
        assert_eq!(SYLLABLE_STEP, 10);
        assert_eq!(SYLLABLE_EXPONENT, 6.0);
        assert_eq!(TIME_EXPONENT, 2.0);
    }
}
