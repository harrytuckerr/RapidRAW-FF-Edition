//! CIEDE2000 colour-difference formula and Lab conversion helpers.
//!
//! Implemented from the published formula (Sharma, Wu & Dalal 2005:
//! "The CIEDE2000 Color-Difference Formula"). Self-contained ~100 lines of
//! math — no external crate needed.
//!
//! The CIEDE2000 function ([`cie_de2000`]) takes two L*a*b* triples under a
//! common reference white and returns the ΔE₂₀₀₀ distance. The conversion
//! helpers ([`xyz_to_lab`], [`lab_to_xyz`]) provide D50-based Lab for the
//! ProPhoto → XYZ → Lab chain used when comparing rendered ProPhoto pixels.

// ---------------------------------------------------------------------------
// CIEDE2000 formula
// ---------------------------------------------------------------------------

/// Compute the CIEDE2000 colour difference between two L*a*b* values.
///
/// All inputs must share the same reference white. The formula is symmetric.
/// L* is in [0, 100]; a* and b* are unbounded but typically in [-128, 128].
pub fn cie_de2000(lab1: [f64; 3], lab2: [f64; 3]) -> f64 {
    let (l1, a1, b1) = (lab1[0], lab1[1], lab1[2]);
    let (l2, a2, b2) = (lab2[0], lab2[1], lab2[2]);

    // Step 1: compute C*ab
    let c1 = (a1 * a1 + b1 * b1).sqrt();
    let c2 = (a2 * a2 + b2 * b2).sqrt();

    // Step 2: mean C*ab
    let c_mean = (c1 + c2) / 2.0;

    // Step 3: G factor
    let c_mean_7 = c_mean.powi(7);
    let g = 0.5 * (1.0 - (c_mean_7 / (c_mean_7 + 25_f64.powi(7))).sqrt());

    // Step 4: compute a', C', h'
    let a1p = a1 * (1.0 + g);
    let a2p = a2 * (1.0 + g);

    let c1p = (a1p * a1p + b1 * b1).sqrt();
    let c2p = (a2p * a2p + b2 * b2).sqrt();

    let h1p = hue_angle_deg(a1p, b1);
    let h2p = hue_angle_deg(a2p, b2);

    // Step 5: deltas
    let dlp = l2 - l1;
    let dcp = c2p - c1p;

    let dhp = if c1p * c2p == 0.0 {
        0.0
    } else {
        let dh = h2p - h1p;
        if dh.abs() <= 180.0 {
            dh
        } else if h2p <= h1p {
            dh + 360.0
        } else {
            dh - 360.0
        }
    };
    let dhp_rad = 2.0 * (c1p * c2p).sqrt() * (deg_to_rad(dhp) / 2.0).sin();

    // Step 6: mean values
    let lp_mean = (l1 + l2) / 2.0;
    let cp_mean = (c1p + c2p) / 2.0;

    let hp_mean = if c1p * c2p == 0.0 {
        h1p + h2p
    } else if (h1p - h2p).abs() <= 180.0 {
        (h1p + h2p) / 2.0
    } else if h1p + h2p < 360.0 {
        (h1p + h2p + 360.0) / 2.0
    } else {
        (h1p + h2p - 360.0) / 2.0
    };

    let t = 1.0 - 0.17 * deg_to_rad(hp_mean - 30.0).cos()
        + 0.24 * deg_to_rad(2.0 * hp_mean).cos()
        + 0.32 * deg_to_rad(3.0 * hp_mean + 6.0).cos()
        - 0.20 * deg_to_rad(4.0 * hp_mean - 63.0).cos();

    let dtheta = 30.0 * (-((hp_mean - 275.0) / 25.0).powi(2)).exp();

    let cp_mean_7 = cp_mean.powi(7);
    let rc = 2.0 * (cp_mean_7 / (cp_mean_7 + 25_f64.powi(7))).sqrt();

    let lp_mean_m50_sq = (lp_mean - 50.0).powi(2);
    let sl = 1.0 + (0.015 * lp_mean_m50_sq) / (20.0 + lp_mean_m50_sq).sqrt();
    let sc = 1.0 + 0.045 * cp_mean;
    let sh = 1.0 + 0.015 * cp_mean * t;

    let rt = -deg_to_rad(2.0 * dtheta).sin() * rc;

    // Step 7: weighted components
    let kl = 1.0;
    let kc = 1.0;
    let kh = 1.0;

    let term_l = dlp / (kl * sl);
    let term_c = dcp / (kc * sc);
    let term_h = dhp_rad / (kh * sh);

    (term_l * term_l + term_c * term_c + term_h * term_h + rt * term_c * term_h).sqrt()
}

/// Hue angle in degrees from a' and b' (0–360).
fn hue_angle_deg(a: f64, b: f64) -> f64 {
    if a == 0.0 && b == 0.0 {
        return 0.0;
    }
    let h = b.atan2(a).to_degrees();
    if h < 0.0 { h + 360.0 } else { h }
}

// ---------------------------------------------------------------------------
// XYZ ↔ Lab conversion (D50 reference white)
// ---------------------------------------------------------------------------

/// D50 reference white in XYZ (dng_sdk D50_xy_coord). Y = 1.0.
pub const D50_XYZ: [f64; 3] = [0.9642956764, 1.0, 0.8251046025];

/// Conversion threshold: linear below, cube-root above.
const DELTA: f64 = 6.0 / 29.0;
const DELTA2: f64 = DELTA * DELTA;
const DELTA3: f64 = DELTA2 * DELTA;

/// Forward f(t) for the CIE Lab nonlinearity.
fn lab_f(t: f64) -> f64 {
    if t > DELTA3 {
        t.cbrt()
    } else {
        t / (3.0 * DELTA2) + 4.0 / 29.0
    }
}

/// Inverse of lab_f(t).
fn lab_f_inv(t: f64) -> f64 {
    if t > DELTA {
        t * t * t
    } else {
        3.0 * DELTA2 * (t - 4.0 / 29.0)
    }
}

/// Convert XYZ (D50) to CIE L*a*b* (D50).
pub fn xyz_to_lab(xyz: [f64; 3]) -> [f64; 3] {
    let fx = lab_f(xyz[0] / D50_XYZ[0]);
    let fy = lab_f(xyz[1] / D50_XYZ[1]);
    let fz = lab_f(xyz[2] / D50_XYZ[2]);

    let l = 116.0 * fy - 16.0;
    let a = 500.0 * (fx - fy);
    let b = 200.0 * (fy - fz);
    [l, a, b]
}

/// Convert CIE L*a*b* (D50) to XYZ (D50).
pub fn lab_to_xyz(lab: [f64; 3]) -> [f64; 3] {
    let fy = (lab[0] + 16.0) / 116.0;
    let fx = lab[1] / 500.0 + fy;
    let fz = fy - lab[2] / 200.0;

    let x = lab_f_inv(fx) * D50_XYZ[0];
    let y = lab_f_inv(fy) * D50_XYZ[1];
    let z = lab_f_inv(fz) * D50_XYZ[2];
    [x, y, z]
}

/// Convert linear ProPhoto RGB (D50) to CIE L*a*b* (D50) via XYZ(D50).
///
/// Uses the XYZ→ProPhoto matrix inverse to go ProPhoto→XYZ, then XYZ→Lab.
/// `xyz_to_prophoto` is the 3x3 row-major matrix that maps XYZ(D50)→ProPhoto.
pub fn prophoto_to_lab(rgb: [f64; 3], xyz_to_prophoto: &[f64; 9]) -> [f64; 3] {
    // Invert XYZ→ProPhoto to get ProPhoto→XYZ.
    let m = nalgebra::Matrix3::new(
        xyz_to_prophoto[0],
        xyz_to_prophoto[1],
        xyz_to_prophoto[2],
        xyz_to_prophoto[3],
        xyz_to_prophoto[4],
        xyz_to_prophoto[5],
        xyz_to_prophoto[6],
        xyz_to_prophoto[7],
        xyz_to_prophoto[8],
    );
    let m_inv = m
        .try_inverse()
        .unwrap_or_else(nalgebra::Matrix3::<f64>::identity);
    let v = m_inv * nalgebra::Vector3::new(rgb[0], rgb[1], rgb[2]);
    xyz_to_lab([v[0], v[1], v[2]])
}

/// Convert f32 ProPhoto RGB to L*a*b* for delta-E comparison.
pub fn prophoto_f32_to_lab(rgb: [f32; 3], xyz_to_prophoto: &[f64; 9]) -> [f64; 3] {
    prophoto_to_lab(
        [rgb[0] as f64, rgb[1] as f64, rgb[2] as f64],
        xyz_to_prophoto,
    )
}

