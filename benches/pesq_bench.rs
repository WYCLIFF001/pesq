//! Criterion benchmark: score one 10 second reference/degraded pair.
//!
//! The pair is synthesized deterministically: both signals carry
//! pseudo-noise bursts in 60-window blocks with 30-window silent gaps
//! (the same pattern as the library pipeline tests), long enough for
//! the VAD and the delay alignment to find real utterances, and seeded
//! differently so the alignment has actual delay work to do.
//!
//! Run with `cargo bench --bench pesq_bench`.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

/// The window length of the model in samples (spec 01).
const WINDOW_SAMPLES: usize = 32;

/// Build `seconds` of 16 kHz mono PCM with deterministic noise bursts.
///
/// Bursts of 60 windows alternate with 30-window silent gaps, so a
/// 10 s signal holds roughly 28 bursts. The amplitudes come from a
/// fixed-multiplier LCG, which keeps the pair reproducible across runs.
fn noise_bursts_16k(seconds: usize, seed: u32) -> Vec<i16> {
    const RATE: usize = 16_000;
    let burst_samples = 60 * 2 * WINDOW_SAMPLES;
    let cycle_samples = 90 * 2 * WINDOW_SAMPLES;
    let mut pcm = vec![0i16; seconds * RATE];
    let mut state = seed;
    let mut offset = 0usize;
    while offset + burst_samples <= pcm.len() {
        for sample in &mut pcm[offset..offset + burst_samples] {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *sample = (((state as f32 / u32::MAX as f32) * 2.0 - 1.0) * 3000.0) as i16;
        }
        offset += cycle_samples;
    }
    pcm
}

fn bench_pesq_10s_pair(c: &mut Criterion) {
    let reference = noise_bursts_16k(10, 21);
    let degraded = noise_bursts_16k(10, 42);
    c.bench_function("pesq_10s_pair_16k", |b| {
        b.iter(|| pesq::pesq(black_box(&reference), black_box(&degraded)))
    });
}

criterion_group!(benches, bench_pesq_10s_pair, bench_pesq_context_4_variants);
criterion_main!(benches);

/// The motivating workload for [`pesq::PesqContext`]: one 10 s
/// reference scored against four degraded variants of itself. The
/// direct path calls [`pesq::pesq`] four times; the context path
/// prepares the reference once and scores the four variants.
fn bench_pesq_context_4_variants(c: &mut Criterion) {
    let reference = noise_bursts_16k(10, 21);
    let variants: Vec<Vec<i16>> = (0..4)
        .map(|i| noise_bursts_16k(10, 42 + i as u32))
        .collect();
    let mut group = c.benchmark_group("pesq_10s_4_variants");
    group.bench_function("pesq_4_pairs", |b| {
        b.iter(|| {
            for variant in &variants {
                pesq::pesq(black_box(&reference), black_box(variant)).unwrap();
            }
        })
    });
    group.bench_function("context_4_variants", |b| {
        let context = pesq::PesqContext::new(&reference).unwrap();
        b.iter(|| {
            for variant in &variants {
                context.score(black_box(variant)).unwrap();
            }
        })
    });
    group.finish();
}
