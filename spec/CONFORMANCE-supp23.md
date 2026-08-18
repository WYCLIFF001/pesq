# Conformance vectors (P.862 Annex A, ITU-T P-series Supplement 23)

Source: ITU-T P.862 (2001) Amendment 2 (11/2005), Annex A conformance data. The expected values below are reproduced as published from the Annex A distribution. They cover Annex A tests 1(a), 1(b), and 4, which all use the ITU-T P-series Supplement 23 speech material (the g729 characterization experiments).

## 1. Files shipped with the distribution

| File | Content |
|---|---|
| supp23_16k.txt | Test 1(a): 1736 reference/degraded file pairs and the reference raw PESQ scores at 16 kHz |
| supp23_16k.bat | Batch script, one reference invocation per pair, 16 kHz narrowband mode |
| supp23_8k.txt | Test 1(b): the same 1736 pairs downsampled to 8 kHz (file names carry an ".8k." marker before the extension) and the reference raw PESQ scores at 8 kHz |
| supp23_8k.bat | Batch script, one reference invocation per pair, 8 kHz narrowband mode |
| supp23_wb.txt | Test 4: the same 1736 pairs at 16 kHz and the reference P.862.2 MOS-LQO scores |
| supp23_wb.bat | Batch script, one reference invocation per pair, 16 kHz wideband mode |
| process.bat | Sample script for preparing the 8 kHz material from the 16 kHz Supp 23 files with the ITU Software Tool Library resampler |

The Supp 23 audio files are not part of the Annex A distribution: the file lists reference the g729char experiment directories, and Annex A states that ITU-T P-series Supplement 23 must be obtained separately from the ITU. The WAV files that the distribution does ship (the or/dg/u_ sets in the conform directory) belong to tests 2(a) and 2(b), which CONFORMANCE.md covers.

The local file pesq_results.txt in the conform directory is the results log that the reference executable appends to. It is run output, not published conformance data.

## 2. ITU tolerance criteria (Annex A section A.3.2)

The criteria are stated in Annex A itself (the distribution ships the annex as P.862_amd2_e.pdf). The shipped README describes the data sets but does not restate the tolerances. For each pair, the compared quantity is the absolute difference in PESQ score between the implementation under test and the ANSI-C reference implementation.

| Test | File | Pairs | Lower threshold | Upper threshold (mandatory) |
|---|---|---|---|---|
| 1(a) | supp23_16k.txt | 1736 | Difference may not exceed 0.05 in any situation | The same bound; mandatory for all implementations of PESQ at 16 kHz |
| 1(b) | supp23_8k.txt | 1736 | Difference may exceed 0.05 in not more than 2 file pairs (approx. 0.1% of cases) | Difference may not exceed 0.1 in any case; mandatory for all implementations of PESQ at 8 kHz |
| 4 | supp23_wb.txt | 1736 | Difference may not exceed 0.05 in any situation | The same bound; mandatory for all implementations of P.862.2 |

Annex A pass-condition wording, per test:

- Test 1(a): the absolute difference in the raw PESQ score compared to the reference implementation "is not greater than 0.05 in all cases."
- Test 1(b): the absolute difference "is not greater than 0.05 in more than 2 file pairs (these may be any two of the file pairs), and not greater than 0.1 in all cases."
- Test 4: the absolute difference in the wideband PESQ score compared to the reference implementation "is not greater than 0.05 in all cases."

For tests 1(a) and 4 the lower and upper thresholds coincide at 0.05, so a single bound applies. Test 1(b) is the only Supp 23 test with a two-tier criterion: at most 2 pairs beyond 0.05, none beyond 0.1.

## 3. Data set structure

All three files list the same 1736 pairs in the same order. The 8 kHz file inserts ".8k." before each file extension and the wideband file uses backslash separators and upper-case headers; the pair identities are otherwise identical. Per-condition pair counts:

| Condition | Pairs |
|---|---|
| exp1/d | 176 |
| exp1/o | 176 |
| exp2/a | 136 |
| exp2/d | 136 |
| exp2/e | 136 |
| exp3/a | 376 |
| exp3/c | 200 |
| exp3/d | 200 |
| exp3/o | 200 |
| Total | 1736 |

File formats: tab-separated text, one pair per line, with a header row.

- supp23_16k.txt columns: Reference, Degraded, Fsample, PESQ_score. Fsample is 16000 on every row; the score column is the raw PESQ score.
- supp23_8k.txt columns: the same four names. Fsample is 8000 on every row.
- supp23_wb.txt columns: REFERENCE, DEGRADED, Fsample, P.862.2_SCORE, with leading spaces after the separators. Fsample is 16000 on every row; the score column is the P.862.2 MOS-LQO score (wideband mode output). Some wideband scores are published with 2 decimal places; reproduce them as published.

