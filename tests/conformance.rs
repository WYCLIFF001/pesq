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

// ---------------------------------------------------------------------------
// P.862 Annex A conformance via ITU-T P-series Supplement 23.
//
// The Supp 23 audio is not redistributable, so it is not committed here.
// To run these tests, point `PESQ_SUPP23_DIR` at a directory containing
// the g729char experiment tree of the Supp 23 corpus:
//
// ```text
// PESQ_SUPP23_DIR=/path/to/supp23 cargo test --test conformance -- --nocapture
// ```
//
// Without the variable the tests print a skip note and return, so
// `cargo test` stays green everywhere. The expected values and the
// criteria come from spec/CONFORMANCE-supp23.md section 4 (the 40-pair
// excerpt) and section 2 (the Annex A tolerances): test 1(b) at 8 kHz
// allows at most 2 pairs beyond 0.05 and none beyond 0.1; test 4 in
// wideband mode requires every pair within 0.05. The 8 kHz file names
// carry the ".8k." marker of CONFORMANCE-supp23.md section 1.

/// Path to the Supp 23 conformance excerpt shipped in the repository.
const SUPP23_MD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/spec/CONFORMANCE-supp23.md");

/// Supp 23 test 1(b): at most this many pairs may exceed 0.05.
const SUPP23_8K_MAX_BEYOND: usize = 2;

/// Supp 23 test 1(b): no pair may exceed this difference.
const SUPP23_8K_HARD_LIMIT: f32 = 0.1;

/// Supp 23 test 4: every pair must stay within this difference.
const SUPP23_WB_LIMIT: f32 = 0.05;

/// One Supp 23 vector of the 40-pair excerpt of CONFORMANCE-supp23.md
/// section 4: a reference/degraded pair with the reference scores in
/// all three scoring modes.
struct Supp23Vector {
    index: usize,
    reference: String,
    degraded: String,
    expected_16k: f32,
    expected_8k: f32,
    expected_wb: f32,
}

/// Parse the 40-pair excerpt table of CONFORMANCE-supp23.md section 4.
///
/// Table rows have the form
/// `| 1 | g729char/...src | g729char/...out | 3.575 | 3.613 | 2.824 |`.
/// Rows that do not parse are ignored (headers and prose).
fn parse_supp23_vectors(markdown: &str) -> Vec<Supp23Vector> {
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
        let Some(expected_16k) = cells.next() else {
            continue;
        };
        let Some(expected_8k) = cells.next() else {
            continue;
        };
        let Some(expected_wb) = cells.next() else {
            continue;
        };
        let Ok(index) = index.trim().parse::<usize>() else {
            continue;
        };
        let (Ok(expected_16k), Ok(expected_8k), Ok(expected_wb)) = (
            expected_16k.trim().parse::<f32>(),
            expected_8k.trim().parse::<f32>(),
            expected_wb.trim().parse::<f32>(),
        ) else {
            continue;
        };
        let reference = reference.trim().to_string();
        let degraded = degraded.trim().to_string();
        if reference.is_empty() || degraded.is_empty() {
            continue;
        }
        vectors.push(Supp23Vector {
            index,
            reference,
            degraded,
            expected_16k,
            expected_8k,
            expected_wb,
        });
    }
    vectors
}

/// Read headerless mono 16-bit little-endian PCM (spec 01, 1.2 step 2:
/// names ending in ".raw", ".src", or ".s" have no header).
fn read_raw_pcm(path: &Path) -> Vec<i16> {
    let bytes =
        std::fs::read(path).unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()));
    bytes
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect()
}

/// Insert the ".8k." marker of CONFORMANCE-supp23.md section 1 before
/// the file extension: `a_f01s01.src` becomes `a_f01s01.8k.src`.
fn with_8k_marker(path: &str) -> String {
    let mut marked = String::with_capacity(path.len() + 4);
    match path.rfind('.') {
        Some(dot) => {
            marked.push_str(&path[..dot]);
            marked.push_str(".8k.");
            marked.push_str(&path[dot + 1..]);
        }
        None => marked.push_str(path),
    }
    marked
}

/// Round a score to 3 decimal places, the reporting precision of the
/// reference (spec 05, 5.3 and spec 06, 6.5).
fn round_3dp(value: f32) -> f32 {
    (value * 1000.0).round() / 1000.0
}

