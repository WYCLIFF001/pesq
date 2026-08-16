//! Conformance harness for the P.862 Annex A 8 kHz VoIP vectors.
//!
//! The ITU Annex A WAV files are not redistributable, so they are not
//! committed to this repository. To run the harness, point the
//! `PESQ_CONFORMANCE_DIR` environment variable at a directory containing
//! the Annex A file tree with `voip/` at its root:
//!
//! ```text
//! PESQ_CONFORMANCE_DIR=/path/to/annex-a cargo test --test conformance -- --nocapture
//! ```
//!
//! Without the variable the test prints a skip note and returns, so
//! `cargo test` stays green everywhere. The expected values are parsed
//! from `spec/CONFORMANCE.md`, and the acceptance criteria are those of
//! CONFORMANCE.md section 2: at most one pair may differ from the
//! expected value by more than 0.05, and no pair may differ by more than
//! 0.5. Scores are compared rounded to 3 decimal places (CONFORMANCE.md
//! section 6).
//!
//! The harness feeds the original 8 kHz WAV samples to the 8 kHz entry
//! point [`pesq::pesq_8k`] (CONFORMANCE.md section 6 item 4); no rate
//! conversion is applied on either side.

use std::path::{Path, PathBuf};

/// Path to the conformance vectors shipped in the repository.
const CONFORMANCE_MD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/spec/CONFORMANCE.md");

/// Tolerance for a pair to count as conformant (CONFORMANCE.md section 2).
const DELTA_LIMIT: f32 = 0.05;

/// Maximum pairs allowed to exceed [`DELTA_LIMIT`] (CONFORMANCE.md
/// section 2).
const MAX_PAIRS_BEYOND_DELTA: usize = 1;

/// Absolute difference no pair may exceed (CONFORMANCE.md section 2).
const HARD_DELTA_LIMIT: f32 = 0.5;

/// One conformance vector: a reference/degraded pair and its expected
/// raw P.862 score.
struct Vector {
    index: usize,
    reference: String,
    degraded: String,
    expected: f32,
}

