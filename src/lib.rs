//! Pure Rust implementation of ITU-T P.862 (PESQ) speech quality
//! assessment, narrowband mode, and its wideband extension P.862.2,
//! built clean-room from the behavioral specification in `spec/`.
//!
//! [`pesq`] wires the stages into the processing order of spec 01
//! section 1.15: input handling, preprocessing, and time alignment
//! ([`input`]), the perceptual model ([`psychoacoustic`], spec 03), the
//! disturbance processing ([`disturbance`], spec 04), and the scoring
//! ([`score`], spec 05). Wideband mode runs the same pipeline at 16 kHz
//! with the three differences of spec 06: the wideband input filter, the
//! P.862.2 score mapping, and the 16 kHz-only sample rate.
//!
//! # Public API
//!
//! Input: two slices of mono 16-bit linear PCM, one for the reference
//! signal and one for the degraded signal. Scoring several degraded
//! variants of one utterance is cheaper through [`PesqContext`], which
//! runs the reference-side preprocessing once and scores each variant
//! against the prepared state. [`pesq`] accepts 16 kHz
//! input and decimates to the native 8 kHz model rate (spec 01,
//! table 1.1); [`pesq_8k`] accepts 8 kHz input directly and is the
//! entry point for the conformance data of CONFORMANCE.md, which the
//! reference model scores at 8 kHz; [`pesq_wb`] accepts 16 kHz input
//! and returns the P.862.2 MOS-LQO (spec 06). Output: the raw P.862
//! score (about -0.5 to 4.5, spec 05 section 5.1), the wideband
//! MOS-LQO, or a [`PesqError`].
//!
//! ```ignore
//! let score = pesq::pesq_8k(&reference_pcm, &degraded_pcm)?;
//! println!("raw PESQ score: {score:.3}");
//! println!("MOS-LQO: {:.3}", pesq::score::mos_lqo(f64::from(score)));
//! let wb = pesq::pesq_wb(&reference_16k, &degraded_16k)?;
//! println!("P.862.2 MOS-LQO: {wb:.3}");
//! # Ok::<(), pesq::PesqError>(())
//! ```
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

/// Prepared reference signal for scoring several degraded variants of
/// one reference utterance.
///
/// [`PesqContext::new`] runs the reference-side preprocessing of the
/// 16 kHz pipeline once (spec 01 sections 1.2 to 1.8 on the reference:
/// decimation, level calibration, the IRS receive filter and the saved
/// model copy, DC removal, the input IIR cascade, and the VAD). Each
/// [`PesqContext::score`] call then pays only the degraded-side
/// preprocessing and the pair-wise stages (VAD of the degraded signal,
/// alignment, the perceptual model, disturbance processing, and
/// scoring), so a harness scoring N degraded variants of one utterance
/// pays the reference preprocessing once instead of N times.
///
/// Scores are bit-identical to [`pesq`]. A degraded signal longer than
/// the reference changes the shared normalization divisor of spec 01
/// section 1.3 step 3, so such a pair recomputes the reference chain
/// with that divisor instead of reusing the prepared state.
///
/// # Errors
///
/// The same as [`pesq`]: [`PesqError::SignalTooShort`] when an input
/// holds fewer than 2000 samples after decimation, and
/// [`PesqError::NoUtterancesFound`] when no utterance qualifies.
pub struct PesqContext {
    /// The prepared 8 kHz reference buffer, kept unmodified for pairs
    /// whose degraded signal is longer than the reference.
    original: types::SignalBuffer,
    /// Working reference buffer after the reference-side stages of
    /// spec 01 sections 1.3 to 1.6.
    working: types::SignalBuffer,
    /// Saved model copy of the reference (spec 01, 1.4 step 2).
    model: types::SignalBuffer,
    /// Reference VAD output (spec 01, 1.8).
    vad: types::VadData,
    /// Whether the entry point is the 8 kHz rate (no decimation).
    eight_k: bool,
}

impl PesqContext {
    /// Prepare a 16 kHz reference, mirroring the input handling of
    /// [`pesq`]: the PCM is decimated to the native 8 kHz model rate
    /// before preprocessing.
    pub fn new(ref_wav: &[i16]) -> Result<Self, PesqError> {
        Self::from_buffer(input::prepare_input(ref_wav)?, false)
    }