/// Supp 23 test 1(b): the 1736-pair corpus at 8 kHz in narrowband mode,
/// run here over the 40-pair excerpt of CONFORMANCE-supp23.md. At most
/// 2 pairs may differ by more than 0.05 and no pair by more than 0.1.
#[test]
fn supp23_8khz_conformance() {
    let Some(dir) = std::env::var_os("PESQ_SUPP23_DIR") else {
        eprintln!(
            "skip: PESQ_SUPP23_DIR is not set; the Supp 23 audio is not \
             shipped in this repository"
        );
        return;
    };
    let base = PathBuf::from(dir);
    let markdown = std::fs::read_to_string(SUPP23_MD)
        .expect("spec/CONFORMANCE-supp23.md must be present in the repository");
    let vectors = parse_supp23_vectors(&markdown);
    assert!(
        vectors.len() == 40,
        "expected 40 Supp 23 excerpt pairs, parsed {}",
        vectors.len()
    );

    let mut beyond: Vec<usize> = Vec::new();
    let mut max_delta = 0.0f32;
    for vector in &vectors {
        let reference = read_raw_pcm(&base.join(with_8k_marker(&vector.reference)));
        let degraded = read_raw_pcm(&base.join(with_8k_marker(&vector.degraded)));
        let score = pesq::pesq_8k(&reference, &degraded)
            .unwrap_or_else(|err| panic!("pair {}: pesq failed: {err}", vector.index));
        let delta = (round_3dp(score) - vector.expected_8k).abs();
        if delta > max_delta {
            max_delta = delta;
        }
        if delta > DELTA_LIMIT {
            beyond.push(vector.index);
        }
        eprintln!(
            "supp23 8k pair {:2}: {:26} {:26} score {:.3} expected {:.3} delta {delta:+.3}",
            vector.index, vector.reference, vector.degraded, round_3dp(score), vector.expected_8k
        );
    }

    // Acceptance criteria of CONFORMANCE-supp23.md section 2, test 1(b).
    assert!(
        beyond.len() <= SUPP23_8K_MAX_BEYOND,
        "{} Supp 23 8 kHz pairs differ by more than {DELTA_LIMIT} \
         (at most {SUPP23_8K_MAX_BEYOND} allowed): {beyond:?}",
        beyond.len()
    );
    assert!(
        max_delta <= SUPP23_8K_HARD_LIMIT,
        "maximum absolute difference {max_delta:.3} exceeds the hard limit \
         of {SUPP23_8K_HARD_LIMIT}"
    );
}

/// Supp 23 test 4: the same pairs at 16 kHz in wideband mode, scored
/// with [`pesq::pesq_wb`] and compared to the P.862.2 MOS-LQO column.
/// Every pair must stay within 0.05 of the reference value.
#[test]
fn supp23_wideband_conformance() {
    let Some(dir) = std::env::var_os("PESQ_SUPP23_DIR") else {
        eprintln!(
            "skip: PESQ_SUPP23_DIR is not set; the Supp 23 audio is not \
             shipped in this repository"
        );
        return;
    };
    let base = PathBuf::from(dir);
    let markdown = std::fs::read_to_string(SUPP23_MD)
        .expect("spec/CONFORMANCE-supp23.md must be present in the repository");
    let vectors = parse_supp23_vectors(&markdown);
    assert!(
        vectors.len() == 40,
        "expected 40 Supp 23 excerpt pairs, parsed {}",
        vectors.len()
    );

    let mut beyond: Vec<usize> = Vec::new();
    let mut max_delta = 0.0f32;
    for vector in &vectors {
        let reference = read_raw_pcm(&base.join(&vector.reference));
        let degraded = read_raw_pcm(&base.join(&vector.degraded));
        let score = pesq::pesq_wb(&reference, &degraded)
            .unwrap_or_else(|err| panic!("pair {}: pesq_wb failed: {err}", vector.index));
        let delta = (round_3dp(score) - vector.expected_wb).abs();
        if delta > max_delta {
            max_delta = delta;
        }
        if delta > SUPP23_WB_LIMIT {
            beyond.push(vector.index);
        }
        eprintln!(
            "supp23 wb pair {:2}: {:26} {:26} score {:.3} expected {:.3} delta {delta:+.3}",
            vector.index, vector.reference, vector.degraded, round_3dp(score), vector.expected_wb
        );
    }

    // Acceptance criteria of CONFORMANCE-supp23.md section 2, test 4.
    assert!(
        max_delta <= SUPP23_WB_LIMIT,
        "{} wideband pairs differ by more than {SUPP23_WB_LIMIT} \
         (maximum {max_delta:.3}): {beyond:?}",
        beyond.len()
    );
}

/// The excerpt parser reads exactly the 40 documented pairs, in order,
/// with the three published scores.
#[test]
fn supp23_parser_reads_the_shipped_excerpt() {
    let markdown = std::fs::read_to_string(SUPP23_MD).unwrap();
    let vectors = parse_supp23_vectors(&markdown);
    assert_eq!(
        vectors.len(),
        40,
        "CONFORMANCE-supp23.md documents a 40-pair excerpt"
    );
    assert_eq!(vectors[0].index, 1);
    assert_eq!(vectors[0].reference, "g729char/exp3/original/a/a_f01s01.src");
    assert_eq!(vectors[0].degraded, "g729char/exp1/coded/a/ae1f5901.out");
    assert_eq!(vectors[0].expected_16k, 3.575);
    assert_eq!(vectors[0].expected_8k, 3.613);
    assert_eq!(vectors[0].expected_wb, 2.824);
    assert_eq!(vectors[39].index, 40);
    assert_eq!(vectors[39].reference, "g729char/exp3/original/a/a_m02s10.src");
    assert_eq!(vectors[39].degraded, "g729char/exp1/coded/a/ae1m3610.out");
    assert_eq!(vectors[39].expected_8k, 3.694);
    assert_eq!(vectors[39].expected_wb, 2.934);
    // The 8 kHz marker lands before the extension.
    assert_eq!(
        with_8k_marker("g729char/exp3/original/a/a_f01s01.src"),
        "g729char/exp3/original/a/a_f01s01.8k.src"
    );
    // Wideband scores published with 2 decimals are parsed as published.
    assert_eq!(vectors[1].expected_wb, 2.97);
}
