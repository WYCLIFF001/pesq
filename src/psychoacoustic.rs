//! Perceptual model part 1: spectra, Bark warping, loudness, scaling
//! (spec 03).
//!
//! [`run_frame_loop`] runs sections 3.1 to 3.7 over the saved model
//! copies of the two signals (spec 01 section 1.4 step 2), equalized to
//! a common nominal length (spec 01 section 1.7), with the per-utterance
//! delays of spec 01 sections 1.10 to 1.13. It produces the per-frame
//! pitch power densities, loudness densities, silence flags, and
//! reference audible powers that spec 04 consumes.

mod table;

use crate::types::{FrameRange, SignalBuffer, Utterance};
use crate::types::{MARGIN_SAMPLES, PADDING_SAMPLES, WINDOW_SAMPLES};
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

/// Frame length F in samples, 32 ms (spec 03, 3.1).
pub const FRAME_LEN: usize = 256;

/// Frame hop Q in samples, 16 ms (spec 03, 3.1).
pub const FRAME_HOP: usize = 128;

/// Number of Bark bands B for the 8 kHz narrowband mode (spec 03, 3.3
/// and 3.8).
pub const NUM_BANDS: usize = 42;

/// Number of power spectrum bins the band grouping of spec 03 section
/// 3.3 consumes: bins 1..=128. Section 3.2 lists bins 0..127, but the
/// group counts of Table 1 sum to 128 starting at bin 1, so the array
/// holds 129 entries (bins 0..=128, including the Nyquist bin).
const NUM_POWER_BINS: usize = 129;

/// Absolute-sum threshold of the 5-sample silence skip probes
/// (spec 03, 3.1 steps 1 and 2).
const SILENCE_SKIP_THRESHOLD: f64 = 500.0;

/// Audibility factor of the silence flag (spec 03, 3.4 step 2).
const SILENCE_FLAG_FACTOR: f64 = 100.0;

/// Silence flag threshold on the audible power (spec 03, 3.4 step 2).
const SILENCE_FLAG_POWER: f64 = 1e7;

/// Audibility factor of the compensation averaging (spec 03, 3.5 step 1).
const COMPENSATION_FACTOR: f64 = 100.0;

/// Offset added to both per-band averages in the compensation ratio
/// (spec 03, 3.5 step 3).
const COMPENSATION_OFFSET: f64 = 1000.0;

/// Compensation factor clamp bounds (spec 03, 3.5 step 3).
const COMPENSATION_MIN: f64 = 0.01;
const COMPENSATION_MAX: f64 = 100.0;

/// Offset added to both audible powers in the local scale ratio
/// (spec 03, 3.7 step 2).
const SCALE_OFFSET: f64 = 5000.0;

/// Smoothing weights of the local scale (spec 03, 3.7 step 3).
const SCALE_SMOOTH_PREVIOUS: f64 = 0.2;
const SCALE_SMOOTH_CURRENT: f64 = 0.8;

/// Local scale clamp bounds (spec 03, 3.7 step 5).
const SCALE_MIN: f64 = 3e-4;
const SCALE_MAX: f64 = 5.0;

/// Bark width w[b] of Table 1 (spec 03, 3.8), the band weight of the
/// disturbance norm of spec 04 section 4.2.
pub fn bark_width(band: usize) -> f32 {
    table::BARK_BANDS[band].bark_width
}

/// Short-term power spectra of spec 03 section 3.2, holding the FFT
/// plan, the Hann window, and a scratch buffer shared by all frames.
struct Spectra {
    fft: std::sync::Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    scratch: Vec<Complex<f32>>,
}

