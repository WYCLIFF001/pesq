# 01. Input handling, preprocessing, and time alignment

Scope: narrowband P.862 at an 8 kHz sample rate. Wideband mode is out of scope.

## 1.1 Notation and constants

| Symbol | Value | Meaning |
|---|---|---|
| f | 8000 | sample rate in Hz |
| W | 32 | window length in samples (4 ms), the VAD analysis unit |
| M | 75 | margin in windows, 2400 samples |
| P | 2560 | data padding of 320 ms in samples |
| L | varies | number of PCM samples in the input file |
| N | L + 4800 | nominal signal length = L + 2*M*W |
| A | 512 | FFT length used for fine time alignment |

Integer division in the formulas below truncates toward zero (rounds toward zero), including for negative operands. Comparisons and thresholds are exact as written.

Numerical conventions for bit-close reproduction of the reference results:

- Sample-domain arithmetic (filters, scaling, VAD, FFT) uses IEEE 754 binary32 (f32).
- Sums of squares and loudness computations accumulate in binary64 (f64) and are stored back as f32.
- FFT twiddle factors are f32 cosine and sine values.
- Curve interpolation (1.3.1) is performed in f64; the linear gain factor is 10 raised to the power of (interpolated dB divided by 20), computed with double-precision pow and rounded once to f32.
- The VAD logarithm is natural log computed in f32.
- Small deviations from these conventions are acceptable: the conformance tolerance (see CONFORMANCE.md) is far wider than the differences between reasonable libm implementations, but keeping the conventions above removes all algorithmic sources of mismatch.

## 1.2 Input format

1. Input is mono 16-bit linear PCM, signed, in the machine byte order of the file.
2. If the file name ends in ".wav" or ".WAV", skip the first 44 bytes (22 samples). Names ending in ".raw", ".src", or ".s" have no header.
3. An optional byte-swap applies to all samples for big-endian files.
4. The number of PCM samples is L = floor(file size in bytes / 2) minus the number of skipped header samples.
5. Minimum length check: L must be at least f/4 = 2000 samples (250 ms). Otherwise processing stops with an error.
6. Build the sample buffer as follows: 2400 zeros, then the L samples, then 4960 zeros. The buffer length is L + 7360 = N + P. Positions [2400, 2400 + L) hold the signal; the nominal length N covers [0, N).

## 1.3 Level normalization

1. Copy the raw signal buffer into a scratch buffer of length N + P.
2. Apply the band-pass alignment filter to the scratch buffer (procedure 1.3.1, using the alignment curve table from 02).
3. Compute the mean power of the filtered scratch buffer over the interval [2400, N - 2400 + P), i.e. the sum of squares of the samples in that interval, divided by the divisor (Nmax - 4800 + P), where Nmax is the larger of the two nominal lengths (reference and degraded).
4. The calibration scale is sqrt(1e7 / mean power).
5. Multiply the first N samples of the original signal buffer (not the scratch copy) by the scale. The padding is left unchanged (it is zero).
6. Perform steps 1 to 5 independently for the reference and for the degraded signal, using the same Nmax in step 3.

The target level 1e7 corresponds to the published P.862 calibration (an average power of 10^7 after 300 Hz high-pass filtering, equivalent to a listening level of about 79 dB SPL).

1.3.1 FFT-domain filter application. Given a buffer, a region start S = 2400, a region length n = N - 4800 + P, and a dB curve with pairs (frequency in Hz, gain in dB):

1. Let R be the smallest power of two at least n. Prepare a real array of length R, filled with the n samples starting at S, then zeros. (The packed spectrum of the real FFT needs R/2 + 1 complex bins, i.e. R + 2 floats of storage.)
2. Compute the forward real FFT (see 02 for conventions).
3. For each bin k from 0 to R/2 inclusive: bin frequency is k*f/R; the gain in dB is the curve value at that frequency minus the curve value at 1000 Hz; the linear factor is 10^(gain/20). Multiply both stored components (real and imaginary) of the bin by the factor.
4. Compute the inverse real FFT and divide as defined in 02.
5. Write the first n values of the result back into the buffer starting at S. Values beyond n are discarded.