    /// Prepare a reference at the native 8 kHz model rate, mirroring
    /// the input handling of [`pesq_8k`]: the PCM feeds the pipeline
    /// without rate conversion. Otherwise identical to
    /// [`PesqContext::new`].
    pub fn new_8k(ref_wav: &[i16]) -> Result<Self, PesqError> {
        Self::from_buffer(types::SignalBuffer::from_pcm(ref_wav)?, true)
    }

    /// Run the reference-side stages of spec 01 sections 1.3 to 1.8 on
    /// an 8 kHz signal buffer, with the shared normalization divisor of
    /// spec 01 section 1.3 step 3 derived from the reference alone.
    fn from_buffer(reference: types::SignalBuffer, eight_k: bool) -> Result<Self, PesqError> {
        let original = reference.clone();
        let mut working = reference;
        let divisor = (original.nominal_len - 2 * types::MARGIN_SAMPLES + types::PADDING_SAMPLES)
            as f64;
        input_buffers::normalize_one(&mut working, divisor);
        dsp::apply_filter_curve(
            &mut working.samples,
            &dsp::IRS_RECEIVE_CURVE,
            types::RATE_8K,
        );
        let model = working.clone();
        input_filters::remove_dc(&mut working);
        input_filters::input_iir_filter(&mut working);
        let vad = vad::voice_activity_detection(&working);
        Ok(Self {
            original,
            working,
            model,
            vad,
            eight_k,
        })
    }

    /// Score one degraded signal against the prepared reference, at the
    /// sample rate of the constructor (16 kHz for [`PesqContext::new`],
    /// 8 kHz for [`PesqContext::new_8k`]).
    ///
    /// The reference-side stages of spec 01 run once at construction;
    /// this call runs the degraded-side stages and the pair-wise
    /// alignment, then the perceptual model, the disturbance
    /// computation, and the scoring. A degraded signal longer than the
    /// prepared reference recomputes the reference chain with the
    /// larger shared divisor, exactly as [`pesq`] would.
    pub fn score(&self, deg_wav: &[i16]) -> Result<f32, PesqError> {
        let mut degraded = if self.eight_k {
            types::SignalBuffer::from_pcm(deg_wav)?
        } else {
            input::prepare_input(deg_wav)?
        };
        if degraded.nominal_len > self.original.nominal_len {
            // spec 01, 1.3 step 3: the shared divisor is the larger
            // nominal length's, so the reference chain built at
            // construction no longer applies; rerun the full pipeline.
            return score_pair(self.original.clone(), degraded);
        }
        let n_max = self.original.nominal_len;
        let divisor = (n_max - 2 * types::MARGIN_SAMPLES + types::PADDING_SAMPLES) as f64;
        input_buffers::normalize_one(&mut degraded, divisor);
        dsp::apply_filter_curve(
            &mut degraded.samples,
            &dsp::IRS_RECEIVE_CURVE,
            types::RATE_8K,
        );
        let mut model_degraded = degraded.clone();
        input_filters::remove_dc(&mut degraded);
        input_filters::input_iir_filter(&mut degraded);
        let deg_vad = vad::voice_activity_detection(&degraded);
        let utterances =
            utterances::align_utterances(&self.working, &degraded, &self.vad, &deg_vad)?;
        let mut model_reference = self.model.clone();
        model_reference
            .samples
            .resize(n_max + types::PADDING_SAMPLES, 0.0);
        model_degraded
            .samples
            .resize(n_max + types::PADDING_SAMPLES, 0.0);
        Ok(score_aligned(input::AlignedPair {
            reference: model_reference,
            degraded: model_degraded,
            utterances,
        }))
    }
}

/// Score a reference/degraded pair (narrowband P.862) at 16 kHz.
///
/// Both inputs are mono 16-bit linear PCM at 16 kHz. The model decimates
/// to its native 8 kHz rate with the anti-aliasing filter of
/// [`input::decimate_16k_to_8k`] (spec 01, table 1.1 and
/// spec/CONFORMANCE.md section 6 item 4) and follows the processing
/// order of spec 01 section 1.15: input handling and alignment
/// (1.2 to 1.13), length equalization (1.7), the perceptual model
/// (spec 03), the disturbance computation (spec 04), and the scoring
/// (spec 05). Use [`pesq_8k`] to score 8 kHz PCM directly, without any
/// rate conversion. The returned value is the raw P.862 score, unclipped
/// (spec 05, 5.1); map it to MOS-LQO with [`score::mos_lqo`].
///
/// # Errors
///
/// * [`PesqError::SignalTooShort`] when an input holds fewer than 2000
///   samples after decimation (spec 01, 1.2 step 5).
/// * [`PesqError::NoUtterancesFound`] when no utterance qualifies
///   (spec 01, 1.11 step 5).
pub fn pesq(ref_wav: &[i16], deg_wav: &[i16]) -> Result<f32, PesqError> {
    // spec 01, 1.2: input format and the margin-layout buffers, with the
    // 16 kHz to 8 kHz decimation of this entry point.
    let reference = input::prepare_input(ref_wav)?;
    let degraded = input::prepare_input(deg_wav)?;
    score_pair(reference, degraded)
}

