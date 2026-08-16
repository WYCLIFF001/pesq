# 04. Disturbance processing and aggregation

Continues the per-frame loop of 03 after 3.7. All per-frame arrays are indexed over [frame_start, frame_stop] from 03 section 3.1.

## 4.1 Disturbance densities and deadzone

Per frame and band b (band 0 included in this stage, unlike the audibility sums):

1. Raw disturbance: d[b] = loudness_deg[b] - loudness_ref[b].
2. Deadzone margin: m[b] = 0.25 * min(loudness_deg[b], loudness_ref[b]).
3. Deadzone removal: if d[b] > m[b], subtract m[b]; else if d[b] < -m[b], add m[b]; else set d[b] = 0.

## 4.2 Frame disturbance norms

Weighted Lp norm over bands 1..41 (band 0 excluded), with weights equal to the Bark widths w[b] of Table 1 in 03:

Lp(d, p) = ( sum over b=1..41 of (|d[b]| * w[b])^p / sum over b=1..41 of w[b] )^(1/p) * sum over b=1..41 of w[b].

1. Symmetric frame disturbance: D[frame] = Lp(d, 2) with the deadzone-removed densities. Accumulate in f64, store f32.
2. First-pass bad-frame gate: if D[frame] > 30, set a global flag that bad intervals exist (used in 4.5). The gate is evaluated during the first pass, before the frame skipping of 4.4 zeroes any values.

## 4.3 Asymmetric disturbance

Per frame and band b, using the pitch power densities after 3.5 and 3.7 (the degraded densities are the scaled ones):

1. Ratio r = (p_deg[b] + 50) / (p_ref[b] + 50).
2. Factor h = r^1.2. Cap h at 12. If h < 3, set h = 0.
3. Multiply the deadzone-removed disturbance density d[b] by h. (Disturbances that sound like added content, where the degraded density exceeds the reference, get amplified; removed content is suppressed.)
4. Asymmetric frame disturbance: A[frame] = Lp(d, 1) on the multiplied densities.

## 4.4 Frame skipping at negative delay jumps

Applied before the bad-interval processing, after the first pass (see 01 section 1.14): for the frames in the computed range [f1, f2] of each affected utterance boundary, set D[frame] = 0 and A[frame] = 0 and record a skip flag for the frame. The flag has no further effect on the score.

## 4.5 Bad intervals and re-alignment

The bad-interval machinery runs only if the gate of 4.2 step 2 was set.

### 4.5.1 Interval detection

1. Badness mask per frame: D[frame] > 30, re-evaluated after the zeroing of 4.4. Frame 0 is forced to be not bad.
2. Dilation by 2 frames on each side: for each frame in [2, frame_stop - 2]: the dilated value is the minimum of (maximum of the mask over the frame and its two left neighbours) and (maximum of the mask over the frame and its two right neighbours). Frames outside [2, frame_stop - 2] keep the dilated value "not bad".
3. Intervals: contiguous runs of dilated bad frames. A run from frame a to frame b exclusive is recorded as an interval only if b - a >= 5. At most 1000 intervals are processed (the reference stores at most 1000).

### 4.5.2 First re-aligned degraded signal

1. Build an array T of length Nmax + P, all zeros.
2. For each sample position i in [2400, Nmax + P - 2400): find the governing utterance at i (the last utterance u with start[u]*W <= i; delay[u]; if none, delay[0]). Let j = i + delay, clamped to [2400, Nmax + P - 2400 - 1]. Set T[i] = deg[j].
3. Samples outside [2400, Nmax + P - 2400) stay zero.

### 4.5.3 Per-interval delay search

For each interval, with sample bounds start_sample = a*Q + 2400 and stop_sample = b*Q + F + 2400, where a and b are the interval's frame bounds (b capped to frame_stop after start_sample and stop_sample are formed; if b > frame_stop, set b = frame_stop), and interval length n = stop_sample - start_sample:

