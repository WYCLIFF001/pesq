# pesq

[![Crates.io](https://img.shields.io/crates/v/pesq.svg)](https://crates.io/crates/pesq)
[![Documentation](https://docs.rs/pesq/badge.svg)](https://docs.rs/pesq)
[![License](https://img.shields.io/crates/l/pesq.svg)](https://github.com/WYCLIFF001/pesq/blob/master/LICENSE)
[![CI](https://github.com/WYCLIFF001/pesq/workflows/CI/badge.svg)](https://github.com/WYCLIFF001/pesq/actions)

**Pure Rust implementation of ITU-T P.862 (PESQ) speech quality assessment: narrowband P.862 and wideband P.862.2.**

`pesq` scores the quality of a degraded speech signal against a clean reference by modeling human auditory perception. Both signals are level-aligned and delay-aligned, filtered through a perceptual model, and compared per frame in the Bark domain to produce the two disturbance indicators of the standard, which are combined into the raw P.862 score (about -0.5 to 4.5) and mapped to P.862.1 MOS-LQO. Wideband scoring (`pesq_wb`) returns the P.862.2 MOS-LQO directly. The whole crate is Rust, with a single runtime dependency ([`rustfft`](https://crates.io/crates/rustfft)), and it passes the ITU-T P.862 Annex A conformance test for narrowband operation.

## Quick start

Add to your `Cargo.toml`:

```toml
[dependencies]
pesq = "0.2"
```

### Scoring a pair

`pesq(ref, deg)` scores mono 16-bit linear PCM at 16 kHz and returns the raw P.862 score:

```rust
use pesq::score::mos_lqo;

let reference: Vec<i16> = load_pcm("reference.wav"); // 16 kHz mono PCM
let degraded: Vec<i16> = load_pcm("degraded.wav");   // 16 kHz mono PCM

let raw = pesq::pesq(&reference, &degraded)?;
println!("raw PESQ score: {raw:.3}");
println!("MOS-LQO: {:.3}", mos_lqo(f64::from(raw)));
# Ok::<(), pesq::PesqError>(())
```

For 8 kHz PCM use `pesq::pesq_8k`, which scores natively at the model rate without any resampling:

```rust
let raw = pesq::pesq_8k(&reference_8k, &degraded_8k)?;
```

For wideband scoring (P.862.2) use `pesq::pesq_wb`, which accepts 16 kHz PCM and returns the wideband MOS-LQO:

```rust
let wb = pesq::pesq_wb(&reference_16k, &degraded_16k)?;
println!("wideband MOS-LQO: {wb:.3}");
```

### Command-line scorer

The `pesq-cli` binary scores one pair of WAV files and prints the raw P.862 score with 6 decimals:

```text
cargo run --bin pesq-cli -- reference.wav degraded.wav
```

or, after `cargo install pesq`:

```text
pesq-cli reference.wav degraded.wav
```

The files must be mono 16-bit PCM. The sample rate follows the file header: 8 kHz files are scored natively, and 16 kHz files are decimated to the model rate with an anti-aliasing filter. With `--wb` the pair is scored in wideband mode (P.862.2): the files must be 16 kHz, and the printed value is the wideband MOS-LQO of spec 06 section 6.5 instead of the raw score.

## Conformance

The port passes the ITU-T P.862 Annex A conformance test for the 8 kHz narrowband VoIP set (test 2(b)): 40 of 40 pairs within the 0.05 tolerance, and no pair beyond the 0.5 hard limit. The largest delta observed during the convergence rounds was 0.014, on pair 26; it was traced to a frame-boundary bug and fixed, and every pair now matches the published raw scores at 3 decimal places (delta 0.000).

The conformance harness lives in `tests/conformance.rs`. The Annex A WAV files are not redistributable, so they are not committed; point the harness at your copy:

```text
PESQ_CONFORMANCE_DIR=/path/to/annex-a cargo test --test conformance -- --nocapture
```

Without the variable the harness prints a skip note and returns, so `cargo test` stays green everywhere. The per-pair table and the convergence history are in `CONVERGENCE.md`.

A Supplement 23 harness is also wired, gated on `PESQ_SUPP23_DIR` (the 8 kHz test 1(b) criteria of at most 2 pairs beyond 0.05 and none beyond 0.1, and the wideband test 4 criterion of all pairs within 0.05). The official 1736-pair audio is not redistributable and must be obtained from the ITU; until then, wideband convergence is recorded against a proxy corpus in `CONVERGENCE.md` section 6 (64 pairs matching the reference implementation's MOS-LQO to 3 decimals, max delta 0.004).

## Clean-room note

The specification in `spec/` was written by reading the ITU-T P.862 Annex A reference implementation and the published PESQ literature, and it records the algorithm's behavior in implementation-neutral terms: numbered processing steps, exact numeric constants, filter coefficient tables, and conformance vectors.

The Rust implementers did not read the C reference implementation; they implemented only from the files in `spec/`. Anyone who has read the C source may review or test the Rust code but must not write it. This separation is what makes the port legally independent of the ITU reference code. This repository contains no code copied or translated from the ITU reference implementation.

## Patents

PESQ is standardized by ITU-T, and the P.862 technology may be subject to ITU patents (historically held by BT/Psytechnics and KPN/OPTICOM). Some of those patents have expired, but anyone redistributing or commercially deploying PESQ software should verify the patent situation for their use and jurisdiction.

## Roadmap

- Run the official Supplement 23 vectors (tests 1(b) and 4, 1736 pairs each) once the ITU audio is available via `PESQ_SUPP23_DIR`.
- A public 16 kHz narrowband entry point (test 1(a)) and a wideband `PesqContext`, both trivial follow-ups over the rate-parameterized pipeline.
- Release-mode profiling of the harness and the CLI before any large batch scoring.

## Testing

```text
cargo test                     # unit tests; the conformance harness skips without data
cargo bench                    # Criterion benchmark on a synthesized 10 s pair
cargo clippy --all-targets     # lints
```

## Documentation

- [API documentation](https://docs.rs/pesq)
- [Examples](https://github.com/WYCLIFF001/pesq/tree/master/examples)

## Contributing

Contributions are welcome. Please:

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure `cargo test` and `cargo fmt --check` pass
5. Submit a pull request

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as MIT OR Apache-2.0, without any additional terms or conditions.

## License

Dual licensed under either of:

- the MIT license ([LICENSE](LICENSE) or https://opensource.org/licenses/MIT)
- the Apache License, Version 2.0 (https://www.apache.org/licenses/LICENSE-2.0)

at your option.

## Acknowledgments

- The ITU-T P.862 recommendation and its Annex A conformance data.
- The [`rustfft`](https://crates.io/crates/rustfft) crate, the port's single runtime dependency.
