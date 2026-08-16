# Convergence report: Annex A test 2(b)

Date: 2026-08-16. Data: the Annex A 8 kHz VoIP set (40 pairs) from `PESQ_CONFORMANCE_DIR`.
Harness: `cargo test --test conformance -- --nocapture`, which feeds the original 8 kHz
WAV samples to the native 8 kHz entry point and compares raw P.862 scores rounded to
3 decimals against `spec/CONFORMANCE.md` (spec section 6).

## 0. Round 4 (current): zero residual, proper decimator, CLI

Round 4 resolves the Round 3 open items 1, 2, and 5 below:

1. **Pair 26 residual fixed.** The trace localized the residual to the last 9 processed
   frames of pair 26, whose degraded spectra were zeroed by a wrong boundary: the
   degraded frame bound `d0 + F >= Nmax + P` of spec 03 section 3.2 step 4 was checked
   against the reference signal's own nominal length instead of the common nominal
   length Nmax of spec 01 section 1.7. With a degraded file longer than the reference
   (pair 26: 72000 vs 64000 samples), frames near the end were wrongly declared out of
   bounds and their degraded content dropped, costing 0.0138 of the raw score. The
   fix threads the common Nmax through the frame layout, the degraded bounds, the
   compensation divisor (spec 03, 3.5), the bad-interval buffers and clamps (spec 04,
   4.5), and the long-signal time weights (spec 04, 4.7). Every use of the reference's
   own nominal length in those stages was a latent form of the same bug. All 40 pairs
   now score at 3-decimal delta 0.000 against the C reference.
2. **16 kHz decimation.** The provisional pair averaging is replaced by a short
   windowed-sinc anti-aliasing decimator: 33 taps, Hamming window, cutoff at 0.45 of
   the 8 kHz Nyquist frequency (1800 Hz), unit DC gain, stopband below -45 dB. See
   `spec/CONFORMANCE.md` section 6 item 4 and the crate docs of
   `input::decimate_16k_to_8k`. The 8 kHz entry point and the conformance path are
   untouched by this change.
3. **`pesq-cli` scorer.** `cargo run --bin pesq-cli -- <ref.wav> <deg.wav>` prints the
   raw P.862 score with 6 decimals; 8 kHz files score natively, 16 kHz files go
   through the decimating entry point. Documented in the README.

## 1. Result

CONFORMANCE: PASS, 40 of 40 pairs at 3-decimal delta 0.000 (Round 4).

| Metric | Round 1 (first attempt) | Round 3 | Round 4 |
|---|---|---|---|
| Pairs beyond 0.05 | 29 of 40 | 0 of 40 | 0 of 40 |
| Pairs beyond 0.5 | not recorded | 0 of 40 | 0 of 40 |
| Mean abs delta | 0.238 | 0.000350 | 0.000000 |
| RMSE | not recorded | 0.002214 | 0.000000 |
| Max abs delta | 1.234 | 0.014 (pair 26) | 0.000 |

Criteria from `spec/CONFORMANCE.md` section 2: at most 1 of 40 pairs may differ by more
than 0.05, and no pair may differ by more than 0.5. Both criteria are met with margin:
0 pairs beyond 0.05 and 0 pairs beyond 0.5.

## 2. Summary numbers

- Pairs within 0.05 of expected: 40 of 40.
- Pairs beyond 0.05: 0 (at most 1 allowed).
- Pairs beyond 0.5: 0 (none allowed).
- Mean abs delta: 0.000350 (deltas per the spec section 6 rule, scores rounded to
  3 decimals before comparison); 0.000589 without rounding.
- RMSE: 0.002214 (rounded deltas); 0.002204 (unrounded).
- Max abs delta: 0.014 on pair 26 (`u_am1s03.wav` / `u_am1s03b1c18.wav`):
  raw score 2.792179, expected 2.806 (delta 0.013821 unrounded).
- Second largest delta: 0.000498 on pair 33 (`u_am1s01.wav` / `u_am1s01b2c8.wav`).

Round 4, after the pair 26 fix of section 0: pair 26 raw score 2.806101, expected
2.806 (unrounded delta 0.000101); mean abs delta 0.000000 and max abs delta 0.000 on
the 3-decimal comparison of the conformance rule. The per-pair table of section 4
holds with every delta at 0.000.

## 3. What improved since the first attempt

The first conformance run, right after the full pipeline was wired (commit `fe1903e`),
reported 29 of 40 pairs beyond 0.05, mean abs delta 0.238, max abs delta 1.234. Round 3
brings every pair inside 0.05. The changes in this round, in order:

1. `da29491` Implemented spec 04 disturbance processing and spec 05 scoring, closing the
   tail of the pipeline that the first attempt lacked (per-frame disturbances, skip
   zeroing, aggregation into the two indicators, raw score).
2. `79e8f2a` / `3764556` Amended and applied spec 03 Bark bin grouping, spec 01 split
   fine-pass recording, and the negative-delay skip bounds.
3. `3764556` Added the native 8 kHz entry point `pesq_8k` and pointed the harness at it,
   removing the lossy 8k-to-16k-to-8k round trip called out in spec/CONFORMANCE.md
   section 6 item 4. The harness no longer resamples; it feeds the WAV samples as stored.
4. `f3668a2` / `eae77f2` Amended and applied spec 01 delay alignment: split scan order,
   effective split window, and VAD threshold order. These were the dominant source of
   the large early deltas on the variable-delay pairs.
