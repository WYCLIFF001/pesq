# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