Curve evaluation rule (linear interpolation): if the query frequency is at or below the first table frequency, extrapolate using the segment between the first and second table points; if at or above the last table frequency, extrapolate using the segment between the second-to-last and last table points; otherwise use the segment bracketing the query frequency.

## 1.4 IRS receive filtering

1. Apply procedure 1.3.1 to both signals using the IRS receive curve table from 02 (the same region start S = 2400 and region length n = N - 4800 + P of each signal).
2. Save a copy of each signal's full buffer (N + P samples) at this point. These saved copies are the inputs to the perceptual model (03). All remaining preprocessing steps operate on the working buffers only.

## 1.5 DC removal

1. Compute the mean of the working buffer over the interval [2400, N - 2400), dividing the sum by N (the nominal length, not the interval length). This is intentional and must be reproduced.
2. Subtract this mean from the samples in [2400, N - 2400) only. The margins are untouched.
3. At the start of the interval, for k from 0 to W-1: multiply sample 2400 + k by (0.5 + k)/W.
4. At the end of the interval, for k from 0 to W-1: multiply sample (N - 2400 - 1 - k) by (0.5 + k)/W.
5. Do this for both signals.

## 1.6 Input IIR filter

1. Apply the 8-section IIR cascade from 02 to the entire working buffer of N + P samples, in place, with zero initial state in each section.
2. Do this for both signals.
3. The working buffers are used only for VAD computation and time alignment. The perceptual model (03 and 04) uses the saved copies from 1.4 step 2, and the delays estimated here are applied to those copies.

## 1.7 Length equalization

After alignment and before the perceptual model: if the two nominal lengths differ, extend the shorter signal's saved buffer with zeros so that both saved buffers have length Nmax + P. (The shorter buffer keeps its own samples in [0, N_short + P); the new tail is zero.)

## 1.8 Voice activity detection (VAD)

Input: the working buffer of length N + P (after 1.5 and 1.6). The analysis uses only the first N samples, split into N/W windows of W samples each. The window count is V = N/W (integer; equals L/W + 150).

1. Window energy: for each window v, e[v] = mean of squares of its W samples.
2. Initial threshold t = mean of e over all windows.
3. Noise floor m = maximum of e over all windows; if m > 0 then m = m * 1e-4, else m = 1. Then floor every window: e[v] = max(e[v], m).
4. Iterate 12 times: let Q be the set of windows with e[v] <= t. Compute the noise mean as the mean of e over Q and the noise std as the population standard deviation of e over Q (square root of the mean squared deviation), accumulating both sums from 0, so an empty Q would give 0 for both. Then update t = 1.001 * (noise mean + 2 * noise std) unconditionally, on every iteration; with an empty Q, t would become 0. The empty case is unreachable: the floor of step 3 makes every window value at least m, and the minimum window is always in Q, because t starts at the mean of all windows (at least the minimum) and every update produces t = 1.001 * (noise mean + 2 * noise std), which is at least 1.001 times the minimum, as the minimum window lies in Q and the noise std is never negative.
5. After the iterations: signal level = mean of e over windows with e[v] > t, or 0 if there are none (in which case t is set to -1). Noise level = mean of e over the remaining windows, or 1 if none remain.
6. Sign encoding: negate e[v] for every window with e[v] <= t. (Positive = speech, negative = noise.)
7. Force e[0] = -m and e[V-1] = -m.
8. Remove short speech runs: for every maximal run of consecutive positive windows, if the run length is at most 4 windows, negate all its windows.
9. Low-energy run removal: if signal level >= 1000 * noise level, then for every maximal positive run, if the sum of e over the run is less than 3 * t * run length, negate the whole run.
10. Gap joining: for every pair of consecutive positive runs separated by a gap of at most 50 windows, set the gap windows to +m.
11. If after step 10 no window is positive, take the absolute value of every window, then force e[0] = -m and e[V-1] = -m.
12. Edge smoothing: scan v from 3 upward while v < V-2, in one pass with the following control flow per scan step: (a) if e[v] > 0 and e[v-2] <= 0, set e[v-2] = 0.1 * e[v] and e[v-1] = 0.3 * e[v], then v = v + 1; (b) then, with the possibly updated v, if e[v] <= 0 and e[v-1] > 0, set e[v] = 0.3 * e[v-1] and e[v+1] = 0.1 * e[v-1], then v = v + 3; (c) then v = v + 1.
13. Zero the negatives: e[v] = max(e[v], 0) for all windows.
14. If t <= 0 after step 5, set t = m. Log-domain VAD: l[v] = 0 if e[v] <= t, else natural log of (e[v]/t).

