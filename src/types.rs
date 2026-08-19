//! Shared data structures and constants implied by the specification.
//!
//! This module holds the types that more than one stage of the algorithm
//! exchanges: the signal buffer layout of spec 01 section 1.2, the voice
//! activity detection output of spec 01 section 1.8, the utterance and
//! delay structures of spec 01 sections 1.9 to 1.13, and the frame range
//! of spec 03 section 3.1. Stage-specific tables live in the module that
//! uses them (`dsp` for spec 02, `psychoacoustic` for spec 03).

use std::fmt;

/// Sample rate the P.862 pipeline operates at: 8 kHz (narrowband) or
/// 16 kHz (narrowband or wideband, spec 06 section 6.2). Every
/// rate-dependent constant of specs 01 to 05 and spec 06 section 6.4
/// derives from this value, so all processing is rate-selected, not
/// mode-selected (spec 06, 6.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rate {
    /// Native narrowband model rate (spec 01, table 1.1).
    Rate8k,
    /// Wideband rate, shared by narrowband and wideband mode
    /// (spec 06, 6.2 and 6.4).
    Rate16k,
}

impl Rate {
    /// Sample rate f in Hz (spec 06, 6.4).
    pub const fn sample_rate(self) -> usize {
        match self {
            Self::Rate8k => 8000,
            Self::Rate16k => 16000,
        }
    }

    /// Window length W in samples used by the VAD analysis: 4 ms at both
    /// rates (spec 01, table 1.1 and spec 06, 6.4).
    pub const fn window_samples(self) -> usize {
        match self {
            Self::Rate8k => 32,
            Self::Rate16k => 64,
        }
    }

    /// Margin M in windows on each side of the nominal signal
    /// (spec 01, table 1.1 and spec 06, 6.4).
    pub const fn margin_windows(self) -> usize {
        75
    }

    /// Margin in samples on each side of the nominal signal,
    /// `M * W` (spec 01, table 1.1).
    pub const fn margin_samples(self) -> usize {
        self.margin_windows() * self.window_samples()
    }

    /// Data padding P of 320 ms in samples (spec 01, table 1.1).
    pub const fn padding_samples(self) -> usize {
        match self {
            Self::Rate8k => 2560,
            Self::Rate16k => 5120,
        }
    }

    /// FFT length A used for fine time alignment (spec 01, table 1.1).
    pub const fn align_fft_len(self) -> usize {
        match self {
            Self::Rate8k => 512,
            Self::Rate16k => 1024,
        }
    }

    /// Minimum input length, `f / 4` samples (250 ms), below which
    /// processing stops with an error (spec 01, 1.2 step 5 and
    /// spec 06, 6.2 item 4).
    pub const fn min_input_samples(self) -> usize {
        match self {
            Self::Rate8k => 2000,
            Self::Rate16k => 4000,
        }
    }

    /// Model frame length F in samples, 32 ms (spec 03, 3.1).
    pub const fn frame_len(self) -> usize {
        match self {
            Self::Rate8k => 256,
            Self::Rate16k => 512,
        }
    }

    /// Model frame hop Q in samples, 16 ms (spec 03, 3.1).
    pub const fn frame_hop(self) -> usize {
        match self {
            Self::Rate8k => 128,
            Self::Rate16k => 256,
        }
    }

    /// Number of Bark bands B (spec 03, 3.3 and 3.8; spec 06, 6.4.1).
    pub const fn num_bands(self) -> usize {
        match self {
            Self::Rate8k => 42,
            Self::Rate16k => 49,
        }
    }

    /// Number of power spectrum bins the band grouping consumes
    /// (spec 03, 3.2 and 3.3): bins 0..=F/2 - 1.
    pub const fn num_power_bins(self) -> usize {
        self.frame_len() / 2
    }

    /// Pitch power density scale Sp (spec 03, 3.3 step 3; spec 06, 6.4).
    pub const fn pitch_power_scale(self) -> f64 {
        match self {
            Self::Rate8k => 2.764_344e-5,
            Self::Rate16k => 6.910_853e-6,
        }
    }

    /// Loudness scale Sl (spec 03, 3.6 step 4): identical at both rates
    /// (spec 06, 6.4).
    pub const fn loudness_scale(self) -> f64 {
        1.866_055e-1
    }
}

/// The two rate values. [`SignalBuffer`] carries the rate of its
/// samples, and every stage derives its constants from that rate.
pub const RATE_8K: Rate = Rate::Rate8k;
pub const RATE_16K: Rate = Rate::Rate16k;

/// Sample rate in Hz that the P.862 model operates at (spec 01, table 1.1).
pub const SAMPLE_RATE_HZ: usize = RATE_8K.sample_rate();

/// Window length W in samples used by the VAD analysis (spec 01, table 1.1).
pub const WINDOW_SAMPLES: usize = RATE_8K.window_samples();