/// Compute CIEDE2000 between two ProPhoto f32 pixels.
pub fn prophoto_delta_e(a: [f32; 3], b: [f32; 3], xyz_to_prophoto: &[f64; 9]) -> f64 {
    let la = prophoto_f32_to_lab(a, xyz_to_prophoto);
    let lb = prophoto_f32_to_lab(b, xyz_to_prophoto);
    cie_de2000(la, lb)
}

fn deg_to_rad(d: f64) -> f64 {
    d * std::f64::consts::PI / 180.0
}

// ---------------------------------------------------------------------------
// Statistics helpers
// ---------------------------------------------------------------------------

/// Invalid samples/accounting or a bounded statistics-storage failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaEStatsError {
    EmptySample,
    MaskLengthMismatch {
        samples: usize,
        shadows: usize,
        highlights: usize,
    },
    NonFiniteSample {
        index: usize,
    },
    NegativeSample {
        index: usize,
    },
    InvalidStatistics(&'static str),
    Resource(&'static str),
}

impl std::fmt::Display for DeltaEStatsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptySample => f.write_str("EMPTY_SAMPLE: no included delta-E samples"),
            Self::MaskLengthMismatch {
                samples,
                shadows,
                highlights,
            } => write!(
                f,
                "MASK_INVALID: {samples} samples, {shadows} shadow flags, {highlights} highlight flags"
            ),
            Self::NonFiniteSample { index } => {
                write!(f, "NONFINITE_SAMPLE: delta-E at index {index}")
            }
            Self::NegativeSample { index } => write!(f, "negative delta-E at index {index}"),
            Self::InvalidStatistics(detail) => write!(f, "invalid delta-E statistics: {detail}"),
            Self::Resource(detail) => write!(f, "RESOURCE: {detail}"),
        }
    }
}

impl std::error::Error for DeltaEStatsError {}

/// Mean / nearest-rank p95 / max of a nonempty included delta-E sample.
///
/// Exclusion categories are disjoint; the two legacy exclusion counters are
/// inclusive of overlap. Public fields are checked again by both threshold APIs.
#[derive(Debug, Clone, PartialEq)]
pub struct DeltaEStats {
    pub mean: f64,
    pub p95: f64,
    pub max: f64,
    pub count: usize,
    pub total: usize,
    /// Union of both supplied exclusion masks (each pixel counted once).
    pub excluded: usize,
    pub shadow_only_excluded: usize,
    pub clipped_highlights_only_excluded: usize,
    pub overlap_excluded: usize,
    /// Inclusive shadow count, including overlap.
    pub deep_shadows_excluded: usize,
    /// Inclusive highlight/ceiling count, including overlap.
    pub clipped_highlights_excluded: usize,
}

// The reviewed v1 capacity ceiling is independent of colour tolerances. This
// helper owns only its f64 copy; callers already own the samples and masks.
const MAX_STATS_SAMPLES: usize = 80_000_000;
const MAX_STATS_SAMPLE_BYTES: usize = MAX_STATS_SAMPLES * std::mem::size_of::<f64>();

fn sample_storage_bytes(count: usize, byte_limit: usize) -> Result<usize, DeltaEStatsError> {
    let bytes = count
        .checked_mul(std::mem::size_of::<f64>())
        .ok_or(DeltaEStatsError::Resource("sample byte count overflow"))?;
    if count > MAX_STATS_SAMPLES || bytes > byte_limit {
        return Err(DeltaEStatsError::Resource(
            "statistics sample capacity exceeded",
        ));
    }
    Ok(bytes)
}

fn reserve_samples_with(
    count: usize,
    byte_limit: usize,
    reserve: impl FnOnce(&mut Vec<f64>, usize) -> Result<(), DeltaEStatsError>,
) -> Result<Vec<f64>, DeltaEStatsError> {
    sample_storage_bytes(count, byte_limit)?;
    let mut samples = Vec::new();
    reserve(&mut samples, count)?;
    sample_storage_bytes(samples.capacity(), byte_limit)?;
    if samples.capacity() < count {
        return Err(DeltaEStatsError::Resource(
            "sample reservation was insufficient",
        ));
    }
    Ok(samples)
}

// Nonnegative binary64 values are integers in units of 2^-1074. Each is
// <2^1024, and the 80M cap is <2^27, so a sum needs fewer than 2125 bits in
// those units. 34 u64 limbs provide 2176 bits, independent of input order.
// This stack storage introduces no new sample allocation.
const EXACT_SUM_WORDS: usize = 34;
struct ExactNonnegativeSum([u64; EXACT_SUM_WORDS]);

impl ExactNonnegativeSum {
    fn new() -> Self {
        Self([0; EXACT_SUM_WORDS])
    }

    fn add_word(&mut self, mut index: usize, mut value: u64) -> Result<(), DeltaEStatsError> {
        while value != 0 {
            let word = self
                .0
                .get_mut(index)
                .ok_or(DeltaEStatsError::InvalidStatistics(
                    "exact sum exceeded its proven capacity",
                ))?;
            let (sum, carry) = word.overflowing_add(value);
            *word = sum;
            value = u64::from(carry);
            index += 1;
        }
        Ok(())
    }

    fn add_shifted_word(&mut self, value: u64, shift: usize) -> Result<(), DeltaEStatsError> {
        let (index, offset) = (shift / 64, shift % 64);
        self.add_word(index, value << offset)?;
        if offset != 0 {
            self.add_word(index + 1, value >> (64 - offset))?;
        }
        Ok(())
    }

    fn add_weighted(&mut self, value: f64, weight: usize) -> Result<(), DeltaEStatsError> {
        if !value.is_finite() || value < 0.0 || weight > MAX_STATS_SAMPLES {
            return Err(DeltaEStatsError::InvalidStatistics(
                "invalid exact-sum term",
            ));
        }
        let bits = value.to_bits();
        let exponent = ((bits >> 52) & 0x7ff) as usize;
        let fraction = bits & ((1_u64 << 52) - 1);
        let significand = if exponent == 0 {
            fraction
        } else {
            fraction | (1_u64 << 52)
        };
        let shift = exponent.saturating_sub(1);
        let product = u128::from(significand).checked_mul(weight as u128).ok_or(
            DeltaEStatsError::InvalidStatistics("exact weighted sum overflow"),
        )?;
        self.add_shifted_word(product as u64, shift)?;
        self.add_shifted_word((product >> 64) as u64, shift + 64)
    }

    /// Divide the exact sum by count, then round once to binary64, nearest/even.
    /// The quotient is still in 2^-1074 units; the remainder retains all smaller
    /// information, including half-minimum-subnormal and normal-boundary ties.
    fn rounded_mean(&self, count: usize) -> Result<f64, DeltaEStatsError> {
        if count == 0 || count > MAX_STATS_SAMPLES {
            return Err(DeltaEStatsError::InvalidStatistics(
                "invalid exact-mean count",
            ));
        }
        let divisor = count as u128;
        let mut quotient = [0_u64; EXACT_SUM_WORDS];
        let mut remainder = 0_u128;
        for index in (0..EXACT_SUM_WORDS).rev() {
            // remainder <80M, so this intermediate occupies fewer than 91 bits.
            let numerator = (remainder << 64) | u128::from(self.0[index]);
            quotient[index] = u64::try_from(numerator / divisor)
                .map_err(|_| DeltaEStatsError::InvalidStatistics("exact quotient overflow"))?;
            remainder = numerator % divisor;
        }
        let highest = quotient
            .iter()
            .rposition(|&word| word != 0)
            .map(|index| index * 64 + 63 - quotient[index].leading_zeros() as usize)
            .unwrap_or(0);
        let shift = highest.saturating_sub(52);
        let (index, offset) = (shift / 64, shift % 64);
        let mut significand = quotient[index] >> offset;
        if offset != 0 && index + 1 < EXACT_SUM_WORDS {
            significand |= quotient[index + 1] << (64 - offset);
        }
        significand &= (1_u64 << 53) - 1;
        let round_up = if shift == 0 {
            // Includes the zero/subnormal region. Division remainder is exact.
            remainder * 2 > divisor || (remainder * 2 == divisor && significand & 1 != 0)
        } else {
            let half_position = shift - 1;
            let (word, bit) = (half_position / 64, half_position % 64);
            let half = quotient[word] & (1_u64 << bit) != 0;
            let sticky = remainder != 0
                || quotient[..word].iter().any(|&value| value != 0)
                || (bit != 0 && quotient[word] & ((1_u64 << bit) - 1) != 0);
            half && (sticky || significand & 1 != 0)
        };
        significand += u64::from(round_up);
        let result = if highest < 52 {
            // A carry into bit52 is exactly the minimum normal encoding.
            f64::from_bits(significand)
        } else {
            let mut exponent = shift + 1;
            if significand == 1_u64 << 53 {
                significand >>= 1;
                exponent += 1;
            }
            if exponent >= 0x7ff {
                return Err(DeltaEStatsError::InvalidStatistics(
                    "exact mean exceeds finite binary64",
                ));
            }
            f64::from_bits(((exponent as u64) << 52) | (significand & ((1_u64 << 52) - 1)))
        };
        if !result.is_finite() {
            return Err(DeltaEStatsError::InvalidStatistics("nonfinite exact mean"));
        }
        Ok(result)
    }
}

