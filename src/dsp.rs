//! Fourier transform conventions, windows, and filter tables (spec 02).
//!
//! This module is the public face of the two DSP submodules:
//! [`crate::dsp_fft`] holds the FFT conventions, the Hann window, and
//! the correlation products; [`crate::dsp_filters`] holds the dB curve
//! machinery,
//! the two filter curves, the input IIR cascade, and the FFT-domain
//! filter application of spec 01 section 1.3.1.

pub use crate::dsp_fft::{
    circular_convolve, correlate, hann_window, inverse_real_fft, real_fft, spectral_cross_correlate,
};
pub use crate::dsp_filters::{
    ALIGNMENT_CURVE, Curve, IIR_SECTIONS, IRS_RECEIVE_CURVE, apply_filter_curve, apply_input_iir,
};