1. Search range s = 4*F = 1024 samples. Segment length m = 2s + n.
2. Reference segment: a buffer of m samples: s zeros, then the n reference samples from [start_sample, stop_sample), then s zeros.
3. Degraded segment: a buffer of m samples: for i in [0, m): deg_seg[i] = T[j] with j = start_sample - s + i, clamped to [2400, Nmax + P - 2400 - 1].
4. Powers: let R be the smallest power of two >= 2m. S1 = sum over i in [0, m) of |ref_seg[i]|^2 and S2 = the same sum for the degraded segment (plain sums, no division). Let p1 = S1/R and p2 = S2/R. If p1 <= 1e-6 or p2 <= 1e-6, the correlation result is 0 and the delay is 0.
5. Normalized circular cross-correlation at period R: c(tau) = sum over i in [0, m) of |ref_seg[i]| * |deg_seg[(i + tau) mod R]|, and h(tau) = |c(tau)| / sqrt(S1 * S2). Compute c with FFTs: fill two length-R buffers with the absolute values of the first m samples of each segment, zero beyond the first m values, forward real FFT, divide the first buffer's bins (0..R/2) by R, binwise conjugate of the first times the second, inverse real FFT (whose 1/R is part of the inverse transform, 02 section 2.1). The output at position k is c(k)/R, and sqrt(p1 * p2) = sqrt(S1 * S2)/R, so the two 1/R factors cancel: h(tau) at a wrapped position is the absolute value of the output divided by sqrt(p1 * p2), equal to |c(tau)| / sqrt(S1 * S2).
6. Search order: tau from -s to -1 (reading the wrapped positions), then tau from 0 to s - 1. Keep the first tau with a strictly larger h. Initial best value 0 and best lag 0. If the best h is below 0.5, the delay for this interval is forced to 0.

### 4.5.4 Second re-aligned degraded signal and recomputation

1. Copy T into a second buffer T2 of length Nmax + P.
2. For each interval, for i in [start_sample, stop_sample): j = i + interval delay, clamped to [0, Nmax - 1]; T2[i] = T[j].
3. Temporarily replace the degraded signal with T2 and re-run for the frames in [a, b) of each interval:
   a. Degraded short-term spectrum with start = reference start r0 (zero relative delay), Hann window (03 section 3.2).
   b. Warping to Bark bands (03 section 3.3).
   c. Scaling: previous scale reset to 1 before each interval's frame range; per frame compute and smooth exactly as 03 section 3.7 (the "frame > 0" condition still uses the global frame index), clamp to [3e-4, 5], multiply the degraded densities.
   d. Loudness for both signals (03 section 3.6), disturbance and deadzone (4.1), symmetric norm with p = 2: D[frame] = min(existing D[frame], new value).
   e. Asymmetry (4.3) and asymmetric norm with p = 1: A[frame] = min(existing A[frame], new value).
4. Restore the original degraded signal.

## 4.6 Power normalization and cap

Per frame:

1. h = ((total_power_ref[frame] + 1e5) / 1e7)^0.04, where total_power_ref is the frame's reference audible power stored in 03 section 3.7.
2. D[frame] = D[frame] / h and A[frame] = A[frame] / h.
3. Cap both at 45: values above 45 become 45.

(The published algorithm description cites the value 45 as the threshold for a "bad frame". The reference implementation detects bad intervals with the earlier threshold 30 on the pre-normalization symmetric disturbance (4.2) and applies 45 here as a hard cap on the power-normalized disturbances. This specification follows the reference implementation.)

## 4.7 Time weighting for long signals

1. If frame_stop + 1 > 1000: let n = (Nmax - 4800)/Q - 1 and f = (n - 1000)/5500, capped at 0.5. The time weight of a frame is (1 - f) + f * frame / n.
2. Otherwise every frame's time weight is 1.

## 4.8 Aggregation over syllables and time

For each indicator (symmetric D and asymmetric A):

1. Sweep a syllable window of 20 frames with a step of 10 frames over [frame_start, frame_stop]: for each window start s (s = frame_start, frame_start + 10, ..., while s <= frame_stop):
   a. Syllable value: ((1/20) * sum over f = s..s+19, f <= frame_stop, of X[f]^6)^(1/6), where X is the indicator array. The denominator is always 20; frames beyond frame_stop contribute 0.
   b. Accumulate S += (weight[s - frame_start] * syllable)^2 and T += weight[s - frame_start]^2, where weight is the time weight of the frame at the window start (4.7).
2. The aggregated indicator is (S / T)^(1/2).

Both indicators use the exponent 6 within a syllable and the exponent 2 over time. The symmetric indicator comes from D (per-frame norm exponent 2, 4.2) and the asymmetric indicator from A (per-frame norm exponent 1, 4.3). These three pairs of exponents (2/1 per frame, 6 per syllable, 2 over time) are the published PESQ values.
