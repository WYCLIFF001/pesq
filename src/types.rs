//! Shared data structures and constants implied by the specification.
//!
//! This module holds the types that more than one stage of the algorithm
//! exchanges: the signal buffer layout of spec 01 section 1.2, the voice
//! activity detection output of spec 01 section 1.8, the utterance and
//! delay structures of spec 01 sections 1.9 to 1.13, and the frame range
//! of spec 03 section 3.1. Stage-specific tables live in the module that
//! uses them (`dsp` for spec 02, `psychoacoustic` for spec 03).

use std::fmt;

/// Sample rate in Hz that the P.862 model operates at (spec 01, table 1.1).
pub const SAMPLE_RATE_HZ: usize = 8000;

/// Window length W in samples used by the VAD analysis (spec 01, table 1.1).
pub const WINDOW_SAMPLES: usize = 32;

/// Margin M in windows on each side of the nominal signal (spec 01, table 1.1).
pub const MARGIN_WINDOWS: usize = 75;

/// Margin in samples on each side of the nominal signal,
/// `MARGIN_WINDOWS * WINDOW_SAMPLES` (spec 01, table 1.1).
pub const MARGIN_SAMPLES: usize = 2400;

/// Data padding P of 320 ms in samples (spec 01, table 1.1).
pub const PADDING_SAMPLES: usize = 2560;

/// FFT length A used for fine time alignment (spec 01, table 1.1).
pub const ALIGN_FFT_LEN: usize = 512;

/// Minimum input length, `f / 4 = 2000` samples (250 ms), below which
/// processing stops with an error (spec 01, 1.2 step 5).
pub const MIN_INPUT_SAMPLES: usize = 2000;

/// Errors reported by [`pesq`](crate::pesq) and the processing stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PesqError {
    /// An input holds fewer than [`MIN_INPUT_SAMPLES`] samples
    /// (spec 01, 1.2 step 5).
    SignalTooShort { samples: usize },
    /// No utterance qualified for scoring (spec 01, 1.11 step 5).
    NoUtterancesFound,
    /// The processing stage is not implemented yet; this Round 2 scaffold
    /// stubs the algorithm and returns this error from [`crate::pesq`].
    NotImplemented,
}

impl fmt::Display for PesqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SignalTooShort { samples } => write!(
                f,
                "input has {samples} samples, the minimum is {MIN_INPUT_SAMPLES} (250 ms at 8 kHz)"
            ),
            Self::NoUtterancesFound => {
                write!(f, "no utterance found in the input signals")
            }
            Self::NotImplemented => write!(
                f,
                "pesq is not implemented yet: this Round 2 scaffold stubs the processing stages"
            ),
        }
    }
}

impl std::error::Error for PesqError {}

/// A mono signal buffer in the layout defined by spec 01 section 1.2
/// step 6: [`MARGIN_SAMPLES`] zeros, then the L signal samples, then
/// [`MARGIN_SAMPLES`] + [`PADDING_SAMPLES`] zeros, for a total length of
/// `L + 7360` samples. The nominal length N covers `[0, L + 4800)`.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalBuffer {
    /// Full buffer of length `L + 7360 = N + P`.
    pub samples: Vec<f32>,
    /// Nominal length `N = L + 2 * MARGIN_SAMPLES`.
    pub nominal_len: usize,
    /// Number of PCM samples L held in the buffer.
    pub input_len: usize,
}

impl SignalBuffer {
    /// Build the margin layout of spec 01 section 1.2 step 6 from a slice
    /// of 8 kHz PCM samples, enforcing the minimum length check of
    /// step 5.
    pub fn from_pcm(pcm: &[i16]) -> Result<Self, PesqError> {
        let l = pcm.len();
        if l < MIN_INPUT_SAMPLES {
            return Err(PesqError::SignalTooShort { samples: l });
        }
        let nominal_len = l + 2 * MARGIN_SAMPLES;
        let mut samples = vec![0.0f32; nominal_len + PADDING_SAMPLES];
        for (i, &sample) in pcm.iter().enumerate() {
            samples[MARGIN_SAMPLES + i] = f32::from(sample);
        }
        Ok(Self {
            samples,
            nominal_len,
            input_len: l,
        })
    }

