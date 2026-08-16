//! Input handling, preprocessing, and time alignment (spec 01).
//!
//! This module orchestrates the pipeline of spec 01: the 16 kHz to 8 kHz
//! rate conversion and margin-layout buffers (1.2), level normalization
//! (1.3), IRS receive filtering and the model copies (1.4), DC removal
//! and the input IIR filter (1.5 and 1.6), the VAD (1.8), and the
//! per-utterance alignment (1.9 to 1.13). The stage implementations live
//! in [`input_buffers`], [`input_filters`], [`vad`], [`alignment`], and
//! [`utterances`].

use crate::input_buffers::normalize_levels;
use crate::input_filters::{apply_irs_receive, input_iir_filter, remove_dc};
use crate::types::{PADDING_SAMPLES, PesqError, SignalBuffer, Utterance};
use crate::utterances::align_utterances;
use crate::vad::voice_activity_detection;

/// Cutoff frequency of the decimation filter: 0.45 of the 8 kHz Nyquist
/// frequency, 1800 Hz (spec/CONFORMANCE.md section 6 item 4).
const DECIMATOR_CUTOFF_HZ: f64 = 0.45 * 8000.0 / 2.0;

/// Half-length of the decimation filter in taps: 16 per side, 33 total.
const DECIMATOR_HALF_TAPS: usize = 16;

/// Downsample a 16 kHz PCM stream to the 8 kHz rate the model operates
/// at (spec 01, table 1.1).
///
/// The public API accepts 16 kHz input while the model itself is
/// narrowband at 8 kHz, so this entry point decimates by a factor of 2
/// with a short windowed-sinc anti-aliasing filter: 33 taps, Hamming
/// window, cutoff at 0.45 of the 8 kHz Nyquist frequency (1800 Hz),
/// normalized to unit DC gain (spec/CONFORMANCE.md section 6 item 4).
/// The filter is linear phase and centered on each output sample, so it
/// introduces no delay between the two signals. The passband is flat
/// within about 0.03 dB up to the cutoff and the stopband sits below
/// -45 dB, so content above 4 kHz cannot alias into the 8 kHz model
/// band. The result is rounded to the nearest sample value and clamped
/// to the 16-bit range.
///
/// A trailing odd input sample has no output sample centered on it and
/// is dropped; the output holds `pcm.len() / 2` samples.
pub fn decimate_16k_to_8k(pcm: &[i16]) -> Vec<i16> {
    let taps = decimator_taps();
    let half = DECIMATOR_HALF_TAPS as isize;
    let mut output = Vec::with_capacity(pcm.len() / 2);
    for j in 0..output.capacity() {
        let mut sum = 0.0f64;
        for (tap_index, &coefficient) in taps.iter().enumerate() {
            let offset = 2 * j as isize - (tap_index as isize - half);
            if offset >= 0 && offset < pcm.len() as isize {
                sum += coefficient * f64::from(pcm[offset as usize]);
            }
        }
        output.push(sum.round().clamp(-32768.0, 32767.0) as i16);
    }
    output
}

/// The Hamming-windowed sinc taps of [`decimate_16k_to_8k`], computed
/// once on first use and normalized to unit DC gain. Tap `k` belongs to
/// sample offset `k - DECIMATOR_HALF_TAPS`.
fn decimator_taps() -> &'static [f64] {
    use std::sync::OnceLock;
    static TAPS: OnceLock<Vec<f64>> = OnceLock::new();
    TAPS.get_or_init(|| {
        let cutoff = DECIMATOR_CUTOFF_HZ / 16000.0;
        let length = 2 * DECIMATOR_HALF_TAPS + 1;
        let mut taps = vec![0.0f64; length];
        for (i, tap) in taps.iter_mut().enumerate() {
            let n = i as isize - DECIMATOR_HALF_TAPS as isize;
            let sinc = if n == 0 {
                2.0 * cutoff
            } else {
                (2.0 * core::f64::consts::PI * cutoff * n as f64).sin()
                    / (core::f64::consts::PI * n as f64)
            };
            let window =
                0.54 - 0.46 * (2.0 * core::f64::consts::PI * i as f64 / (length - 1) as f64).cos();
            *tap = sinc * window;
        }
        let sum: f64 = taps.iter().sum();
        for tap in taps.iter_mut() {
            *tap /= sum;
        }
        taps
    })
}

