//! Time weighting of long signals (spec 04, 4.7).

use crate::types::Rate;

use super::LONG_SIGNAL_FRAME_COUNT;

/// Per-frame time weights of spec 04 section 4.7, indexed by absolute
/// frame over `0..=frame_stop`.
///
/// For short signals (at most [`LONG_SIGNAL_FRAME_COUNT`] frames) every
/// weight is 1. Otherwise, with `n = (Nmax - 4800)/Q - 1` and
/// `f = (n - 1000)/5500` capped at 0.5, the weight of frame `frame` is
/// `(1 - f) + f * frame / n`: frames later in a long signal are
/// weighted more heavily.
pub(crate) fn time_weights(frame_stop: usize, n_max: usize, rate: Rate) -> Vec<f32> {
    let mut weights = vec![1.0f32; frame_stop + 1];
    if frame_stop < LONG_SIGNAL_FRAME_COUNT {
        return weights;
    }
    let n = ((n_max - 2 * rate.margin_samples()) / rate.frame_hop()) - 1;
    let f = ((n as f64 - 1000.0) / 5500.0).min(0.5);
    let n = n as f64;
    for (frame, weight) in weights.iter_mut().enumerate() {
        *weight = ((1.0 - f) + f * frame as f64 / n) as f32;
    }
    weights
}
