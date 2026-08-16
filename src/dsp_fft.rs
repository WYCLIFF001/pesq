//! FFT procedures of spec 02 section 2.1, built on `rustfft`.
//!
//! The specification pins the transform conventions: forward transform
//! un-normalized, inverse transform carrying the 1/T factor, and the
//! real-input transforms stored as the interleaved (real, imag) pairs of
//! the bins 0..=T/2 (T + 2 floats of storage for a length T transform).
//! `rustfft` complex transforms provide both; this module adds the real
//! packing and the three correlation products of spec 02 section 2.8.

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

/// Forward real FFT of a power-of-two length `T` buffer.
/// Shared FFT planner: one lazily initialized planner per process, held
/// behind a mutex. `rustfft`'s planner caches one plan per (size,
/// direction) internally, so repeated calls with the same size reuse
/// the same `Arc` plan instead of rebuilding it per transform.
fn shared_planner() -> &'static std::sync::Mutex<FftPlanner<f32>> {
    use std::sync::{Mutex, OnceLock};
    static PLANNER: OnceLock<Mutex<FftPlanner<f32>>> = OnceLock::new();
    PLANNER.get_or_init(|| Mutex::new(FftPlanner::new()))
}

/// Forward complex FFT plan for `len`, from the shared planner cache.
pub(crate) fn forward_plan(len: usize) -> std::sync::Arc<dyn Fft<f32>> {
    shared_planner().lock().unwrap().plan_fft_forward(len)
}

/// Inverse complex FFT plan for `len`, from the shared planner cache.
pub(crate) fn inverse_plan(len: usize) -> std::sync::Arc<dyn Fft<f32>> {
    shared_planner().lock().unwrap().plan_fft_inverse(len)
}

///
/// Returns the T/2 + 1 complex bins of spec 02 section 2.1, interleaved
/// as (real, imag) pairs in positions 2k and 2k+1 for k = 0..=T/2, for
/// a total of T + 2 floats. The transform is un-normalized.
pub fn real_fft(input: &[f32]) -> Vec<f32> {
    let len = input.len();
    let fft = forward_plan(len);
    let mut buffer: Vec<Complex<f32>> = input.iter().map(|&re| Complex::new(re, 0.0)).collect();
    fft.process(&mut buffer);
    let mut packed = Vec::with_capacity(len + 2);
    for bin in buffer.iter().take(len / 2 + 1) {
        packed.push(bin.re);
        packed.push(bin.im);
    }
    packed
}

/// Inverse real FFT of a packed half-spectrum of the shape produced by
/// [`real_fft`].
///
/// The full spectrum is reconstructed by conjugate symmetry (bin T-k =
/// conjugate of bin k, spec 02 section 2.1), the inverse complex
/// transform runs, and the real parts are taken, producing T samples.
/// The 1/T factor of the inverse transform is applied (spec 02, 2.1).
pub fn inverse_real_fft(packed: &[f32]) -> Vec<f32> {
    let len = packed.len() - 2;
    let fft = inverse_plan(len);
    let mut buffer = vec![Complex::new(0.0, 0.0); len];
    for k in 0..=len / 2 {
        buffer[k] = Complex::new(packed[2 * k], packed[2 * k + 1]);
    }
    for k in 1..len / 2 {
        buffer[len - k] = buffer[k].conj();
    }
    fft.process(&mut buffer);
    // `rustfft` does not normalize its inverse transform, so the 1/T
    // factor of spec 02 section 2.1 is applied here.
    let scale = 1.0 / len as f32;
    buffer.iter().map(|bin| bin.re * scale).collect()
}

/// Hann window of length `len` (spec 02, 2.3):
/// `w[n] = 0.5 * (1 - cos(2*pi*n/T))` for n = 0..T-1.
pub fn hann_window(len: usize) -> Vec<f32> {
    let denominator = len as f32;
    (0..len)
        .map(|n| {
            let phase = std::f32::consts::TAU * n as f32 / denominator;
            0.5 * (1.0 - phase.cos())
        })
        .collect()
}

/// Correlate two sequences with the FFT procedure of spec 01 section
/// 1.9 step 3 and spec 02 section 2.8.
///
/// The first sequence is reversed into the front of a length R buffer
/// (R = 2 times the smallest power of two at least the longer length),
/// both buffers are transformed with [`real_fft`], the binwise product is
/// plain (no conjugation), and the inverse transform is taken. The
/// returned vector holds the first `first.len() + second.len() - 1`
/// outputs: `c[k] = sum over i of first[i] * second[i + k -
/// (first.len() - 1)]` with `second` read circularly at period R and
/// zero outside its length.
pub fn correlate(first: &[f32], second: &[f32]) -> Vec<f32> {
    if first.is_empty() || second.is_empty() {
        return Vec::new();
    }
    let max_len = first.len().max(second.len());
    let r = 2 * max_len.next_power_of_two();
    let mut x = vec![0.0f32; r];
    let mut y = vec![0.0f32; r];
    for (i, &value) in first.iter().enumerate() {
        x[first.len() - 1 - i] = value;
    }
    y[..second.len()].copy_from_slice(second);
    let spectrum_x = real_fft(&x);
    let spectrum_y = real_fft(&y);
    let product = complex_product(&spectrum_x, &spectrum_y);
    let correlation = inverse_real_fft(&product);
    correlation[..first.len() + second.len() - 1].to_vec()
}