    /// Index of the first signal sample (always [`MARGIN_SAMPLES`]).
    pub const fn signal_start(&self) -> usize {
        MARGIN_SAMPLES
    }

    /// Index one past the last signal sample.
    pub fn signal_end(&self) -> usize {
        MARGIN_SAMPLES + self.input_len
    }
}

/// Voice activity detection output (spec 01, section 1.8).
///
/// The pair `(e, l)` of that section: the processed per-window energies
/// (steps 1 to 13) and the log-domain array (step 14), plus the levels
/// and threshold the alignment stages need.
#[derive(Debug, Clone, PartialEq)]
pub struct VadData {
    /// Window count `V = N / W`.
    pub window_count: usize,
    /// Processed window energies `e[v]`; non-negative after step 13.
    pub energy: Vec<f32>,
    /// Log-domain VAD `l[v]` (step 14).
    pub log_vad: Vec<f32>,
    /// VAD threshold `t` after step 5 (possibly -1 in the silent case).
    pub threshold: f32,
    /// Mean energy of the windows above the threshold (step 5), 0 if none.
    pub signal_level: f32,
    /// Mean energy of the remaining windows (step 5), 1 if none.
    pub noise_level: f32,
}

/// A detected utterance with its alignment results
/// (spec 01, sections 1.9 to 1.13).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Utterance {
    /// Start window index, inclusive.
    pub start_window: usize,
    /// End window index, inclusive.
    pub end_window: usize,
    /// Per-utterance coarse delay estimate in samples
    /// (spec 01, 1.9 step 7).
    pub coarse_delay: i32,
    /// Fine delay estimate in samples (spec 01, 1.10 step 8).
    pub fine_delay: i32,
    /// Confidence of the fine estimate (spec 01, 1.10 step 8).
    pub confidence: f32,
    /// Breakpoint window when this utterance is half of a split
    /// (spec 01, 1.13 step 9); `None` otherwise.
    pub split_frame: Option<usize>,
}

/// Processed frame range of the perceptual model (spec 03, section 3.1).
///
/// Frames are indexed from 0; the model processes the inclusive range
/// `[start, stop]`, where `start` is `frame_start` and `stop` is
/// `frame_stop` of the specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRange {
    /// First processed frame index (`frame_start`).
    pub start: usize,
    /// Last processed frame index (`frame_stop`), inclusive.
    pub stop: usize,
    /// Silence skipped at the start of the signal, in samples.
    pub skip_start: usize,
    /// Silence skipped at the end of the signal, in samples.
    pub skip_end: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_input_is_rejected() {
        let err = SignalBuffer::from_pcm(&[0i16; MIN_INPUT_SAMPLES - 1]).unwrap_err();
        assert_eq!(
            err,
            PesqError::SignalTooShort {
                samples: MIN_INPUT_SAMPLES - 1
            }
        );
    }

    #[test]
    fn buffer_layout_matches_spec_1_2_step_6() {
        let pcm: Vec<i16> = (0..4000).map(|i| (i % 1000) as i16).collect();
        let signal = SignalBuffer::from_pcm(&pcm).unwrap();
        assert_eq!(signal.input_len, 4000);
        assert_eq!(signal.nominal_len, 4000 + 2 * MARGIN_SAMPLES);
        assert_eq!(signal.samples.len(), 4000 + 2 * MARGIN_SAMPLES + PADDING_SAMPLES);
        assert!(signal.samples[..MARGIN_SAMPLES].iter().all(|&s| s == 0.0));
        assert!(signal.samples[signal.signal_end()..].iter().all(|&s| s == 0.0));
        assert_eq!(signal.samples[MARGIN_SAMPLES], 0.0);
        assert_eq!(signal.samples[signal.signal_end() - 1], 999.0);
    }

    #[test]
    fn errors_display_without_panicking() {
        let err = PesqError::SignalTooShort { samples: 100 };
        assert!(err.to_string().contains("100"));
        assert!(!PesqError::NoUtterancesFound.to_string().is_empty());
        assert!(!PesqError::NotImplemented.to_string().is_empty());
    }
}
