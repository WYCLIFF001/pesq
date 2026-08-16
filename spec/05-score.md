# 05. Final score and MOS-LQO mapping

## 5.1 Raw PESQ score

Let D be the aggregated symmetric disturbance indicator and A the aggregated asymmetric disturbance indicator from 04 section 4.8.

Raw PESQ score = 4.5 - 0.1 * D - 0.0309 * A.

This is the published PESQ combination (weights 0.1 and 0.0309). The raw score is reported to 3 decimal places. No further clipping is applied to the raw score; its range in practice is about -0.5 to 4.5.

## 5.2 P.862.1 MOS-LQO mapping

The P.862.1 mapping polynomial applied to the raw score x:

MOS-LQO = 0.999 + 4.0 / (1.0 + e^(-1.4945 * x + 4.6607)).

Reported to 3 decimal places.

## 5.3 Reference values for verification

For each input pair, the implementation prints the raw score and the mapped score, both rounded to 3 decimals. The conformance vectors and tolerances are in CONFORMANCE.md.

## 5.4 Documented choices where sources differ

1. Bad-interval detection threshold: this specification uses 30 on the pre-normalization symmetric frame disturbance (04 section 4.2 and 4.5), as the reference implementation does. The Rix et al. 2001 paper describes bad frames with a threshold of 45; the reference implementation uses 45 as the cap on the power-normalized disturbances (04 section 4.6). Both numbers appear in this specification at the reference implementation's positions.
2. The DC removal step divides the mean by the nominal length N instead of the length of the interval the mean is taken over (01 section 1.5). This quirk of the reference implementation is specified as normative.
3. The frequency response compensation (03 section 3.5) is applied to the reference signal, matching both the reference implementation and the published description.
4. The published description of the utterance/delay estimation is less detailed than the reference implementation; sections 1.8 to 1.14 follow the reference implementation exactly, including integer-division truncation semantics.