/// Score a reference/degraded pair (narrowband P.862) at 8 kHz.
///
/// Both inputs are mono 16-bit linear PCM at 8 kHz, the native model
/// rate (spec 01, table 1.1). The samples feed the pipeline without any
/// rate conversion, exactly as the reference model scores the
/// conformance data of CONFORMANCE.md section 6. Otherwise identical to
/// [`pesq`], including the error conditions.
pub fn pesq_8k(ref_wav: &[i16], deg_wav: &[i16]) -> Result<f32, PesqError> {
    // spec 01, 1.2: input format and the margin-layout buffers.
    let reference = types::SignalBuffer::from_pcm(ref_wav)?;
    let degraded = types::SignalBuffer::from_pcm(deg_wav)?;
    score_pair(reference, degraded)
}

/// Score a reference/degraded pair in wideband mode (P.862.2).
///
/// Both inputs are mono 16-bit linear PCM at 16 kHz, matching the
/// specification exactly; no rate conversion is performed (spec 06,
/// 6.2 item 4). The pipeline is the narrowband one at 16 kHz with the
/// wideband input filter of spec 06 section 6.3 replacing the IRS
/// receive curve, the 49-band Bark table, and the 16 kHz constants of
/// spec 06 section 6.4. The raw score is computed with the shared
/// formula but not reported; the returned value is the P.862.2 MOS-LQO
/// of [`score::mos_lqo_wb`] (spec 06, 6.5). Report it to 3 decimal
/// places as the specification does.
///
/// # Errors
///
/// * [`PesqError::SignalTooShort`] when an input holds fewer than 4000
///   samples (spec 06, 6.2 item 4).
/// * [`PesqError::NoUtterancesFound`] when no utterance qualifies
///   (spec 01, 1.11 step 5).
pub fn pesq_wb(ref_wav: &[i16], deg_wav: &[i16]) -> Result<f32, PesqError> {
    // spec 06, 6.2: 16 kHz buffers, margin 4800, padding 5120.
    let reference = types::SignalBuffer::from_pcm_at(ref_wav, types::RATE_16K)?;
    let degraded = types::SignalBuffer::from_pcm_at(deg_wav, types::RATE_16K)?;
    let raw = score_aligned(input::process_pair_wideband(reference, degraded)?);
    Ok(score::mos_lqo_wb(f64::from(raw)))
}

/// The shared pipeline of [`pesq`] and [`pesq_8k`] on 8 kHz signal
/// buffers: spec 01, 1.3 to 1.13 (level normalization, IRS receive
/// filtering, DC removal, the input IIR filter, VAD, alignment, and the
/// saved model copies equalized to Nmax + P samples per 1.7), then the
/// perceptual model of spec 03, the disturbance computation of spec 04,
/// and the scoring of spec 05.
fn score_pair(
    reference: types::SignalBuffer,
    degraded: types::SignalBuffer,
) -> Result<f32, PesqError> {
    Ok(score_aligned(input::process_pair(reference, degraded)?))
}

