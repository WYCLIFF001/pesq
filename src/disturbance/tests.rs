//! Unit tests for spec 04: deadzone, norms, asymmetry factor, bad
//! intervals, correlation, power normalization, time weights, and
//! aggregation.

use super::bad_intervals::{collect_intervals, correlation_heights, dilate};
use super::norms::{asymmetric_densities, deadzone_removed, lp_norm, power_normalized};
use super::time_weight::time_weights;
use super::*;
use crate::psychoacoustic::{NUM_BANDS, bark_width};

/// Sum of the Bark widths over bands 1..=41, the norm's weight sum.
fn band_weight_sum() -> f64 {
    (1..NUM_BANDS).map(|band| f64::from(bark_width(band))).sum()
}

#[test]
fn deadzone_removes_disturbances_within_the_margin() {
    let mut reference = [0.0f32; NUM_BANDS];
    let mut degraded = [0.0f32; NUM_BANDS];
    reference[1] = 4.0; // margin 0.25 * min(4, 1) = 0.25: d = -3, result -2.75
    degraded[1] = 1.0;
    reference[2] = 1.0; // margin 0.25: d = 1.2, result 0.95
    degraded[2] = 2.2;
    reference[3] = 1.0; // margin 0.25: d = 0.2 inside, result 0
    degraded[3] = 1.2;
    reference[4] = 2.0; // margin 0.25 * min(2, 1.5) = 0.375: d = -0.5, result -0.125
    degraded[4] = 1.5;
    let mut d = [0.0f32; NUM_BANDS];
    deadzone_removed(&reference, &degraded, &mut d);
    assert!((d[1] - -2.75).abs() < 1e-6, "band 1: {}", d[1]);
    assert!((d[2] - 0.95).abs() < 1e-6, "band 2: {}", d[2]);
    assert_eq!(d[3], 0.0, "band 3");
    assert!((d[4] - -0.125).abs() < 1e-6, "band 4: {}", d[4]);
    for (band, &value) in d[5..].iter().enumerate() {
        assert_eq!(value, 0.0, "band {}", band + 5);
    }
}

#[test]
fn lp_norm_matches_the_spec_formula_on_known_arrays() {
    let weight_sum = band_weight_sum();
    // Constant disturbance c: the spec formula reduces to
    // c * (sum w^p / sum w)^(1/p) * sum w.
    let mut d = [0.0f32; NUM_BANDS];
    d[1..].fill(3.0);
    let squared: f64 = (1..NUM_BANDS)
        .map(|band| f64::from(bark_width(band)).powi(2))
        .sum();
    let expected_2 = 3.0 * (squared / weight_sum).sqrt() * weight_sum;
    assert!((lp_norm(&d, 2.0) - expected_2).abs() < 1e-4);
    assert!((lp_norm(&d, 1.0) - 3.0 * weight_sum).abs() < 1e-4);
    // A single band, with W the width sum over all bands 1..41:
    // L2 = |d| * w * sqrt(W), L1 = |d| * w.
    d = [0.0f32; NUM_BANDS];
    d[7] = 5.0;
    let w = f64::from(bark_width(7));
    assert!((lp_norm(&d, 2.0) - 5.0 * w * weight_sum.sqrt()).abs() < 1e-4);
    assert!((lp_norm(&d, 1.0) - 5.0 * w).abs() < 1e-5);
    // Mixed signs use absolute values.
    d[7] = -5.0;
    assert!((lp_norm(&d, 2.0) - 5.0 * w * weight_sum.sqrt()).abs() < 1e-4);
}

#[test]
fn asymmetry_factor_caps_at_12_and_floors_at_3() {
    let pitch_ref = [0.0f32; NUM_BANDS];
    let mut pitch_deg = [0.0f32; NUM_BANDS];
    let mut d = [0.0f32; NUM_BANDS];
    let mut out = [0.0f32; NUM_BANDS];
    // r = 1: h = 1 < 3, floored to 0.
    pitch_deg[1] = 0.0;
    // r = 8: h = 8^1.2 = 12.13 capped at 12. With p_ref = 0 the
    // degraded density for ratio r is r * 50 - 50.
    pitch_deg[2] = 8.0 * 50.0 - 50.0;
    // r = 5: h = 5^1.2 = 6.898, kept.
    pitch_deg[3] = 5.0 * 50.0 - 50.0;
    d[1] = 1.0;
    d[2] = 1.0;
    d[3] = 1.0;
    asymmetric_densities(&d, &pitch_ref, &pitch_deg, &mut out);
    assert_eq!(out[1], 0.0);
    assert!((out[2] - 12.0).abs() < 1e-3, "band 2: {}", out[2]);
    assert!(
        (out[3] - 5.0f32.powf(1.2)).abs() < 1e-3,
        "band 3: {}",
        out[3]
    );
    // The factor multiplies the deadzone-removed density, including
    // negative (removed content) values.
    d[2] = -2.0;
    asymmetric_densities(&d, &pitch_ref, &pitch_deg, &mut out);
    assert!((out[2] - -24.0).abs() < 1e-3, "band 2: {}", out[2]);
}