impl Spectra {
    fn new() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        Self {
            fft: planner.plan_fft_forward(FRAME_LEN),
            window: crate::dsp::hann_window(FRAME_LEN),
            scratch: Vec::with_capacity(FRAME_LEN),
        }
    }

    /// Power spectrum of the frame starting at sample `start`: window
    /// with the Hann window, forward FFT, `real^2 + imag^2` per bin, and
    /// bin 0 forced to zero (spec 03, 3.2 steps 1 to 3). The output
    /// covers bins 0..=128 so the band grouping of 3.3 can consume 128
    /// bins starting at bin 1.
    fn power(&mut self, samples: &[f32], start: usize, out: &mut [f64; NUM_POWER_BINS]) {
        self.scratch.clear();
        self.scratch.extend(
            self.window
                .iter()
                .enumerate()
                .map(|(i, &weight)| Complex::new(samples[start + i] * weight, 0.0)),
        );
        self.fft.process(&mut self.scratch);
        for (bin, value) in self.scratch.iter().take(NUM_POWER_BINS).enumerate() {
            out[bin] = f64::from(value.re * value.re + value.im * value.im);
        }
        out[0] = 0.0;
    }
}

/// Start-of-signal silence skip of spec 03 section 3.1 step 1: the
/// first s whose 5-sample absolute sum at 2400 + s reaches 500, capped
/// at Nmax / 2.
fn silence_skip_start(reference: &SignalBuffer, n_max: usize) -> usize {
    let mut skip = 0;
    while skip < n_max / 2
        && (0..5)
            .map(|i| f64::from(reference.samples[MARGIN_SAMPLES + skip + i].abs()))
            .sum::<f64>()
            < SILENCE_SKIP_THRESHOLD
    {
        skip += 1;
    }
    skip
}

/// End-of-signal silence skip of spec 03 section 3.1 step 2: the mirror
/// of [`silence_skip_start`] probing backward from sample
/// `Nmax - 2400 + P - 1`.
fn silence_skip_end(reference: &SignalBuffer, n_max: usize) -> usize {
    let last = n_max - MARGIN_SAMPLES + PADDING_SAMPLES - 1;
    let mut skip = 0;
    while skip < n_max / 2
        && (0..5)
            .map(|i| f64::from(reference.samples[last - skip - i].abs()))
            .sum::<f64>()
            < SILENCE_SKIP_THRESHOLD
    {
        skip += 1;
    }
    skip
}

/// Frame range of spec 03 section 3.1: first processed frame
/// `skip_start / Q` (step 3), last processed frame
/// `(Nmax - 4800 + P - skip_end) / Q - 1` (step 4), integer divisions
/// (spec 01, 1.1). With a fully silent signal `stop` can fall below
/// `start`; the frame loop then processes an empty range.
pub fn frame_range(reference: &SignalBuffer) -> FrameRange {
    let n_max = reference.nominal_len;
    let skip_start = silence_skip_start(reference, n_max);
    let skip_end = silence_skip_end(reference, n_max);
    let start = skip_start / FRAME_HOP;
    let stop =
        ((n_max - 2 * MARGIN_SAMPLES + PADDING_SAMPLES - skip_end) / FRAME_HOP).saturating_sub(1);
    FrameRange {
        start,
        stop,
        skip_start,
        skip_end,
    }
}

/// Delay of the governing utterance of a reference start sample: the
/// last utterance u with `start[u] * W <= r0`, the first utterance's
/// delay when none governs (spec 03, 3.2 step 4). The per-utterance
/// delay is the fine delay of spec 01 section 1.10 step 8, which already
/// includes the coarse estimate.
fn governing_delay(r0: usize, utterances: &[Utterance]) -> i32 {
    utterances
        .iter()
        .rev()
        .find(|u| u.start_window * WINDOW_SAMPLES <= r0)
        .or_else(|| utterances.first())
        .map_or(0, |u| u.fine_delay)
}

/// Degraded frame start `d0 = r0 + delay`; `None` when the frame lies
/// out of bounds and the degraded spectrum must be all zeros
/// (spec 03, 3.2 step 4).
fn degraded_start(r0: usize, n_max: usize, utterances: &[Utterance]) -> Option<usize> {
    let d0 = r0 as i64 + i64::from(governing_delay(r0, utterances));
    if d0 <= 0 || d0 + FRAME_LEN as i64 >= (n_max + PADDING_SAMPLES) as i64 {
        return None;
    }
    Some(d0 as usize)
}

