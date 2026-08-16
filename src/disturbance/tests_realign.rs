//! Unit tests for spec 04 section 4.5: the per-interval delay search
//! and the bad-interval re-alignment.

use super::bad_intervals::{interval_delay, realign};
use crate::psychoacoustic::{NUM_BANDS, PerceptualModel};
use crate::types::{PADDING_SAMPLES, SignalBuffer};

/// A pseudo-random burst in [-1, 1) over a sample range, for the
/// re-alignment tests.
fn noise_burst(len: usize, range: std::ops::Range<usize>) -> Vec<f32> {
    let mut samples = vec![0.0f32; len];
    let mut state = 99u32;
    for sample in &mut samples[range] {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *sample = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
    }
    samples
}

/// Signal buffer with a noise burst in `[3680, 4256)` and silence
/// elsewhere; `n_max = 12800`, buffer length `n_max + P`.
fn interval_reference() -> SignalBuffer {
    let samples = noise_burst(12800 + PADDING_SAMPLES, 3680..4256);
    SignalBuffer {
        samples,
        nominal_len: 12800,
        input_len: 8000,
    }
}

#[test]
fn interval_delay_finds_a_positive_shift() {
    let reference = interval_reference();
    // The first re-aligned signal T holds the degraded samples, which
    // here are the reference delayed by +37 samples: T[i] = ref[i - 37].
    let mut realigned = vec![0.0f32; reference.samples.len()];
    let len = realigned.len();
    realigned[2400..len - 2400].copy_from_slice(&reference.samples[2400 - 37..len - 2400 - 37]);
    let delay = interval_delay(&reference, &realigned, 3680, 4608, 12800);
    assert_eq!(delay, 37);
}

#[test]
fn interval_delay_returns_zero_for_silent_degraded_samples() {
    let reference = interval_reference();
    let realigned = vec![0.0f32; reference.samples.len()];
    let delay = interval_delay(&reference, &realigned, 3680, 4608, 12800);
    assert_eq!(delay, 0);
}

/// Perceptual model over 31 frames for the re-alignment tests, with
/// flat pitch densities and loudness everywhere.
fn flat_model(pitch: f32, bad: std::ops::Range<usize>) -> (PerceptualModel, Vec<f32>, Vec<f32>) {
    let frame_count = 31;
    let pitch_ref = vec![pitch; frame_count * NUM_BANDS];
    let pitch_deg = vec![pitch; frame_count * NUM_BANDS];
    let loudness_ref = vec![pitch; frame_count * NUM_BANDS];
    let loudness_deg = vec![pitch; frame_count * NUM_BANDS];
    let mut symmetric = vec![0.0f32; frame_count];
    for frame in &mut symmetric[bad] {
        *frame = 50.0;
    }
    let asymmetric = vec![0.0f32; frame_count];
    let model = PerceptualModel {
        frame_range: crate::types::FrameRange {
            start: 2,
            stop: 30,
            skip_start: 0,
            skip_end: 0,
        },
        pitch_ref,
        pitch_deg,
        loudness_ref,
        loudness_deg,
        silence_flags: vec![false; frame_count],
        audible_ref: vec![1e5; frame_count],
    };
    (model, symmetric, asymmetric)
}

#[test]
fn realign_replaces_bad_frames_when_recomputation_is_quieter() {
    let reference = interval_reference();
    let degraded = SignalBuffer {
        samples: vec![0.0f32; reference.samples.len()],
        nominal_len: 12800,
        input_len: 8000,
    };
    let (model, mut symmetric, mut asymmetric) = flat_model(0.1, 10..16);
    // Pitch densities of 0.1 sit below every absolute hearing threshold
    // (the smallest is 0.251 at band 23), so the recomputed loudness,
    // and with it the new disturbance, is zero: the minimum of 50 and 0
    // replaces the bad frames.
    realign(
        &reference,
        &degraded,
        &[],
        &model,
        &mut symmetric,
        &mut asymmetric,
    );
    for (frame, &d) in symmetric.iter().enumerate().take(16).skip(10) {
        assert_eq!(d, 0.0, "frame {frame}");
    }
    assert!(symmetric[..10].iter().all(|&d| d == 0.0));
    assert!(symmetric[16..].iter().all(|&d| d == 0.0));
    // Frame 0 is never bad and the disturbed frames do not spread.
    assert!(asymmetric.iter().all(|&a| a == 0.0));
}

#[test]
fn realign_keeps_existing_values_when_recomputation_is_louder() {
    let reference = interval_reference();
    let degraded = SignalBuffer {
        samples: vec![0.0f32; reference.samples.len()],
        nominal_len: 12800,
        input_len: 8000,
    };
    // Pitch densities far above every threshold: the recomputed
    // disturbance exceeds the existing 50, so the minimum keeps 50.
    let (model, mut symmetric, mut asymmetric) = flat_model(1e9, 10..16);
    realign(
        &reference,
        &degraded,
        &[],
        &model,
        &mut symmetric,
        &mut asymmetric,
    );
    for (frame, &d) in symmetric.iter().enumerate().take(16).skip(10) {
        assert_eq!(d, 50.0, "frame {frame}");
    }
}
