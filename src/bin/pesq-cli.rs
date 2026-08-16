//! Command-line scorer for the `pesq` crate.
//!
//! Usage:
//!
//! ```text
//! pesq-cli <reference.wav> <degraded.wav>
//! ```
//!
//! Reads two mono 16-bit PCM WAV files, picks the 8 kHz entry point for
//! 8 kHz files and the 16 kHz entry point for 16 kHz files, and prints
//! the raw P.862 score (spec 05, 5.1) on stdout. Anything else goes to
//! stderr, and the exit code is nonzero on failure.

use std::fmt;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: {} <reference.wav> <degraded.wav>", args[0]);
        eprintln!("scores the pair and prints the raw P.862 score");
        std::process::exit(2);
    }
    let reference = match read_wav(&args[1]) {
        Ok(wav) => wav,
        Err(err) => {
            eprintln!("{}: {err}", args[1]);
            std::process::exit(1);
        }
    };
    let degraded = match read_wav(&args[2]) {
        Ok(wav) => wav,
        Err(err) => {
            eprintln!("{}: {err}", args[2]);
            std::process::exit(1);
        }
    };
    if reference.sample_rate != degraded.sample_rate {
        eprintln!(
            "sample rates differ: {} Hz vs {} Hz",
            reference.sample_rate, degraded.sample_rate
        );
        std::process::exit(1);
    }
    let score = match reference.sample_rate {
        8000 => pesq::pesq_8k(&reference.samples, &degraded.samples),
        16000 => pesq::pesq(&reference.samples, &degraded.samples),
        rate => {
            eprintln!("unsupported sample rate {rate} Hz (expected 8000 or 16000)");
            std::process::exit(1);
        }
    };
    match score {
        Ok(raw) => println!("{raw:.6}"),
        Err(err) => {
            eprintln!("pesq failed: {err}");
            std::process::exit(1);
        }
    }
}

/// A mono 16-bit PCM WAV file loaded from disk.
struct Wav {
    sample_rate: u32,
    samples: Vec<i16>,
}

/// An error while reading a WAV file.
#[derive(Debug)]
enum WavError {
    Io(std::io::Error),
    /// The file is not a RIFF/WAVE container, or a chunk is malformed.
    Format(&'static str),
}

impl fmt::Display for WavError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WavError::Io(err) => write!(f, "cannot read file: {err}"),
            WavError::Format(what) => write!(f, "not a supported WAV file: {what}"),
        }
    }
}

impl From<std::io::Error> for WavError {
    fn from(err: std::io::Error) -> Self {
        WavError::Io(err)
    }
}

/// Parse a mono 16-bit PCM WAV file.
///
/// Chunk layout follows the RIFF specification: "RIFF" tag, size, "WAVE"
/// form, then an "fmt " chunk (PCM format code 1, one channel, 16 bits
/// per sample) and a "data" chunk, in any order, with other chunks
/// skipped.
fn read_wav(path: &str) -> Result<Wav, WavError> {
    let data = std::fs::read(path)?;
    let bytes = data.as_slice();
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(WavError::Format("missing RIFF/WAVE header"));
    }
    let mut sample_rate = None;
    let mut pcm = None;
    let mut offset = 12usize;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        let body = offset + 8;
        if body + size > bytes.len() {
            return Err(WavError::Format("chunk extends past end of file"));
        }
        match id {
            b"fmt " => {
                if size < 16 || u16::from_le_bytes([bytes[body], bytes[body + 1]]) != 1 {
                    return Err(WavError::Format("not 16-bit PCM"));
                }
                let channels = u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]);
                if channels != 1 {
                    return Err(WavError::Format("not mono"));
                }
                let bits = u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]);
                if bits != 16 {
                    return Err(WavError::Format("not 16 bits per sample"));
                }
                sample_rate = Some(u32::from_le_bytes([
                    bytes[body + 4],
                    bytes[body + 5],
                    bytes[body + 6],
                    bytes[body + 7],
                ]));
            }
            b"data" => pcm = Some(&bytes[body..body + size]),
            _ => {}
        }
        offset = body + size + (size & 1);
    }
    let Some(sample_rate) = sample_rate else {
        return Err(WavError::Format("missing fmt chunk"));
    };
    let Some(pcm) = pcm else {
        return Err(WavError::Format("missing data chunk"));
    };
    let samples = pcm
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    Ok(Wav {
        sample_rate,
        samples,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal 8 kHz mono 16-bit WAV with two samples: 1000, -2000.
    fn wav_bytes() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&36u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&16000u32.to_le_bytes()); // byte rate
        bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
        bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&1000i16.to_le_bytes());
        bytes.extend_from_slice(&(-2000i16).to_le_bytes());
        bytes
    }

    #[test]
    fn wav_reader_parses_pcm_and_metadata() {
        let path = std::env::temp_dir().join("pesq-cli-test-parse.wav");
        std::fs::write(&path, wav_bytes()).unwrap();
        let wav = read_wav(path.to_str().unwrap()).unwrap();
        assert_eq!(wav.sample_rate, 8000);
        assert_eq!(wav.samples, [1000, -2000]);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn wav_reader_rejects_non_pcm_and_non_mono() {
        let path = std::env::temp_dir().join("pesq-cli-test-reject.wav");
        let mut bytes = wav_bytes();
        bytes[20] = 2; // format code 2 (not PCM)
        std::fs::write(&path, &bytes).unwrap();
        assert!(read_wav(path.to_str().unwrap()).is_err());
        bytes = wav_bytes();
        bytes[22] = 2; // two channels
        std::fs::write(&path, &bytes).unwrap();
        assert!(read_wav(path.to_str().unwrap()).is_err());
        std::fs::remove_file(&path).unwrap();
    }
}