#[test]
fn dilation_merges_runs_split_by_one_frame_and_keeps_runs_intact() {
    let mut mask = vec![false; 40];
    mask[10..=12].iter_mut().for_each(|m| *m = true);
    mask[14..=16].iter_mut().for_each(|m| *m = true);
    let dilated = dilate(&mask);
    let expected: Vec<bool> = (0..40)
        .map(|f| (10..=12).contains(&f) || (14..=16).contains(&f) || f == 13)
        .collect();
    assert_eq!(dilated, expected);
    // A single bad frame stays a single dilated bad frame.
    let mut single = vec![false; 40];
    single[20] = true;
    let dilated = dilate(&single);
    let expected: Vec<bool> = (0..40).map(|f| f == 20).collect();
    assert_eq!(dilated, expected);
}

#[test]
fn intervals_need_five_frames_and_respect_the_cap() {
    // A run of 4 frames does not become an interval.
    let mut mask = vec![false; 30];
    mask[5..9].iter_mut().for_each(|m| *m = true);
    assert!(collect_intervals(&dilate(&mask)).is_empty());
    // A run of 5 frames becomes the interval (a, b) with b - a = 5.
    mask = vec![false; 30];
    mask[5..10].iter_mut().for_each(|m| *m = true);
    assert_eq!(collect_intervals(&dilate(&mask)), vec![(5, 10)]);
    // More than 1000 intervals are truncated at 1000. Runs of 5 bad
    // frames separated by 3-frame gaps survive the dilation untouched;
    // the runs start at frame 10 because frames 0 and 1 lie outside the
    // dilation range and are always not bad.
    let mut mask = vec![false; 8020];
    for i in 0..1001 {
        mask[10 + i * 8..15 + i * 8]
            .iter_mut()
            .for_each(|m| *m = true);
    }
    let intervals = collect_intervals(&dilate(&mask));
    assert_eq!(intervals.len(), MAX_BAD_INTERVALS);
    assert_eq!(intervals[0], (10, 15));
    assert_eq!(
        intervals[MAX_BAD_INTERVALS - 1],
        (10 + 999 * 8, 15 + 999 * 8)
    );
}

#[test]
fn circular_correlation_peaks_at_the_shift_of_an_impulse() {
    let r = 1024;
    let m = 500;
    // Degraded segment delayed by +37 samples: the height at lag 37 is
    // exactly 1 and every other lag is near zero.
    let mut x = vec![0.0f32; r];
    let mut y = vec![0.0f32; r];
    x[100] = 2.0;
    y[137] = 3.0;
    let heights = correlation_heights(&x, &y, m);
    assert!((heights[37] - 1.0).abs() < 1e-3, "lag 37: {}", heights[37]);
    for (lag, &h) in heights.iter().enumerate() {
        if lag != 37 {
            assert!(h < 1e-3, "lag {lag}: {h}");
        }
    }
    // A degraded segment advanced by 37 samples peaks at the wrapped
    // position r - 37, the position of tau = -37.
    y = vec![0.0f32; r];
    y[63] = 3.0;
    let heights = correlation_heights(&x, &y, m);
    assert!(
        (heights[r - 37] - 1.0).abs() < 1e-3,
        "lag -37: {}",
        heights[r - 37]
    );
}

#[test]
fn correlation_heights_are_zero_when_a_segment_power_is_tiny() {
    let r = 1024;
    let mut x = vec![0.0f32; r];
    x[100] = 0.0001;
    let mut y = vec![0.0f32; r];
    y[100] = 1.0;
    assert!(correlation_heights(&x, &y, 500).iter().all(|&h| h == 0.0));
}