Scores in the 16 kHz and 8 kHz files are published with 2 or 3 decimal places; reproduce them as published.

Integrity checks (SHA-256 of the shipped files):

| File | SHA-256 |
|---|---|
| supp23_16k.txt | 4b7f6bb06a621a048efdd8f5a03ea0cba7396c2ffed5c7a2f7095322e2683cae |
| supp23_8k.txt | fbb9e732c97603ed63947948daf603e87ab2260474bbb2760edc7c3b5ca00c18 |
| supp23_wb.txt | 46680faa15f03ec8c22bf467145b24da306051e75097df8a4011c0c2c743f41b |
| supp23_16k.bat | 93c89509524cfcb287c24d7bceb9774344e8898edd2b24c1b9828c5d5e559e08 |
| supp23_8k.bat | 35aa6266d4f445c4ed3f1fa225a451a14bcadc36b90eade9f9818bfa7fa364f2 |
| supp23_wb.bat | 8ea3875e6d2e331aaaee7dabaa0812bbad4b8915aa6dc8bebb0186b435954b93 |

## 4. Expected values

The complete vectors are 1736 rows per file, which is too large to reproduce inside this document (this file's line budget is 500 lines). The complete vectors live in the shipped txt files of section 1, and their integrity is verifiable with the hashes of section 3. The first 40 pairs are reproduced below in all three scoring modes as a spot check; the reference and degraded paths are as published in supp23_16k.txt.

| Pair | Reference | Degraded | Raw P.862 at 16 kHz | Raw P.862 at 8 kHz | P.862.2 MOS-LQO |
|---|---|---|---|---|---|
| 1 | g729char/exp3/original/a/a_f01s01.src | g729char/exp1/coded/a/ae1f5901.out | 3.575 | 3.613 | 2.824 |
| 2 | g729char/exp3/original/a/a_f02s01.src | g729char/exp1/coded/a/ae1f8501.out | 3.603 | 3.673 | 2.97 |
| 3 | g729char/exp3/original/a/a_m01s01.src | g729char/exp1/coded/a/ae1m0101.out | 3.919 | 3.941 | 3.463 |
| 4 | g729char/exp3/original/a/a_m02s01.src | g729char/exp1/coded/a/ae1m2d01.out | 3.968 | 3.983 | 3.415 |
| 5 | g729char/exp3/original/a/a_f01s02.src | g729char/exp1/coded/a/ae1f5a02.out | 3.229 | 3.316 | 2.398 |
| 6 | g729char/exp3/original/a/a_f02s02.src | g729char/exp1/coded/a/ae1f8602.out | 3.272 | 3.327 | 2.242 |
| 7 | g729char/exp3/original/a/a_m01s02.src | g729char/exp1/coded/a/ae1m0202.out | 3.63 | 3.68 | 3.209 |
| 8 | g729char/exp3/original/a/a_m02s02.src | g729char/exp1/coded/a/ae1m2e02.out | 3.473 | 3.557 | 2.727 |
| 9 | g729char/exp3/original/a/a_f01s03.src | g729char/exp1/coded/a/ae1f5b03.out | 2.798 | 2.902 | 1.821 |
| 10 | g729char/exp3/original/a/a_f02s03.src | g729char/exp1/coded/a/ae1f8703.out | 2.999 | 3.096 | 2.11 |
| 11 | g729char/exp3/original/a/a_m01s03.src | g729char/exp1/coded/a/ae1m0303.out | 3.108 | 3.202 | 2.21 |
| 12 | g729char/exp3/original/a/a_m02s03.src | g729char/exp1/coded/a/ae1m2f03.out | 3.177 | 3.296 | 2.285 |
| 13 | g729char/exp3/original/a/a_f01s04.src | g729char/exp1/coded/a/ae1f5c04.out | 3.876 | 3.9 | 3.315 |
| 14 | g729char/exp3/original/a/a_f02s04.src | g729char/exp1/coded/a/ae1f8804.out | 3.746 | 3.795 | 3.433 |
| 15 | g729char/exp3/original/a/a_m01s04.src | g729char/exp1/coded/a/ae1m0404.out | 4.155 | 4.18 | 3.642 |
| 16 | g729char/exp3/original/a/a_m02s04.src | g729char/exp1/coded/a/ae1m3004.out | 4.097 | 4.116 | 3.146 |
| 17 | g729char/exp3/original/a/a_f01s05.src | g729char/exp1/coded/a/ae1f5d05.out | 3.316 | 3.362 | 3.055 |
| 18 | g729char/exp3/original/a/a_f02s05.src | g729char/exp1/coded/a/ae1f8905.out | 3.326 | 3.397 | 2.555 |
| 19 | g729char/exp3/original/a/a_m01s05.src | g729char/exp1/coded/a/ae1m0505.out | 3.54 | 3.568 | 2.471 |
| 20 | g729char/exp3/original/a/a_m02s05.src | g729char/exp1/coded/a/ae1m3105.out | 3.513 | 3.568 | 2.605 |
| 21 | g729char/exp3/original/a/a_f01s06.src | g729char/exp1/coded/a/ae1f5e06.out | 3.663 | 3.709 | 2.941 |
| 22 | g729char/exp3/original/a/a_f02s06.src | g729char/exp1/coded/a/ae1f8a06.out | 3.551 | 3.583 | 2.752 |
| 23 | g729char/exp3/original/a/a_m01s06.src | g729char/exp1/coded/a/ae1m0606.out | 3.992 | 4.011 | 3.568 |
| 24 | g729char/exp3/original/a/a_m02s06.src | g729char/exp1/coded/a/ae1m3206.out | 3.942 | 3.975 | 3.667 |
| 25 | g729char/exp3/original/a/a_f01s07.src | g729char/exp1/coded/a/ae1f5f07.out | 4.084 | 4.146 | 3.854 |
| 26 | g729char/exp3/original/a/a_f02s07.src | g729char/exp1/coded/a/ae1f8b07.out | 4.131 | 4.153 | 3.738 |
| 27 | g729char/exp3/original/a/a_m01s07.src | g729char/exp1/coded/a/ae1m0707.out | 4.206 | 4.242 | 3.58 |
| 28 | g729char/exp3/original/a/a_m02s07.src | g729char/exp1/coded/a/ae1m3307.out | 4.304 | 4.311 | 3.561 |
| 29 | g729char/exp3/original/a/a_f01s08.src | g729char/exp1/coded/a/ae1f6008.out | 3.127 | 3.209 | 2.054 |
| 30 | g729char/exp3/original/a/a_f02s08.src | g729char/exp1/coded/a/ae1f8c08.out | 3.209 | 3.294 | 2.583 |
| 31 | g729char/exp3/original/a/a_m01s08.src | g729char/exp1/coded/a/ae1m0808.out | 3.644 | 3.687 | 3.006 |
| 32 | g729char/exp3/original/a/a_m02s08.src | g729char/exp1/coded/a/ae1m3408.out | 3.775 | 3.841 | 3.583 |
| 33 | g729char/exp3/original/a/a_f01s09.src | g729char/exp1/coded/a/ae1f6109.out | 3.154 | 3.216 | 2.335 |
| 34 | g729char/exp3/original/a/a_f02s09.src | g729char/exp1/coded/a/ae1f8d09.out | 3.343 | 3.407 | 2.654 |
| 35 | g729char/exp3/original/a/a_m01s09.src | g729char/exp1/coded/a/ae1m0909.out | 3.548 | 3.559 | 2.818 |
| 36 | g729char/exp3/original/a/a_m02s09.src | g729char/exp1/coded/a/ae1m3509.out | 3.8 | 3.802 | 3.149 |
| 37 | g729char/exp3/original/a/a_f01s10.src | g729char/exp1/coded/a/ae1f6210.out | 3.027 | 3.076 | 2.108 |
| 38 | g729char/exp3/original/a/a_f02s10.src | g729char/exp1/coded/a/ae1f8e10.out | 3.169 | 3.218 | 2.47 |
| 39 | g729char/exp3/original/a/a_m01s10.src | g729char/exp1/coded/a/ae1m0a10.out | 3.257 | 3.36 | 2.415 |
| 40 | g729char/exp3/original/a/a_m02s10.src | g729char/exp1/coded/a/ae1m3610.out | 3.683 | 3.694 | 2.934 |

## 5. Notes

1. Test 1(b) scores were produced on 8 kHz versions of the Supp 23 files downsampled with the ITU Software Tool Library 2000 release 3, program filter, using the command "filter -down HQ2" (Annex A section A.3.2.1(b)). A harness that reruns test 1(b) must prepare the audio the same way.
2. Annex A describes the material as "all files from all ten experiments as released with ITU-T P-series Supplement 23"; the shipped pair list spans experiments 1 to 3 across the 9 condition groups of section 3. The pair count (1736) and the shipped lists are authoritative for conformance.
3. Tests 1(a) and 4 use the same 16 kHz audio; they differ only in the operating mode and the reported score (raw PESQ versus P.862.2 MOS-LQO).
4. The Supp 23 audio itself must be obtained from the ITU. The Annex A distribution ships only the pair lists, scores, and batch scripts.
5. The 16 kHz scores of test 1(a) and the wideband scores of test 4 are produced from the same 16 kHz Supp 23 files, so one audio corpus purchase covers both tests.
