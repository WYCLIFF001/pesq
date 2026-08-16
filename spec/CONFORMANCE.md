# Conformance vectors (P.862 Annex A, 8 kHz narrowband)

Source: ITU-T P.862 (2001) Amendment 2 (11/2005), Annex A conformance data. The expected values below are the raw P.862 scores published for the 8 kHz VoIP data set (test 2(b) of Annex A). They are reference data, reproduced as published, to 3 decimal places.

## 1. Test 2(b): VoIP variable delay data, 8 kHz

40 reference/degraded file pairs. The degraded files of this set exercise variable delay. Sample rate for scoring: 8000 Hz.

| Pair | Reference | Degraded | Expected raw P.862 score |
|---|---|---|---|
| 1 | voip/or105.wav | voip/dg105.wav | 2.237 |
| 2 | voip/or109.wav | voip/dg109.wav | 3.180 |
| 3 | voip/or114.wav | voip/dg114.wav | 2.147 |
| 4 | voip/or129.wav | voip/dg129.wav | 2.680 |
| 5 | voip/or134.wav | voip/dg134.wav | 2.365 |
| 6 | voip/or137.wav | voip/dg137.wav | 3.670 |
| 7 | voip/or145.wav | voip/dg145.wav | 3.016 |
| 8 | voip/or149.wav | voip/dg149.wav | 2.558 |
| 9 | voip/or152.wav | voip/dg152.wav | 2.768 |
| 10 | voip/or154.wav | voip/dg154.wav | 2.694 |
| 11 | voip/or155.wav | voip/dg155.wav | 2.606 |
| 12 | voip/or161.wav | voip/dg161.wav | 2.608 |
| 13 | voip/or164.wav | voip/dg164.wav | 2.850 |
| 14 | voip/or166.wav | voip/dg166.wav | 2.527 |
| 15 | voip/or170.wav | voip/dg170.wav | 2.452 |
| 16 | voip/or179.wav | voip/dg179.wav | 1.828 |
| 17 | voip/or221.wav | voip/dg221.wav | 2.774 |
| 18 | voip/or229.wav | voip/dg229.wav | 2.940 |
| 19 | voip/or246.wav | voip/dg246.wav | 2.205 |
| 20 | voip/or272.wav | voip/dg272.wav | 3.288 |
| 21 | voip/u_am1s01.wav | voip/u_am1s01b1c1.wav | 3.483 |
| 22 | voip/u_am1s01.wav | voip/u_am1s01b1c7.wav | 2.420 |
| 23 | voip/u_am1s02.wav | voip/u_am1s02b1c9.wav | 4.042 |
| 24 | voip/u_am1s01.wav | voip/u_am1s01b1c15.wav | 3.179 |
| 25 | voip/u_am1s03.wav | voip/u_am1s03b1c16.wav | 2.872 |
| 26 | voip/u_am1s03.wav | voip/u_am1s03b1c18.wav | 2.806 |
| 27 | voip/u_am1s01.wav | voip/u_am1s01b2c1.wav | 4.300 |
| 28 | voip/u_am1s02.wav | voip/u_am1s02b2c4.wav | 3.634 |
| 29 | voip/u_am1s02.wav | voip/u_am1s02b2c5.wav | 3.369 |
| 30 | voip/u_am1s03.wav | voip/u_am1s03b2c5.wav | 3.911 |
| 31 | voip/u_am1s03.wav | voip/u_am1s03b2c6.wav | 2.905 |
| 32 | voip/u_am1s03.wav | voip/u_am1s03b2c7.wav | 3.579 |
| 33 | voip/u_am1s01.wav | voip/u_am1s01b2c8.wav | 2.198 |
| 34 | voip/u_am1s03.wav | voip/u_am1s03b2c11.wav | 3.276 |
| 35 | voip/u_am1s02.wav | voip/u_am1s02b2c14.wav | 3.316 |
| 36 | voip/u_af1s01.wav | voip/u_af1s01b2c16.wav | 3.307 |
| 37 | voip/u_af1s03.wav | voip/u_af1s03b2c16.wav | 3.592 |
| 38 | voip/u_af1s02.wav | voip/u_af1s02b2c17.wav | 2.614 |
| 39 | voip/u_af1s03.wav | voip/u_af1s03b2c17.wav | 2.806 |
| 40 | voip/u_am1s03.wav | voip/u_am1s03b2c18.wav | 2.540 |

## 2. Conformance requirement for this set (Annex A test 2(b))

For each pair, the absolute difference between the implementation's raw P.862 score and the expected value above must satisfy:

1. At most 1 of the 40 pairs may differ by more than 0.05.
2. No pair may differ by more than 0.5.

## 3. Other Annex A narrowband test (test 1(b), not shipped here)

The Supplement 23 8 kHz set has 1736 pairs (reference material downsampled from ITU-T P-series Supplement 23). Requirements: at most 2 pairs may differ by more than 0.05 (about 0.1% of cases); no pair may differ by more than 0.1.

## 4. Open-ended test (test 3)

Based on general, unknown data, with no fixed data set: the score may differ by more than 0.05 in at most 0.5% of cases (this lower threshold is advisory); it may differ by more than 0.05 in at most 5% of cases (upper threshold).

## 5. Out of scope

Tests 1(a) and 2(a) are the 16 kHz variants of tests 1(b) and 2(b); test 4 is the P.862.2 wideband conformance validation (mandatory only for wideband operation). They do not apply to this narrowband-only port.

## 6. Test harness notes

1. The reference pairs are WAV files: the loader must skip the 44-byte header (see spec 01 section 1.2) before scoring.
2. Compare raw P.862 scores rounded to 3 decimal places against the values above.
3. Run all 40 pairs; count pairs whose absolute difference exceeds 0.05, and record the maximum absolute difference.
