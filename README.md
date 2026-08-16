# pesq

A clean-room Rust implementation of ITU-T P.862 (PESQ), narrowband mode (8 kHz), including the P.862.1 MOS-LQO mapping.

## What this repository will contain

- `spec/`: a complete behavioral specification of the P.862 narrowband algorithm, written in neutral language from the ITU-T P.862 Annex A (2005) reference implementation and the published algorithm literature. Implementers work from these documents only.
- A Rust library and a command-line scorer (to be added in later rounds).
- A conformance test harness that runs the Annex A 8 kHz test vectors.

## Clean-room note

The specification in `spec/` was written by reading the ITU-T P.862 Annex A reference implementation and the published PESQ literature, and it records the algorithm's behavior in implementation-neutral terms: numbered processing steps, exact numeric constants, filter coefficient tables, and conformance vectors.

The Rust implementers must NOT read the C reference implementation. They implement only from the files in `spec/`. Anyone who has read the C source may review or test the Rust code but must not write it. This separation is what makes the port legally independent of the ITU reference code.

## Licensing

The Rust code written here will be published under MIT OR Apache-2.0. The specification documents are original prose and tables of published numeric constants. PESQ itself is standardized by ITU-T and was subject to patents held by BT/Psytechnics and KPN/OPTICOM; those patents have expired, but anyone redistributing or commercially deploying PESQ software should perform their own patent and standards review. This repository does not contain any code copied or translated from the ITU reference implementation.

## Status

- Round 1 (this state): specification complete, awaiting implementer review.
- Round 2: Rust implementation of the model per `spec/`.
- Round 3: conformance run against the vectors in `spec/CONFORMANCE.md`.

## Reading order for implementers

1. `spec/01-input-and-alignment.md` (input, preprocessing, VAD, delay estimation)
2. `spec/02-fft-and-filters.md` (FFT conventions, windows, all filter tables)
3. `spec/03-perceptual-model.md` (Bark bands, loudness, scaling)
4. `spec/04-disturbance.md` (disturbance computation and aggregation)
5. `spec/05-score.md` (final score and MOS-LQO mapping)
6. `spec/CONFORMANCE.md` (test vectors and tolerances)