/// Warp a 129-bin power spectrum into the 42 Bark bands (spec 03,
/// section 3.3): band b sums the next n[b] bins starting at bin 1 (bin 0
/// is already zero), then multiplies by the correction factor c[b] and
/// by Sp. The sums accumulate in f64 and the result is stored as f32
/// (spec 01, 1.1).
pub fn warp_to_bark(power: &[f64; NUM_POWER_BINS]) -> [f32; NUM_BANDS] {
    let mut density = [0.0f32; NUM_BANDS];
    let mut bin = 1;
    for (band, row) in table::BARK_BANDS.iter().enumerate() {
        let sum: f64 = power[bin..bin + row.bins].iter().sum();
        bin += row.bins;
        density[band] = (sum * f64::from(row.correction) * table::PITCH_POWER_SCALE) as f32;
    }
    debug_assert_eq!(bin, NUM_POWER_BINS);
    density
}

/// Audible power of a frame with a factor (spec 03, 3.4 step 1): the
/// sum over bands 1..=41 (band 0 excluded) of the densities that exceed
/// `factor * t[b]`.
pub fn audible_power(density: &[f32], factor: f64) -> f64 {
    let mut total = 0.0f64;
    for (band, &p) in density.iter().enumerate().skip(1) {
        if f64::from(p) > factor * f64::from(table::BARK_BANDS[band].threshold) {
            total += f64::from(p);
        }
    }
    total
}

/// Zwicker loudness density of one band (spec 03, 3.6), including the
/// loudness scaling Sl of step 4. Computed in f64 and stored as f32
/// (spec 01, 1.1).
pub fn zwicker_loudness(pitch_power: f32, band: usize) -> f32 {
    let row = &table::BARK_BANDS[band];
    let threshold = f64::from(row.threshold);
    let p = f64::from(pitch_power);
    if p <= threshold {
        return 0.0;
    }
    // Low-band correction (step 1): below 4 Bark, h = 6 / (bark + 2),
    // capped at 2, raised to the power 0.15.
    let h = (if row.bark_centre < 4.0 {
        (6.0 / (f64::from(row.bark_centre) + 2.0)).min(2.0)
    } else {
        1.0
    })
    .powf(0.15);
    // Modified Zwicker exponent (step 2) and the power law (step 3).
    let z = 0.23 * h;
    let loudness = (threshold / 0.5).powf(z) * ((0.5 + 0.5 * p / threshold).powf(z) - 1.0);
    (loudness * table::LOUDNESS_SCALE) as f32
}

/// Compensation factor of spec 03 section 3.5 step 3: the ratio of the
/// per-band averages with a +1000 offset on both sides, clamped to
/// [0.01, 100].
pub fn compensation_factor(avg_ref: f64, avg_deg: f64) -> f64 {
    ((avg_deg + COMPENSATION_OFFSET) / (avg_ref + COMPENSATION_OFFSET))
        .clamp(COMPENSATION_MIN, COMPENSATION_MAX)
}

/// One step of the local gain scaling of spec 03 section 3.7 steps 2 to
/// 5. The pair is (unclamped, clamped): step 4 stores the unclamped
/// value as the next frame's previous scale, before the clamp of step 5.
pub fn local_scale(a_ref: f64, a_deg: f64, previous: f64, frame: usize) -> (f64, f64) {
    let raw = (a_ref + SCALE_OFFSET) / (a_deg + SCALE_OFFSET);
    let unclamped = if frame > 0 {
        SCALE_SMOOTH_PREVIOUS * previous + SCALE_SMOOTH_CURRENT * raw
    } else {
        raw
    };
    (unclamped, unclamped.clamp(SCALE_MIN, SCALE_MAX))
}

