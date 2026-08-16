# 03. Perceptual model part 1: spectra, Bark warping, loudness, scaling

Inputs to this stage: the saved model copies of the two signals (01 section 1.4 step 2), equalized to a common length (01 section 1.7), and the per-utterance delays from 01.

## 3.1 Frame layout

Frame length F = 256 samples (32 ms), hop Q = 128 samples (16 ms). Hann window of length F (02 section 2.3). All frames are indexed from 0. Let Nmax be the common nominal length.

1. Start-of-signal silence skip: let s = 0. While the sum of |ref[2400 + s + i]| for i = 0..4 is below 500 and s < Nmax/2, increment s. Let skip_start = s.
2. End-of-signal silence skip: the mirrored procedure from the end: sum of |ref[Nmax - 2400 + P - 1 - s - i]| for i = 0..4 below 500, while s < Nmax/2. Let skip_end = s.
3. First processed frame: frame_start = skip_start / Q (integer division).
4. Last processed frame: frame_stop = (Nmax - 4800 + P - skip_end) / Q - 1 (integer division).
5. Frames are processed in the ranges defined by these two values. Any array indexed by frame in this file has frame_stop + 1 entries.

## 3.2 Short-term power spectrum

For a frame at reference start sample r0 = 2400 + frame*Q:

1. Take F = 256 samples of the reference starting at r0, multiply by the Hann window, forward real FFT.
2. Power spectrum: for each bin k = 0..127, power = real(k)^2 + imag(k)^2.
3. Force bin 0 (DC) to 0.
4. For the degraded signal, the frame start is d0 = r0 + delay, where delay is the delay of the governing utterance for this frame: the last utterance u with start[u]*W <= r0 gives delay[u]; if none exists, delay[0]. Compute the degraded spectrum the same way, except: if d0 <= 0 or d0 + F >= Nmax + P, the degraded spectrum is all zeros.

## 3.3 Frequency warping to Bark bands

The 128 Hz bins are grouped into B = 42 Bark bands. Band b consumes the next n[b] bins (from the group count column of Table 1), in order of increasing frequency, starting at bin 1 (bin 0 is already zero). The group counts sum to 128.

1. For band b: sum the power spectrum values of its n[b] bins.
2. Multiply the sum by the power density correction factor c[b] from Table 1.
3. Multiply by Sp = 2.764344E-5 (the pitch power density scaling).
4. Store as the pitch power density p[frame][b] (f32; the sum accumulates in f64).

This is applied to both signals per frame.

## 3.4 Audibility and the silence flag

Absolute hearing threshold t[b] from Table 1.

1. Audible power of a frame with a factor g: the sum over bands 1..41 (band 0 excluded) of p[frame][b] for every band where p[frame][b] > g * t[b].
2. Silence flag for a frame: the audible power of the reference frame with factor 100 is below 1e7. Silent frames are flagged but not excluded from disturbance computation; they only affect the averaging of 3.5.

## 3.5 Frequency response compensation

1. For each band b: average over frames 0..frame_stop of p[frame][b], counting only frames that are not flagged silent and only values p[frame][b] > 100 * t[b]. Divide this sum by the divisor (Nmax - 4800 + P)/Q - 1 (the frame count of the unskipped signal; note this is not the count of counted frames).
2. This yields an average per band for the reference and for the degraded signal.
3. For each band b: x = (avg_deg[b] + 1000) / (avg_ref[b] + 1000). Clamp x to the range [0.01, 100].
4. Multiply the reference pitch power density of every frame in band b by x. The degraded densities are unchanged. (The reference spectrum is compensated toward the degraded average; this removes long-term linear filtering differences before the disturbance stage.)

## 3.6 Loudness densities (Zwicker law)

For each frame and each band b, with threshold t = t[b] and input p = p[frame][b]:

1. Low-band correction: if the Bark centre of band b (Table 1) is below 4, h = 6 / (bark centre + 2), else h = 1. Cap h at 2. Raise h to the power 0.15.
2. Modified Zwicker exponent: z = 0.23 * h. (The base Zwicker power is 0.23; the published value. The correction only applies below 4 Bark.)
3. If p > t: loudness = (t / 0.5)^z * ((0.5 + 0.5 * p / t)^z - 1). Otherwise loudness = 0. (Computed in f64, stored f32.)
4. Multiply by Sl = 1.866055E-1 (the loudness scaling factor).
5. The result is the loudness density per band for that frame, computed for both signals.

## 3.7 Local gain scaling of the degraded signal

Per frame, in frame order, with a running state "previous scale" initialized to 1:

1. Let a_ref = audible power of the reference frame with factor 1 (3.4), and a_deg likewise for the degraded frame.
2. scale = (a_ref + 5000) / (a_deg + 5000).
3. If frame > 0: scale = 0.2 * previous_scale + 0.8 * scale.
4. previous_scale = scale.
5. Clamp scale to [3e-4, 5].
6. Multiply the degraded pitch power density of every band in this frame by the clamped scale.
7. Store a_ref as the frame's reference audible power (used in 04).

Order within the frame processing loop (3.2 to 3.7): spectra, warping, silence flag and averages (first pass over all frames); then per frame: compensation (3.5, applied once, before the loop), scaling, loudness for both signals, then the disturbance steps of 04.

## 3.8 Table 1: Bark bands for 8 kHz

Columns: band index; n = number of Hz bins grouped; Bark centre; Hz centre; Bark width; Hz width; power density correction factor; absolute threshold power.