/// Convert one 16 kHz input to the 8 kHz signal buffer of spec 01
/// section 1.2, enforcing the minimum length check of step 5.
pub fn prepare_input(pcm_16k: &[i16]) -> Result<SignalBuffer, PesqError> {
    SignalBuffer::from_pcm(&decimate_16k_to_8k(pcm_16k))
}

/// The output of the input pipeline (spec 01 sections 1.2 to 1.13),
/// consumed by the perceptual model of spec 03.
#[derive(Debug, Clone)]
pub struct AlignedPair {
    /// Saved model copy of the reference (spec 01, 1.4 step 2),
    /// equalized to the common length (1.7).
    pub reference: SignalBuffer,
    /// Saved model copy of the degraded signal, likewise equalized.
    pub degraded: SignalBuffer,
    /// The aligned utterances (spec 01, sections 1.9 to 1.13), carrying
    /// the fine delays the model applies.
    pub utterances: Vec<Utterance>,
}

impl AlignedPair {
    /// The larger of the two nominal lengths Nmax (spec 01, 1.3 step 3).
    pub fn nominal_max(&self) -> usize {
        self.reference.nominal_len.max(self.degraded.nominal_len)
    }
}

/// Run the input pipeline of spec 01 sections 1.2 to 1.13 on a pair of
/// 8 kHz signal buffers, following the processing order of 1.15.
///
/// The returned pair holds the saved model copies (1.4 step 2),
/// equalized to `Nmax + P` samples (1.7), and the aligned utterances.
/// Frame skipping of 1.14 is not applied here: it needs the frame range
/// of spec 03, so callers apply
/// [`crate::utterances::negative_delay_skip_flags`] with the frame stop
/// of the perceptual model.
pub fn process_pair(
    mut reference: SignalBuffer,
    mut degraded: SignalBuffer,
) -> Result<AlignedPair, PesqError> {
    // 1.3: level normalization, then 1.4: IRS receive filtering and the
    // saved model copies.
    normalize_levels(&mut reference, &mut degraded);
    let (mut model_reference, mut model_degraded) =
        apply_irs_receive(&mut reference, &mut degraded);

    // 1.5 and 1.6: DC removal and the input IIR filter on the working
    // buffers only; the model copies are untouched.
    for buffer in [&mut reference, &mut degraded] {
        remove_dc(buffer);
        input_iir_filter(buffer);
    }

    // 1.8: VAD for both signals, then 1.9 to 1.13: alignment.
    let ref_vad = voice_activity_detection(&reference);
    let deg_vad = voice_activity_detection(&degraded);
    let utterances = align_utterances(&reference, &degraded, &ref_vad, &deg_vad)?;

    // 1.7: extend the shorter saved buffer with zeros to Nmax + P.
    let n_max = reference.nominal_len.max(degraded.nominal_len);
    for model in [&mut model_reference, &mut model_degraded] {
        model.samples.resize(n_max + PADDING_SAMPLES, 0.0);
    }

    Ok(AlignedPair {
        reference: model_reference,
        degraded: model_degraded,
        utterances,
    })
}

