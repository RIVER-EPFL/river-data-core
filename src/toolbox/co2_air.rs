use super::pco2::{GasConstants, H_CO2_LITERAL};

/// Headspace CO2 concentration (µmol/L) from Picarro analysis.
///
/// From R `calcCO2`:
///   exponent = exp(c_const * (1/T_lab_K - 1/298.15))
///   CO2 = co2_ppm * P_lab * (vol_sa + 0.034 * exponent * vol_water * R_atm * T_lab_K) / (R_atm * vol_water * T_lab_K)
///
/// The 0.034 Henry constant is the shared `H_CO2_LITERAL` portal literal. Volumes
/// only enter as a ratio, so any consistent unit works. Returns CO2 concentration
/// in µmol/L.
#[must_use]
pub fn co2_headspace(
    co2_ppm: f64,
    lab_temp_c: f64,
    lab_pressure_atm: f64,
    vol_sa_ml: f64,
    vol_water_ml: f64,
    constants: &GasConstants,
) -> f64 {
    let t_lab_k = lab_temp_c + 273.15;
    let exponent = (constants.c_const * (1.0 / t_lab_k - 1.0 / 298.15)).exp();
    let dividend = co2_ppm
        * lab_pressure_atm
        * (vol_sa_ml
            + H_CO2_LITERAL * exponent * vol_water_ml * constants.gas_const_r_atm * t_lab_k);
    let divisor = constants.gas_const_r_atm * vol_water_ml * t_lab_k;

    if divisor == 0.0 {
        return f64::NAN;
    }
    dividend / divisor
}

/// CH4 dry concentration from wet measurement.
///
/// From R `calcCH4dry`:
///   ch4_dry = (h2o * 1.2347 - 0.0016) * ch4 / 100 + ch4
///
/// Re-exported from pco2 module but also available here for the air context.
#[must_use]
pub fn ch4_dry_air(ch4_wet: f64, h2o_percent: f64) -> f64 {
    super::pco2::ch4_dry(ch4_wet, h2o_percent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_co2_headspace_positive() {
        let constants = GasConstants::default();
        let result = co2_headspace(400.0, 22.0, 0.95, 60.0, 40.0, &constants);
        assert!(
            result > 0.0 && result.is_finite(),
            "expected positive CO2, got {result}"
        );
    }

    #[test]
    fn test_co2_headspace_zero_water_volume() {
        let constants = GasConstants::default();
        assert!(co2_headspace(400.0, 22.0, 0.95, 60.0, 0.0, &constants).is_nan());
    }

    #[test]
    fn test_ch4_dry_air_matches_pco2() {
        let a = ch4_dry_air(2000.0, 1.5);
        let b = crate::toolbox::pco2::ch4_dry(2000.0, 1.5);
        assert!((a - b).abs() < 1e-10);
    }
}