/// Output of the perceptual model frame loop (spec 03, sections 3.1 to
/// 3.7), the input to the disturbance stage of spec 04.
///
/// The density arrays are flat with `frame * NUM_BANDS + band` indexing
/// and hold `frame_stop + 1` entries (spec 03, 3.1 step 5). Pitch
/// densities cover frames 0..=frame_stop because the compensation
/// averages of 3.5 span that range; loudness and audible power are only
/// produced for the processed range `[frame_start, frame_stop]` and are
/// zero before it.
#[derive(Debug, Clone)]
pub struct PerceptualModel {
    /// Processed frame range of spec 03 section 3.1.
    pub frame_range: FrameRange,
    /// Reference pitch power densities after the compensation of spec 03
    /// section 3.5.
    pub pitch_ref: Vec<f32>,
    /// Degraded pitch power densities after the local gain scaling of
    /// spec 03 section 3.7.
    pub pitch_deg: Vec<f32>,
    /// Reference loudness densities (spec 03, 3.6).
    pub loudness_ref: Vec<f32>,
    /// Degraded loudness densities (spec 03, 3.6), from the scaled
    /// degraded densities (spec 03, 3.7 step 6).
    pub loudness_deg: Vec<f32>,
    /// Silence flag per frame (spec 03, 3.4 step 2).
    pub silence_flags: Vec<bool>,
    /// Reference audible power per frame, stored by spec 03 section 3.7
    /// step 7 and consumed by spec 04 section 4.6.
    pub audible_ref: Vec<f32>,
}

impl PerceptualModel {
    /// Stored frame count: `frame_stop + 1` (spec 03, 3.1 step 5).
    pub fn frame_count(&self) -> usize {
        self.silence_flags.len()
    }

    /// Reference pitch power density of one frame and band.
    pub fn pitch_ref_at(&self, frame: usize, band: usize) -> f32 {
        self.pitch_ref[frame * NUM_BANDS + band]
    }

    /// Degraded pitch power density of one frame and band.
    pub fn pitch_deg_at(&self, frame: usize, band: usize) -> f32 {
        self.pitch_deg[frame * NUM_BANDS + band]
    }

    /// Reference loudness density of one frame and band.
    pub fn loudness_ref_at(&self, frame: usize, band: usize) -> f32 {
        self.loudness_ref[frame * NUM_BANDS + band]
    }

    /// Degraded loudness density of one frame and band.
    pub fn loudness_deg_at(&self, frame: usize, band: usize) -> f32 {
        self.loudness_deg[frame * NUM_BANDS + band]
    }
}