The pair (e, l) is produced for both signals.

## 1.9 Coarse delay estimation (log-VAD correlation)

1. For the whole-signal pass: let x be the reference log-VAD array of length Vr and y the degraded log-VAD array of length Vd.
2. If both lengths exceed 1: compute the FFT-based cross-correlation defined below. Otherwise the lag estimate is 0.
3. Correlation output: the first Vr + Vd - 1 values of the FFT procedure in 02, which computes c[k] = sum over i of x[i] * y[i + k - (Vr - 1)] for k in [0, Vr + Vd - 2], with y read circularly at period R and treated as zero outside [0, Vd). The procedure is: reverse x into the front of a length-R buffer (R = 2 times the smallest power of two at least max(Vr, Vd)), forward real FFT, likewise forward real FFT of y in a second length-R buffer, take the plain binwise product (no conjugation), inverse real FFT, and read the first Vr + Vd - 1 values.
4. Find the index k maximizing c[k] over the output range. Initialization: best value 0 and best index Vr - 1, updated only on strict improvement. Thus if every c[k] <= 0 (including the all-zero case), the result is k = Vr - 1, i.e. lag 0.
5. The lag in samples is (k - Vr + 1) * W. Positive lag means the degraded signal is delayed relative to the reference.
6. Whole-signal pass: store this as the coarse delay estimate.
7. Per-utterance pass (used to seed fine alignment): given a search window [s0, s1] (1.11) and a seed delay d: let x be the reference log-VAD over windows [s0, s1) (length nr = s1 - s0) and let sd = s0 + d/W (integer division). If sd < 0, set s0 = -d/W (integer division) and sd = 0, and re-derive nr = s1 - s0. The degraded range is [sd, sd + nd) with nd = nr, shortened so that sd + nd <= Vd. Run the correlation of steps 3 to 5 with these sequences (if nr <= 1 or nd <= 1, the lag is 0). The per-utterance coarse estimate is (k - nr + 1)*W + d.

## 1.10 Fine time alignment (per utterance)

Input: the working buffers, an utterance search start window s0, search end window s1, and an initial delay estimate d0 in samples.

1. Window function: Hann of length A: w[n] = 0.5 * (1 - cos(2*pi*n/A)).
2. Allocate a histogram array H of A slots, all zero. Let Hsum = 0 (not used until step 5).
3. Set the reference cursor to s0*W and the degraded cursor to s0*W + d0. If the degraded cursor is negative, set the reference cursor to -d0, replacing the value derived from s0 rather than adding to it, and set the degraded cursor to 0. The offset between the two cursors remains d0. This replacement matches the cursor clamps of 1.9 step 7 and 1.13 step 7.
4. While the degraded cursor + A <= degraded nominal length N_deg and the reference cursor + A <= s1*W:
   a. Take A reference samples at the reference cursor times the window, forward real FFT, keep as X1. Take A degraded samples at the degraded cursor times the window, forward real FFT, keep as X2.
   b. Spectral cross-correlation: replace X1 binwise by conjugate(X1) times X2.
   c. Inverse real FFT of X1. Take absolute values.
   d. Let v be 0.99 times the maximum of these absolute values. For each lag position p where the absolute value exceeds v, add v^0.125 to H[p].
   e. Advance both cursors by A/4 (75% overlap).
