//! Pure Rust implementation of ITU-T P.862 (PESQ) speech quality
//! assessment, narrowband mode, built clean-room from the behavioral
//! specification in `spec/`.
//!
//! [`pesq`] wires the stages into the processing order of spec 01
//! section 1.15: input handling, preprocessing, and time alignment
//! ([`input`]), the perceptual model ([`psychoacoustic`], spec 03), the
//! disturbance processing ([`disturbance`], spec 04), and the scoring
//! ([`score`], spec 05). One stage is not implemented yet: the two
//! public functions of [`disturbance`] are still stubs, so [`pesq`]
//! panics with "not yet implemented" when the pipeline reaches the
//! disturbance stage.
//!
//! # Public API
//!
//! Input: two slices of mono 16-bit linear PCM at 16 kHz, one for the
//! reference signal and one for the degraded signal. Output: the raw
//! P.862 score (about -0.5 to 4.5, spec 05 section 5.1) or a
//! [`PesqError`].
//!
//! ```ignore
//! let score = pesq::pesq(&reference_pcm, &degraded_pcm)?;
//! println!("raw PESQ score: {score:.3}");
//! println!("MOS-LQO: {:.3}", pesq::score::mos_lqo(f64::from(score)));
//! # Ok::<(), pesq::PesqError>(())
//! ```
//!
//! The model itself runs at 8 kHz (spec 01, table 1.1); the 16 kHz input
//! is decimated in [`input`].
//!
//! # Module map
//!
//! * [`input`]: spec 01, input handling, preprocessing, and time
//!   alignment.
//! * [`dsp`]: spec 02, FFT conventions, windows, and filter tables.
//! * [`psychoacoustic`]: spec 03, spectra, Bark warping, loudness,
//!   scaling.
//! * [`disturbance`]: spec 04, disturbance processing and aggregation.
//! * [`score`]: spec 05, final score and MOS-LQO mapping.
//! * [`types`]: shared data structures and constants.

pub mod alignment;
pub mod disturbance;
pub mod dsp;
pub mod dsp_fft;
pub mod dsp_filters;
pub mod input;
pub mod input_buffers;
pub mod input_filters;
pub mod psychoacoustic;
pub mod score;
pub mod splitting;
pub mod types;
pub mod utterances;
pub mod vad;

pub use types::PesqError;

/// Score a reference/degraded pair (narrowband P.862).
///
/// Both inputs are mono 16-bit linear PCM at 16 kHz. The model downsamples
/// to its native 8 kHz rate and follows the processing order of spec 01
/// section 1.15: input handling and alignment (1.2 to 1.13), length
/// equalization (1.7), the perceptual model (spec 03), the disturbance
/// computation (spec 04), and the scoring (spec 05). The returned value
/// is the raw P.862 score, unclipped (spec 05, 5.1); map it to MOS-LQO
/// with [`score::mos_lqo`].
///
/// # Errors
///
/// * [`PesqError::SignalTooShort`] when an input holds fewer than 2000
///   samples after decimation (spec 01, 1.2 step 5).
/// * [`PesqError::NoUtterancesFound`] when no utterance qualifies
///   (spec 01, 1.11 step 5).
///
/// # Panics
///
/// The disturbance stage of spec 04 is the last remaining stub: this
/// function panics with "not yet implemented" when the pipeline reaches
/// [`disturbance::frame_disturbances`].
pub fn pesq(ref_wav: &[i16], deg_wav: &[i16]) -> Result<f32, PesqError> {
    // spec 01, 1.2: input format and the margin-layout buffers, with the
    // 16 kHz to 8 kHz decimation of the public API.
    let reference = input::prepare_input(ref_wav)?;
    let degraded = input::prepare_input(deg_wav)?;

    // spec 01, 1.3 to 1.13: level normalization, IRS receive filtering,
    // DC removal, the input IIR filter, VAD, alignment, and the saved
    // model copies equalized to Nmax + P samples (1.7).
    let pair = input::process_pair(reference, degraded)?;

    // spec 03: perceptual model over the saved copies with the
    // per-utterance delays.
    let model = psychoacoustic::run_frame_loop(&pair.reference, &pair.degraded, &pair.utterances);

    // spec 04, 4.1 to 4.7: per-frame disturbances. The frame skipping at
    // negative delay jumps (spec 01, 1.14 and spec 04, 4.4) belongs in
    // this stage: the skip range derives from `model.frame_range.stop`
    // via `crate::utterances::negative_delay_skip_flags`.
    let frames = disturbance::frame_disturbances(&pair.reference, &pair.degraded, &pair.utterances);

    // spec 04, 4.8: aggregation over syllables and time into the two
    // disturbance indicators.
    let indicators = disturbance::aggregate(&frames, model.frame_range.start);

    // spec 05, 5.1: the raw score from the two indicators.
    Ok(score::raw_score(
        indicators.symmetric,
        indicators.asymmetric,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::WINDOW_SAMPLES;

    /// A 16 kHz silence of just over the minimum 8 kHz length.
    fn silence_16k() -> Vec<i16> {
        vec![0i16; 2 * types::MIN_INPUT_SAMPLES + 100]
    }

    /// A pair of 16 kHz buffers holding the same aperiodic noise bursts
    /// in 60-window bursts with 30-window silent gaps, as in the `input`
    /// module tests. The content is real enough for the VAD and the
    /// alignment to find utterances, so the pipeline runs through the
    /// perceptual model and reaches the disturbance stage.
    fn noise_pair_16k() -> (Vec<i16>, Vec<i16>) {
        let mut pcm = vec![0i16; 2 * 8000];
        let mut state = 21u32;
        let mut burst = 75usize;
        let last = 8000 / WINDOW_SAMPLES - 75;
        while burst + 60 <= last {
            for i in 0..60 * WINDOW_SAMPLES {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                pcm[2 * (burst * WINDOW_SAMPLES + i)] =
                    (((state as f32 / u32::MAX as f32) * 2.0 - 1.0) * 3000.0) as i16;
            }
            burst += 90;
        }
        (pcm.clone(), pcm)
    }

    #[test]
    fn pesq_rejects_short_input() {
        let short = vec![0i16; 2 * types::MIN_INPUT_SAMPLES - 1];
        assert_eq!(
            pesq(&short, &silence_16k()).unwrap_err(),
            PesqError::SignalTooShort {
                samples: types::MIN_INPUT_SAMPLES - 1
            }
        );
        assert_eq!(
            pesq(&silence_16k(), &short).unwrap_err(),
            PesqError::SignalTooShort {
                samples: types::MIN_INPUT_SAMPLES - 1
            }
        );
    }

    /// The wired pipeline runs input, alignment, and the perceptual
    /// model and then reaches the disturbance stage, which is the last
    /// remaining stub (spec 04): the call panics there with the stage
    /// name in the message.
    #[test]
    #[should_panic(expected = "spec 04")]
    fn pesq_pipeline_reaches_the_stubbed_disturbance_stage() {
        let (reference, degraded) = noise_pair_16k();
        let _ = pesq(&reference, &degraded);
    }
}