/// Run the frame loop of spec 03: frame range (3.1), power spectra
/// (3.2), Bark warping (3.3), audibility and silence flags (3.4),
/// frequency response compensation (3.5), Zwicker loudness (3.6), and
/// local gain scaling (3.7). The disturbance steps of spec 04 continue
/// from the returned [`PerceptualModel`].
///
/// The inputs are the saved model copies of spec 01 section 1.4 step 2,
/// equalized to the common nominal length (spec 01, 1.7), and the
/// per-utterance delays of spec 01 sections 1.10 to 1.13.
pub fn run_frame_loop(
    reference: &SignalBuffer,
    degraded: &SignalBuffer,
    utterances: &[Utterance],
) -> PerceptualModel {
    let n_max = reference.nominal_len;
    let frame_range = frame_range(reference);
    let frame_count = frame_range.stop + 1;
    let mut spectra = Spectra::new();

    let mut pitch_ref = vec![0.0f32; frame_count * NUM_BANDS];
    let mut pitch_deg = vec![0.0f32; frame_count * NUM_BANDS];
    let mut silence_flags = vec![false; frame_count];
    let mut ref_sums = [0.0f64; NUM_BANDS];
    let mut deg_sums = [0.0f64; NUM_BANDS];
    let mut ref_power = [0.0f64; NUM_POWER_BINS];
    let mut deg_power = [0.0f64; NUM_POWER_BINS];

    // First pass: spectra, warping, silence flags, and the per-band sums
    // of the compensation averages (spec 03, 3.2 to 3.5 step 1).
    for frame in 0..=frame_range.stop {
        let r0 = MARGIN_SAMPLES + frame * FRAME_HOP;
        spectra.power(&reference.samples, r0, &mut ref_power);
        let ref_density = warp_to_bark(&ref_power);
        let deg_density = match degraded_start(r0, n_max, utterances) {
            Some(d0) => {
                spectra.power(&degraded.samples, d0, &mut deg_power);
                warp_to_bark(&deg_power)
            }
            None => [0.0f32; NUM_BANDS],
        };
        silence_flags[frame] =
            audible_power(&ref_density, SILENCE_FLAG_FACTOR) < SILENCE_FLAG_POWER;
        for band in 0..NUM_BANDS {
            pitch_ref[frame * NUM_BANDS + band] = ref_density[band];
            pitch_deg[frame * NUM_BANDS + band] = deg_density[band];
            if silence_flags[frame] {
                continue;
            }
            let limit = COMPENSATION_FACTOR * f64::from(table::BARK_BANDS[band].threshold);
            if f64::from(ref_density[band]) > limit {
                ref_sums[band] += f64::from(ref_density[band]);
            }
            if f64::from(deg_density[band]) > limit {
                deg_sums[band] += f64::from(deg_density[band]);
            }
        }
    }

    // Compensation: per-band averages over the fixed divisor of spec 03
    // section 3.5 step 1, the clamped ratio of step 3, applied to every
    // reference frame (step 4).
    let divisor = (((n_max - 2 * MARGIN_SAMPLES + PADDING_SAMPLES) / FRAME_HOP) - 1) as f64;
    for band in 0..NUM_BANDS {
        let factor = compensation_factor(ref_sums[band] / divisor, deg_sums[band] / divisor);
        for frame in 0..=frame_range.stop {
            pitch_ref[frame * NUM_BANDS + band] =
                (f64::from(pitch_ref[frame * NUM_BANDS + band]) * factor) as f32;
        }
    }

    // Second pass over the processed range: local gain scaling and
    // loudness (spec 03, 3.6 and 3.7). The "frame > 0" condition of 3.7
    // step 3 uses the absolute frame index, so with a nonzero frame
    // start the first processed frame is smoothed against the initial
    // previous scale of 1 (step 3, initial state).
    let mut loudness_ref = vec![0.0f32; frame_count * NUM_BANDS];
    let mut loudness_deg = vec![0.0f32; frame_count * NUM_BANDS];
    let mut audible_ref = vec![0.0f32; frame_count];
    let mut previous_scale = 1.0;
    // The range governs the iteration count (empty when stop < start);
    // the zip writes the stored audible power without indexing by frame.
    for (frame, audible) in
        (frame_range.start..=frame_range.stop).zip(audible_ref.iter_mut().skip(frame_range.start))
    {
        let base = frame * NUM_BANDS;
        let a_ref = audible_power(&pitch_ref[base..base + NUM_BANDS], 1.0);
        let a_deg = audible_power(&pitch_deg[base..base + NUM_BANDS], 1.0);
        let (unclamped, clamped) = local_scale(a_ref, a_deg, previous_scale, frame);
        previous_scale = unclamped;
        *audible = a_ref as f32;
        for band in 0..NUM_BANDS {
            pitch_deg[base + band] *= clamped as f32;
        }
        for band in 0..NUM_BANDS {
            loudness_ref[base + band] = zwicker_loudness(pitch_ref[base + band], band);
            loudness_deg[base + band] = zwicker_loudness(pitch_deg[base + band], band);
        }
    }

    PerceptualModel {
        frame_range,
        pitch_ref,
        pitch_deg,
        loudness_ref,
        loudness_deg,
        silence_flags,
        audible_ref,
    }
}

#[cfg(test)]
mod tests;