5. `22cb244` Added the disturbance indicators and raw score to the diagnose example, and
   this round extended its raw-score precision to 6 decimals to measure residual deltas.

## 4. Per-pair table

Scores are raw P.862 scores; deltas are computed on scores rounded to 3 decimals
(spec/CONFORMANCE.md section 6).

| Pair | Reference | Degraded | Expected | Score | Delta |
|---|---|---|---|---|---|
| 1 | or105.wav | dg105.wav | 2.237 | 2.237 | 0.000 |
| 2 | or109.wav | dg109.wav | 3.180 | 3.180 | 0.000 |
| 3 | or114.wav | dg114.wav | 2.147 | 2.147 | 0.000 |
| 4 | or129.wav | dg129.wav | 2.680 | 2.680 | 0.000 |
| 5 | or134.wav | dg134.wav | 2.365 | 2.365 | 0.000 |
| 6 | or137.wav | dg137.wav | 3.670 | 3.670 | 0.000 |
| 7 | or145.wav | dg145.wav | 3.016 | 3.016 | 0.000 |
| 8 | or149.wav | dg149.wav | 2.558 | 2.558 | 0.000 |
| 9 | or152.wav | dg152.wav | 2.768 | 2.768 | 0.000 |
| 10 | or154.wav | dg154.wav | 2.694 | 2.694 | 0.000 |
| 11 | or155.wav | dg155.wav | 2.606 | 2.606 | 0.000 |
| 12 | or161.wav | dg161.wav | 2.608 | 2.608 | 0.000 |
| 13 | or164.wav | dg164.wav | 2.850 | 2.850 | 0.000 |
| 14 | or166.wav | dg166.wav | 2.527 | 2.527 | 0.000 |
| 15 | or170.wav | dg170.wav | 2.452 | 2.452 | 0.000 |
| 16 | or179.wav | dg179.wav | 1.828 | 1.828 | 0.000 |
| 17 | or221.wav | dg221.wav | 2.774 | 2.774 | 0.000 |
| 18 | or229.wav | dg229.wav | 2.940 | 2.940 | 0.000 |
| 19 | or246.wav | dg246.wav | 2.205 | 2.205 | 0.000 |
| 20 | or272.wav | dg272.wav | 3.288 | 3.288 | 0.000 |
| 21 | u_am1s01.wav | u_am1s01b1c1.wav | 3.483 | 3.483 | 0.000 |
| 22 | u_am1s01.wav | u_am1s01b1c7.wav | 2.420 | 2.420 | 0.000 |
| 23 | u_am1s02.wav | u_am1s02b1c9.wav | 4.042 | 4.042 | 0.000 |
| 24 | u_am1s01.wav | u_am1s01b1c15.wav | 3.179 | 3.179 | 0.000 |
| 25 | u_am1s03.wav | u_am1s03b1c16.wav | 2.872 | 2.872 | 0.000 |
| 26 | u_am1s03.wav | u_am1s03b1c18.wav | 2.806 | 2.792 | 0.014 |
| 27 | u_am1s01.wav | u_am1s01b2c1.wav | 4.300 | 4.300 | 0.000 |
| 28 | u_am1s02.wav | u_am1s02b2c4.wav | 3.634 | 3.634 | 0.000 |
| 29 | u_am1s02.wav | u_am1s02b2c5.wav | 3.369 | 3.369 | 0.000 |
| 30 | u_am1s03.wav | u_am1s03b2c5.wav | 3.911 | 3.911 | 0.000 |
| 31 | u_am1s03.wav | u_am1s03b2c6.wav | 2.905 | 2.905 | 0.000 |
| 32 | u_am1s03.wav | u_am1s03b2c7.wav | 3.579 | 3.579 | 0.000 |
| 33 | u_am1s01.wav | u_am1s01b2c8.wav | 2.198 | 2.198 | 0.000 |
| 34 | u_am1s03.wav | u_am1s03b2c11.wav | 3.276 | 3.276 | 0.000 |
| 35 | u_am1s02.wav | u_am1s02b2c14.wav | 3.316 | 3.316 | 0.000 |
| 36 | u_af1s01.wav | u_af1s01b2c16.wav | 3.307 | 3.307 | 0.000 |
| 37 | u_af1s03.wav | u_af1s03b2c16.wav | 3.592 | 3.592 | 0.000 |
| 38 | u_af1s02.wav | u_af1s02b2c17.wav | 2.614 | 2.614 | 0.000 |
| 39 | u_af1s03.wav | u_af1s03b2c17.wav | 2.806 | 2.806 | 0.000 |
| 40 | u_am1s03.wav | u_am1s03b2c18.wav | 2.540 | 2.540 | 0.000 |

## 5. Remaining gaps

Round 4 closed the first three items below. Open items for a later round:

1. The 16 kHz Annex A tests 1(a) and 2(a) and the P.862.2 wideband mode are out of
   scope for this narrowband port (spec/CONFORMANCE.md section 5); no action unless
   scope changes.
2. The Supplement 23 8 kHz set (test 1(b), 1736 pairs) is documented but not shipped;
   running it would broaden coverage beyond the VoIP set.
3. The debug-mode conformance run takes about 160 s for 40 pairs; a release-mode run
   of the harness and the CLI timing would be worth profiling before any large
   batch scoring.