/// Spectral cross-correlation of spec 01 section 1.10 step 4b and spec
/// 02 section 2.8: binwise `conjugate(first spectrum) times second
/// spectrum`, inverse transform, for two equal-length buffers.
pub fn spectral_cross_correlate(first: &[f32], second: &[f32]) -> Vec<f32> {
    assert_eq!(
        first.len(),
        second.len(),
        "cross-correlation needs equal lengths"
    );
    let spectrum_x = real_fft(first);
    let spectrum_y = real_fft(second);
    let product = conjugate_product(&spectrum_x, &spectrum_y);
    inverse_real_fft(&product)
}

/// Circular convolution of two equal-length real sequences, computed as
/// the plain binwise product of their packed spectra followed by the
/// inverse real transform (spec 01, 1.10 step 5 permits any equivalent
/// exact method).
pub fn circular_convolve(a: &[f32], b: &[f32]) -> Vec<f32> {
    assert_eq!(a.len(), b.len(), "circular convolution needs equal lengths");
    let spectrum_a = real_fft(a);
    let spectrum_b = real_fft(b);
    let product = complex_product(&spectrum_a, &spectrum_b);
    inverse_real_fft(&product)
}

/// Plain binwise product of two packed spectra (spec 02, 2.8).
fn complex_product(x: &[f32], y: &[f32]) -> Vec<f32> {
    let mut product = vec![0.0f32; x.len()];
    for k in 0..x.len() / 2 {
        let (x_re, x_im) = (x[2 * k], x[2 * k + 1]);
        let (y_re, y_im) = (y[2 * k], y[2 * k + 1]);
        product[2 * k] = x_re * y_re - x_im * y_im;
        product[2 * k + 1] = x_re * y_im + x_im * y_re;
    }
    product
}

/// Binwise conjugate-first product `conj(x) * y` of two packed spectra
/// (spec 01, 1.10 step 4b).
fn conjugate_product(x: &[f32], y: &[f32]) -> Vec<f32> {
    let mut product = vec![0.0f32; x.len()];
    for k in 0..x.len() / 2 {
        let (x_re, x_im) = (x[2 * k], x[2 * k + 1]);
        let (y_re, y_im) = (y[2 * k], y[2 * k + 1]);
        product[2 * k] = x_re * y_re + x_im * y_im;
        product[2 * k + 1] = x_re * y_im - x_im * y_re;
    }
    product
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-random f32 sequence in [-1, 1) for tests.
    fn noise(seed: u32, len: usize) -> Vec<f32> {
        let mut state = seed;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state as f32 / u32::MAX as f32) * 2.0 - 1.0
            })
            .collect()
    }

    #[test]
    fn real_fft_of_a_delta_is_flat() {
        let mut delta = vec![0.0f32; 64];
        delta[0] = 1.0;
        let spectrum = real_fft(&delta);
        assert_eq!(spectrum.len(), 66);
        for k in 0..=32 {
            assert!((spectrum[2 * k] - 1.0).abs() < 1e-5, "bin {k}");
            assert!(spectrum[2 * k + 1].abs() < 1e-5, "bin {k}");
        }
    }

    #[test]
    fn inverse_real_fft_round_trips_with_the_one_over_t_factor() {
        let signal = noise(7, 64);
        let restored = inverse_real_fft(&real_fft(&signal));
        for (i, (&a, &b)) in signal.iter().zip(restored.iter()).enumerate() {
            assert!((a - b).abs() < 1e-4, "sample {i}");
        }
    }

    #[test]
    fn real_fft_of_a_sine_concentrates_on_one_bin() {
        let len = 64;
        let signal: Vec<f32> = (0..len)
            .map(|n| (std::f32::consts::TAU * 3.0 * n as f32 / len as f32).sin())
            .collect();
        let spectrum = real_fft(&signal);
        for k in 0..=len / 2 {
            let magnitude = (spectrum[2 * k].powi(2) + spectrum[2 * k + 1].powi(2)).sqrt();
            if k == 3 {
                assert!((magnitude - len as f32 / 2.0).abs() < 1e-2);
            } else {
                assert!(magnitude < 1e-3, "bin {k} magnitude {magnitude}");
            }
        }
    }

    #[test]
    fn correlate_finds_a_shifted_impulse() {
        // first has its impulse at position 2, second at position 0; the
        // correlation peak sits at k = 2 per spec 01, 1.9 step 3.
        let first = [0.0, 0.0, 1.0, 0.0, 0.0];
        let second = [1.0, 0.0, 0.0, 0.0, 0.0];
        let correlation = correlate(&first, &second);
        assert_eq!(correlation.len(), 9);
        for (k, &value) in correlation.iter().enumerate() {
            let expected = if k == 2 { 1.0 } else { 0.0 };
            assert!((value - expected).abs() < 1e-4, "lag {k}");
        }
    }

    #[test]
    fn spectral_cross_correlate_peaks_at_zero_for_identical_input() {
        let signal = noise(11, 128);
        let correlation = spectral_cross_correlate(&signal, &signal);
        let peak = correlation
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        assert_eq!(peak, 0);
    }

    #[test]
    fn circular_convolve_matches_the_direct_sum() {
        let a = noise(13, 16);
        let b = noise(17, 16);
        let fast = circular_convolve(&a, &b);
        let mut direct = [0.0f32; 16];
        for n in 0..16 {
            for k in 0..16 {
                direct[n] += a[k] * b[(n + 16 - k) % 16];
            }
        }
        for n in 0..16 {
            assert!((fast[n] - direct[n]).abs() < 1e-3, "sample {n}");
        }
    }
}
