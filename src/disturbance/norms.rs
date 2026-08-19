//! Disturbance densities, deadzone, and the two weighted frame norms
//! (spec 04, sections 4.1 to 4.3).

use crate::psychoacoustic::bark_width;
use crate::types::Rate;

/// Remove the deadzone margin from one frame of loudness densities
/// (spec 04, 4.1).
///
/// Per band b the raw disturbance is `loudness_deg[b] - loudness_ref[b]`
/// and the margin is `0.25 * min(loudness_deg[b], loudness_ref[b])`.
/// Disturbances within the margin collapse to zero; larger ones shrink
/// by the margin toward zero. Band 0 is included (spec 04, 4.1), unlike
/// the audibility sums of spec 03.
pub(crate) fn deadzone_removed(loudness_ref: &[f32], loudness_deg: &[f32], out: &mut [f32]) {
    for band in 0..out.len() {
        let margin = 0.25 * loudness_ref[band].min(loudness_deg[band]);
        let mut d = loudness_deg[band] - loudness_ref[band];
        if d > margin {
            d -= margin;
        } else if d < -margin {
            d += margin;
        } else {
            d = 0.0;
        }
        out[band] = d;
    }
}

/// Weighted Lp norm of one frame of disturbance densities (spec 04,
/// 4.2): over bands 1..=41 (band 0 excluded) with the Bark widths of
/// Table 1 as weights,
///
/// ```text
/// (sum (|d[b]| * w[b])^p / sum w[b])^(1/p) * sum w[b].
/// ```
///
/// The sums accumulate in f64 (spec 01, 1.1); callers store the result
/// as f32.
pub(crate) fn lp_norm(d: &[f32], p: f64, rate: Rate) -> f64 {
    let mut weighted = 0.0f64;
    let mut weight_sum = 0.0f64;
    for (band, &value) in d.iter().enumerate().skip(1) {
        let w = f64::from(bark_width(band, rate));
        weighted += (f64::from(value.abs()) * w).powf(p);
        weight_sum += w;
    }
    (weighted / weight_sum).powf(1.0 / p) * weight_sum
}

/// Multiply one frame of deadzone-removed disturbance densities by the
/// asymmetry factor (spec 04, 4.3 steps 1 to 3): per band
/// `r = (p_deg + 50) / (p_ref + 50)`, `h = r^1.2` capped at 12, and
/// `h = 0` whenever the uncapped factor is below 3. Added content, where
/// the degraded density exceeds the reference, is amplified; removed
/// content is suppressed.
pub(crate) fn asymmetric_densities(
    d: &[f32],
    pitch_ref: &[f32],
    pitch_deg: &[f32],
    out: &mut [f32],
) {
    for (((&d, &p_ref), &p_deg), out) in d.iter().zip(pitch_ref).zip(pitch_deg).zip(out.iter_mut())
    {
        let r = (f64::from(p_deg) + 50.0) / (f64::from(p_ref) + 50.0);
        let mut h = r.powf(1.2).min(12.0);
        if h < 3.0 {
            h = 0.0;
        }
        *out = (f64::from(d) * h) as f32;
    }
}

/// Power normalization of one frame's disturbance (spec 04, 4.6 steps 1
/// to 3): divide by `((total_power_ref + 1e5) / 1e7)^0.04` and cap at 45.
pub(crate) fn power_normalized(value: f32, total_power_ref: f32) -> f32 {
    let h = ((f64::from(total_power_ref) + 1e5) / 1e7).powf(0.04);
    (f64::from(value) / h).min(f64::from(super::DISTURBANCE_CAP)) as f32
}