/// Margin M in windows on each side of the nominal signal (spec 01, table 1.1).
pub const MARGIN_WINDOWS: usize = RATE_8K.margin_windows();

/// Margin in samples on each side of the nominal signal,
/// `MARGIN_WINDOWS * WINDOW_SAMPLES` (spec 01, table 1.1).
pub const MARGIN_SAMPLES: usize = RATE_8K.margin_samples();

/// Data padding P of 320 ms in samples (spec 01, table 1.1).
pub const PADDING_SAMPLES: usize = RATE_8K.padding_samples();

/// FFT length A used for fine time alignment (spec 01, table 1.1).
pub const ALIGN_FFT_LEN: usize = RATE_8K.align_fft_len();

/// Minimum input length, `f / 4 = 2000` samples (250 ms), below which
/// processing stops with an error (spec 01, 1.2 step 5).
pub const MIN_INPUT_SAMPLES: usize = RATE_8K.min_input_samples();

/// Errors reported by [`pesq`](crate::pesq) and the processing stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PesqError {
    /// An input holds fewer than [`MIN_INPUT_SAMPLES`] samples
    /// (spec 01, 1.2 step 5).
    SignalTooShort { samples: usize },
    /// No utterance qualified for scoring (spec 01, 1.11 step 5).
    NoUtterancesFound,
}

impl fmt::Display for PesqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SignalTooShort { samples } => {
                write!(
                    f,
                    "input has {samples} samples, which is below the minimum of \
                     f/4 samples (250 ms) at the operating sample rate"
                )
            }
            Self::NoUtterancesFound => {
                write!(f, "no utterance found in the input signals")
            }
        }
    }
}

impl std::error::Error for PesqError {}

/// A mono signal buffer in the layout defined by spec 01 section 1.2
/// step 6: a margin of zeros, then the L signal samples, then
/// margin + padding zeros. At 8 kHz the margins are
/// [`MARGIN_SAMPLES`] samples each and the total length is `L + 7360`;
/// at 16 kHz the margins are 4800 samples each and the total length is
/// `L + 14720` (spec 06, 6.3). The nominal length N covers
/// `[0, L + 2 * margin)`.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalBuffer {
    /// Full buffer of `L + 2 * margin + P` samples.
    pub samples: Vec<f32>,
    /// Nominal length `N = L + 2 * margin`.
    pub nominal_len: usize,
    /// Number of PCM samples L held in the buffer.
    pub input_len: usize,
    /// Sample rate the buffer operates at; every stage derives its
    /// constants from this value (spec 06, 6.4).
    pub rate: Rate,
}

impl SignalBuffer {
    /// Build the margin layout of spec 01 section 1.2 step 6 from a slice
    /// of 8 kHz PCM samples, enforcing the minimum length check of
    /// step 5.
    pub fn from_pcm(pcm: &[i16]) -> Result<Self, PesqError> {
        Self::from_pcm_at(pcm, RATE_8K)
    }

    /// Build the margin layout from PCM samples at an explicit rate,
    /// enforcing the rate's minimum length check (spec 01, 1.2 step 5
    /// and spec 06, 6.2 item 4).
    pub fn from_pcm_at(pcm: &[i16], rate: Rate) -> Result<Self, PesqError> {
        let l = pcm.len();
        if l < rate.min_input_samples() {
            return Err(PesqError::SignalTooShort { samples: l });
        }
        let margin = rate.margin_samples();
        let nominal_len = l + 2 * margin;
        let mut samples = vec![0.0f32; nominal_len + rate.padding_samples()];
        for (i, &sample) in pcm.iter().enumerate() {
            samples[margin + i] = f32::from(sample);
        }
        Ok(Self {
            samples,
            nominal_len,
            input_len: l,
            rate,
        })
    }

    /// Index of the first signal sample (one margin before the PCM).
    pub const fn signal_start(&self) -> usize {
        self.rate.margin_samples()
    }

    /// Index one past the last signal sample.
    pub fn signal_end(&self) -> usize {
        self.rate.margin_samples() + self.input_len
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
        assert_eq!(
            signal.samples.len(),
            4000 + 2 * MARGIN_SAMPLES + PADDING_SAMPLES
        );
        assert!(signal.samples[..MARGIN_SAMPLES].iter().all(|&s| s == 0.0));
        assert!(
            signal.samples[signal.signal_end()..]
                .iter()
                .all(|&s| s == 0.0)
        );
        assert_eq!(signal.samples[MARGIN_SAMPLES], 0.0);
        assert_eq!(signal.samples[signal.signal_end() - 1], 999.0);
    }

    #[test]
    fn errors_display_without_panicking() {
        let err = PesqError::SignalTooShort { samples: 100 };
        assert!(err.to_string().contains("100"));
        assert!(!PesqError::NoUtterancesFound.to_string().is_empty());
    }
}
