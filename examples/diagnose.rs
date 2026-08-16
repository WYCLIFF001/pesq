//! Per-stage diagnostics for the conformance vectors.
//!
//! Usage:
//!
//! ```text
//! PESQ_CONFORMANCE_DIR=<dir-with-voip> cargo run --example diagnose
//! ```
//!
//! The example runs the pipeline through the perceptual model (spec 03)
//! and prints one line per conformance pair with the intermediate
//! quantities the conformance pass needs: the coarse delay, the
//! utterance count and the
//! per-utterance fine delays with confidences, the negative-delay skip
//! frame count (spec 01, 1.14), the frame range (spec 03, 3.1), the
//! silent frame count, and the mean per-frame energy of the reference and
//! degraded pitch and loudness densities.
//!
//! The 8 kHz WAV files are upsampled to 16 kHz exactly as the conformance
//! harness does, so this measures the same code path as `pesq::pesq`.

use std::path::{Path, PathBuf};

fn main() {
    let Some(dir) = std::env::var_os("PESQ_CONFORMANCE_DIR") else {
        eprintln!("set PESQ_CONFORMANCE_DIR to a directory with voip/ at its root");
        std::process::exit(2);
    };
    let base = PathBuf::from(dir);
    let markdown =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/spec/CONFORMANCE.md"))
            .expect("spec/CONFORMANCE.md must be present");
    println!(
        "pair  Nmax    coarse  utt  fine delays (confidence)  skip  frames[start..=stop]  \
         silent  pitch ref/deg (dB)  loud ref/deg (dB)"
    );
    for line in markdown.lines() {
        let mut cells = line.split('|');
        let _ = cells.next();
        let Some(index) = cells.next().and_then(|c| c.trim().parse::<usize>().ok()) else {
            continue;
        };
        let Some(reference) = cells.next().map(str::trim) else {
            continue;
        };
        let Some(degraded) = cells.next().map(str::trim) else {
            continue;
        };
        if reference.is_empty() || degraded.is_empty() {
            continue;
        }
        let reference = read_wav_pcm(&base.join(reference));
        let degraded = read_wav_pcm(&base.join(degraded));
        diagnose(index, &reference, &degraded);
    }
}

/// Run the pipeline for one pair and print the stage observations.
fn diagnose(index: usize, reference_8k: &[i16], degraded_8k: &[i16]) {
    use pesq::input::prepare_input;
    use pesq::input::process_pair;
    use pesq::psychoacoustic;
    use pesq::types::PesqError;
    use pesq::utterances::negative_delay_skip_flags;

    let reference = prepare_input(&upsample_8k_to_16k(reference_8k));
    let degraded = prepare_input(&upsample_8k_to_16k(degraded_8k));
    let (reference, degraded) = match (reference, degraded) {
        (Ok(r), Ok(d)) => (r, d),
        (Err(err), _) | (_, Err(err)) => {
            println!("pair {index:2}: input error: {err}");
            return;
        }
    };

    let pair = match process_pair(reference, degraded) {
        Ok(pair) => pair,
        Err(PesqError::NoUtterancesFound) => {
            println!("pair {index:2}: no utterances found");
            return;
        }
        Err(err) => {
            println!("pair {index:2}: alignment error: {err}");
            return;
        }
    };

    let model = psychoacoustic::run_frame_loop(&pair.reference, &pair.degraded, &pair.utterances);
    let frame_count = model.frame_count();
    let skip_flags = negative_delay_skip_flags(&pair.utterances, model.frame_range.stop);

    // Mean per-frame pitch and loudness energy over the processed range,
    // summed over all bands, reported in dB relative to the reference
    // pitch level.
    let range = model.frame_range;
    let mut pitch_ref = 0.0f64;
    let mut pitch_deg = 0.0f64;
    let mut loud_ref = 0.0f64;
    let mut loud_deg = 0.0f64;
    let mut counted = 0usize;
    for frame in range.start..=range.stop {
        pitch_ref += model.pitch_ref[frame * 42..(frame + 1) * 42]
            .iter()
            .map(|&p| f64::from(p))
            .sum::<f64>();
        pitch_deg += model.pitch_deg[frame * 42..(frame + 1) * 42]
            .iter()
            .map(|&p| f64::from(p))
            .sum::<f64>();
        loud_ref += model.loudness_ref[frame * 42..(frame + 1) * 42]
            .iter()
            .map(|&l| f64::from(l))
            .sum::<f64>();
        loud_deg += model.loudness_deg[frame * 42..(frame + 1) * 42]
            .iter()
            .map(|&l| f64::from(l))
            .sum::<f64>();
        counted += 1;
    }
    let mean = |sum: f64| sum / counted.max(1) as f64;
    let db = |ratio: f64| 10.0 * ratio.max(1e-30).log10();

    let n_max = pair.nominal_max();
    let delays: Vec<String> = pair
        .utterances
        .iter()
        .map(|u| format!("{}({:.3})", u.fine_delay, u.confidence))
        .collect();
    println!(
        "pair {index:2}: {n_max:6} {coarse:6} {utt:3} [{delays:60}] {skip:4} \
         {start}..={stop:4} of {frames:4} {silent:4} {pitch_ref:7.2}/{pitch_deg:7.2} \
         {loud_ref:6.2}/{loud_deg:6.2}",
        coarse = pair.utterances.first().map_or(0, |u| u.coarse_delay),
        utt = pair.utterances.len(),
        delays = delays.join(" "),
        skip = skip_flags.iter().filter(|&&s| s).count(),
        start = range.start,
        stop = range.stop,
        frames = frame_count,
        silent = model
            .silence_flags
            .iter()
            .take(frame_count)
            .filter(|&&s| s)
            .count(),
        pitch_ref = db(mean(pitch_ref)),
        pitch_deg = db(mean(pitch_deg)),
        loud_ref = db(mean(loud_ref)),
        loud_deg = db(mean(loud_deg)),
    );
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

/// Upsample 8 kHz PCM to 16 kHz by linear interpolation, matching the
/// conformance harness.
fn upsample_8k_to_16k(samples: &[i16]) -> Vec<i16> {
    let mut upsampled = Vec::with_capacity(samples.len().saturating_mul(2));
    for (i, &sample) in samples.iter().enumerate() {
        upsampled.push(sample);
        let next = samples.get(i + 1).copied().unwrap_or(sample);
        upsampled.push(((i32::from(sample) + i32::from(next)) / 2) as i16);
    }
    upsampled
}
