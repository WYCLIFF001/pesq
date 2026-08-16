//! Unit tests of the perceptual model part 1 (spec 03).

use super::table::{BARK_BANDS, LOUDNESS_SCALE, PITCH_POWER_SCALE};
use super::*;
use crate::types::Utterance;

fn utterance(start_window: usize, fine_delay: i32) -> Utterance {
    Utterance {
        start_window,
        end_window: 0,
        coarse_delay: fine_delay,
        fine_delay,
        confidence: 1.0,
        split_frame: None,
    }
}

/// A 440 Hz tone at the given amplitude, 8000 samples at 8 kHz.
fn tone_pcm(amplitude: f32) -> Vec<i16> {
    (0..8000)
        .map(|n| (amplitude * (std::f32::consts::TAU * 440.0 * n as f32 / 8000.0).sin()) as i16)
        .collect()
}

#[test]
fn bark_table_covers_all_128_bins_and_42_bands() {
    assert_eq!(BARK_BANDS.len(), NUM_BANDS);
    let total: usize = BARK_BANDS.iter().map(|row| row.bins).sum();
    assert_eq!(total, 128, "group counts must sum to 128 (spec 03, 3.3)");
    assert!(
        BARK_BANDS
            .windows(2)
            .all(|w| w[0].bark_centre < w[1].bark_centre)
    );
    assert!(BARK_BANDS.iter().all(|row| row.threshold > 0.0));
}

#[test]
fn bark_warping_of_a_flat_spectrum_scales_by_group_and_factor() {
    // A flat spectrum warps to density[b] = power * n[b] * c[b] * Sp
    // (spec 03, 3.3 steps 1 to 3).
    let mut power = [0.0f64; NUM_POWER_BINS];
    for bin in power.iter_mut().skip(1) {
        *bin = 10_000.0;
    }
    let density = warp_to_bark(&power);
    for (band, row) in BARK_BANDS.iter().enumerate() {
        let expected = 10_000.0 * row.bins as f64 * f64::from(row.correction) * PITCH_POWER_SCALE;
        assert!(
            (f64::from(density[band]) - expected).abs() <= expected * 1e-6,
            "band {band}"
        );
    }
}

#[test]
fn bark_warping_ignores_the_forced_zero_dc_bin() {
    let mut power = [0.0f64; NUM_POWER_BINS];
    power[0] = 1e30;
    for bin in power.iter_mut().skip(1) {
        *bin = 1.0;
    }
    let density = warp_to_bark(&power);
    // Band 0 groups only bin 1, so a huge DC bin must not leak in.
    let expected = 1.0 * f64::from(BARK_BANDS[0].correction) * PITCH_POWER_SCALE;
    assert!((f64::from(density[0]) - expected).abs() < 1e-9);
}

#[test]
fn loudness_follows_the_zwicker_power_law() {
    // Band 12 sits at 4.09 Bark, so the low-band correction is off:
    // h = 1 and z = 0.23 (spec 03, 3.6 steps 1 and 2).
    let band = 12;
    assert!(BARK_BANDS[band].bark_centre >= 4.0);
    let threshold = f64::from(BARK_BANDS[band].threshold);
    for p in [3.0f32, 10.0, 1e3, 1e6] {
        assert!(f64::from(p) > threshold, "p = {p} must stay above t[b]");
        let expected = LOUDNESS_SCALE
            * (threshold / 0.5).powf(0.23)
            * ((0.5 + 0.5 * f64::from(p) / threshold).powf(0.23) - 1.0);
        let loudness = f64::from(zwicker_loudness(p, band));
        assert!(
            (loudness - expected).abs() <= expected.abs() * 1e-6,
            "p = {p}"
        );
    }
    // At or below the threshold the loudness is zero (step 3).
    assert_eq!(zwicker_loudness(BARK_BANDS[band].threshold, band), 0.0);
    assert_eq!(
        zwicker_loudness(0.5 * BARK_BANDS[band].threshold, band),
        0.0
    );
    // And it grows monotonically with the input density.
    assert!(zwicker_loudness(2.0, band) > zwicker_loudness(1.0, band));
}

