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

/// Downsample a 16 kHz PCM stream to the 8 kHz rate the model operates
/// at (spec 01, table 1.1).
///
/// The public API accepts 16 kHz input while the model itself is
/// narrowband at 8 kHz. The specification does not prescribe a decimation
/// filter, so this provisional implementation averages pairs of samples,
/// which attenuates the highest octave. The final decimator is chosen
/// when the processing stages land in Round 2.
pub fn decimate_16k_to_8k(pcm: &[i16]) -> Vec<i16> {
    pcm.chunks_exact(2)
        .map(|pair| ((i32::from(pair[0]) + i32::from(pair[1])) / 2) as i16)
        .collect()
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
    fn decimation_halves_the_rate_and_averages_pairs() {
        let input = [100i16, 200, 300, 400, 500, 600];
        let output = decimate_16k_to_8k(&input);
        assert_eq!(output, [150, 350, 550]);
    }

    #[test]
    fn decimation_drops_a_trailing_odd_sample() {
        let input = [100i16, 200, 300];
        let output = decimate_16k_to_8k(&input);
        assert_eq!(output, [150]);
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
