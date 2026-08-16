//! Pure Rust implementation of ITU-T P.862 (PESQ) speech quality
//! assessment, narrowband mode, built clean-room from the behavioral
//! specification in `spec/`.
//!
//! This is the Round 2 scaffold: the module tree, the shared types, the
//! specification's tables and constants are in place, and the processing
//! stages are stubbed. [`pesq`] validates its inputs and then returns
//! [`PesqError::NotImplemented`] until the implementers fill the modules
//! in.
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

pub mod disturbance;
pub mod dsp;
pub mod input;
pub mod psychoacoustic;
pub mod score;
pub mod types;

pub use types::PesqError;

/// Score a reference/degraded pair (narrowband P.862).
///
/// Both inputs are mono 16-bit linear PCM at 16 kHz. The model downsamples
/// to its native 8 kHz rate and follows the processing order of spec 01
/// section 1.15. The returned value is the raw P.862 score, unclipped
/// (spec 05, 5.1); map it to MOS-LQO with [`score::mos_lqo`].
///
/// # Errors
///
/// * [`PesqError::SignalTooShort`] when an input holds fewer than 2000
///   samples after decimation (spec 01, 1.2 step 5).
/// * [`PesqError::NoUtterancesFound`] when no utterance qualifies
///   (spec 01, 1.11 step 5).
/// * [`PesqError::NotImplemented`] in this Round 2 scaffold, where the
///   processing stages are still stubs.
pub fn pesq(ref_wav: &[i16], deg_wav: &[i16]) -> Result<f32, PesqError> {
    let _reference = input::prepare_input(ref_wav)?;
    let _degraded = input::prepare_input(deg_wav)?;
    // Round 2: level normalization, IRS filtering, VAD, and alignment
    // (spec 01), the perceptual model (spec 03), the disturbance
    // computation (spec 04), and the scoring (spec 05).
    Err(PesqError::NotImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16 kHz silence of just over the minimum 8 kHz length.
    fn silence_16k() -> Vec<i16> {
        vec![0i16; 2 * types::MIN_INPUT_SAMPLES + 100]
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

    #[test]
    fn pesq_reports_the_scaffold_gap() {
        assert_eq!(
            pesq(&silence_16k(), &silence_16k()).unwrap_err(),
            PesqError::NotImplemented
        );
    }
}