5. Smoothing: convolve H circularly (period A) with a symmetric triangular kernel of radius K = A/64 = 8, i.e. kernel[0] = 1 and kernel[k] = 1 - k/8 for k in 1..7, with kernel[A - k] = 1 - k/8. Compute the circular convolution with FFTs of length A (plain product, no conjugation) or any equivalent exact method.
6. Normalize: let Hsum = sum of H over all lag positions before normalization (sum of the raw histogram, captured before the smoothing of step 5). If Hsum > 0, divide every (smoothed) H value by Hsum and take absolute values; otherwise set all H to 0.
7. Find the peak: the lag p0 is the position of the maximum H value (first position wins ties by the scan order). If p0 >= A/2, replace p0 by p0 - A (negative lag, degraded ahead).
8. The fine delay for this utterance is d0 + p0. The confidence is the peak value H[p0].

## 1.11 Utterance search windows

Using the reference processed energy array (the e array of 1.8, as left by step 13; step 14 only derives the log-domain array l from it) and the coarse delay from 1.9:

1. Degradation bounds: b1 = 50 - (coarse delay / W) and b2 = (N_deg - coarse delay)/W - 50 (integer division).
2. Scan the reference windows in order, tracking runs of speech windows. Speech is defined by e[v] > 0 on the processed energy array, not by the log-domain VAD l[v]: a window with 0 < e[v] <= t has l[v] = 0 (it contributes nothing to the correlation of 1.9) but still counts as speech here and never ends a run. A window with e[v] == 0 is non-speech; negative values no longer exist after 1.8 step 13.
3. Run end: a run ends when the scan finds a non-speech window, or when the scan reaches the last window index. Let c be the trigger window: with a non-speech trigger the run is [a, c) and c is exclusive; with the last-window trigger c = V - 1 and the last window is inside the run, so the run is [a, c]. The run qualifies as an utterance if (c - a) >= 50, a < b2, and c > b1. For a run reaching the last window, the first condition is (V - 1 - a) >= 50.
4. For each qualifying run, record a search window: start = max(a - 75, 0), end = min(c + 75, V - 1).
5. If no run qualifies, there are no utterances and the model cannot proceed.

For each utterance in order: run the coarse correlation of 1.9 restricted to the search window (with the coarse delay as seed), then the fine alignment of 1.10. This yields, per utterance: a coarse estimate, a fine delay in samples, and a confidence.

## 1.12 Utterance boundaries

1. Re-scan the reference windows exactly as in 1.11, with speech defined by e[v] > 0 and the run-end trigger of 1.11 step 3; for each qualifying run record start a and end c, where c is the trigger window (exclusive for a non-speech trigger, V - 1 for a run reaching the last window).
2. Boundary merging: start[0] = 75; end[u-1] = V - 75 for the last utterance u-1. For u from 1 to u-1: the boundary between utterance u-1 and u is the midpoint (start[u] + end[u-1])/2 (integer division); set end[u-1] and start[u] to it.
3. Delay clamp at the left edge: if start[0]*W + delay[0] < 2400, set start[0] = 75 + (W - 1 - delay[0])/W. The left operand is start[0]*W, not (start[0] - 75)*W: step 2 has just set start[0] = 75, so the clamp triggers only when delay[0] < 0. Testing (start[0] - 75)*W + delay[0] < 2400 instead would trigger for nearly every positive delay and wrongly move start[0] below 75.
4. Delay clamp at the right edge: if end[u-1]*W + delay[u-1] > N_deg - 2400, set end[u-1] = (N_deg - delay[u-1])/W - 75.
5. Overlap fix, for u from 1 to u-1: let a = start[u]*W + delay[u] and b = end[u-1]*W + delay[u-1]. If a < b: let c = (a + b)/2 (integer division); set start[u] = (W - 1 + c - delay[u])/W and end[u-1] = (c - delay[u-1])/W.

