# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-08-19

### Added

- P.862.2 wideband mode: the `pesq_wb` entry point scores 16 kHz PCM
  through the rate-parameterized pipeline with the wideband input
  filter of spec 06 section 6.3 and returns the P.862.2 MOS-LQO of
  spec 06 section 6.5. The narrowband entry points `pesq` and `pesq_8k`
  are unchanged and their scores remain bit-identical.
- A `types::Rate` value on every signal buffer selects the 16 kHz
  constants of spec 06 section 6.4 (64-sample VAD window, 4800-sample
  margins, 1024-point alignment FFT, 512/256 model frames, the 49-band
  Bark table, the 12-section input IIR cascade, and the 16 kHz pitch
  power scale).
- Supp 23 conformance harness in tests/conformance.rs, gated on the
  `PESQ_SUPP23_DIR` environment variable: the 8 kHz test 1(b) criteria
  (at most 2 pairs beyond 0.05, none beyond 0.1) and the wideband
  test 4 criterion (all pairs within 0.05) over the 40-pair excerpt of
  spec/CONFORMANCE-supp23.md.
- Wideband convergence against a proxy corpus, recorded in
  CONVERGENCE.md section 6: 14 pairs at 5 dB SNR scored by the C
  reference binary (version 2.0, wideband mode) as a black-box oracle,
  max absolute MOS-LQO delta 0.004 and mean 0.0005; 14 identity pairs
  at delta 0.000; a 15 dB SNR set with max delta 0.005 and mean 0.0009.
  Every pair sits at least an order of magnitude inside the Supp 23
  test 4 bound of 0.05.
- The official Supp 23 vectors await the ITU audio: the 1736-pair
  test 4 set is not redistributable, so the harness activates only
  when `PESQ_SUPP23_DIR` points at a local copy.

## [0.1.2] - 2026-08-17

Documentation and formatting release: the same code and scores, now clean
under every gate in CI.

### Changed

- Fixed all 11 rustdoc warnings (qualified intra-doc links, markdown
  escaping); cargo doc --no-deps is now warning-free.
- cargo fmt --all applied; cargo fmt --all --check is clean.

### Fixed

- No behaviour changes; the Annex A conformance remains 40 of 40 pairs
  at a 3-decimal delta of 0.000.

## [0.1.1] - 2026-08-16

Performance update: the same scores, several times faster.

### Changed

- FFT plans are shared process-wide: one lazily initialized planner
  serves every transform, and rustfft's internal per-size plan cache
  reuses the same plan across calls instead of rebuilding one per
  transform (previously the perceptual model also re-planned its
  256-point FFT for every scored pair).
- Hann windows are computed once per length and cloned per call.
- Filter-curve gains (the alignment and IRS receive curves) are
  interpolated once per (curve, FFT size) pair and cached; previously
  every filter application re-evaluated the dB curve per bin with f64
  interpolation and a power function.
- The fine-alignment smoothing kernel of spec 01 section 1.10 step 5
  is a process-wide static instead of a per-call allocation and fill.

### Added

- `PesqContext`: prepare a reference once and score any number of
  degraded variants against it (`PesqContext::new` for 16 kHz PCM,
  `PesqContext::new_8k` for 8 kHz). Scores are bit-identical to `pesq`
  and `pesq_8k`; a degraded signal longer than the reference
  transparently recomputes the reference chain with the larger shared
  normalization divisor of spec 01 section 1.3 step 3.

### Not changed

- Real-FFT plans: every transform in this crate is real-input and runs
  through a complex plan. rustfft 6 has no real FFT planner, and every
  real-FFT algorithm computes different butterfly rounding than the
  zero-imaginary complex transform; with conformance scores pinned to
  3 decimals at delta 0.000 and the tightest pair 2e-6 from a rounding
  boundary, the plans stay complex.
- No per-utterance parallelism: the second pass of the perceptual
  model (local gain scaling, spec 03 section 3.7) is sequentially
  dependent, and the f64 band sums of the first pass accumulate in
  frame order, so parallel accumulation would change rounding and risk
  the 3-decimal conformance. No cargo feature was added.

### Measured

- One 10 s pair (16 kHz, criterion bench, this machine): about 580 ms
  before the update, about 180 to 270 ms after across runs, roughly
  2.5x to 3x faster (the machine ran a heavy background load during
  the measurements, so these are approximate).
- Four degraded variants of one 10 s reference: about 847 ms as four
  `pesq` calls, about 655 ms through `PesqContext`, the reference
  preprocessing paid once instead of four times.

### Conformance

Annex A test 2(b) re-run after every optimization step: 40/40 pairs
at 3-decimal delta 0.000, unchanged. The new context test additionally
asserts the preprocessed-reference scores are bit-identical to
`pesq_8k` across all 40 pairs.

[0.1.1]: https://github.com/WYCLIFF001/pesq/releases/tag/v0.1.1

## [0.1.0] - 2026-08-16

First release: a pure Rust port of the ITU-T P.862 (PESQ) narrowband
speech quality algorithm, written clean-room from a behavioral
specification and validated against the Annex A conformance data.

### Added

- `pesq` and `pesq_8k` entry points for scoring reference/degraded
  pairs of mono 16-bit PCM at 16 kHz and 8 kHz (spec 01, table 1.1).
- The P.862.1 MOS-LQO mapping (`pesq::score::mos_lqo`).
- A command-line scorer (`pesq-cli`) that reads two WAV files, picks
  the 8 kHz or 16 kHz entry point from the file header, and prints the
  raw P.862 score with 6 decimal places.
- The clean-room behavioral specification in `spec/`: five algorithm
  documents plus the conformance vectors and tolerances
  (`spec/CONFORMANCE.md`).
- A conformance harness (`tests/conformance.rs`) driven by the
  `PESQ_CONFORMANCE_DIR` environment variable; it skips cleanly when
  the Annex A WAV files are not present, so `cargo test` stays green
  everywhere.
- Examples: `basic` (score one WAV pair with MOS-LQO) and `diagnose`
  (per-stage diagnostics for the conformance vectors).
- A Criterion benchmark scoring a synthesized 10 second pair.

### Conformance

Annex A test 2(b) (8 kHz VoIP set, 40 pairs): all 40 pairs score within
0.05 of the published raw P.862 values, and no pair exceeds the 0.5
hard limit (CONFORMANCE.md section 2). At the 3-decimal comparison of
the conformance rule every pair now matches with delta 0.000. See
`CONVERGENCE.md` for the per-pair table and the full history.

### History in brief

- Round 1: the clean-room specification of the narrowband algorithm
  was written from the ITU-T P.862 Annex A (2005) reference
  implementation and the published PESQ literature.
- Round 2: the Rust pipeline (input, alignment, DSP, perceptual model,
  disturbance processing, scoring) was implemented from `spec/` only.
- Round 3: the first wired conformance run reported 29 of 40 pairs
  beyond 0.05; a convergence loop against the C reference, used only
  as a black-box oracle, brought all 40 pairs inside 0.05, with a
  maximum delta of 0.014 on pair 26.
- Round 4: pair 26 was traced to a wrong degraded frame boundary and
  fixed (delta 0.000); the 16 kHz entry point gained a proper
  anti-aliasing decimator; `pesq-cli` and this publication metadata
  were added.

[0.1.0]: https://github.com/WYCLIFF001/pesq/releases/tag/v0.1.0