#[test]
fn power_normalization_divides_and_caps_at_45() {
    // With no audible power h = (1e5/1e7)^0.04 = 0.01^0.04.
    let h = 0.01f64.powf(0.04);
    let normalized = power_normalized(1.0, 0.0);
    assert!((f64::from(normalized) - 1.0 / h).abs() < 1e-4);
    // A disturbance of 100 normalizes above 45 and caps there.
    assert_eq!(power_normalized(100.0, 0.0), 45.0);
    // At 1e7 audible power h = 1.01^0.04 > 1, so the value shrinks.
    let expected = 10.0 / 1.01f64.powf(0.04);
    assert!((f64::from(power_normalized(10.0, 1e7)) - expected).abs() < 1e-4);
}

#[test]
fn time_weights_are_one_below_the_long_signal_count() {
    assert_eq!(time_weights(999, 100_000), vec![1.0f32; 1000]);
}

#[test]
fn time_weights_grow_linearly_for_long_signals() {
    // frame_stop + 1 > 1000, n = (Nmax - 4800)/128 - 1 = 1101,
    // f = 101/5500.
    let n_max = 4800 + 128 * 1102;
    let weights = time_weights(1500, n_max);
    assert_eq!(weights.len(), 1501);
    let f = 101.0 / 5500.0;
    assert!((f64::from(weights[0]) - (1.0 - f)).abs() < 1e-6);
    assert!((f64::from(weights[1101]) - 1.0).abs() < 1e-6);
    // Monotone growth.
    assert!(weights.windows(2).all(|pair| pair[0] <= pair[1]));
    // A huge n drives f to the 0.5 cap.
    let n_max = 4800 + 128 * 100_001;
    let capped = time_weights(1500, n_max);
    assert!((f64::from(capped[0]) - 0.5).abs() < 1e-6);
}

#[test]
fn aggregate_returns_the_constant_on_a_flat_disturbance() {
    let frame_stop = 19;
    let values = vec![3.0f32; frame_stop + 1];
    let frames = FrameDisturbances {
        symmetric: values.clone(),
        asymmetric: values.clone(),
        skipped: vec![false; frame_stop + 1],
        time_weights: vec![1.0f32; frame_stop + 1],
    };
    // Windows at s = 0 (20 full frames, syllable 3) and s = 10
    // (frames 10..19, syllable 3 * 0.5^(1/6) with zero padding).
    let expected = 3.0 * ((1.0 + 0.5f64.powf(1.0 / 3.0)) / 2.0).sqrt();
    let indicators = aggregate(&frames, 0);
    assert!((indicators.symmetric - expected).abs() < 1e-4);
    assert!((indicators.asymmetric - expected).abs() < 1e-4);
}

#[test]
fn aggregate_zero_pads_the_last_syllable_to_20_frames() {
    let frame_stop = 24;
    let values = vec![2.0f32; frame_stop + 1];
    let frames = FrameDisturbances {
        symmetric: values,
        asymmetric: vec![0.0f32; frame_stop + 1],
        skipped: vec![false; frame_stop + 1],
        time_weights: vec![1.0f32; frame_stop + 1],
    };
    // Windows at s = 0 (20 frames), s = 10 (15 frames), and s = 20
    // (5 frames): syllable = 2 * (k/20)^(1/6) for k counted frames.
    let expected =
        ((4.0 + 4.0 * 0.75f64.powf(1.0 / 3.0) + 4.0 * 0.25f64.powf(1.0 / 3.0)) / 3.0).sqrt();
    assert!((aggregate(&frames, 0).symmetric - expected).abs() < 1e-4);
}

#[test]
fn aggregate_ignores_frames_before_frame_start_and_scales_with_weights() {
    let frame_stop = 19;
    let values = vec![4.0f32; frame_stop + 1];
    let frames = FrameDisturbances {
        symmetric: values,
        asymmetric: vec![0.0f32; frame_stop + 1],
        skipped: vec![false; frame_stop + 1],
        time_weights: vec![2.0f32; frame_stop + 1],
    };
    // The time weights cancel out of the S/T ratio.
    let expected = 4.0 * ((1.0 + 0.5f64.powf(1.0 / 3.0)) / 2.0).sqrt();
    assert!((aggregate(&frames, 0).symmetric - expected).abs() < 1e-4);
    // An empty range aggregates to zero.
    assert_eq!(aggregate(&frames, frame_stop + 5).symmetric, 0.0);
}

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
