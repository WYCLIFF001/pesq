//! Perceptual model part 1: spectra, Bark warping, loudness, scaling
//! (spec 03).
//!
//! This Round 2 scaffold holds the frame constants, the scaling constants,
//! and Table 1 (the 42 Bark bands for 8 kHz) of spec 03. The frame loop
//! of sections 3.1 to 3.7 is stubbed and will be implemented in a later
//! round.

use crate::types::SignalBuffer;

/// Frame length F in samples, 32 ms (spec 03, 3.1).
pub const FRAME_LEN: usize = 256;

/// Frame hop Q in samples, 16 ms (spec 03, 3.1).
pub const FRAME_HOP: usize = 128;

/// Number of Bark bands B for 8 kHz narrowband (spec 03, 3.3 and 3.8).
pub const NUM_BANDS: usize = 42;

/// Pitch power density scaling Sp (spec 03, 3.3 step 3).
pub const PITCH_POWER_SCALE: f32 = 2.764_344e-5;

/// Loudness scaling Sl (spec 03, 3.6 step 4).
pub const LOUDNESS_SCALE: f32 = 1.866_055e-1;

/// One Bark band row of Table 1 (spec 03, 3.8).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarkBand {
    /// Number of 128 Hz bins grouped into this band.
    pub bins: usize,
    /// Bark centre of the band.
    pub bark_centre: f32,
    /// Hz centre of the band.
    pub hz_centre: f32,
    /// Bark width of the band.
    pub bark_width: f32,
    /// Hz width of the band.
    pub hz_width: f32,
    /// Power density correction factor.
    pub correction: f32,
    /// Absolute hearing threshold power.
    pub threshold: f32,
}

impl BarkBand {
    /// Construct a row of Table 1. Column order matches spec 03 section
    /// 3.8: bins, bark centre, Hz centre, bark width, Hz width,
    /// correction factor, threshold.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        bins: usize,
        bark_centre: f32,
        hz_centre: f32,
        bark_width: f32,
        hz_width: f32,
        correction: f32,
        threshold: f32,
    ) -> Self {
        Self {
            bins,
            bark_centre,
            hz_centre,
            bark_width,
            hz_width,
            correction,
            threshold,
        }
    }
}

