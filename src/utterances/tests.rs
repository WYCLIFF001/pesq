//! Tests for utterance boundaries, splitting, and frame skipping.

use super::*;
use crate::vad::voice_activity_detection;

/// A work utterance with the given boundaries and zero delays.
fn work(start: usize, end: usize) -> UtteranceWork {
    UtteranceWork {
        start,
        end,
        search_start: start,
        search_end: end,
        coarse: 0,
        fine: 0,
        confidence: 1.0,
    }
}

/// A signal buffer with two sine bursts of `burst` windows separated by
/// `gap` windows of silence.
fn two_bursts(burst_windows: usize, gap_windows: usize) -> SignalBuffer {
    let burst = burst_windows * WINDOW_SAMPLES;
    let gap = gap_windows * WINDOW_SAMPLES;
    let len = 2 * burst + gap + 2 * MARGIN_SAMPLES;
    let mut pcm = vec![0i16; len];
    for i in 0..burst {
        let phase = std::f32::consts::TAU * 1000.0 * i as f32 / 8000.0;
        let sample = (3000.0 * phase.sin()) as i16;
        pcm[i] = sample;
        pcm[burst + gap + i] = sample;
    }
    SignalBuffer::from_pcm(&pcm).unwrap()
}

#[test]
fn boundary_merging_moves_edges_and_centers_midpoints() {
    let mut utterances = vec![work(100, 200), work(300, 400), work(500, 600)];
    merge_boundaries(&mut utterances, 800);
    assert_eq!(utterances[0].start, 75);
    assert_eq!(utterances[2].end, 800 - 75);
    // (300 + 200) / 2 = 250 and (500 + 400) / 2 = 450.
    assert_eq!(utterances[0].end, 250);
    assert_eq!(utterances[1].start, 250);
    assert_eq!(utterances[1].end, 450);
    assert_eq!(utterances[2].start, 450);
}

#[test]
fn two_bursts_yield_two_aligned_utterances() {
    let reference = two_bursts(200, 200);
    let degraded = reference.clone();
    let ref_vad = voice_activity_detection(&reference);
    let deg_vad = voice_activity_detection(&degraded);
    let utterances = align_utterances(&reference, &degraded, &ref_vad, &deg_vad).unwrap();
    assert_eq!(utterances.len(), 2);
    // Boundaries after merging: [75, 375) and [375, V - 75).
    assert_eq!(utterances[0].start_window, 75);
    assert_eq!(utterances[0].end_window, 375);
    assert_eq!(utterances[1].start_window, 375);
    assert_eq!(utterances[1].end_window, ref_vad.window_count - 75);
    for utterance in &utterances {
        assert_eq!(utterance.fine_delay, 0);
        assert!(utterance.confidence > 0.0);
    }
}

/// Noise in 60-window bursts separated by 30-window silent gaps, which
/// keeps the VAD noise floor low (the gaps anchor the threshold) while
/// the gap joining of 1.8 step 10 merges everything into one long
/// utterance with dense log-domain activity in the bursts.
fn bursty_noise(len: usize, amplitude: i16) -> SignalBuffer {
    let mut pcm = vec![0i16; len];
    let mut state = 7u32;
    let mut burst = 75usize;
    let last = len / WINDOW_SAMPLES - 75;
    while burst + 60 <= last {
        for i in 0..60 * WINDOW_SAMPLES {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            pcm[burst * WINDOW_SAMPLES + i] =
                (((state as f32 / u32::MAX as f32) * 2.0 - 1.0) * f32::from(amplitude)) as i16;
        }
        burst += 90;
    }
    SignalBuffer::from_pcm(&pcm).unwrap()
}

#[test]
fn split_finds_the_delay_jump_in_a_long_utterance() {
    // The degraded copy of the bursty noise is delayed by 256 samples
    // before the midpoint and aligned after it. The split procedure must
    // cut near the delay jump and assign the two halves delays near 256
    // and 0.
    let len = 16000usize;
    let midpoint = len / 2;
    let reference = bursty_noise(len, 3000);
    let mut deg_pcm = vec![0i16; len];
    for (i, sample) in deg_pcm.iter_mut().enumerate() {
        let source = if i < midpoint {
            i.saturating_sub(256)
        } else {
            i
        };
        *sample = reference.samples[MARGIN_SAMPLES + source] as i16;
    }
    let degraded = SignalBuffer::from_pcm(&deg_pcm).unwrap();
    let ref_vad = voice_activity_detection(&reference);
    let deg_vad = voice_activity_detection(&degraded);
    let utterances = align_utterances(&reference, &degraded, &ref_vad, &deg_vad).unwrap();
    // The split cuts inside a wide band around the delay jump; the
    // split can cascade (1.13 step 11 re-examines the left half at the
    // same index), so the list may hold three utterances, but the first
    // delay sits near 256 and the last near 0.
    assert!(utterances.len() >= 2, "expected a split: {utterances:?}");
    let first = utterances[0];
    let last = utterances[utterances.len() - 1];
    assert!(
        (first.fine_delay - 256).abs() <= 64,
        "first delay {} should sit near 256",
        first.fine_delay
    );
    assert!(
        last.fine_delay.abs() <= 64,
        "last delay {} should sit near 0",
        last.fine_delay
    );
    // Some boundary between the halves lands within a wide band around
    // the midpoint window 250.
    let boundary = utterances[1].start_window;
    assert!(boundary > 150 && boundary < 350, "boundary {boundary}");
    assert!(first.confidence > 0.0 && last.confidence > 0.0);
}