fn p95_rank(count: usize) -> Result<usize, DeltaEStatsError> {
    let rank = (count as f64 * 0.95).ceil() as usize;
    if rank == 0 || rank > count {
        return Err(DeltaEStatsError::InvalidStatistics("invalid p95 rank"));
    }
    Ok(rank)
}

fn validate_stats(stats: &DeltaEStats) -> Result<(), DeltaEStatsError> {
    if stats.count == 0 {
        return Err(DeltaEStatsError::EmptySample);
    }
    sample_storage_bytes(stats.total, MAX_STATS_SAMPLE_BYTES)?;
    let add = |a: usize, b: usize| {
        a.checked_add(b)
            .ok_or(DeltaEStatsError::InvalidStatistics("pixel count overflow"))
    };
    let excluded = add(
        add(
            stats.shadow_only_excluded,
            stats.clipped_highlights_only_excluded,
        )?,
        stats.overlap_excluded,
    )?;
    if excluded != stats.excluded
        || add(stats.count, excluded)? != stats.total
        || add(stats.shadow_only_excluded, stats.overlap_excluded)? != stats.deep_shadows_excluded
        || add(
            stats.clipped_highlights_only_excluded,
            stats.overlap_excluded,
        )? != stats.clipped_highlights_excluded
    {
        return Err(DeltaEStatsError::InvalidStatistics(
            "inconsistent exclusion accounting",
        ));
    }
    if [stats.mean, stats.p95, stats.max]
        .iter()
        .any(|v| !v.is_finite() || *v < 0.0)
    {
        return Err(DeltaEStatsError::InvalidStatistics(
            "metrics must be finite and nonnegative",
        ));
    }
    if stats.mean > stats.max
        || stats.p95 > stats.max
        || (stats.count <= 19 && stats.p95 != stats.max)
        || (stats.count == 1 && stats.mean != stats.max)
    {
        return Err(DeltaEStatsError::InvalidStatistics(
            "inconsistent metric bounds",
        ));
    }
    let rank = p95_rank(stats.count)?;
    let tail = stats
        .count
        .checked_sub(rank)
        .ok_or(DeltaEStatsError::InvalidStatistics(
            "invalid p95 count accounting",
        ))?;
    // Necessary real bounds for rank k, percentile P and maximum M:
    // ((N-k)*P + M)/N <= mean <= (k*P + (N-k)*M)/N.
    // Correct rounding is monotonic, so correctly rounded exact bounds include
    // every correctly rounded feasible mean, even when subnormals round to zero.
    // This validity check adds no epsilon to either acceptance comparison.
    let mut lower = ExactNonnegativeSum::new();
    lower.add_weighted(stats.p95, tail)?;
    lower.add_weighted(stats.max, 1)?;
    let mut upper = ExactNonnegativeSum::new();
    upper.add_weighted(stats.p95, rank)?;
    upper.add_weighted(stats.max, tail)?;
    if stats.mean < lower.rounded_mean(stats.count)?
        || stats.mean > upper.rounded_mean(stats.count)?
    {
        return Err(DeltaEStatsError::InvalidStatistics(
            "mean is infeasible for count, p95 and maximum",
        ));
    }
    Ok(())
}

/// Compute statistics using caller-supplied exclusion masks.
///
/// This helper does not derive masks from candidate pixels or infer clipping.
/// The comparator defines reference-only mask semantics. Every sample, including
/// excluded samples, must be finite and nonnegative. Empty primary samples fail.
/// The owned f64 copy is fallibly reserved, capped at 80 million samples, and
/// sorted in place without a scratch allocation. Later one-pass comparison code
/// must not use this helper to create a second full-size copy of its records.
pub fn compute_stats(
    delta_es: &[f64],
    shadow_mask: &[bool],
    highlight_mask: &[bool],
) -> Result<DeltaEStats, DeltaEStatsError> {
    if shadow_mask.len() != delta_es.len() || highlight_mask.len() != delta_es.len() {
        return Err(DeltaEStatsError::MaskLengthMismatch {
            samples: delta_es.len(),
            shadows: shadow_mask.len(),
            highlights: highlight_mask.len(),
        });
    }
    sample_storage_bytes(delta_es.len(), MAX_STATS_SAMPLE_BYTES)?;
    let (mut count, mut shadow_only, mut highlight_only, mut overlap) = (0, 0, 0, 0);
    for (index, &value) in delta_es.iter().enumerate() {
        if !value.is_finite() {
            return Err(DeltaEStatsError::NonFiniteSample { index });
        }
        if value < 0.0 {
            return Err(DeltaEStatsError::NegativeSample { index });
        }
        match (shadow_mask[index], highlight_mask[index]) {
            (false, false) => count += 1,
            (true, false) => shadow_only += 1,
            (false, true) => highlight_only += 1,
            (true, true) => overlap += 1,
        }
    }
    if count == 0 {
        return Err(DeltaEStatsError::EmptySample);
    }
    let mut included = reserve_samples_with(count, MAX_STATS_SAMPLE_BYTES, |values, capacity| {
        values
            .try_reserve_exact(capacity)
            .map_err(|_| DeltaEStatsError::Resource("sample reservation failed"))
    })?;
    for (index, &value) in delta_es.iter().enumerate() {
        if !shadow_mask[index] && !highlight_mask[index] {
            included.push(value);
        }
    }
    let mut exact_sum = ExactNonnegativeSum::new();
    for &value in &included {
        exact_sum.add_weighted(value, 1)?;
    }
    let mean = exact_sum.rounded_mean(count)?;
    included.sort_unstable_by(f64::total_cmp);
    let p95_index = p95_rank(count)? - 1;
    let stats = DeltaEStats {
        mean,
        p95: included[p95_index],
        max: included[count - 1],
        count,
        total: delta_es.len(),
        excluded: shadow_only + highlight_only + overlap,
        shadow_only_excluded: shadow_only,
        clipped_highlights_only_excluded: highlight_only,
        overlap_excluded: overlap,
        deep_shadows_excluded: shadow_only + overlap,
        clipped_highlights_excluded: highlight_only + overlap,
    };
    validate_stats(&stats)?;
    Ok(stats)
}

/// Strict CPU/reference thresholds; invalid statistics return an error rather
/// than a measured failure or a zero-valued pass.
pub fn check_thresholds(stats: &DeltaEStats) -> Result<bool, DeltaEStatsError> {
    validate_stats(stats)?;
    Ok(stats.mean < 2.0 && stats.p95 < 3.0 && stats.max < 5.0)
}

/// Strict GPU/CPU thresholds, with the same validity/accounting requirements.
pub fn check_gpu_parity(stats: &DeltaEStats) -> Result<bool, DeltaEStatsError> {
    validate_stats(stats)?;
    Ok(stats.mean < 0.1 && stats.max < 0.5)
}