| b | n | bark centre | Hz centre | bark width | Hz width | corr. factor | threshold |
|---|---|---|---|---|---|---|---|
| 0 | 1 | 0.078672 | 7.867213 | 0.157344 | 15.734426 | 100.000000 | 51286152.000000 |
| 1 | 1 | 0.316341 | 31.634144 | 0.317994 | 31.799433 | 99.999992 | 2454709.500000 |
| 2 | 1 | 0.636559 | 63.655895 | 0.322441 | 32.244064 | 100.000000 | 70794.593750 |
| 3 | 1 | 0.961246 | 96.124611 | 0.326934 | 32.693359 | 100.000008 | 4897.788574 |
| 4 | 1 | 1.290450 | 129.044968 | 0.331474 | 33.147385 | 100.000008 | 1174.897705 |
| 5 | 1 | 1.624217 | 162.421738 | 0.336061 | 33.606140 | 100.000015 | 389.045166 |
| 6 | 1 | 1.962597 | 196.259659 | 0.340697 | 34.069702 | 99.999992 | 104.712860 |
| 7 | 1 | 2.305636 | 230.563568 | 0.345381 | 34.538116 | 99.999969 | 45.708820 |
| 8 | 2 | 2.653383 | 265.338348 | 0.350114 | 35.011429 | 50.000027 | 17.782795 |
| 9 | 1 | 3.005889 | 300.588867 | 0.354897 | 35.489655 | 100.000000 | 9.772372 |
| 10 | 1 | 3.363201 | 336.320129 | 0.359729 | 35.972870 | 99.999969 | 4.897789 |
| 11 | 1 | 3.725371 | 372.537140 | 0.364611 | 36.461121 | 100.000015 | 3.090296 |
| 12 | 1 | 4.092449 | 409.244934 | 0.369544 | 36.954407 | 99.999947 | 1.905461 |
| 13 | 1 | 4.464486 | 446.448578 | 0.374529 | 37.452911 | 100.000061 | 1.258925 |
| 14 | 2 | 4.841533 | 484.568604 | 0.379565 | 40.269653 | 53.047077 | 0.977237 |
| 15 | 1 | 5.223642 | 526.600586 | 0.384653 | 42.311859 | 110.000046 | 0.724436 |
| 16 | 1 | 5.610866 | 570.303833 | 0.389794 | 45.992554 | 117.991989 | 0.562341 |
| 17 | 2 | 6.003256 | 619.423340 | 0.394989 | 51.348511 | 65.000000 | 0.457088 |
| 18 | 2 | 6.400869 | 672.121643 | 0.400236 | 55.040527 | 68.760147 | 0.389045 |
| 19 | 2 | 6.803755 | 728.525696 | 0.405538 | 56.775208 | 69.999931 | 0.331131 |
| 20 | 2 | 7.211971 | 785.675964 | 0.410894 | 58.699402 | 71.428818 | 0.295121 |
| 21 | 2 | 7.625571 | 846.835693 | 0.416306 | 62.445862 | 75.000038 | 0.269153 |
| 22 | 2 | 8.044611 | 909.691650 | 0.421773 | 64.820923 | 76.843384 | 0.257040 |
| 23 | 2 | 8.469146 | 977.063293 | 0.427297 | 69.195374 | 80.968781 | 0.251189 |
| 24 | 2 | 8.899232 | 1049.861694 | 0.432877 | 76.745667 | 88.646126 | 0.251189 |
| 25 | 3 | 9.334927 | 1129.635986 | 0.438514 | 84.016235 | 63.864388 | 0.251189 |
| 26 | 3 | 9.776288 | 1217.257568 | 0.444209 | 90.825684 | 68.155350 | 0.251189 |
| 27 | 3 | 10.223374 | 1312.109497 | 0.449962 | 97.931152 | 72.547775 | 0.263027 |
| 28 | 3 | 10.676242 | 1412.501465 | 0.455774 | 103.348877 | 75.584831 | 0.288403 |
| 29 | 4 | 11.134952 | 1517.999390 | 0.461645 | 107.801880 | 58.379192 | 0.309030 |
| 30 | 3 | 11.599563 | 1628.894165 | 0.467577 | 113.552246 | 80.950836 | 0.338844 |
| 31 | 4 | 12.070135 | 1746.194336 | 0.473569 | 121.490601 | 64.135651 | 0.371535 |
| 32 | 5 | 12.546731 | 1871.568848 | 0.479621 | 130.420410 | 54.384785 | 0.398107 |
| 33 | 4 | 13.029408 | 2008.776123 | 0.485736 | 143.431763 | 73.821884 | 0.436516 |
| 34 | 5 | 13.518232 | 2158.979248 | 0.491912 | 158.486816 | 64.437073 | 0.467735 |
| 35 | 6 | 14.013264 | 2326.743164 | 0.498151 | 176.872803 | 59.176456 | 0.489779 |
| 36 | 6 | 14.514566 | 2513.787109 | 0.504454 | 198.314697 | 65.521278 | 0.501187 |
| 37 | 7 | 15.022202 | 2722.488770 | 0.510819 | 219.549561 | 61.399822 | 0.501187 |
| 38 | 8 | 15.536238 | 2952.586670 | 0.517250 | 240.600098 | 58.144047 | 0.512861 |
| 39 | 9 | 16.056736 | 3205.835449 | 0.523745 | 268.702393 | 57.004543 | 0.524807 |
| 40 | 9 | 16.583761 | 3492.679932 | 0.530308 | 306.060059 | 64.126297 | 0.524807 |
| 41 | 11 | 17.117382 | 3820.219238 | 0.536934 | 349.937012 | 59.248363 | 0.524807 |

The band count B = 42, the loudness scale Sl = 1.866055E-1 and the pitch power density scale Sp = 2.764344E-5 apply for the 8 kHz narrowband mode. (The 16 kHz wideband mode uses 49 bands and different scales and is out of scope.)