#[test]
fn skip_flags_mark_the_frame_range_of_a_negative_jump() {
    let utterances = vec![
        Utterance {
            start_window: 0,
            end_window: 150,
            coarse_delay: 0,
            fine_delay: 0,
            confidence: 0.0,
            split_frame: None,
        },
        Utterance {
            start_window: 200,
            end_window: 400,
            coarse_delay: -200,
            fine_delay: -200,
            confidence: 0.0,
            split_frame: None,
        },
    ];
    let flags = negative_delay_skip_flags(&utterances, 40);
    // f1 = trunc((125 * 32 - 200) / 128) = 29, j = trunc(2400 / 128) = 18,
    // f1 > j so f1 = 18; f2 = trunc(4200 / 128) + 1 = 33.
    assert!(!flags[17]);
    assert!(flags[18]);
    assert!(flags[33]);
    assert!(!flags[34]);
}

#[test]
fn skip_flags_never_mark_frame_stop_itself() {
    // 1.14 step 3 skips frames strictly below frame_stop: with the same
    // jump as above and frame_stop = 33, frame 33 stays unflagged.
    let utterances = vec![
        Utterance {
            start_window: 0,
            end_window: 150,
            coarse_delay: 0,
            fine_delay: 0,
            confidence: 0.0,
            split_frame: None,
        },
        Utterance {
            start_window: 200,
            end_window: 400,
            coarse_delay: -200,
            fine_delay: -200,
            confidence: 0.0,
            split_frame: None,
        },
    ];
    let flags = negative_delay_skip_flags(&utterances, 33);
    assert!(flags[18]);
    assert!(flags[32]);
    assert!(!flags[33]);
}

#[test]
fn skip_flags_use_truncating_division_for_the_upper_bound() {
    // end[0] = 50 lies below 75, so (end - 75) * 32 is negative and the
    // truncating division of 1.14 step 1 differs from a floor.
    let utterances = vec![
        Utterance {
            start_window: 0,
            end_window: 50,
            coarse_delay: 0,
            fine_delay: 0,
            confidence: 0.0,
            split_frame: None,
        },
        Utterance {
            start_window: 60,
            end_window: 400,
            coarse_delay: -256,
            fine_delay: -256,
            confidence: 0.0,
            split_frame: None,
        },
    ];
    let flags = negative_delay_skip_flags(&utterances, 40);
    // f1 = trunc(-736 / 128) = -5, j = trunc(-800 / 128) = -6, f1 > j so
    // f1 = j = -6, then f1 < 0 so f1 = 0; f2 = trunc(-224 / 128) + 1 = 0.
    assert!(flags[0]);
    assert!(!flags[1]);
}

#[test]
fn skip_flags_ignore_small_jumps() {
    let utterances = vec![
        Utterance {
            start_window: 0,
            end_window: 150,
            coarse_delay: 0,
            fine_delay: 0,
            confidence: 0.0,
            split_frame: None,
        },
        Utterance {
            start_window: 200,
            end_window: 400,
            coarse_delay: -100,
            fine_delay: -100,
            confidence: 0.0,
            split_frame: None,
        },
    ];
    let flags = negative_delay_skip_flags(&utterances, 40);
    assert!(flags.iter().all(|&flag| !flag));
}

#[test]
fn align_utterances_finds_one_utterance_for_silence() {
    // For an all-silent input the VAD floor of spec 01 section 1.8
    // step 3 leaves every interior window positive (the threshold of
    // step 4 stays 0, so step 6 negates nothing), and 1.11 reads
    // that as one giant utterance with zero log-domain activity.
    let reference = SignalBuffer::from_pcm(&vec![0i16; 4000]).unwrap();
    let degraded = reference.clone();
    let ref_vad = voice_activity_detection(&reference);
    let deg_vad = voice_activity_detection(&degraded);
    let utterances = align_utterances(&reference, &degraded, &ref_vad, &deg_vad).unwrap();
    assert_eq!(utterances.len(), 1);
    assert_eq!(utterances[0].start_window, 75);
    assert_eq!(utterances[0].end_window, ref_vad.window_count - 75);
    assert_eq!(utterances[0].fine_delay, 0);
}

#[test]
fn align_utterances_rejects_runs_outside_the_degredation_bounds() {
    // A coarse delay of 260 windows pushes b2 of spec 01 section 1.11
    // step 1 below every run start, so no run qualifies and the model
    // cannot proceed (1.11 step 5).
    let total = 400usize;
    let mut ref_log = vec![0.0f32; total];
    let mut deg_log = vec![0.0f32; total];
    for value in ref_log[100..140].iter_mut() {
        *value = 1.0;
    }
    for value in deg_log[360..400].iter_mut() {
        *value = 1.0;
    }
    let mut ref_energy = ref_log.clone();
    for value in ref_energy[100..140].iter_mut() {
        *value = 1.0;
    }
    let ref_vad = VadData {
        window_count: total,
        energy: ref_energy,
        log_vad: ref_log,
        threshold: 0.5,
        signal_level: 1.0,
        noise_level: 0.0,
    };
    let deg_vad = VadData {
        window_count: total,
        energy: vec![0.0; total],
        log_vad: deg_log,
        threshold: 0.5,
        signal_level: 1.0,
        noise_level: 0.0,
    };
    let reference = SignalBuffer::from_pcm(&vec![0i16; 4000]).unwrap();
    let result = align_utterances(&reference, &reference, &ref_vad, &deg_vad);
    assert_eq!(result.unwrap_err(), PesqError::NoUtterancesFound);
}
