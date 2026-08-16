//! Score one reference/degraded pair of WAV files.
//!
//! Usage:
//!
//! ```text
//! cargo run --example basic -- <reference.wav> <degraded.wav>
//! ```
//!
//! The example reads mono 16-bit PCM WAV files at 16 kHz (skipping the
//! 44-byte header per spec 01, 1.2 step 2) and prints the raw P.862 score
//! and the P.862.1 MOS-LQO mapping (spec 05). For 8 kHz files, or for
//! rate-aware dispatch, use the `pesq-cli` binary instead.

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: {} <reference.wav> <degraded.wav>", args[0]);
        std::process::exit(2);
    }
    let reference = read_wav_pcm(Path::new(&args[1]));
    let degraded = read_wav_pcm(Path::new(&args[2]));
    match pesq::pesq(&reference, &degraded) {
        Ok(raw) => {
            println!("raw PESQ score: {raw:.3}");
            println!("MOS-LQO: {:.3}", pesq::score::mos_lqo(f64::from(raw)));
        }
        Err(err) => {
            eprintln!("pesq failed: {err}");
            std::process::exit(1);
        }
    }
}

/// Read mono 16-bit little-endian PCM from a WAV file, skipping the
/// 44-byte header (spec 01, 1.2 step 2).
fn read_wav_pcm(path: &Path) -> Vec<i16> {
    let bytes =
        std::fs::read(path).unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()));
    assert!(
        bytes.len() > 44,
        "{} is shorter than a WAV header",
        path.display()
    );
    bytes[44..]
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect()
}
