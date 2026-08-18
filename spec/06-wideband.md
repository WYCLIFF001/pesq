# 06. Wideband mode (P.862.2)

Scope: this file specifies the wideband extension P.862.2 as a delta on the narrowband specification in files 01 to 05. Wideband operation runs the same pipeline as narrowband at a 16 kHz sample rate, with three mode-specific differences: the input filter (6.3), the reported score and its mapping (6.5), and the sample rate restriction (6.2). Everything else, including the alignment filters, the perceptual model structure, the disturbance processing, and the raw score formula, is shared with narrowband 16 kHz operation and uses the rate-dependent constants of 6.4.

Listening condition: P.862.2 models a listener using wideband headphones, intended for wideband audio systems of approximately 50 to 7000 Hz, in place of the IRS-type narrowband handset that P.862 assumes. Scores from the two modes are not directly comparable.

## 6.1 Mode differences at a glance

| Aspect | Narrowband | Wideband |
|---|---|---|
| Allowed sample rates | 8000 and 16000 | 16000 only |
| Input filter after level normalization | IRS receive curve (02 section 2.6) | Edge ramps plus one second-order IIR section (6.3) |
| VAD and alignment preprocessing | DC removal plus the input IIR cascade (01 sections 1.5 and 1.6) | Identical; at 16 kHz the cascade has 12 sections (6.4.2) |
| Bark bands | 42 at 8 kHz | 49 at 16 kHz (6.4.1) |
| Raw score | Reported, and mapped to MOS-LQO by the P.862.1 mapping (05 section 5.2) | Not reported; mapped to MOS-LQO by the P.862.2 mapping (6.5) |
| Reported output | Raw score and MOS-LQO | MOS-LQO only |

## 6.2 Mode selection and sample rate

1. The mode is a global setting with two values: narrowband (the default) and wideband. The reference command line selects wideband with the option spelled "+wb".
2. Wideband mode requires the 16 kHz sample rate. Selecting wideband mode together with the 8 kHz rate is an error and processing stops before any audio is read.
3. Narrowband mode remains available at both sample rates. The narrowband 16 kHz path shares every rate-dependent constant with wideband (6.4) and differs only in the three aspects of 6.1.
4. Input files are mono 16-bit linear PCM at 16 kHz. The header and length rules of 01 section 1.2 apply with f = 16000: minimum L = f/4 = 4000 samples (250 ms). No internal resampling is performed; both files must already be at 16 kHz.

## 6.3 Wideband input filter

This step replaces the IRS receive filtering of 01 section 1.4. It is applied to both signals, independently, after level normalization (01 section 1.3, unchanged; the alignment curve of 02 section 2.5 is used at 16 kHz with the same table values), and before the model copies are saved (01 section 1.4 step 2).

At 16 kHz the buffer constants are: margin 4800 samples on each side, nominal length N = L + 9600, padding P = 5120. The signal region is [4800, N - 4800).

1. Edge ramps. Multiply the first 16 samples of the signal region by a rising ramp: sample 4800 + k is multiplied by (k + 1)/16 for k = 0..15. Multiply the last 16 samples of the signal region by a falling ramp: sample N - 4800 - 1 - k is multiplied by (k + 1)/16 for k = 0..15. All other samples of the region are unchanged. These ramps reduce filter transients at the start and end of the file.
2. Second-order IIR section. Apply one section with the recurrence and state conventions of 02 section 2.7 to the signal region [4800, N - 4800), in place, with zero initial state. The margins outside the region are not filtered (they are zero and stay zero).

Coefficients for 16 kHz (the normative set for P.862.2 operation):

| b0 | b1 | b2 | a1 | a2 |
|---|---|---|---|---|
| 2.740826 | -5.4816519 | 2.740826 | -1.9444777 | 0.94597794 |

The reference also carries an 8 kHz coefficient set (b0 = 2.6657628, b1 = -5.3315255, b2 = 2.6657628, a1 = -1.8890331, a2 = 0.89487434), published so that the two rate variants have matching gain in the 10 Hz to 4 kHz range. Wideband mode at 8 kHz is rejected (6.2), so that set is reproduced here for completeness only and must not be used.

