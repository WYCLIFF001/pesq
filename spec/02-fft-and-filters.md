# 02. FFT conventions, windows, and filter tables

## 2.1 Fourier transform conventions

All transforms in the algorithm are of length a power of two.

Forward complex transform of length T: X[k] = sum over n of x[n] * e^(-i*2*pi*k*n/T), un-normalized.

Inverse complex transform: x[n] = (1/T) * sum over k of X[k] * e^(+i*2*pi*k*n/T). The 1/T factor is part of the inverse transform.

Real-input forward transform: given T real samples, output the T/2 + 1 complex bins of the forward transform, stored interleaved as pairs (real, imag) in positions 2k and 2k+1 for k = 0..T/2.

Real inverse transform: given the T/2 + 1 bins, reconstruct the full spectrum by conjugate symmetry (bin T-k = conjugate of bin k), run the inverse complex transform, and take the real parts, producing T samples.

Twiddle factors (cos and sin of 2*pi*k/T) are precomputed in f32. Any FFT implementation with identical mathematical conventions and f32 arithmetic reproduces the reference results; the internal butterfly schedule does not matter.

## 2.2 Transform sizes in use

| Where | Length |
|---|---|
| Fine and split time alignment frames | A = 512 |
| Perceptual model short-term frames | 256 |
| FFT-domain filters (1.3.1) | R = smallest power of two >= (N - 4800 + P) |
| Coarse log-VAD correlation | 2 * R, with R = smallest power of two >= max of the two window counts |
| Bad-interval re-alignment correlation | R = smallest power of two >= 2 * (number of samples in the search segment) |

The real-input transforms are used in all of these, with the packing of 2.1.

## 2.3 Window functions

Hann window of length T: w[n] = 0.5 * (1 - cos(2*pi*n/T)) for n = 0..T-1.

Used in fine alignment (T = 512) and in the perceptual model frames (T = 256).

## 2.4 Curve evaluation rule (repeat of 1.3.1)

The filter curves below are tables of (frequency in Hz, gain in dB). Evaluating a curve at frequency q:

- If q <= first table frequency: linear extrapolation using the first and second table points.
- If q >= last table frequency: linear extrapolation using the second-to-last and last table points.
- Otherwise: linear interpolation on the segment bracketing q.

The gain applied at a bin is 10^((curve value at the bin frequency minus curve value at 1000 Hz) / 20), so every curve is normalized to 0 dB at 1000 Hz (evaluated on the curve itself, not on the raw table).

## 2.5 Alignment filter curve (band-pass for level calibration)

Applied to both signals in the level normalization step (01 section 1.3). Frequency in Hz, gain in dB:

| Hz | dB |
|---|---|
| 0 | -500 |
| 50 | -500 |
| 100 | -500 |
| 125 | -500 |
| 160 | -500 |
| 200 | -500 |
| 250 | -500 |
| 300 | -500 |
| 350 | 0 |
| 400 | 0 |
| 500 | 0 |
| 600 | 0 |
| 630 | 0 |
| 800 | 0 |
| 1000 | 0 |
| 1250 | 0 |
| 1600 | 0 |
| 2000 | 0 |
| 2500 | 0 |
| 3000 | 0 |
| 3250 | 0 |
| 3500 | -500 |
| 4000 | -500 |
| 5000 | -500 |
| 6300 | -500 |
| 8000 | -500 |

Effect: flat 0 dB from 350 to 3250 Hz with a transition band at 300 to 350 Hz, suppressing everything below 300 Hz and above 3500 Hz.

## 2.6 IRS receive filter curve

Applied to both signals after level normalization (01 section 1.4). Frequency in Hz, gain in dB:

| Hz | dB |
|---|---|
| 0 | -200 |
| 50 | -40 |
| 100 | -20 |
| 125 | -12 |
| 160 | -6 |
| 200 | 0 |
| 250 | 4 |
| 300 | 6 |
| 350 | 8 |
| 400 | 10 |
| 500 | 11 |
| 600 | 12 |
| 700 | 12 |
| 800 | 12 |
| 1000 | 12 |
| 1300 | 12 |
| 1600 | 12 |
| 2000 | 12 |
| 2500 | 12 |
| 3000 | 12 |
| 3250 | 12 |
| 3500 | 4 |
| 4000 | -200 |
| 5000 | -200 |
| 6300 | -200 |
| 8000 | -200 |

This is the receive side of the Intermediate Reference System band shaping used by the model. The curve value at 1000 Hz is 12 dB, so after the normalization of 2.4 the passband gain at 1000 Hz is exactly 0 dB.

## 2.7 Input IIR filter (8 cascaded second-order sections)

Applied to the working buffers in 01 section 1.6. Each section has five coefficients (b0, b1, b2, a1, a2) and the difference equations:

w[n] = x[n] - a1 * w[n-1] - a2 * w[n-2]
y[n] = b0 * w[n] + b1 * w[n-1] + b2 * w[n-2]

with w[-1] = w[-2] = 0 (zero initial state). Sections are applied in table order, each over the whole buffer, in place (the output of one section is the input of the next). All coefficients and the state are f32.

Equivalent transfer function per section: H(z) = (b0 + b1*z^-1 + b2*z^-2) / (1 + a1*z^-1 + a2*z^-2).

| Section | b0 | b1 | b2 | a1 | a2 |
|---|---|---|---|---|---|
| 0 | 0.885535424 | -0.885535424 | 0.000000000 | -0.771070709 | 0.000000000 |
| 1 | 0.895092588 | 1.292907193 | 0.449260174 | 1.268869037 | 0.442025372 |
| 2 | 4.049527940 | -7.865190042 | 3.815662102 | -1.746859852 | 0.786305963 |
| 3 | 0.500002353 | -0.500002353 | 0.000000000 | 0.000000000 | 0.000000000 |
| 4 | 0.565002834 | -0.241585934 | -0.306009671 | 0.259688659 | 0.249979657 |
| 5 | 2.115237288 | 0.919935084 | 1.141240051 | -1.587313419 | 0.665935315 |
| 6 | 0.912224584 | -0.224397719 | -0.641121413 | -0.246029464 | -0.556720590 |
| 7 | 0.444617727 | -0.307589321 | 0.141638062 | -0.996391149 | 0.502251622 |

Section 3 is a pure scaling of 0.5 on the difference term (b0 = 0.500002353, b1 = -0.500002353, a1 = a2 = 0), i.e. a first difference scaled by 0.5. Reproduce the coefficients and the recurrence exactly; do not simplify.

## 2.8 Correlation via FFT (used by 01 and 04)

The coarse correlation of 01 section 1.9 is computed as: reverse the first sequence into a length-R buffer, forward real FFT of both buffers, plain binwise product (real*real - imag*imag and real*imag + imag*real per bin, i.e. the product of the two complex bin values, no conjugation), inverse real FFT, take the first (Vr + Vd - 1) outputs.

The fine alignment cross-correlation of 01 section 1.10 step 4b is the conjugate product: bin becomes conjugate(bin of first spectrum) times (bin of second spectrum).

The bad-interval re-alignment correlation of 04 uses the conjugate product form with an extra 1/R scaling of the first spectrum's bins before the product, which combined with the inverse transform's 1/R yields a normalized correlation value (the exact effect is restated in 04).
