/// SUVA (Specific UV Absorbance) at 254nm.
///
/// From R `calcSUVA`:
///   SUVA = a254 * 1000 / DOC_avg_ppb
///
/// Result is in L/(mg·m). DOC must be in ppb (µg/L).
/// Returns NaN if DOC is zero.
#[must_use]
pub fn suva(a254: f64, doc_avg_ppb: f64) -> f64 {
    if doc_avg_ppb == 0.0 {
        return f64::NAN;
    }
    a254 * 1000.0 / doc_avg_ppb
}

/// Generic absorbance ratio (E2:E3, E4:E6, spectral slope ratio).
///
/// From R `calcRatio`. Returns NaN if denominator is zero.
#[must_use]
pub fn absorbance_ratio(numerator: f64, denominator: f64) -> f64 {
    super::common::ratio(numerator, denominator)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: f64 = 1e-6;

    #[test]
    fn test_suva_basic() {
        // a254=0.1, DOC=2000 ppb => SUVA = 0.1 * 1000 / 2000 = 0.05
        let result = suva(0.1, 2000.0);
        assert!((result - 0.05).abs() < TOL, "expected 0.05, got {result}");
    }

    #[test]
    fn test_suva_zero_doc() {
        assert!(suva(0.1, 0.0).is_nan());
    }

    #[test]
    fn test_absorbance_ratio() {
        // E2:E3 = a250/a365
        let result = absorbance_ratio(0.2, 0.05);
        assert!((result - 4.0).abs() < TOL);
    }
}