Derived response characteristics of the 16 kHz section (informational, computed from the coefficients above):

- A double zero at DC (b0 + b1 + b2 = 0), giving a second-order high-pass shape.
- A pole pair of radius 0.97261 at approximately 71 Hz.
- The -3 dB point lies at approximately 100 Hz; the response is effectively flat above 300 Hz.
- Passband gain 2.818 (9.0 dB). The filter is applied identically to both signals; do not renormalize the gain.

After this step, save the model copies exactly as in 01 section 1.4 step 2. The wideband-filtered samples are what the perceptual model (03 and 04) consumes.

## 6.4 Rate-dependent constants at 16 kHz

These constants apply to all 16 kHz operation, narrowband and wideband. They replace the corresponding 8 kHz values in files 01 to 05; the procedures themselves are unchanged.

| Quantity | 8 kHz value | 16 kHz value |
|---|---|---|
| f (sample rate) | 8000 | 16000 |
| W (VAD window) | 32 samples (4 ms) | 64 samples (4 ms) |
| M (margin in windows) | 75 | 75 |
| Margin in samples | 2400 | 4800 |
| P (padding) | 2560 (320 ms) | 5120 (320 ms) |
| N (nominal length) | L + 4800 | L + 9600 |
| A (alignment FFT length) | 512 | 1024 |
| Model frame F | 256 samples (32 ms) | 512 samples (32 ms) |
| Model hop Q | 128 samples (16 ms) | 256 samples (16 ms) |
| Hz bins per frame | 128 | 256 |
| Bark bands B | 42 | 49 |
| Sp (pitch power density scale) | 2.764344E-5 | 6.910853E-6 |
| Sl (loudness scale) | 1.866055E-1 | 1.866055E-1 |
| Input IIR cascade sections | 8 (02 section 2.7) | 12 (6.4.2) |

The 16 kHz alignment filter curve, the level calibration target 1e7, and the calibration divisor rule (01 section 1.3) are unchanged; only the sample offsets scale (2400 becomes 4800).

### 6.4.1 Table 2: Bark bands for 16 kHz

Columns as in 03 section 3.8: band index; n = number of Hz bins grouped; Bark centre; Hz centre; Bark width; Hz width; power density correction factor; absolute threshold power. The grouping of 03 section 3.3 consumes bins 0..255 with these group counts. Bands 42 to 48 extend coverage to a top band centred at 7796.5 Hz.

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
| 41 | 12 | 17.117382 | 3820.219238 | 0.536934 | 349.937012 | 54.311001 | 0.524807 |
| 42 | 12 | 17.657663 | 4193.938477 | 0.543629 | 398.686279 | 61.114979 | 0.512861 |
| 43 | 15 | 18.204674 | 4619.846191 | 0.550390 | 454.713867 | 55.077751 | 0.478630 |
| 44 | 16 | 18.758478 | 5100.437012 | 0.557220 | 506.841797 | 56.849335 | 0.426580 |
| 45 | 18 | 19.319147 | 5636.199219 | 0.564119 | 564.863770 | 55.628868 | 0.371535 |
| 46 | 21 | 19.886751 | 6234.313477 | 0.571085 | 637.261230 | 53.137054 | 0.363078 |
| 47 | 25 | 20.461355 | 6946.734863 | 0.578125 | 794.717285 | 54.985844 | 0.416869 |
| 48 | 20 | 21.043034 | 7796.473633 | 0.585232 | 931.068359 | 79.546974 | 0.537032 |

### 6.4.2 Input IIR cascade at 16 kHz (12 sections)

Same recurrence, ordering, and application rule as 02 section 2.7: the cascade is applied to the whole working buffer of N + P samples, in place, with zero initial state, before VAD and alignment (01 sections 1.5 and 1.6). It replaces the 8-section cascade of 02 section 2.7 whenever the sample rate is 16 kHz, in both modes.