/// Whole-signal coarse delay estimation of spec 01 section 1.9, see
/// [`crate::alignment::coarse_delay_whole`].
pub fn coarse_delay(reference: &crate::types::VadData, degraded: &crate::types::VadData) -> i32 {
    crate::alignment::coarse_delay_whole(reference, degraded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::WINDOW_SAMPLES;

    #[test]
    fn decimation_halves_the_rate_and_drops_a_trailing_odd_sample() {
        let input = [100i16, 200, 300, 400, 500, 600];
        assert_eq!(decimate_16k_to_8k(&input).len(), 3);
        let odd = [100i16, 200, 300];
        assert_eq!(decimate_16k_to_8k(&odd).len(), 1);
    }

    /// A constant signal passes through with unit DC gain (the taps are
    /// normalized so their sum is exactly 1). The first samples still see
    /// the leading zeros of the margin, so only the settled middle of the
    /// output must equal the input value.
    #[test]
    fn decimation_has_unit_dc_gain() {
        let input = vec![1000i16; 128];
        let output = decimate_16k_to_8k(&input);
        for &sample in &output[24..40] {
            assert_eq!(sample, 1000);
        }
    }

    /// Impulse response energy: an impulse of amplitude 30000 at the
    /// input spreads into the output as the taps from index 16 upward;
    /// their energy is 0.0758388 in the unit-DC-gain windowed-sinc design
    /// of the function docs. The constant pins the filter design so an
    /// accidental change of window or cutoff fails the test.
    #[test]
    fn decimation_impulse_response_energy() {
        let mut input = vec![0i16; 256];
        input[0] = 30000;
        let output = decimate_16k_to_8k(&input);
        let energy: f64 = output.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
        let normalized = energy / (30000.0 * 30000.0);
        assert!(
            (normalized - 0.075_838_833_052_037_23).abs() < 1e-4,
            "normalized impulse response energy {normalized} diverged from the pinned design"
        );
    }

    /// Frequency response sanity: sine RMS ratios of the settled output.
    /// The passband (400 Hz, 1000 Hz) is within 0.5 dB of unity and the
    /// cutoff sits at the -6 dB point; the stopband (3000 Hz, 4000 Hz)
    /// and the 5000 Hz tone aliasing to 3000 Hz are attenuated by more
    /// than 40 dB.
    #[test]
    fn decimation_frequency_response() {
        for &(frequency, lower, upper) in &[
            (400.0f64, 0.99f64, 1.01f64),
            (1000.0f64, 0.98f64, 1.02f64),
            (1800.0f64, 0.48f64, 0.52f64),
            (3000.0f64, 0.0f64, 0.01f64),
            (4000.0f64, 0.0f64, 0.01f64),
            (5000.0f64, 0.0f64, 0.01f64),
        ] {
            let input: Vec<i16> = (0..1600)
                .map(|i| {
                    let phase = 2.0 * core::f64::consts::PI * frequency * i as f64 / 16000.0;
                    (phase.sin() * 10000.0).round() as i16
                })
                .collect();
            let output = decimate_16k_to_8k(&input);
            let rms = |samples: &[i16]| -> f64 {
                (samples
                    .iter()
                    .map(|&s| f64::from(s) * f64::from(s))
                    .sum::<f64>()
                    / samples.len() as f64)
                    .sqrt()
            };
            // Skip the transient at the start of the output.
            let gain = rms(&output[200..600]) / rms(&input);
            assert!(
                gain >= lower && gain <= upper,
                "gain {gain:.5} at {frequency} Hz outside [{lower}, {upper}]"
            );
        }
    }

    #[test]
    fn prepare_input_enforces_the_minimum_length() {
        let too_short = vec![0i16; 2 * crate::types::MIN_INPUT_SAMPLES - 1];
        assert_eq!(
            prepare_input(&too_short).unwrap_err(),
            PesqError::SignalTooShort {
                samples: crate::types::MIN_INPUT_SAMPLES - 1
            }
        );
        let ok = vec![0i16; 2 * crate::types::MIN_INPUT_SAMPLES];
        assert!(prepare_input(&ok).is_ok());
    }

    /// A pair of buffers holding the same aperiodic noise bursts in
    /// 60-window bursts with 30-window silent gaps; the degraded one
    /// optionally differs in length.
    fn noise_pair(reference_len: usize, degraded_len: usize) -> (SignalBuffer, SignalBuffer) {
        let build = |len: usize| {
            let mut pcm = vec![0i16; len];
            let mut state = 21u32;
            let mut burst = 75usize;
            let last = len / WINDOW_SAMPLES - 75;
            while burst + 60 <= last {
                for i in 0..60 * WINDOW_SAMPLES {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    pcm[burst * WINDOW_SAMPLES + i] =
                        (((state as f32 / u32::MAX as f32) * 2.0 - 1.0) * 3000.0) as i16;
                }
                burst += 90;
            }
            SignalBuffer::from_pcm(&pcm).unwrap()
        };
        (build(reference_len), build(degraded_len))
    }

    #[test]
    fn process_pair_aligns_and_equalizes_the_model_copies() {
        let (reference, degraded) = noise_pair(8000, 12000);
        let pair = process_pair(reference, degraded).unwrap();
        let n_max = pair.nominal_max();
        assert_eq!(pair.reference.samples.len(), n_max + PADDING_SAMPLES);
        assert_eq!(pair.degraded.samples.len(), n_max + PADDING_SAMPLES);
        assert!(!pair.utterances.is_empty());
        for utterance in &pair.utterances {
            assert_eq!(utterance.fine_delay, 0);
        }
    }

    #[test]
    fn process_pair_accepts_identical_buffers() {
        let (reference, degraded) = noise_pair(8000, 8000);
        let pair = process_pair(reference, degraded).unwrap();
        assert!(!pair.utterances.is_empty());
        assert!(pair.utterances[0].start_window >= 75);
    }
}