/// The stages after the input pipeline of [`input::process_pair`]: the
/// perceptual model of spec 03, the disturbance computation of spec 04,
/// and the scoring of spec 05, on an aligned pair.
fn score_aligned(pair: input::AlignedPair) -> f32 {
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
    score::raw_score(indicators.symmetric, indicators.asymmetric)
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

    /// The wired pipeline runs all stages and returns a finite score.
    /// The pair is the same signal twice, so the score must be at the
    /// clean end of the raw P.862 range (spec 05, 5.1).
    #[test]
    fn pesq_pipeline_runs_end_to_end() {
        let (reference, degraded) = noise_pair_16k();
        let score = pesq(&reference, &degraded).expect("identical pair must score");
        assert!(score.is_finite(), "score {score} is not finite");
        assert!(score <= 4.5, "raw score {score} exceeds 4.5");
        assert!(score > 3.5, "identical pair scored only {score}");
    }

    /// A 16 kHz noise-burst signal of `seconds` duration (the pattern of
    /// the criterion bench), for exercising the reference preprocessing.
    fn noise_bursts(seconds: usize, seed: u32) -> Vec<i16> {
        let burst_samples = 60 * 2 * WINDOW_SAMPLES;
        let cycle_samples = 90 * 2 * WINDOW_SAMPLES;
        let mut pcm = vec![0i16; seconds * 16_000];
        let mut state = seed;
        let mut offset = 0usize;
        while offset + burst_samples <= pcm.len() {
            for sample in &mut pcm[offset..offset + burst_samples] {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                *sample = (((state as f32 / u32::MAX as f32) * 2.0 - 1.0) * 3000.0) as i16;
            }
            offset += cycle_samples;
        }
        pcm
    }

    #[test]
    fn pesq_context_scores_bit_identically_to_pesq() {
        let reference = noise_bursts(2, 21);
        let degraded = noise_bursts(2, 42);
        let expected = pesq(&reference, &degraded).unwrap();
        let context = PesqContext::new(&reference).unwrap();
        assert_eq!(
            context.score(&degraded).unwrap().to_bits(),
            expected.to_bits()
        );
        let variant = noise_bursts(2, 43);
        assert_eq!(
            context.score(&variant).unwrap().to_bits(),
            pesq(&reference, &variant).unwrap().to_bits()
        );
    }

    #[test]
    fn pesq_context_8k_scores_bit_identically_to_pesq_8k() {
        let reference = input::decimate_16k_to_8k(&noise_bursts(2, 21));
        let degraded = input::decimate_16k_to_8k(&noise_bursts(2, 42));
        let expected = pesq_8k(&reference, &degraded).unwrap();
        let context = PesqContext::new_8k(&reference).unwrap();
        assert_eq!(
            context.score(&degraded).unwrap().to_bits(),
            expected.to_bits()
        );
    }

    #[test]
    fn pesq_context_recomputes_for_a_longer_degraded_signal() {
        let reference = noise_bursts(2, 21);
        let degraded = noise_bursts(3, 42);
        let expected = pesq(&reference, &degraded).unwrap();
        let context = PesqContext::new(&reference).unwrap();
        assert_eq!(
            context.score(&degraded).unwrap().to_bits(),
            expected.to_bits()
        );
    }

    #[test]
    fn pesq_wb_rejects_short_input() {
        // spec 06, 6.2 item 4: the wideband minimum is f/4 = 4000.
        let short = vec![0i16; 2 * types::MIN_INPUT_SAMPLES - 1];
        assert_eq!(
            pesq_wb(&short, &noise_bursts(1, 21)).unwrap_err(),
            PesqError::SignalTooShort {
                samples: 2 * types::MIN_INPUT_SAMPLES - 1
            }
        );
        assert_eq!(
            pesq_wb(&noise_bursts(1, 21), &short).unwrap_err(),
            PesqError::SignalTooShort {
                samples: 2 * types::MIN_INPUT_SAMPLES - 1
            }
        );
        assert!(pesq_wb(&noise_bursts(1, 21), &noise_bursts(1, 22)).is_ok());
    }

    /// The wideband pipeline runs all stages at 16 kHz and returns a
    /// MOS-LQO inside the mapping range of spec 06 section 6.5. The
    /// pair is the same signal twice, so the score sits at the clean
    /// end of the scale.
    #[test]
    fn pesq_wb_pipeline_runs_end_to_end() {
        let reference = noise_bursts(2, 21);
        let score = pesq_wb(&reference, &reference).expect("identical pair must score");
        assert!(score.is_finite(), "score {score} is not finite");
        assert!(
            (0.999..=4.999).contains(&score),
            "wideband MOS-LQO {score} outside [0.999, 4.999]"
        );
        assert!(score > 3.5, "identical pair scored only {score}");
        // A degraded variant scores below the clean pair.
        let degraded = noise_bursts(2, 42);
        assert!(pesq_wb(&reference, &degraded).unwrap() <= score);
    }
}