## 1.13 Utterance splitting

For each utterance with at least 200 windows of actual speech (see below), a split is attempted while the utterance count stays below 50.

1. Speech extent: set start = utterance start and end = utterance end. Advance start while start < end and the reference energy value e[start] is 0. Retreat end while end > utterance start (the original start, not the advanced one) and the reference energy value e[end] is 0. Then increment end by one, so the speech end is exclusive and lies one window past the last speech window. The speech length is end - start (windows).
2. If the speech length is below 200 windows, no split is attempted for this utterance.
3. Breakpoint grid: D = A/(4*W) = 4. Step S = floor((0.801*speech_length + 40*D - 1)/(40*D)) * D. Pad = max(speech_length/10 (integer division), 75). Candidates: b[0] = speech_start + Pad, then b[j] = b[j-1] + S, while b[j] <= speech_end - Pad and j < 40. The candidate count is at most 40.
4. Per-candidate coarse estimates: for each candidate, run the coarse correlation (1.9) twice, once over [utterance start, candidate] and once over [candidate, utterance end], each seeded with the utterance's coarse estimate. Call the results the left estimate and right estimate.
5. Forward fine pass, in candidate order: find the first candidate whose forward confidence is not yet computed. Accumulate the histogram of 1.10 steps 1 to 7 with the following changes: the search runs from the utterance start toward the candidate (reference cursor starts at utterance start*W; the loop end condition is cursor + A <= candidate*W); the initial delay is the candidate's left estimate; the negative-degraded-cursor clamp of 1.10 step 3 applies as written there (if the degraded cursor is negative, the reference cursor is set to minus the initial delay, a replacement not an addition, and the degraded cursor is set to 0); the histogram accumulation is replaced by peak spreading (step 6 below). When the accumulation reaches this candidate's breakpoint, record for this candidate: forward delay = initial delay + folded peak of the raw histogram accumulated so far (first position wins ties by scan order; the spreading of step 6 provides the smoothing), and forward confidence = peak/Hsum (0 if Hsum <= 0), where Hsum accumulates per exceeding lag as in step 6. Then scan forward over the remaining candidates: for every later candidate with the same left estimate whose forward confidence is not yet computed, extend the same accumulation to that candidate's breakpoint and record that candidate's own forward delay and forward confidence from the histogram accumulated so far. Each candidate is recorded separately; the peak can move as more frames accumulate, so the recorded values can differ between candidates of one group. Candidates with a different left estimate are left uncomputed and are not recorded; the cursors keep advancing for a later same-estimate candidate, so the intervening frames are included in the accumulation. The next pass begins fresh, with a zero histogram, at the first candidate that is still uncomputed. (A single shared delay and confidence assigned to a whole group of candidates is wrong.)
6. Peak spreading (used in 5 and 7 only): after each frame correlation, let v be 0.99 times the frame maximum, where the frame maximum is of the absolute correlation values (the spectrum is absolute after 1.10 step 4c); for every lag position p with a value exceeding v, add u*(8 - |k|) to H[(p + k) mod A] for every k from -7 to 7, where u = v^0.125 / 8; and add v^0.125 to Hsum, once per exceeding lag position p. The spreading provides the smoothing: the separate circular smoothing of 1.10 step 5 is not applied in the split passes.
7. Backward fine pass: for the last candidate whose backward confidence is uncomputed and whose forward confidence exceeds the utterance confidence from 1.10: accumulate a histogram scanning backward from the utterance end toward the candidate (reference cursor starts at utterance end*W - A; if the degraded cursor + A exceeds the degraded nominal length, set the degraded cursor to N_deg - A and the reference cursor to that minus the seed; then step both cursors down by A/4 while the degraded cursor >= 0 and the reference cursor >= candidate*W), seeded with the candidate's right estimate, with peak spreading. Record per candidate exactly as in step 5: when the accumulation reaches a candidate's breakpoint, record that candidate's backward delay and backward confidence from the histogram accumulated so far; then scan downward over the remaining candidates and extend to every earlier candidate with the same right estimate that is not yet computed, recording each one separately. The backward pass has no clamp for a negative degraded cursor: when the degraded cursor starts negative, the loop condition (degraded cursor >= 0) ends the accumulation immediately with an empty histogram, giving confidence 0. Candidates whose forward confidence did not exceed the utterance confidence keep backward confidence 0.
8. Best split: among candidates where the forward and backward delays differ by at least W samples in absolute value, both confidences exceed the utterance confidence, and the summed confidence exceeds the best so far: keep the candidate, its breakpoint, both estimates and both delays. Tie handling is by scan order (later candidates replace only on strict improvement of the sum).
9. If a best split exists: shift all utterances after this one one position to the right; the two halves inherit the search window of the original utterance; the left half gets the forward estimates (coarse, fine delay, confidence) and the right half the backward ones. Boundaries: if the backward delay is smaller than the forward delay, left end = right start = breakpoint; otherwise left end = breakpoint + (backward_delay - forward_delay)/(2*W) and right start = breakpoint - (backward_delay - forward_delay)/(2*W), both with integer division truncating toward zero.
10. Clamp after splitting: if (left start - 75)*W + forward delay < 0, set left start = 75 + (W - 1 - forward delay)/W. If right end*W + backward delay > N_deg - 2400, set right end = (N_deg - backward delay)/W - 75.
11. Recompute the search windows and continue scanning utterances from the same position (the split may trigger further splits elsewhere, in scan order).