/// Table 1 of spec 03: the 42 Bark bands for 8 kHz. The group counts sum
/// to 128, covering bins 1..128 of the 256-point power spectrum (bin 0 is
/// forced to zero before warping, spec 03, 3.2 step 3).
///
/// The decimal digits are transcribed verbatim from the specification
/// table; the extra precision beyond f32 is intentional.
#[allow(clippy::excessive_precision)]
pub static BARK_BANDS: [BarkBand; NUM_BANDS] = [
    BarkBand::new(1, 0.078672, 7.867213, 0.157344, 15.734426, 100.0, 51_286_152.0),
    BarkBand::new(1, 0.316341, 31.634144, 0.317994, 31.799433, 99.999_992, 2_454_709.5),
    BarkBand::new(1, 0.636559, 63.655895, 0.322441, 32.244064, 100.0, 70_794.593_75),
    BarkBand::new(1, 0.961246, 96.124611, 0.326934, 32.693359, 100.000_008, 4_897.788_574),
    BarkBand::new(1, 1.290450, 129.044968, 0.331474, 33.147385, 100.000_008, 1_174.897_705),
    BarkBand::new(1, 1.624217, 162.421738, 0.336061, 33.606140, 100.000_015, 389.045_166),
    BarkBand::new(1, 1.962597, 196.259659, 0.340697, 34.069702, 99.999_992, 104.712_86),
    BarkBand::new(1, 2.305636, 230.563568, 0.345381, 34.538116, 99.999_969, 45.708_82),
    BarkBand::new(2, 2.653383, 265.338348, 0.350114, 35.011429, 50.000_027, 17.782_795),
    BarkBand::new(1, 3.005889, 300.588867, 0.354897, 35.489655, 100.0, 9.772_372),
    BarkBand::new(1, 3.363201, 336.320129, 0.359729, 35.972870, 99.999_969, 4.897_789),
    BarkBand::new(1, 3.725371, 372.537140, 0.364611, 36.461121, 100.000_015, 3.090_296),
    BarkBand::new(1, 4.092449, 409.244934, 0.369544, 36.954407, 99.999_947, 1.905_461),
    BarkBand::new(1, 4.464486, 446.448578, 0.374529, 37.452911, 100.000_061, 1.258_925),
    BarkBand::new(2, 4.841533, 484.568604, 0.379565, 40.269653, 53.047_077, 0.977_237),
    BarkBand::new(1, 5.223642, 526.600586, 0.384653, 42.311859, 110.000_046, 0.724_436),
    BarkBand::new(1, 5.610866, 570.303833, 0.389794, 45.992554, 117.991_989, 0.562_341),
    BarkBand::new(2, 6.003256, 619.423340, 0.394989, 51.348511, 65.0, 0.457_088),
    BarkBand::new(2, 6.400869, 672.121643, 0.400236, 55.040527, 68.760_147, 0.389_045),
    BarkBand::new(2, 6.803755, 728.525696, 0.405538, 56.775208, 69.999_931, 0.331_131),
    BarkBand::new(2, 7.211971, 785.675964, 0.410894, 58.699402, 71.428_818, 0.295_121),
    BarkBand::new(2, 7.625571, 846.835693, 0.416306, 62.445862, 75.000_038, 0.269_153),
    BarkBand::new(2, 8.044611, 909.691650, 0.421773, 64.820923, 76.843_384, 0.257_04),
    BarkBand::new(2, 8.469146, 977.063293, 0.427297, 69.195374, 80.968_781, 0.251_189),
    BarkBand::new(2, 8.899232, 1049.861694, 0.432877, 76.745667, 88.646_126, 0.251_189),
    BarkBand::new(3, 9.334927, 1129.635986, 0.438514, 84.016235, 63.864_388, 0.251_189),
    BarkBand::new(3, 9.776288, 1217.257568, 0.444209, 90.825684, 68.155_35, 0.251_189),
    BarkBand::new(3, 10.223374, 1312.109497, 0.449962, 97.931152, 72.547_775, 0.263_027),
    BarkBand::new(3, 10.676242, 1412.501465, 0.455774, 103.348877, 75.584_831, 0.288_403),
    BarkBand::new(4, 11.134952, 1517.999390, 0.461645, 107.801880, 58.379_192, 0.309_03),
    BarkBand::new(3, 11.599563, 1628.894165, 0.467577, 113.552246, 80.950_836, 0.338_844),
    BarkBand::new(4, 12.070135, 1746.194336, 0.473569, 121.490601, 64.135_651, 0.371_535),
    BarkBand::new(5, 12.546731, 1871.568848, 0.479621, 130.420410, 54.384_785, 0.398_107),
    BarkBand::new(4, 13.029408, 2008.776123, 0.485736, 143.431763, 73.821_884, 0.436_516),
    BarkBand::new(5, 13.518232, 2158.979248, 0.491912, 158.486816, 64.437_073, 0.467_735),
    BarkBand::new(6, 14.013264, 2326.743164, 0.498151, 176.872803, 59.176_456, 0.489_779),
    BarkBand::new(6, 14.514566, 2513.787109, 0.504454, 198.314697, 65.521_278, 0.501_187),
    BarkBand::new(7, 15.022202, 2722.488770, 0.510819, 219.549561, 61.399_822, 0.501_187),
    BarkBand::new(8, 15.536238, 2952.586670, 0.517250, 240.600098, 58.144_047, 0.512_861),
    BarkBand::new(9, 16.056736, 3205.835449, 0.523745, 268.702393, 57.004_543, 0.524_807),
    BarkBand::new(9, 16.583761, 3492.679932, 0.530308, 306.060059, 64.126_297, 0.524_807),
    BarkBand::new(11, 17.117382, 3820.219238, 0.536934, 349.937012, 59.248_363, 0.524_807),
];

/// Run the perceptual model frame loop of spec 03 (stub, Round 2):
/// silence skip and frame range (3.1), spectra (3.2), Bark warping
/// (3.3), audibility and silence flags (3.4), frequency response
/// compensation (3.5), loudness densities (3.6), and local gain scaling
/// (3.7). The disturbance part of the loop lives in [`crate::disturbance`].
pub fn run_frame_loop(
    _reference: &SignalBuffer,
    _degraded: &SignalBuffer,
    _utterances: &[crate::types::Utterance],
) {
    todo!("spec 03: perceptual model frame loop")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bark_groups_consume_all_128_bins() {
        let total: usize = BARK_BANDS.iter().map(|band| band.bins).sum();
        assert_eq!(total, 128, "group counts must sum to 128 (spec 03, 3.3)");
    }

    #[test]
    fn bark_table_has_the_documented_band_count() {
        assert_eq!(BARK_BANDS.len(), NUM_BANDS);
    }
}