#[test]
fn loudness_applies_the_low_bark_correction_below_4_bark() {
    // Band 4 (1.29 Bark): h = 6 / (bark + 2), the cap at 2 does not
    // bind (spec 03, 3.6 step 1).
    let band = 4;
    let bark = f64::from(BARK_BANDS[band].bark_centre);
    assert!(bark < 4.0);
    let threshold = f64::from(BARK_BANDS[band].threshold);
    let p = 1e10f32;
    let h = (6.0 / (bark + 2.0)).powf(0.15);
    let expected = LOUDNESS_SCALE
        * (threshold / 0.5).powf(0.23 * h)
        * ((0.5 + 0.5 * f64::from(p) / threshold).powf(0.23 * h) - 1.0);
    assert!((f64::from(zwicker_loudness(p, band)) - expected).abs() <= expected * 1e-6);
    // Band 0 (0.079 Bark): 6 / (bark + 2) exceeds 2, so the cap binds
    // and h = 2^0.15.
    let band = 0;
    let threshold = f64::from(BARK_BANDS[band].threshold);
    let h = 2.0f64.powf(0.15);
    let expected = LOUDNESS_SCALE
        * (threshold / 0.5).powf(0.23 * h)
        * ((0.5 + 0.5 * f64::from(p) / threshold).powf(0.23 * h) - 1.0);
    assert!((f64::from(zwicker_loudness(p, band)) - expected).abs() <= expected * 1e-6);
}

#[test]
fn audible_power_applies_the_threshold_and_excludes_band_0() {
    let mut density = [0.0f32; NUM_BANDS];
    density[0] = 1e30; // band 0 never counts (spec 03, 3.4 step 1)
    let factor = 100.0;
    let mut expected = 0.0f64;
    for band in 1..NUM_BANDS {
        density[band] = if band % 2 == 0 {
            (factor * f64::from(BARK_BANDS[band].threshold) * 0.5) as f32 // below: dropped
        } else {
            (factor * f64::from(BARK_BANDS[band].threshold) * 2.0) as f32 // above: kept
        };
        if band % 2 == 1 {
            expected += f64::from(density[band]);
        }
    }
    assert!((audible_power(&density, factor) - expected).abs() <= expected * 1e-6);
}

#[test]
fn silence_flag_uses_factor_100_and_power_1e7() {
    // One band at exactly 100 * t is not audible with factor 100, so
    // the frame is silent; at 100 * t + 1e7 the audible power passes
    // 1e7 and the frame is not silent (spec 03, 3.4 step 2).
    let band = 20;
    let mut density = [0.0f32; NUM_BANDS];
    density[band] = (100.0 * f64::from(BARK_BANDS[band].threshold)) as f32;
    assert!(audible_power(&density, SILENCE_FLAG_FACTOR) < SILENCE_FLAG_POWER);
    density[band] = (100.0 * f64::from(BARK_BANDS[band].threshold) + SILENCE_FLAG_POWER) as f32;
    assert!(audible_power(&density, SILENCE_FLAG_FACTOR) > SILENCE_FLAG_POWER);
}

#[test]
fn local_scale_clamps_to_the_spec_bounds() {
    // A tiny degraded power pushes the scale above 5, a tiny reference
    // power below 3e-4 (spec 03, 3.7 steps 2 and 5).
    let (_, high) = local_scale(1e9, 0.0, 1.0, 0);
    assert_eq!(high, SCALE_MAX);
    let (_, low) = local_scale(0.0, 1e9, 1.0, 0);
    assert_eq!(low, SCALE_MIN);
    // In-range values pass through untouched on frame 0 (step 3).
    let (unclamped, clamped) = local_scale(10_000.0, 10_000.0, 1.0, 0);
    assert!((unclamped - 1.0).abs() < 1e-9);
    assert_eq!(clamped, unclamped);
}

#[test]
fn local_scale_smooths_and_stores_the_unclamped_value() {
    // Frames above 0 blend 0.2 of the previous scale with 0.8 of the
    // raw ratio (spec 03, 3.7 step 3); step 4 stores the unclamped
    // value as the next previous scale, before the clamp of step 5.
    let (unclamped, clamped) = local_scale(0.0, 1e9, 100.0, 7);
    let raw = SCALE_OFFSET / (1e9 + SCALE_OFFSET);
    assert!((unclamped - (0.2 * 100.0 + 0.8 * raw)).abs() < 1e-12);
    assert!(unclamped > SCALE_MAX, "the unclamped value exceeds the cap");
    assert_eq!(clamped, SCALE_MAX);
}