## 1.14 Frame skipping at negative delay jumps

After splitting, before the model: for every adjacent utterance pair, let j1 be the delay of the later utterance and j0 the delay of the earlier one. If j1 - j0 < -128 samples:

1. Let f1 = ((start[u] - 75)*W + j1) divided by 128 with integer division truncating toward zero, and let j = ((end[u-1] - 75)*W + j0) divided by 128 the same way. The reference implementation writes floor() around the quotient, but the quotient is integer arithmetic, so the floor is a no-op and both values truncate toward zero. The numerator can be negative because the boundary merging of 1.12 step 2 can leave end[u-1] below 75 and because j0 can be negative. If f1 > j, set f1 = j. If f1 < 0, set f1 = 0. (The f1 < 0 clamp makes floor versus truncating division immaterial for f1 itself; the wording matters for j, which has no such clamp.)
2. Let f2 = ((start[u] - 75)*W + max(0, |j1 - j0|))/128 + 1 (integer division).
3. Mark frames f1 through f2 inclusive as skipped and force their symmetric and asymmetric frame disturbances to zero, for every such frame below frame_stop (frame indexing is defined in 03), i.e. over [f1, min(f2, frame_stop - 1)]. Frame_stop itself is never skipped. The skip flags themselves have no further effect in the reference algorithm.

## 1.15 Processing order summary

1. Load both signals (1.2).
2. Level normalization (1.3).
3. IRS receive filter (1.4), then save model copies.
4. DC removal (1.5) and input IIR filter (1.6) on the working buffers.
5. VAD for both signals (1.8).
6. Coarse alignment on the whole signal (1.9).
7. Utterance search windows (1.11); per-utterance coarse (1.9) and fine (1.10) alignment.
8. Utterance boundaries (1.12), then splitting (1.13).
9. Restore the model copies; equalize lengths (1.7).
10. Run the perceptual model (03 and 04) and scoring (05).
