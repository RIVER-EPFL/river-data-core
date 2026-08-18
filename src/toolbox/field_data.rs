/// Barometric pressure (hPa) estimated from site altitude and air temperature.
///
/// From R `calcAlt2BP`: round(bigleaf::pressure.from.elevation(elev, temp) * 10)
/// where bigleaf computes P = 101.325 / exp(g * elev / (Rd * T_K)) in kPa with
/// g = 9.81, Rd = 287.0586. R's round() ties to even.
#[must_use]
pub fn barometric_pressure_from_altitude(elevation_m: f64, temp_c: f64) -> f64 {
    let temp_k = temp_c + 273.15;
    let pressure_kpa = 101.325 / (9.81 * elevation_m / (287.0586 * temp_k)).exp();
    (pressure_kpa * 10.0).round_ties_even()
}

/// CO2 correction using standard curve + pressure/temperature.
///
/// From R `calcCO2corr`:
///   1. Optionally apply standard curve: raw_co2 = raw_co2 * slope + intercept
///   2. Correct: raw_co2 * pressure_hpa * 298 / (1013 * (273 + temp_c))
///
/// `std_curve`: optional (slope, intercept) pair; pass `None` to skip correction.
#[must_use]
pub fn co2_correction(
    raw_co2: f64,
    pressure_hpa: f64,
    temp_c: f64,
    std_curve: Option<(f64, f64)>,
) -> f64 {
    let corrected = match std_curve {
        Some((slope, intercept)) => raw_co2 * slope + intercept,
        None => raw_co2,
    };
    corrected * pressure_hpa * 298.0 / (1013.0 * (273.0 + temp_c))
}

/// Compute mean and standard deviation of reach depth measurements.
#[must_use]
pub fn reach_depth_stats(depths: &[f64]) -> (f64, f64) {
    (super::common::mean(depths), super::common::std_dev(depths))
}

/// Barometric pressure validity band (hPa) from R `calcCO2corr`.
pub const PRESSURE_HPA_MIN: f64 = 700.0;
pub const PRESSURE_HPA_MAX: f64 = 1050.0;

/// Validate an operator-entered barometric pressure against the 700-1050 hPa band.
/// Catches values entered in the wrong unit (atm, kPa) before they skew a calculation.
pub fn validate_pressure_hpa(pressure_hpa: f64) -> Result<f64, String> {
    if (PRESSURE_HPA_MIN..=PRESSURE_HPA_MAX).contains(&pressure_hpa) {
        Ok(pressure_hpa)
    } else {
        Err(format!(
            "pressure {pressure_hpa} is outside the valid {PRESSURE_HPA_MIN}-{PRESSURE_HPA_MAX} hPa band (is the value in hPa?)"
        ))
    }
}

/// Select the best available pressure value.
///
/// From R pattern: use field_pressure if it's in [700, 1050] hPa, else fall back to altitude_pressure.
#[must_use]
pub fn select_pressure(field_pressure: Option<f64>, altitude_pressure: Option<f64>) -> Option<f64> {
    if let Some(fp) = field_pressure
        && (PRESSURE_HPA_MIN..=PRESSURE_HPA_MAX).contains(&fp)
    {
        return Some(fp);
    }
    altitude_pressure
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_barometric_pressure_sea_level() {
        let result = barometric_pressure_from_altitude(0.0, 15.0);
        assert!(
            (result - 1013.0).abs() < 1.0,
            "expected ~1013 at sea level, got {result}"
        );
    }

    #[test]
    fn test_barometric_pressure_high_altitude() {
        // 101.325 / exp(9.81 * 2000 / (287.0586 * 283.15)) * 10 = 796 rounded
        let result = barometric_pressure_from_altitude(2000.0, 10.0);
        assert!(
            (result - 796.0).abs() < 1.0,
            "expected ~796 at 2000m, got {result}"
        );
    }

    #[test]
    fn test_co2_correction_no_curve() {
        // raw=500, pressure=900, temp=15 => 500 * 900 * 298 / (1013 * 288)
        let result = co2_correction(500.0, 900.0, 15.0, None);
        let expected = 500.0 * 900.0 * 298.0 / (1013.0 * 288.0);
        assert!(
            (result - expected).abs() < 0.001,
            "expected {expected}, got {result}"
        );
    }

    #[test]
    fn test_co2_correction_with_curve() {
        // raw=500, slope=1.1, intercept=-5 => corrected=545
        // 545 * 900 * 298 / (1013 * 288)
        let result = co2_correction(500.0, 900.0, 15.0, Some((1.1, -5.0)));
        let corrected = 500.0 * 1.1 + (-5.0);
        let expected = corrected * 900.0 * 298.0 / (1013.0 * 288.0);
        assert!(
            (result - expected).abs() < 0.001,
            "expected {expected}, got {result}"
        );
    }

    #[test]
    fn test_validate_pressure_hpa_in_band() {
        assert_eq!(validate_pressure_hpa(970.0), Ok(970.0));
        assert_eq!(validate_pressure_hpa(700.0), Ok(700.0));
        assert_eq!(validate_pressure_hpa(1050.0), Ok(1050.0));
    }

    #[test]
    fn test_validate_pressure_hpa_out_of_band() {
        assert!(validate_pressure_hpa(699.9).is_err());
        assert!(validate_pressure_hpa(1050.1).is_err());
        // An atm-scale entry is rejected instead of being silently ~1013x wrong
        assert!(validate_pressure_hpa(0.957).is_err());
    }

    #[test]
    fn test_select_pressure_valid_field() {
        assert_eq!(select_pressure(Some(950.0), Some(800.0)), Some(950.0));
    }

    #[test]
    fn test_select_pressure_out_of_range() {
        assert_eq!(select_pressure(Some(600.0), Some(800.0)), Some(800.0));
    }

    #[test]
    fn test_select_pressure_no_field() {
        assert_eq!(select_pressure(None, Some(800.0)), Some(800.0));
    }
}