/// Parse the test 2(b) table out of CONFORMANCE.md.
///
/// Table rows have the form
/// `| 1 | voip/or105.wav | voip/dg105.wav | 2.237 |`. Rows that do not
/// parse are ignored (headers and prose).
fn parse_vectors(markdown: &str) -> Vec<Vector> {
    let mut vectors = Vec::new();
    for line in markdown.lines() {
        let mut cells = line.split('|');
        let _first = cells.next();
        let Some(index) = cells.next() else { continue };
        let Some(reference) = cells.next() else {
            continue;
        };
        let Some(degraded) = cells.next() else {
            continue;
        };
        let Some(expected) = cells.next() else {
            continue;
        };
        let Ok(index) = index.trim().parse::<usize>() else {
            continue;
        };
        let Ok(expected) = expected.trim().parse::<f32>() else {
            continue;
        };
        let reference = reference.trim().to_string();
        let degraded = degraded.trim().to_string();
        if reference.is_empty() || degraded.is_empty() {
            continue;
        }
        vectors.push(Vector {
            index,
            reference,
            degraded,
            expected,
        });
    }
    vectors
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

#[test]
fn annex_a_8khz_voip_conformance() {
    let Some(dir) = std::env::var_os("PESQ_CONFORMANCE_DIR") else {
        eprintln!(
            "skip: PESQ_CONFORMANCE_DIR is not set; the Annex A WAV files are not \
             shipped in this repository"
        );
        return;
    };
    let base = PathBuf::from(dir);
    let markdown = std::fs::read_to_string(CONFORMANCE_MD)
        .expect("spec/CONFORMANCE.md must be present in the repository");
    let vectors = parse_vectors(&markdown);
    assert!(
        !vectors.is_empty(),
        "no conformance vectors parsed from {CONFORMANCE_MD}"
    );
    eprintln!(
        "conformance: {} vectors from {CONFORMANCE_MD}",
        vectors.len()
    );

    let mut beyond_delta: Vec<usize> = Vec::new();
    let mut max_delta = 0.0f32;
    for vector in &vectors {
        let reference = read_wav_pcm(&base.join(&vector.reference));
        let degraded = read_wav_pcm(&base.join(&vector.degraded));
        let score = pesq::pesq_8k(&reference, &degraded)
            .unwrap_or_else(|err| panic!("pair {}: pesq failed: {err}", vector.index));
        let rounded = (score * 1000.0).round() / 1000.0;
        let delta = (rounded - vector.expected).abs();
        if delta > max_delta {
            max_delta = delta;
        }
        if delta > DELTA_LIMIT {
            beyond_delta.push(vector.index);
        }
        eprintln!(
            "pair {:2}: {:26} {:26} score {rounded:.3} expected {:.3} delta {delta:+.3}",
            vector.index, vector.reference, vector.degraded, vector.expected
        );
    }

    // Acceptance criteria of CONFORMANCE.md section 2.
    assert!(
        beyond_delta.len() <= MAX_PAIRS_BEYOND_DELTA,
        "{} pairs differ from the expected value by more than {DELTA_LIMIT} \
         (at most {MAX_PAIRS_BEYOND_DELTA} allowed): {beyond_delta:?}",
        beyond_delta.len()
    );
    assert!(
        max_delta <= HARD_DELTA_LIMIT,
        "maximum absolute difference {max_delta:.3} exceeds the hard limit \
         of {HARD_DELTA_LIMIT}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_parser_reads_the_shipped_table() {
        let markdown = std::fs::read_to_string(CONFORMANCE_MD).unwrap();
        let vectors = parse_vectors(&markdown);
        assert_eq!(vectors.len(), 40, "CONFORMANCE.md documents 40 pairs");
        assert_eq!(vectors[0].index, 1);
        assert_eq!(vectors[0].reference, "voip/or105.wav");
        assert_eq!(vectors[0].degraded, "voip/dg105.wav");
        assert_eq!(vectors[0].expected, 2.237);
        assert_eq!(vectors[39].index, 40);
        assert_eq!(vectors[39].reference, "voip/u_am1s03.wav");
        assert_eq!(vectors[39].degraded, "voip/u_am1s03b2c18.wav");
        assert_eq!(vectors[39].expected, 2.540);
    }
}

/// The preprocessed-reference API must reproduce the full pipeline bit
/// for bit: each Annex A reference is prepared once and every degraded
/// variant scored through the context, then compared to the `pesq_8k`
/// score of the same pair.
#[test]
fn pesq_context_matches_pesq_8k_on_the_annex_a_vectors() {
    let Some(dir) = std::env::var_os("PESQ_CONFORMANCE_DIR") else {
        eprintln!(
            "skip: PESQ_CONFORMANCE_DIR is not set; the Annex A WAV files are not \
             shipped in this repository"
        );
        return;
    };
    let base = PathBuf::from(dir);
    let markdown = std::fs::read_to_string(CONFORMANCE_MD)
        .expect("spec/CONFORMANCE.md must be present in the repository");
    let vectors = parse_vectors(&markdown);
    let mut by_reference: std::collections::HashMap<&str, Vec<&Vector>> =
        std::collections::HashMap::new();
    for vector in &vectors {
        by_reference
            .entry(vector.reference.as_str())
            .or_default()
            .push(vector);
    }
    for (&reference, group) in &by_reference {
        let reference_pcm = read_wav_pcm(&base.join(reference));
        let context = pesq::PesqContext::new_8k(&reference_pcm)
            .unwrap_or_else(|err| panic!("{reference}: context failed: {err}"));
        for vector in group {
            let degraded = read_wav_pcm(&base.join(&vector.degraded));
            let expected = pesq::pesq_8k(&reference_pcm, &degraded)
                .unwrap_or_else(|err| panic!("pair {}: pesq failed: {err}", vector.index));
            let score = context
                .score(&degraded)
                .unwrap_or_else(|err| panic!("pair {}: context failed: {err}", vector.index));
            assert_eq!(
                score.to_bits(),
                expected.to_bits(),
                "pair {}: PesqContext score diverged from pesq_8k",
                vector.index
            );
        }
    }
    eprintln!(
        "context: {} references scored bit-identically to pesq_8k",
        by_reference.len()
    );
}
