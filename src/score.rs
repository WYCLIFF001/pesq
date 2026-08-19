//! Final score and MOS-LQO mapping (spec 05).

/// Raw PESQ score from the aggregated disturbance indicators
/// (spec 05, 5.1): `4.5 - 0.1 * D - 0.0309 * A`.
///
/// No clipping is applied; the raw score ranges in practice from about
/// -0.5 to 4.5.
pub fn raw_score(symmetric: f64, asymmetric: f64) -> f32 {
    (4.5 - 0.1 * symmetric - 0.0309 * asymmetric) as f32
}

/// P.862.1 MOS-LQO mapping of a raw PESQ score (spec 05, 5.2):
/// `0.999 + 4.0 / (1.0 + e^(-1.4945 * x + 4.6607))`.
pub fn mos_lqo(raw: f64) -> f32 {
    (0.999 + 4.0 / (1.0 + (-1.4945 * raw + 4.6607).exp())) as f32
}

/// P.862.2 MOS-LQO mapping of the raw score (spec 06, 6.5):
/// `0.999 + 4.0 / (1.0 + e^(-1.3669 * x + 3.8224))`. The input is the
/// unrounded f32 raw score, exactly as in narrowband mode; wideband mode
/// computes the raw score but reports only this mapped value (spec 06,
/// 6.5). The two mappings are not interchangeable.
pub fn mos_lqo_wb(raw: f64) -> f32 {
    (0.999 + 4.0 / (1.0 + (-1.3669 * raw + 3.8224).exp())) as f32
}

/// Round a score to 3 decimal places, the reporting precision of the
/// specification (spec 05, 5.3).
pub fn round_3dp(value: f32) -> f32 {
    (value * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_score_matches_spec_5_1() {
        // 4.5 - 0.1 * 10 - 0.0309 * 20 = 2.882.
        let score = raw_score(10.0, 20.0);
        assert!((score - 2.882).abs() < 1e-5, "got {score}");
        // A clean pair (zero disturbance) scores 4.5.
        assert_eq!(raw_score(0.0, 0.0), 4.5);
    }

    #[test]
    fn mos_lqo_stays_in_range_and_grows() {
        for raw in [-0.5, 0.0, 1.0, 2.5, 4.5] {
            let mos = mos_lqo(raw);
            assert!(
                (0.9..=4.7).contains(&mos),
                "mos_lqo({raw}) = {mos} is out of range"
            );
        }
        assert!(mos_lqo(4.5) > mos_lqo(1.0));
        assert!(mos_lqo(1.0) > mos_lqo(-0.5));
    }

    #[test]
    fn rounding_keeps_three_decimals() {
        assert_eq!(round_3dp(2.2374), 2.237);
        assert_eq!(round_3dp(2.2375), 2.238);
        assert_eq!(round_3dp(-0.0004), -0.0);
    }

    /// Hand-computed mappings of the spec 05 formula
    /// `0.999 + 4 / (1 + e^(-1.4945x + 4.6607))`.
    #[test]
    fn mos_lqo_matches_hand_computed_values() {
        // x = 0: e^4.6607 = 105.71, so 0.999 + 4/106.71 = 1.03648.
        assert!((mos_lqo(0.0) - 1.03648).abs() < 1e-4);
        // x = 4.5: e^(4.6607 - 6.72525) = e^-2.06455 = 0.12687, so
        // 0.999 + 4/1.12687 = 4.54864.
        assert!((mos_lqo(4.5) - 4.54864).abs() < 1e-4);
    }

    /// Hand-computed values of the spec 06 formula
    /// `0.999 + 4 / (1 + e^(-1.3669x + 3.8224))`.
    #[test]
    fn mos_lqo_wb_matches_hand_computed_values() {
        // x = 3.575: exponent -1.3669 * 3.575 + 3.8224 = -1.06416...,
        // e^-1.06416 = 0.34494, so 0.999 + 4/1.34494 = 3.9730.
        assert!((mos_lqo_wb(3.575) - 3.9730).abs() < 5e-4);
        // x = 2.798: exponent -1.3669 * 2.798 + 3.8224 = -0.00218...,
        // e^-0.00218 = 0.99782, so 0.999 + 4/1.99782 = 3.00118.
        assert!((mos_lqo_wb(2.798) - 3.00118).abs() < 5e-4);
        // The mapping is monotone and stays inside [0.999, 4.999].
        assert!(mos_lqo_wb(4.5) > mos_lqo_wb(1.0));
        assert!(mos_lqo_wb(1.0) > mos_lqo_wb(-0.5));
        assert!(mos_lqo_wb(-0.5) > 0.999);
        assert!(mos_lqo_wb(4.5) < 4.999);
    }
}