// ---------------------------------------------------------------------------
// Sharma et al. 2005 test data (embedded for self-verification)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The CIEDE2000 published test suite (Sharma, Wu & Dalal 2005) contains 34
    /// colour-difference pairs with expected ΔE₂₀₀₀ values. We embed all 34,
    /// covering the formula's key features: neutral axis, high-chroma, hue
    /// wraparound, and the RT rotation term.
    ///
    /// Test pair format: (L1, a1, b1, L2, a2, b2, expected_dE2000, tolerance)
    const SHARMA_PAIRS: &[([f64; 3], [f64; 3], f64)] = &[
        // Pair 1: neutral vs near-neutral (tests SL term)
        ([50.0, 2.6772, -79.7751], [50.0, 0.0, -82.7485], 2.0425),
        // Pair 2: high-chroma blue
        ([50.0, 3.1571, -77.2803], [50.0, 0.0, -82.7485], 2.8615),
        // Pair 3: higher-chroma blue
        ([50.0, 2.8361, -74.0200], [50.0, 0.0, -82.7485], 3.4412),
        // Pair 4: achromatic pair (near-grey)
        ([50.0, -1.3802, -84.2814], [50.0, 0.0, -82.7485], 1.0000),
        // Pair 5: another achromatic
        ([50.0, -1.1848, -84.8006], [50.0, 0.0, -82.7485], 1.0000),
        // Pair 6: grey vs grey (tests the purely-L* path)
        ([50.0, -0.9009, -85.5211], [50.0, 0.0, -82.7485], 1.0000),
        // Pair 7: hue rotation example (tests the RT term)
        ([50.0, 0.0, 0.0], [50.0, -1.0, 2.0], 2.3669),
        // Pair 8: near-achromatic in the opposite quadrant
        ([50.0, -1.0, 2.0], [50.0, 0.0, 0.0], 2.3669),
        // Pair 9: small hue difference at high chroma
        ([50.0, 2.4900, -0.0010], [50.0, -2.4900, 0.0009], 7.1792),
        // Pair 10: near-complementary hues
        ([50.0, 2.4900, -0.0010], [50.0, -2.4900, 0.0010], 7.1792),
        // Pair 11: another near-complementary
        ([50.0, 2.4900, -0.0010], [50.0, -2.4900, 0.0011], 7.2195),
        // Pair 12: complementary with small difference
        ([50.0, 2.4900, -0.0010], [50.0, -2.4900, 0.0012], 7.2195),
        // Pair 13: same hue, close
        ([50.0, -0.0010, 2.4900], [50.0, 0.0009, -2.4900], 4.8045),
        // Pair 14: near-same, small shift
        ([50.0, -0.0010, 2.4900], [50.0, 0.0010, -2.4900], 4.8045),
        // Pair 15: edge case near hue discontinuity
        ([50.0, -0.0010, 2.4900], [50.0, 0.0011, -2.4900], 4.7461),
        // Pair 16: L*=50 neutral (hue-difference dominated)
        ([50.0, 2.5, 0.0], [50.0, 0.0, -2.5], 4.3065),
        // Pair 17: 90° hue difference at L*=50
        ([50.0, 2.5, 0.0], [73.0, 25.0, -18.0], 27.1492),
        // Pair 18: large perceptual difference
        ([50.0, 2.5, 0.0], [61.0, -5.0, 29.0], 22.8977),
        // Pair 19: different L*, same chromatic
        ([50.0, 2.5, 0.0], [56.0, -27.0, -3.0], 31.9030),
        // Pair 20: large L* difference
        ([50.0, 2.5, 0.0], [58.0, 24.0, 15.0], 19.4535),
        // Pair 21: neutral vs neutral (only L* difference)
        ([50.0, 2.5, 0.0], [50.0, 3.1736, 0.5854], 1.0000),
        // Pair 22: very close pair
        ([50.0, 2.5, 0.0], [50.0, 3.2972, 0.0000], 1.0000),
        // Pair 23: small L* difference
        ([50.0, 2.5, 0.0], [50.0, 1.8634, 0.5757], 1.0000),
        // Pair 24: small hue difference near 0°
        ([50.0, 2.5, 0.0], [50.0, 3.2592, 0.3350], 1.0000),
        // Pair 25: L*=60 region
        (
            [60.2574, -34.0099, 36.2677],
            [60.4626, -34.1751, 39.4387],
            1.2644,
        ),
        // Pair 26: medium-chroma green pair
        (
            [63.0109, -31.0961, -5.8663],
            [62.8187, -29.7946, -4.0864],
            1.2630,
        ),
        // Pair 27: medium-high chroma yellow-green
        (
            [61.2901, 3.7196, -5.3901],
            [61.4292, 2.2480, -4.9620],
            1.8731,
        ),
        // Pair 28: L*=35 region, low-chroma
        (
            [35.0831, -44.1164, 3.7933],
            [35.0232, -40.0716, 1.5901],
            1.8645,
        ),
        // Pair 29: high-chroma yellow at low L*
        (
            [22.7233, 20.0904, -46.6940],
            [23.0331, 14.9730, -42.5619],
            2.0373,
        ),
        // Pair 30: high-chroma red/orange
        (
            [36.4612, 47.8580, 18.3852],
            [36.2715, 50.5065, 21.2231],
            1.4146,
        ),
        // Pair 31: high-chroma blue at low L*
        (
            [90.8027, -2.0831, 1.4410],
            [91.1528, -1.6435, 0.0447],
            1.4441,
        ),
        // Pair 32: very close neutral at high L*
        (
            [90.9257, -0.5406, -0.9208],
            [88.6381, -0.8985, -0.7239],
            1.5381,
        ),
        // Pair 33: medium L*, blue region
        (
            [6.7747, -0.2908, -2.4247],
            [5.8714, -0.0985, -2.2286],
            0.6377,
        ),
        // Pair 34: very dark near-neutral
        (
            [2.0776, 0.0795, -1.1350],
            [0.9033, -0.0636, -0.5514],
            0.9082,
        ),
    ];

    /// Verify CIEDE2000 against all 34 published Sharma test pairs.
    #[test]
    fn sharma_all_34_pairs() {
        let mut max_err = 0.0f64;
        let mut max_pair = 0usize;
        for (i, (lab1, lab2, expected)) in SHARMA_PAIRS.iter().enumerate() {
            let got = cie_de2000(*lab1, *lab2);
            let err = (got - expected).abs();
            if err > max_err {
                max_err = err;
                max_pair = i + 1;
            }
            assert!(
                err < 1e-4,
                "Sharma pair {}: L1a1b1={lab1:?} L2a2b2={lab2:?} expected {expected}, got {got} (err {err})",
                i + 1
            );
        }
        // All 34 pairs should match to 1e-4 precision.
        eprintln!(
            "Sharma verification: max error {} on pair {} (all 34 pairs < 1e-4)",
            max_err, max_pair
        );
    }

    /// CIEDE2000 of identical colours must be zero.
    #[test]
    fn identical_is_zero() {
        let lab1 = [50.0, 10.0, 20.0];
        assert!(cie_de2000(lab1, lab1) < 1e-12);
    }

    /// CIEDE2000 is symmetric.
    #[test]
    fn symmetric() {
        let a = [40.0, -5.0, 15.0];
        let b = [45.0, 3.0, -10.0];
        let d1 = cie_de2000(a, b);
        let d2 = cie_de2000(b, a);
        assert!((d1 - d2).abs() < 1e-12);
    }

    /// In-gamut round-trip: XYZ → Lab → XYZ.
    #[test]
    fn xyz_lab_round_trip() {
        for &xyz in &[
            [50.0, 50.0, 50.0],
            [10.0, 20.0, 30.0],
            [0.1, 0.2, 0.3],
            [1.0, 1.0, 1.0],
        ] {
            let lab = xyz_to_lab(xyz);
            let back = lab_to_xyz(lab);
            for i in 0..3 {
                assert!(
                    (back[i] - xyz[i]).abs() < 1e-6,
                    "XYZ round trip: {xyz:?} -> {lab:?} -> {back:?}"
                );
            }
        }
    }

    /// D50 white (scaled to Y=1) maps to L*=100, a*=0, b*=0.
    #[test]
    fn d50_white_is_lab_white() {
        let lab = xyz_to_lab(D50_XYZ);
        assert!((lab[0] - 100.0).abs() < 1e-6, "L* not 100: {}", lab[0]);
        assert!(lab[1].abs() < 1e-6, "a* not 0: {}", lab[1]);
        assert!(lab[2].abs() < 1e-6, "b* not 0: {}", lab[2]);
    }

    /// XYZ = [0,0,0] → L*=0.
    #[test]
    fn black_is_lab_black() {
        let lab = xyz_to_lab([0.0, 0.0, 0.0]);
        assert!(lab[0].abs() < 1e-6, "L* not 0: {}", lab[0]);
    }

    /// Compute ProPhoto→Lab for a known value via the published XYZ→ProPhoto
    /// matrix from render.rs, and verify the round-trip.
    #[test]
    fn prophoto_to_lab_round_trip() {
        // The XYZ→ProPhoto D50 matrix from render.rs.
        let m: [f64; 9] = [
            1.345_786_881_647_158_5,
            -0.255_572_087_379_794_64,
            -0.051_101_864_975_545_26,
            -0.544_630_705_124_901_9,
            1.508_247_742_845_146_8,
            0.020_527_447_436_421_39,
            0.0,
            0.0,
            1.211_967_545_638_945_2,
        ];

        // A known ProPhoto RGB value.
        let rgb: [f64; 3] = [0.18, 0.18, 0.18];
        let lab = prophoto_to_lab(rgb, &m);

        // Convert back: Lab → XYZ → ProPhoto.
        let xyz = lab_to_xyz(lab);
        let m_mat = nalgebra::Matrix3::new(m[0], m[1], m[2], m[3], m[4], m[5], m[6], m[7], m[8]);
        let prophoto_back = m_mat * nalgebra::Vector3::new(xyz[0], xyz[1], xyz[2]);

        for i in 0..3 {
            assert!(
                (prophoto_back[i] - rgb[i]).abs() < 1e-6,
                "ProPhoto round trip: {rgb:?} -> Lab -> {prophoto_back:?}"
            );
        }
    }

    #[test]
    fn stats_reporting() {
        // 10 pixels: [0.1, 0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]
        let de: Vec<f64> = (0..10).map(|i| i as f64).collect();
        let shadow = vec![false; 10];
        let highlight = vec![false; 10];

        let s = compute_stats(&de, &shadow, &highlight).unwrap();
        assert_eq!(s.count, 10);
        // mean = sum(0..10) / 10 = 45 / 10 = 4.5
        assert!((s.mean - 4.5).abs() < 1e-6, "mean {}", s.mean);
        assert!((s.max - 9.0).abs() < 1e-6, "max {}", s.max);
        // p95: 10 pixels, ceil(10 * 0.95) = ceil(9.5) = 10, sorted[9] = 9
        assert!((s.p95 - 9.0).abs() < 1e-6, "p95 {}", s.p95);
        assert_eq!(s.excluded, 0);
    }

    #[test]
    fn stats_excludes_shadows_and_highlights() {
        let de: Vec<f64> = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
        let shadows = vec![
            true, false, false, false, false, false, false, false, false, false,
        ];
        let highlights = vec![
            false, false, false, false, false, false, false, false, false, true,
        ];
        let s = compute_stats(&de, &shadows, &highlights).unwrap();
        assert_eq!(s.count, 8);
        assert_eq!(s.deep_shadows_excluded, 1);
        assert_eq!(s.clipped_highlights_excluded, 1);
        assert_eq!(s.excluded, 2);
    }

    fn stats_for(values: &[f64]) -> DeltaEStats {
        let mask = vec![false; values.len()];
        compute_stats(values, &mask, &mask).unwrap()
    }

    fn assert_invalid_thresholds(stats: &DeltaEStats) {
        assert!(check_thresholds(stats).is_err());
        assert!(check_gpu_parity(stats).is_err());
    }

    #[test]
    fn stats_reject_empty_and_all_excluded() {
        assert_eq!(
            compute_stats(&[], &[], &[]),
            Err(DeltaEStatsError::EmptySample)
        );
        for (shadows, highlights) in [
            ([true, true], [false, false]),
            ([false, false], [true, true]),
            ([true, false], [false, true]),
            ([true, true], [true, true]),
        ] {
            assert_eq!(
                compute_stats(&[0.1, 0.2], &shadows, &highlights),
                Err(DeltaEStatsError::EmptySample)
            );
        }
    }

    #[test]
    fn stats_reject_short_and_long_masks() {
        for (shadows, highlights) in [
            (vec![false], vec![false; 2]),
            (vec![false; 3], vec![false; 2]),
            (vec![false; 2], vec![false]),
            (vec![false; 2], vec![false; 3]),
        ] {
            assert!(matches!(
                compute_stats(&[0.1, 0.2], &shadows, &highlights),
                Err(DeltaEStatsError::MaskLengthMismatch { .. })
            ));
        }
    }

    #[test]
    fn stats_count_overlap_once_and_preserve_inclusive_counts() {
        let stats = compute_stats(
            &[4.0, 5.0, 6.0, 0.25],
            &[true, false, true, false],
            &[false, true, true, false],
        )
        .unwrap();
        assert_eq!((stats.total, stats.count, stats.excluded), (4, 1, 3));
        assert_eq!(
            (
                stats.shadow_only_excluded,
                stats.clipped_highlights_only_excluded,
                stats.overlap_excluded
            ),
            (1, 1, 1)
        );
        assert_eq!(
            (
                stats.deep_shadows_excluded,
                stats.clipped_highlights_excluded
            ),
            (2, 2)
        );
        assert_eq!((stats.mean, stats.p95, stats.max), (0.25, 0.25, 0.25));
        assert_eq!(check_thresholds(&stats), Ok(true));
    }

    #[test]
    fn stats_reject_nonfinite_even_when_excluded() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for (shadow, highlight) in [(false, false), (true, false), (false, true), (true, true)]
            {
                assert_eq!(
                    compute_stats(&[0.1, value], &[false, shadow], &[false, highlight]),
                    Err(DeltaEStatsError::NonFiniteSample { index: 1 })
                );
            }
        }
        assert_eq!(
            compute_stats(&[f64::NAN], &[true], &[true]),
            Err(DeltaEStatsError::NonFiniteSample { index: 0 })
        );
    }

    #[test]
    fn stats_reject_negative_even_when_excluded() {
        for (shadow, highlight) in [(false, false), (true, false), (false, true), (true, true)] {
            assert_eq!(
                compute_stats(&[0.1, -0.01], &[false, shadow], &[false, highlight]),
                Err(DeltaEStatsError::NegativeSample { index: 1 })
            );
        }
    }

    #[test]
    fn stats_nearest_rank_p95_at_1_19_20_21() {
        for (count, expected) in [(1, 1.0), (19, 19.0), (20, 19.0), (21, 20.0)] {
            let values: Vec<f64> = (1..=count).rev().map(|v| v as f64).collect();
            assert_eq!(stats_for(&values).p95, expected, "sample count {count}");
        }
    }

    #[test]
    fn stats_mean_handles_large_finite_values_without_sum_overflow() {
        let stats = stats_for(&[f64::MAX, f64::MAX]);
        assert_eq!(stats.mean, f64::MAX);
        assert_eq!(check_thresholds(&stats), Ok(false));
        assert_eq!(check_gpu_parity(&stats), Ok(false));
        let constant = stats_for(&[0.1; 19]);
        assert_eq!(constant.mean, constant.max);
        assert_eq!(check_thresholds(&constant), Ok(true));
    }

    #[test]
    fn threshold_functions_reject_forged_accounting() {
        let valid = compute_stats(
            &[0.1; 4],
            &[true, false, true, false],
            &[false, true, true, false],
        )
        .unwrap();
        let corruptions: &[fn(&mut DeltaEStats)] = &[
            |s| s.count = 0,
            |s| s.count += 1,
            |s| s.total = 0,
            |s| s.total += 1,
            |s| s.excluded += 1,
            |s| s.shadow_only_excluded += 1,
            |s| s.clipped_highlights_only_excluded += 1,
            |s| s.overlap_excluded += 1,
            |s| s.deep_shadows_excluded += 1,
            |s| s.clipped_highlights_excluded += 1,
            |s| s.shadow_only_excluded = usize::MAX,
            |s| s.count = usize::MAX,
            |s| s.total = usize::MAX,
        ];
        for corrupt in corruptions {
            let mut forged = valid.clone();
            corrupt(&mut forged);
            assert_invalid_thresholds(&forged);
        }
    }

    #[test]
    fn threshold_functions_reject_nonfinite_negative_and_inconsistent_metrics() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01] {
            for metric in 0..3 {
                let mut forged = stats_for(&[0.05; 20]);
                match metric {
                    0 => forged.mean = value,
                    1 => forged.p95 = value,
                    _ => forged.max = value,
                }
                assert_invalid_thresholds(&forged);
            }
        }
        let mut forged = stats_for(&[0.05; 20]);
        forged.mean = 0.06;
        assert_invalid_thresholds(&forged);
        forged.mean = 0.05;
        forged.p95 = 0.06;
        assert_invalid_thresholds(&forged);
        let mut singleton = stats_for(&[0.05]);
        singleton.mean = 0.01;
        assert_invalid_thresholds(&singleton);
        let mut short = stats_for(&[0.05; 19]);
        short.p95 = 0.01;
        assert_invalid_thresholds(&short);
    }

    #[test]
    fn cpu_thresholds_are_strict_for_each_boundary() {
        assert_eq!(check_thresholds(&stats_for(&[2.0])), Ok(false));
        assert_eq!(
            check_thresholds(&stats_for(&[2.0_f64.next_down()])),
            Ok(true)
        );
        for (boundary, pass) in [(3.0, false), (3.0_f64.next_down(), true)] {
            let mut values = [0.0; 20];
            values[18] = boundary;
            values[19] = boundary;
            assert_eq!(check_thresholds(&stats_for(&values)), Ok(pass));
        }
        for (boundary, pass) in [(5.0, false), (5.0_f64.next_down(), true)] {
            let mut values = [0.0; 100];
            values[99] = boundary;
            assert_eq!(check_thresholds(&stats_for(&values)), Ok(pass));
        }
    }

    #[test]
    fn gpu_thresholds_are_strict_for_each_boundary() {
        assert_eq!(check_gpu_parity(&stats_for(&[0.1])), Ok(false));
        assert_eq!(
            check_gpu_parity(&stats_for(&[0.1_f64.next_down()])),
            Ok(true)
        );
        for (boundary, pass) in [(0.5, false), (0.5_f64.next_down(), true)] {
            let mut values = [0.0; 100];
            values[99] = boundary;
            assert_eq!(check_gpu_parity(&stats_for(&values)), Ok(pass));
        }
    }

    #[test]
    fn stats_size_plans_reject_overflow_and_capacity_without_allocating() {
        assert_eq!(
            sample_storage_bytes(MAX_STATS_SAMPLES, MAX_STATS_SAMPLE_BYTES),
            Ok(640_000_000)
        );
        for count in [MAX_STATS_SAMPLES + 1, usize::MAX] {
            assert!(matches!(
                sample_storage_bytes(count, MAX_STATS_SAMPLE_BYTES),
                Err(DeltaEStatsError::Resource(_))
            ));
            assert!(matches!(
                reserve_samples_with(count, MAX_STATS_SAMPLE_BYTES, |_, _| panic!(
                    "must not allocate"
                )),
                Err(DeltaEStatsError::Resource(_))
            ));
        }
        assert!(matches!(
            reserve_samples_with(2, 8, |_, _| panic!("must not allocate")),
            Err(DeltaEStatsError::Resource(_))
        ));
    }

    #[test]
    fn stats_reservation_failure_is_fallible_and_capacity_is_checked() {
        let injected = reserve_samples_with(1, 8, |_, _| {
            Err(DeltaEStatsError::Resource("injected reserve failure"))
        });
        assert_eq!(
            injected,
            Err(DeltaEStatsError::Resource("injected reserve failure"))
        );
        assert!(matches!(
            reserve_samples_with(1, 8, |_, _| Ok(())),
            Err(DeltaEStatsError::Resource(_))
        ));
        let excess_capacity = reserve_samples_with(1, 8, |values, _| {
            values
                .try_reserve_exact(2)
                .map_err(|_| DeltaEStatsError::Resource("test reserve failed"))
        });
        assert!(matches!(
            excess_capacity,
            Err(DeltaEStatsError::Resource(_))
        ));
    }

    // Generated only by the independent stdlib Python Fraction oracle saved in
    // the revision evidence. Inputs/expected means use exact binary64 bit patterns;
    // runs compress identical inputs without changing their multiplicity.
    type ExactMeanOracleRow = (&'static str, &'static [(u64, usize)], u64, bool, bool);

    const EXACT_MEAN_ORACLE: &[ExactMeanOracleRow] = &[
        (
            "review_boundary_original",
            &[(0x4004000000000000, 4), (0x3ff8000000000000, 4)],
            0x4000000000000000,
            false,
            false,
        ),
        (
            "review_boundary_reverse",
            &[(0x3ff8000000000000, 4), (0x4004000000000000, 4)],
            0x4000000000000000,
            false,
            false,
        ),
        (
            "review_boundary_rotate",
            &[
                (0x4004000000000000, 3),
                (0x3ff8000000000000, 4),
                (0x4004000000000000, 1),
            ],
            0x4000000000000000,
            false,
            false,
        ),
        (
            "cpu_mean_below_original",
            &[(0x4003ffffffffffff, 4), (0x3ff8000000000000, 4)],
            0x3fffffffffffffff,
            true,
            false,
        ),
        (
            "cpu_mean_below_reverse",
            &[(0x3ff8000000000000, 4), (0x4003ffffffffffff, 4)],
            0x3fffffffffffffff,
            true,
            false,
        ),
        (
            "cpu_mean_below_rotate",
            &[
                (0x4003ffffffffffff, 3),
                (0x3ff8000000000000, 4),
                (0x4003ffffffffffff, 1),
            ],
            0x3fffffffffffffff,
            true,
            false,
        ),
        (
            "cpu_mean_above_original",
            &[(0x4004000000000001, 4), (0x3ff8000000000000, 4)],
            0x4000000000000000,
            false,
            false,
        ),
        (
            "cpu_mean_above_reverse",
            &[(0x3ff8000000000000, 4), (0x4004000000000001, 4)],
            0x4000000000000000,
            false,
            false,
        ),
        (
            "cpu_mean_above_rotate",
            &[
                (0x4004000000000001, 3),
                (0x3ff8000000000000, 4),
                (0x4004000000000001, 1),
            ],
            0x4000000000000000,
            false,
            false,
        ),
        (
            "gpu_mixed_original",
            &[(0x3fc0000000000000, 4), (0x3fb3333333333333, 4)],
            0x3fb999999999999a,
            true,
            false,
        ),
        (
            "gpu_mixed_reverse",
            &[(0x3fb3333333333333, 4), (0x3fc0000000000000, 4)],
            0x3fb999999999999a,
            true,
            false,
        ),
        (
            "gpu_mixed_rotate",
            &[
                (0x3fc0000000000000, 3),
                (0x3fb3333333333333, 4),
                (0x3fc0000000000000, 1),
            ],
            0x3fb999999999999a,
            true,
            false,
        ),
        (
            "mixed_magnitudes_original",
            &[
                (0x7e37e43c8800759c, 1),
                (0x01a56e1fc2f8f359, 1),
                (0x400c000000000000, 1),
                (0x39b0000000000000, 1),
                (0x5f30000000000000, 1),
                (0x0000000000000001, 1),
            ],
            0x7e0fdafb60009cd0,
            false,
            false,
        ),
        (
            "mixed_magnitudes_reverse",
            &[
                (0x0000000000000001, 1),
                (0x5f30000000000000, 1),
                (0x39b0000000000000, 1),
                (0x400c000000000000, 1),
                (0x01a56e1fc2f8f359, 1),
                (0x7e37e43c8800759c, 1),
            ],
            0x7e0fdafb60009cd0,
            false,
            false,
        ),
        (
            "mixed_magnitudes_ascending",
            &[
                (0x0000000000000001, 1),
                (0x01a56e1fc2f8f359, 1),
                (0x39b0000000000000, 1),
                (0x400c000000000000, 1),
                (0x5f30000000000000, 1),
                (0x7e37e43c8800759c, 1),
            ],
            0x7e0fdafb60009cd0,
            false,
            false,
        ),
        (
            "mixed_magnitudes_descending",
            &[
                (0x7e37e43c8800759c, 1),
                (0x5f30000000000000, 1),
                (0x400c000000000000, 1),
                (0x39b0000000000000, 1),
                (0x01a56e1fc2f8f359, 1),
                (0x0000000000000001, 1),
            ],
            0x7e0fdafb60009cd0,
            false,
            false,
        ),
        (
            "mixed_magnitudes_rotate",
            &[
                (0x01a56e1fc2f8f359, 1),
                (0x400c000000000000, 1),
                (0x39b0000000000000, 1),
                (0x5f30000000000000, 1),
                (0x0000000000000001, 1),
                (0x7e37e43c8800759c, 1),
            ],
            0x7e0fdafb60009cd0,
            false,
            false,
        ),
        (
            "finite_huge_original",
            &[
                (0x7fefffffffffffff, 1),
                (0x0000000000000000, 1),
                (0x7feffffffffffffe, 1),
                (0x0000000000000001, 1),
                (0x3ff0000000000000, 1),
            ],
            0x7fd9999999999998,
            false,
            false,
        ),
        (
            "finite_huge_reverse",
            &[
                (0x3ff0000000000000, 1),
                (0x0000000000000001, 1),
                (0x7feffffffffffffe, 1),
                (0x0000000000000000, 1),
                (0x7fefffffffffffff, 1),
            ],
            0x7fd9999999999998,
            false,
            false,
        ),
        (
            "finite_huge_ascending",
            &[
                (0x0000000000000000, 1),
                (0x0000000000000001, 1),
                (0x3ff0000000000000, 1),
                (0x7feffffffffffffe, 1),
                (0x7fefffffffffffff, 1),
            ],
            0x7fd9999999999998,
            false,
            false,
        ),
        (
            "finite_huge_descending",
            &[
                (0x7fefffffffffffff, 1),
                (0x7feffffffffffffe, 1),
                (0x3ff0000000000000, 1),
                (0x0000000000000001, 1),
                (0x0000000000000000, 1),
            ],
            0x7fd9999999999998,
            false,
            false,
        ),
        (
            "finite_huge_rotate",
            &[
                (0x0000000000000000, 1),
                (0x7feffffffffffffe, 1),
                (0x0000000000000001, 1),
                (0x3ff0000000000000, 1),
                (0x7fefffffffffffff, 1),
            ],
            0x7fd9999999999998,
            false,
            false,
        ),
        (
            "zero_1",
            &[(0x0000000000000000, 1)],
            0x0000000000000000,
            true,
            true,
        ),
        (
            "decimal_1",
            &[(0x3fb999999999999a, 1)],
            0x3fb999999999999a,
            true,
            false,
        ),
        (
            "smallest_1",
            &[(0x0000000000000001, 1)],
            0x0000000000000001,
            true,
            true,
        ),
        (
            "huge_1",
            &[(0x7fefffffffffffff, 1)],
            0x7fefffffffffffff,
            false,
            false,
        ),
        (
            "zero_2",
            &[(0x0000000000000000, 2)],
            0x0000000000000000,
            true,
            true,
        ),
        (
            "decimal_2",
            &[(0x3fb999999999999a, 2)],
            0x3fb999999999999a,
            true,
            false,
        ),
        (
            "smallest_2",
            &[(0x0000000000000001, 2)],
            0x0000000000000001,
            true,
            true,
        ),
        (
            "huge_2",
            &[(0x7fefffffffffffff, 2)],
            0x7fefffffffffffff,
            false,
            false,
        ),
        (
            "zero_19",
            &[(0x0000000000000000, 19)],
            0x0000000000000000,
            true,
            true,
        ),
        (
            "decimal_19",
            &[(0x3fb999999999999a, 19)],
            0x3fb999999999999a,
            true,
            false,
        ),
        (
            "smallest_19",
            &[(0x0000000000000001, 19)],
            0x0000000000000001,
            true,
            true,
        ),
        (
            "huge_19",
            &[(0x7fefffffffffffff, 19)],
            0x7fefffffffffffff,
            false,
            false,
        ),
        (
            "zero_20",
            &[(0x0000000000000000, 20)],
            0x0000000000000000,
            true,
            true,
        ),
        (
            "decimal_20",
            &[(0x3fb999999999999a, 20)],
            0x3fb999999999999a,
            true,
            false,
        ),
        (
            "smallest_20",
            &[(0x0000000000000001, 20)],
            0x0000000000000001,
            true,
            true,
        ),
        (
            "huge_20",
            &[(0x7fefffffffffffff, 20)],
            0x7fefffffffffffff,
            false,
            false,
        ),
        (
            "zero_21",
            &[(0x0000000000000000, 21)],
            0x0000000000000000,
            true,
            true,
        ),
        (
            "decimal_21",
            &[(0x3fb999999999999a, 21)],
            0x3fb999999999999a,
            true,
            false,
        ),
        (
            "smallest_21",
            &[(0x0000000000000001, 21)],
            0x0000000000000001,
            true,
            true,
        ),
        (
            "huge_21",
            &[(0x7fefffffffffffff, 21)],
            0x7fefffffffffffff,
            false,
            false,
        ),
        (
            "zero_257",
            &[(0x0000000000000000, 257)],
            0x0000000000000000,
            true,
            true,
        ),
        (
            "decimal_257",
            &[(0x3fb999999999999a, 257)],
            0x3fb999999999999a,
            true,
            false,
        ),
        (
            "smallest_257",
            &[(0x0000000000000001, 257)],
            0x0000000000000001,
            true,
            true,
        ),
        (
            "huge_257",
            &[(0x7fefffffffffffff, 257)],
            0x7fefffffffffffff,
            false,
            false,
        ),
        (
            "half_subnormal_original",
            &[(0x0000000000000000, 1), (0x0000000000000001, 1)],
            0x0000000000000000,
            true,
            true,
        ),
        (
            "half_subnormal_reverse",
            &[(0x0000000000000001, 1), (0x0000000000000000, 1)],
            0x0000000000000000,
            true,
            true,
        ),
        (
            "tie_subnormal_original",
            &[(0x0000000000000001, 1), (0x0000000000000002, 1)],
            0x0000000000000002,
            true,
            true,
        ),
        (
            "tie_subnormal_reverse",
            &[(0x0000000000000002, 1), (0x0000000000000001, 1)],
            0x0000000000000002,
            true,
            true,
        ),
        (
            "one_and_three_original",
            &[(0x0000000000000001, 1), (0x0000000000000003, 1)],
            0x0000000000000002,
            true,
            true,
        ),
        (
            "one_and_three_reverse",
            &[(0x0000000000000003, 1), (0x0000000000000001, 1)],
            0x0000000000000002,
            true,
            true,
        ),
        (
            "subnormal_third_original",
            &[(0x0000000000000000, 2), (0x0000000000000001, 1)],
            0x0000000000000000,
            true,
            true,
        ),
        (
            "subnormal_third_reverse",
            &[(0x0000000000000001, 1), (0x0000000000000000, 2)],
            0x0000000000000000,
            true,
            true,
        ),
        (
            "subnormal_third_rotate",
            &[
                (0x0000000000000000, 1),
                (0x0000000000000001, 1),
                (0x0000000000000000, 1),
            ],
            0x0000000000000000,
            true,
            true,
        ),
        (
            "normal_transition_original",
            &[(0x0010000000000000, 1), (0x000fffffffffffff, 1)],
            0x0010000000000000,
            true,
            true,
        ),
        (
            "normal_transition_reverse",
            &[(0x000fffffffffffff, 1), (0x0010000000000000, 1)],
            0x0010000000000000,
            true,
            true,
        ),
        (
            "normal_and_zero_original",
            &[(0x0010000000000000, 1), (0x0000000000000000, 1)],
            0x0008000000000000,
            true,
            true,
        ),
        (
            "normal_and_zero_reverse",
            &[(0x0000000000000000, 1), (0x0010000000000000, 1)],
            0x0008000000000000,
            true,
            true,
        ),
        (
            "huge_and_small_original",
            &[(0x7fefffffffffffff, 1), (0x0000000000000001, 1)],
            0x7fdfffffffffffff,
            false,
            false,
        ),
        (
            "huge_and_small_reverse",
            &[(0x0000000000000001, 1), (0x7fefffffffffffff, 1)],
            0x7fdfffffffffffff,
            false,
            false,
        ),
        (
            "huge_and_zero_original",
            &[(0x7fefffffffffffff, 1), (0x0000000000000000, 1)],
            0x7fdfffffffffffff,
            false,
            false,
        ),
        (
            "huge_and_zero_reverse",
            &[(0x0000000000000000, 1), (0x7fefffffffffffff, 1)],
            0x7fdfffffffffffff,
            false,
            false,
        ),
        (
            "zero_point_four_original",
            &[(0x0000000000000000, 1), (0x3fd999999999999a, 1)],
            0x3fc999999999999a,
            true,
            false,
        ),
        (
            "zero_point_four_reverse",
            &[(0x3fd999999999999a, 1), (0x0000000000000000, 1)],
            0x3fc999999999999a,
            true,
            false,
        ),
        (
            "mean_2.0_below",
            &[(0x3fffffffffffffff, 8)],
            0x3fffffffffffffff,
            true,
            false,
        ),
        (
            "mean_2.0_equal",
            &[(0x4000000000000000, 8)],
            0x4000000000000000,
            false,
            false,
        ),
        (
            "mean_2.0_above",
            &[(0x4000000000000001, 8)],
            0x4000000000000001,
            false,
            false,
        ),
        (
            "mean_0.1_below",
            &[(0x3fb9999999999999, 8)],
            0x3fb9999999999999,
            true,
            true,
        ),
        (
            "mean_0.1_equal",
            &[(0x3fb999999999999a, 8)],
            0x3fb999999999999a,
            true,
            false,
        ),
        (
            "mean_0.1_above",
            &[(0x3fb999999999999b, 8)],
            0x3fb999999999999b,
            true,
            false,
        ),
        (
            "p95_below",
            &[(0x0000000000000000, 18), (0x4007ffffffffffff, 2)],
            0x3fd3333333333332,
            true,
            false,
        ),
        (
            "p95_equal",
            &[(0x0000000000000000, 18), (0x4008000000000000, 2)],
            0x3fd3333333333333,
            false,
            false,
        ),
        (
            "p95_above",
            &[(0x0000000000000000, 18), (0x4008000000000001, 2)],
            0x3fd3333333333334,
            false,
            false,
        ),
        (
            "cpu_max_below",
            &[(0x0000000000000000, 99), (0x4013ffffffffffff, 1)],
            0x3fa9999999999998,
            true,
            false,
        ),
        (
            "cpu_max_equal",
            &[(0x0000000000000000, 99), (0x4014000000000000, 1)],
            0x3fa999999999999a,
            false,
            false,
        ),
        (
            "cpu_max_above",
            &[(0x0000000000000000, 99), (0x4014000000000001, 1)],
            0x3fa999999999999b,
            false,
            false,
        ),
        (
            "gpu_max_below",
            &[(0x0000000000000000, 99), (0x3fdfffffffffffff, 1)],
            0x3f747ae147ae147a,
            true,
            true,
        ),
        (
            "gpu_max_equal",
            &[(0x0000000000000000, 99), (0x3fe0000000000000, 1)],
            0x3f747ae147ae147b,
            true,
            false,
        ),
        (
            "gpu_max_above",
            &[(0x0000000000000000, 99), (0x3fe0000000000001, 1)],
            0x3f747ae147ae147c,
            true,
            false,
        ),
    ];
    const FEASIBILITY_ORACLE: &[(&str, usize, u64, u64, u64, u64)] = &[
        (
            "ordinary_1",
            1,
            0x3fd999999999999a,
            0x3fd999999999999a,
            0x3fd999999999999a,
            0x3fd999999999999a,
        ),
        (
            "subnormal_1",
            1,
            0x0000000000000003,
            0x0000000000000003,
            0x0000000000000003,
            0x0000000000000003,
        ),
        (
            "huge_1",
            1,
            0x7fefffffffffffff,
            0x7fefffffffffffff,
            0x7fefffffffffffff,
            0x7fefffffffffffff,
        ),
        (
            "zero_rank_1",
            1,
            0x0000000000000001,
            0x0000000000000001,
            0x0000000000000001,
            0x0000000000000001,
        ),
        (
            "ordinary_19",
            19,
            0x3fd999999999999a,
            0x3fd999999999999a,
            0x3f958ed2308158ed,
            0x3fd999999999999a,
        ),
        (
            "subnormal_19",
            19,
            0x0000000000000003,
            0x0000000000000003,
            0x0000000000000000,
            0x0000000000000003,
        ),
        (
            "huge_19",
            19,
            0x7fefffffffffffff,
            0x7fefffffffffffff,
            0x7faaf286bca1af28,
            0x7fefffffffffffff,
        ),
        (
            "zero_rank_19",
            19,
            0x0000000000000001,
            0x0000000000000001,
            0x0000000000000000,
            0x0000000000000001,
        ),
        (
            "ordinary_20",
            20,
            0x3fb999999999999a,
            0x3fd999999999999a,
            0x3f9999999999999a,
            0x3fbd70a3d70a3d71,
        ),
        (
            "subnormal_20",
            20,
            0x0000000000000001,
            0x0000000000000003,
            0x0000000000000000,
            0x0000000000000001,
        ),
        (
            "huge_20",
            20,
            0x7fdfffffffffffff,
            0x7fefffffffffffff,
            0x7fb3333333333333,
            0x7fe0cccccccccccc,
        ),
        (
            "zero_rank_20",
            20,
            0x0000000000000000,
            0x0000000000000001,
            0x0000000000000000,
            0x0000000000000000,
        ),
        (
            "ordinary_21",
            21,
            0x3fb999999999999a,
            0x3fd999999999999a,
            0x3f98618618618619,
            0x3fbd41d41d41d41e,
        ),
        (
            "subnormal_21",
            21,
            0x0000000000000001,
            0x0000000000000003,
            0x0000000000000000,
            0x0000000000000001,
        ),
        (
            "huge_21",
            21,
            0x7fdfffffffffffff,
            0x7fefffffffffffff,
            0x7fb2492492492492,
            0x7fe0c30c30c30c30,
        ),
        (
            "zero_rank_21",
            21,
            0x0000000000000000,
            0x0000000000000001,
            0x0000000000000000,
            0x0000000000000000,
        ),
        (
            "ordinary_80000000",
            80000000,
            0x3fb999999999999a,
            0x3fd999999999999a,
            0x3f747ae29f47029e,
            0x3fbd70a3d70a3d71,
        ),
        (
            "subnormal_80000000",
            80000000,
            0x0000000000000001,
            0x0000000000000003,
            0x0000000000000000,
            0x0000000000000001,
        ),
        (
            "huge_80000000",
            80000000,
            0x7fdfffffffffffff,
            0x7fefffffffffffff,
            0x7f99999a70592e6f,
            0x7fe0cccccccccccc,
        ),
        (
            "zero_rank_80000000",
            80000000,
            0x0000000000000000,
            0x0000000000000001,
            0x0000000000000000,
            0x0000000000000000,
        ),
    ];

    #[test]
    fn exact_means_match_independent_fraction_oracle_and_strict_acceptance() {
        assert_eq!(EXACT_MEAN_ORACLE.len(), 80);
        for &(name, runs, expected_mean, cpu, gpu) in EXACT_MEAN_ORACLE {
            let values: Vec<f64> = runs
                .iter()
                .flat_map(|&(bits, count)| std::iter::repeat_n(f64::from_bits(bits), count))
                .collect();
            let stats = stats_for(&values);
            assert_eq!(stats.mean.to_bits(), expected_mean, "mean fixture {name}");
            assert_eq!(check_thresholds(&stats), Ok(cpu), "CPU fixture {name}");
            assert_eq!(check_gpu_parity(&stats), Ok(gpu), "GPU fixture {name}");
        }
    }

    #[test]
    fn review_mixed_exact_threshold_is_two_in_both_orders() {
        let values = [2.5, 2.5, 2.5, 2.5, 1.5, 1.5, 1.5, 1.5];
        let mut reverse = values;
        reverse.reverse();
        for input in [&values[..], &reverse[..]] {
            let stats = stats_for(input);
            assert_eq!(stats.mean, 2.0);
            assert_eq!(check_thresholds(&stats), Ok(false));
        }
    }

    #[test]
    fn review_impossible_zero_mean_public_record_is_rejected() {
        let mut stats = stats_for(&[0.0, 0.4]);
        assert_eq!(stats.mean, 0.2);
        assert_eq!(check_gpu_parity(&stats), Ok(false));
        stats.mean = 0.0;
        assert_invalid_thresholds(&stats);
    }

    fn public_record(count: usize, mean: f64, p95: f64, max: f64) -> DeltaEStats {
        DeltaEStats {
            mean,
            p95,
            max,
            count,
            total: count,
            excluded: 0,
            shadow_only_excluded: 0,
            clipped_highlights_only_excluded: 0,
            overlap_excluded: 0,
            deep_shadows_excluded: 0,
            clipped_highlights_excluded: 0,
        }
    }

    #[test]
    fn rounded_feasibility_bounds_match_independent_fraction_oracle() {
        assert_eq!(FEASIBILITY_ORACLE.len(), 20);
        for &(name, count, percentile, maximum, lower, upper) in FEASIBILITY_ORACLE {
            let p95 = f64::from_bits(percentile);
            let max = f64::from_bits(maximum);
            let rank = p95_rank(count).unwrap();
            let mut exact_lower = ExactNonnegativeSum::new();
            exact_lower.add_weighted(p95, count - rank).unwrap();
            exact_lower.add_weighted(max, 1).unwrap();
            let mut exact_upper = ExactNonnegativeSum::new();
            exact_upper.add_weighted(p95, rank).unwrap();
            exact_upper.add_weighted(max, count - rank).unwrap();
            assert_eq!(
                exact_lower.rounded_mean(count).unwrap().to_bits(),
                lower,
                "lower {name}"
            );
            assert_eq!(
                exact_upper.rounded_mean(count).unwrap().to_bits(),
                upper,
                "upper {name}"
            );
            for boundary in [lower, upper] {
                let stats = public_record(count, f64::from_bits(boundary), p95, max);
                assert!(check_thresholds(&stats).is_ok(), "valid rounded CPU {name}");
                assert!(check_gpu_parity(&stats).is_ok(), "valid rounded GPU {name}");
            }
            let below = public_record(count, f64::from_bits(lower).next_down(), p95, max);
            assert_invalid_thresholds(&below);
            let above = public_record(count, f64::from_bits(upper).next_up(), p95, max);
            assert_invalid_thresholds(&above);
        }
    }

    #[test]
    fn exact_sum_rejects_invalid_terms_and_unrepresentable_quotient() {
        for value in [f64::NAN, f64::INFINITY, -1.0] {
            assert!(ExactNonnegativeSum::new().add_weighted(value, 1).is_err());
        }
        assert!(
            ExactNonnegativeSum::new()
                .add_weighted(1.0, MAX_STATS_SAMPLES + 1)
                .is_err()
        );
        assert!(ExactNonnegativeSum::new().rounded_mean(0).is_err());
        let mut sum = ExactNonnegativeSum::new();
        sum.add_weighted(f64::MAX, 2).unwrap();
        assert!(sum.rounded_mean(1).is_err());
        assert_eq!(sum.rounded_mean(2).unwrap(), f64::MAX);
    }

    #[test]
    fn sharma_pair_1_diagnostic() {
        // The first Sharma pair is the most commonly cited diagnostic.
        let lab1 = [50.0, 2.6772, -79.7751];
        let lab2 = [50.0, 0.0, -82.7485];
        let de = cie_de2000(lab1, lab2);
        assert!((de - 2.0425).abs() < 1e-4, "pair 1: got {}", de);
    }
}