| Section | b0 | b1 | b2 | a1 | a2 |
|---|---|---|---|---|---|
| 0 | 0.325631521 | -0.086782860 | -0.238848661 | -1.079416490 | 0.434583902 |
| 1 | 0.403961804 | -0.556985881 | 0.153024077 | -0.415115835 | 0.696590244 |
| 2 | 4.736162769 | 3.287251046 | 1.753289019 | -1.859599046 | 0.876284034 |
| 3 | 0.365373469 | 0.000000000 | 0.000000000 | -0.634626531 | 0.000000000 |
| 4 | 0.884811506 | 0.000000000 | 0.000000000 | -0.256725271 | 0.141536777 |
| 5 | 0.723593055 | -1.447186099 | 0.723593044 | -1.129587469 | 0.657232737 |
| 6 | 1.644910855 | -1.817280902 | 1.249658063 | -1.778403899 | 0.801724355 |
| 7 | 0.633692689 | -0.284644314 | -0.319789663 | 0.000000000 | 0.000000000 |
| 8 | 1.032763031 | 0.268428979 | 0.602913323 | 0.000000000 | 0.000000000 |
| 9 | 1.001616361 | -0.823749013 | 0.439731942 | -0.885778255 | 0.000000000 |
| 10 | 0.752472096 | -0.375388990 | 0.188977609 | -0.077258216 | 0.247230734 |
| 11 | 1.023700575 | 0.001661628 | 0.521284240 | -0.183867259 | 0.354324187 |

## 6.5 Score mapping

The raw score is computed with the identical formula as narrowband (05 section 5.1): raw = 4.5 - 0.1 * D - 0.0309 * A, from the same disturbance aggregation. In wideband mode the raw score is computed but not reported.

Wideband MOS-LQO mapping (P.862.2), applied to the raw score x:

MOS-LQO = 0.999 + 4.0 / (1.0 + e^(-1.3669 * x + 3.8224)).

Reported to 3 decimal places. The narrowband mapping of 05 section 5.2 uses the constants -1.4945 and 4.6607; the two mappings are not interchangeable. The mapping input is the unrounded f32 raw score, as in narrowband mode.

## 6.6 Unchanged processing

The following are identical in narrowband and wideband mode, at 16 kHz with the constants of 6.4:

1. Level normalization and the calibration target 1e7 (01 section 1.3, 02 section 2.5).
2. DC removal and the working-buffer input IIR cascade before VAD and alignment (01 sections 1.5 and 1.6, 6.4.2).
3. VAD, coarse and fine delay estimation, utterance location and splitting, bad-interval detection and re-alignment (01 sections 1.8 to 1.14, 04 section 4.5). There is no wideband-specific alignment filter or threshold change anywhere in the pipeline.
4. The perceptual model: spectra, Bark warping, audibility, frequency response compensation, loudness, and local gain scaling (03), using the 49-band table of 6.4.1.
5. Disturbance densities, asymmetric disturbance, power normalization, time weighting, and aggregation (04).
6. The raw score combination weights (05 section 5.1).

## 6.7 Conformance

Annex A conformance test 4 covers wideband operation: 1736 file pairs from ITU-T P-series Supplement 23, scored at 16 kHz in wideband mode. An implementation passes when the absolute difference from the reference implementation is not greater than 0.05 in all cases. The expected values and the full criteria are in CONFORMANCE-supp23.md.

## 6.8 Documented choices where sources differ

1. The published P.862.2 description names the input filter as an IIR filter with a 100 Hz high-pass characteristic modeling the wideband listening condition. The exact coefficients are taken from the reference implementation and are normative here (6.3).
2. The reference implementation carries an 8 kHz wideband input filter coefficient set but rejects wideband mode at 8 kHz. This specification reproduces the 8 kHz set for completeness and forbids its use (6.3).
3. The reference implementation computes the raw score in wideband mode and then omits it from the reported output; this specification keeps that behavior (6.5).
4. All perceptual constants at 16 kHz (6.4) are shared between narrowband and wideband mode; the reference implementation has no further mode-specific perceptual constants.