#[test]
fn compensation_factors_are_reciprocal_when_roles_swap() {
    // Swapping the reference and degraded averages inverts the factor
    // (spec 03, 3.5 step 3), so the product is exactly 1 inside the
    // clamp range.
    for (avg_ref, avg_deg) in [(1.0, 4.0), (123.0, 7.0), (0.0, 0.0), (1e6, 1.0)] {
        let forward = compensation_factor(avg_ref, avg_deg);
        let inverse = compensation_factor(avg_deg, avg_ref);
        assert!(
            (forward * inverse - 1.0).abs() < 1e-12,
            "ref {avg_ref}, deg {avg_deg}"
        );
    }
    // Equal averages leave the reference unchanged: the factor is 1.
    assert_eq!(compensation_factor(42.0, 42.0), 1.0);
    // Clamp bounds (spec 03, 3.5 step 3).
    assert_eq!(compensation_factor(0.0, 1e9), COMPENSATION_MAX);
    assert_eq!(compensation_factor(1e9, 0.0), COMPENSATION_MIN);
}

#[test]
fn governing_delay_uses_the_last_utterance_before_the_frame() {
    let utterances = [
        utterance(0, 100),
        utterance(80, 300),  // 80 * 32 = 2560 samples
        utterance(200, -50), // 200 * 32 = 6400 samples
    ];
    assert_eq!(governing_delay(2400, &utterances), 100);
    assert_eq!(governing_delay(2600, &utterances), 300);
    assert_eq!(governing_delay(7000, &utterances), -50);
    assert_eq!(governing_delay(2400, &[]), 0);
}

#[test]
fn frame_range_skips_the_silent_margins() {
    // The probe at 2400 starts inside the tone, so nothing is skipped
    // at the start. The trailing zero padding begins at sample 10400 and
    // the end probe walks back from 12959 in a 5-sample window; the
    // window [10399, 10403] sums to only |x[10399]| = 339, and the next
    // one, [10398, 10402], sums to |x[10398]| + |x[10399]| = 976, so the
    // skip stops at 12959 - 10402 = 2557.
    let signal = SignalBuffer::from_pcm(&tone_pcm(1000.0)).unwrap();
    let range = frame_range(&signal);
    assert_eq!(range.skip_start, 0);
    assert_eq!(range.skip_end, 2557);
    assert_eq!(range.start, 0);
    assert_eq!(range.stop, 61);
}

#[test]
fn frame_range_of_a_silent_signal_reaches_the_skip_caps() {
    let signal = SignalBuffer::from_pcm(&vec![0i16; 8000]).unwrap();
    let range = frame_range(&signal);
    assert_eq!(range.skip_start, signal.nominal_len / 2);
    assert_eq!(range.skip_end, signal.nominal_len / 2);
    // Both skips consume the signal, so the processed range is empty
    // and the frame loop must not panic on it.
    let model = run_frame_loop(&signal, &signal, &[]);
    assert!(model.frame_range.stop < model.frame_range.start);
    assert!(model.pitch_ref.iter().all(|&p| p == 0.0));
    assert!(model.silence_flags.iter().all(|&silent| silent));
}

#[test]
fn identical_signals_pass_compensation_and_scaling_unchanged() {
    // The compensation factor with equal averages is exactly 1 (3.5)
    // and the scale with equal audible powers is exactly 1 (3.7), so
    // identical inputs leave both density sets identical.
    let signal = SignalBuffer::from_pcm(&tone_pcm(3000.0)).unwrap();
    let model = run_frame_loop(&signal, &signal, &[utterance(0, 0)]);
    assert!(!model.silence_flags.iter().any(|&silent| silent));
    assert_eq!(model.pitch_ref, model.pitch_deg);
    assert_eq!(model.loudness_ref, model.loudness_deg);
    // Non-silent frames store a reference audible power above the
    // silence threshold (spec 03, 3.4 step 2 and 3.7 step 7).
    let first = model.frame_range.start;
    assert!(model.audible_ref[first] > 1e7);
    assert!(model.loudness_ref_at(first, 13) > 0.0);
    assert_eq!(model.frame_count(), model.frame_range.stop + 1);
}

#[test]
fn out_of_range_degraded_frames_get_zero_spectra() {
    // A delay beyond the buffer forces every degraded spectrum to zero
    // (spec 03, 3.2 step 4); the scale then clamps to its maximum and
    // the degraded loudness stays zero.
    let signal = SignalBuffer::from_pcm(&tone_pcm(3000.0)).unwrap();
    let model = run_frame_loop(&signal, &signal, &[utterance(0, 20_000)]);
    for frame in model.frame_range.start..=model.frame_range.stop {
        for band in 0..NUM_BANDS {
            assert_eq!(model.pitch_deg_at(frame, band), 0.0);
            assert_eq!(model.loudness_deg_at(frame, band), 0.0);
        }
    }
    assert!(model.pitch_ref.iter().any(|&p| p > 0.0));
}
