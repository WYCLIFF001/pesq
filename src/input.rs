//! Input handling, preprocessing, and time alignment (spec 01).
//!
//! This Round 2 scaffold wires up the input path: the 16 kHz to 8 kHz
//! rate conversion, the minimum length check, and the margin-layout
//! signal buffers. The preprocessing and alignment stages of spec 01
//! sections 1.3 to 1.14 are stubs documented against their sections and
//! will be filled in by the implementers.

use crate::types::{PesqError, SignalBuffer, Utterance, VadData};

/// Downsample a 16 kHz PCM stream to the 8 kHz rate the model operates at
/// (spec 01, table 1.1).
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

/// Level normalization of spec 01 section 1.3 (stub, Round 2):
/// alignment-filter the scratch copies, compute the calibration scale
/// with the shared Nmax divisor, and scale the first N samples of each
/// signal buffer.
pub fn normalize_levels(_reference: &mut SignalBuffer, _degraded: &mut SignalBuffer) {
    todo!("spec 01, 1.3: level normalization")
}

/// IRS receive filtering of spec 01 section 1.4 (stub, Round 2): apply
/// [`crate::dsp::IRS_RECEIVE_CURVE`] with the filter procedure of 1.3.1
/// and save the model copies.
pub fn apply_irs_receive(_reference: &mut SignalBuffer, _degraded: &mut SignalBuffer) {
    todo!("spec 01, 1.4: IRS receive filtering")
}

/// DC removal of spec 01 section 1.5 (stub, Round 2), including the
/// intentional division by the nominal length N and the W-sample ramp at
/// both interval edges.
pub fn remove_dc(_buffer: &mut SignalBuffer) {
    todo!("spec 01, 1.5: DC removal")
}

/// Voice activity detection of spec 01 section 1.8 (stub, Round 2).
pub fn voice_activity_detection(_buffer: &SignalBuffer) -> VadData {
    todo!("spec 01, 1.8: voice activity detection")
}

/// Whole-signal coarse delay estimation of spec 01 section 1.9
/// (stub, Round 2).
pub fn coarse_delay(_reference: &VadData, _degraded: &VadData) -> i32 {
    todo!("spec 01, 1.9: coarse delay estimation")
}

/// Utterance search, per-utterance alignment, boundaries, and splitting
/// of spec 01 sections 1.10 to 1.13 (stub, Round 2).
pub fn align_utterances(_reference: &SignalBuffer, _degraded: &SignalBuffer) -> Vec<Utterance> {
    todo!("spec 01, 1.10 to 1.13: utterance alignment")
}

/// Frame skipping at negative delay jumps of spec 01 section 1.14
/// (stub, Round 2).
pub fn negative_delay_skip_flags(_utterances: &[Utterance], _frame_stop: usize) -> Vec<bool> {
    todo!("spec 01, 1.14: frame skipping at negative delay jumps")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
