use serde::{Deserialize, Serialize};

// ============================================================================
// Constants
// ============================================================================

/// Henry volatility constant for CO2 hardcoded by the portal's `calcCO2` and
/// `calcpCO2`/`calcpCO2P1`/`calcpCO2P2` (the `h_co2_29815k` constants-table
/// entry is only consulted by the DIC functions).
pub(crate) const H_CO2_LITERAL: f64 = 0.034;

/// Lab pressure literal hardcoded by the portal's `calcCH4`.
const CH4_LAB_PRESSURE_ATM: f64 = 0.957237;

/// CH4 Henry's law temperature dependence hardcoded by the portal's `calcCH4`.
const CH4_TEMP_CONST: f64 = 1750.0;

/// Physical constants for gas calculations, loaded from the `constants` table.
/// Defaults mirror the portal constants table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasConstants {
    /// Constant C of the van't Hoff equation (K). Portal `c_const`.
    pub c_const: f64,
    /// Universal gas constant (L·atm/(mol·K)). Portal `gas_const_r_atm`.
    pub gas_const_r_atm: f64,
    /// Universal gas constant (J/(mol·K)). Portal `gas_const_r_mol`.
    pub gas_const_r_mol: f64,
    /// Henry's law constant for CH4 at 298.15K. Portal `h_ch4_29815k`.
    pub h_ch4_29815k: f64,
    /// Fraction of CH4 in standard air. Portal `ch4_in_sa`.
    pub ch4_in_sa: f64,
}

impl Default for GasConstants {
    fn default() -> Self {
        Self {
            c_const: 2400.0,
            gas_const_r_atm: 0.082_057_4,
            gas_const_r_mol: 8.314_46,
            h_ch4_29815k: 0.002_13,
            ch4_in_sa: 0.000_002,
        }
    }
}

/// Lab-condition defaults from the portal `constants` table, consulted when the
/// operator leaves a lab entry blank (R `calcCO2` 'default' mode).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabConstants {
    /// Lab temperature fallback (degC). Portal `lab_temp_avg_degC`.
    pub lab_temp_avg_degc: f64,
    /// Lab pressure fallback, already in atm. Portal `lab_press_avg_atm`.
    pub lab_press_avg_atm: f64,
    /// Syringe standard-air volume (L). Portal `vol_sa`.
    pub vol_sa: f64,
    /// Syringe water volume (L). Portal `vol_water`.
    pub vol_water: f64,
}

impl Default for LabConstants {
    fn default() -> Self {
        Self {
            lab_temp_avg_degc: 22.5,
            lab_press_avg_atm: 0.957237,
            vol_sa: 0.03,
            vol_water: 0.03,
        }
    }
}

/// Operator-entered lab conditions. Every field is optional; a missing value falls
/// back to the matching `LabConstants` entry. Pressure is entered in hPa, as the
/// portal stores it, and converted to atm during resolution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LabEntry {
    pub lab_temp_c: Option<f64>,
    pub lab_pressure_hpa: Option<f64>,
    #[serde(alias = "vol_sa_ml")]
    pub vol_sa: Option<f64>,
    #[serde(alias = "vol_water_ml")]
    pub vol_water: Option<f64>,
}

/// Resolved lab conditions ready for `co2_headspace`/`pco2_full_pipeline`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabConditions {
    pub lab_temp_c: f64,
    pub lab_pressure_atm: f64,
    pub vol_sa: f64,
    pub vol_water: f64,
}

/// Resolve operator lab entries against constants-table defaults.
///
/// Mirrors R `calcCO2`: an entered lab pressure is hPa and is divided by 1013.25;
/// the `lab_press_avg_atm` fallback is already atm and passes through unchanged.
/// An entered pressure outside 700-1050 hPa is rejected. The two volumes only
/// enter the calculation as a ratio, so any unit shared by both works; the
/// constants-table defaults are in litres.
pub fn resolve_lab_conditions(
    entry: &LabEntry,
    constants: &LabConstants,
) -> Result<LabConditions, String> {
    let lab_pressure_atm = match entry.lab_pressure_hpa {
        Some(hpa) => {
            super::field_data::validate_pressure_hpa(hpa)
                .map_err(|e| format!("lab_pressure_hpa: {e}"))?
                / 1013.25
        }
        None => constants.lab_press_avg_atm,
    };
    Ok(LabConditions {
        lab_temp_c: entry.lab_temp_c.unwrap_or(constants.lab_temp_avg_degc),
        lab_pressure_atm,
        vol_sa: entry.vol_sa.unwrap_or(constants.vol_sa),
        vol_water: entry.vol_water.unwrap_or(constants.vol_water),
    })
}

/// CH4 dry concentration corrected for water vapor.
///
/// From R `calcCH4dry`:
///   ch4_dry = (h2o * 1.2347 - 0.0016) * ch4 / 100 + ch4
#[must_use]
pub fn ch4_dry(ch4_raw: f64, h2o_percent: f64) -> f64 {
    (h2o_percent * 1.2347 - 0.0016) * ch4_raw / 100.0 + ch4_raw
}

/// pCO2 from headspace CO2aq concentration (µM), simplest variant.
///
/// From R `calcpCO2`:
///   pCO2 = CO2_aq / (0.034 * exp(c_const * (1/T_water - 1/298.15)))
#[must_use]
pub fn pco2_from_co2aq(co2_aq_umol: f64, water_temp_c: f64, constants: &GasConstants) -> f64 {
    let t_water_k = water_temp_c + 273.15;
    let kh_t = H_CO2_LITERAL * (constants.c_const * (1.0 / t_water_k - 1.0 / 298.15)).exp();
    if kh_t == 0.0 {
        return f64::NAN;
    }
    co2_aq_umol / kh_t
}

/// pCO2 variant P1: pressure-corrected with barometric pressure.
///
/// From R `calcpCO2P1`:
///   pCO2 = CO2_aq * bp / (0.034 * exp(c_const * (1/T - 1/298.15)) * 1013.25)
#[must_use]
pub fn pco2_p1(
    co2_aq_umol: f64,
    water_temp_c: f64,
    pressure_hpa: f64,
    constants: &GasConstants,
) -> f64 {
    let t_water_k = water_temp_c + 273.15;
    let kh_t = H_CO2_LITERAL * (constants.c_const * (1.0 / t_water_k - 1.0 / 298.15)).exp();
    let divisor = kh_t * 1013.25;
    if divisor == 0.0 {
        return f64::NAN;
    }
    co2_aq_umol * pressure_hpa / divisor
}

/// pCO2 variant P2: inverse pressure correction.
///
/// From R `calcpCO2P2`:
///   pCO2 = CO2_aq * 1013.25 / (0.034 * exp(c_const * (1/T - 1/298.15)) * bp)
#[must_use]
pub fn pco2_p2(
    co2_aq_umol: f64,
    water_temp_c: f64,
    pressure_hpa: f64,
    constants: &GasConstants,
) -> f64 {
    let t_water_k = water_temp_c + 273.15;
    let kh_t = H_CO2_LITERAL * (constants.c_const * (1.0 / t_water_k - 1.0 / 298.15)).exp();
    let divisor = kh_t * pressure_hpa;
    if divisor == 0.0 {
        return f64::NAN;
    }
    co2_aq_umol * 1013.25 / divisor
}

/// Dissolved CH4 from headspace analysis.
///
/// From R `calcCH4`:
///   h_ch4_t_eq = h_ch4_29815k * exp(1750 * (1/T_lab - 1/298.15))
///   A = ch4_dry * (0.957237 * 1013.25) * 101.325 * T_water - bp * (ch4_in_sa * T_lab * 10^3)
///   B = h_ch4_t_eq * R_mol * 10 * T_water + bp
///   CH4 = A * B / (T_lab * bp * R_mol * T_water)
///
/// The 0.957237 lab pressure is a portal literal (the fetched lab pressure is unused there).
/// Returns CH4 in µmol/L.
#[must_use]
pub fn dissolved_ch4(
    ch4_dry_ppm: f64,
    water_temp_c: f64,
    pressure_hpa: f64,
    lab_temp_c: f64,
    constants: &GasConstants,
) -> f64 {
    let t_water_k = water_temp_c + 273.15;
    let t_lab_k = lab_temp_c + 273.15;
    let bp = pressure_hpa;

    let h_ch4_t_eq =
        constants.h_ch4_29815k * (CH4_TEMP_CONST * (1.0 / t_lab_k - 1.0 / 298.15)).exp();

    let a = ch4_dry_ppm * (CH4_LAB_PRESSURE_ATM * 1013.25) * 101.325 * t_water_k
        - bp * (constants.ch4_in_sa * t_lab_k * 1e3);
    let b = h_ch4_t_eq * constants.gas_const_r_mol * 10.0 * t_water_k + bp;

    let dividend = a * b;
    let divisor = t_lab_k * bp * constants.gas_const_r_mol * t_water_k;

    if divisor == 0.0 {
        return f64::NAN;
    }
    dividend / divisor
}

// ============================================================================
// Full pipeline (raw Picarro → all derived values)
// ============================================================================

/// Input for the full pCO2 pipeline starting from raw Picarro data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pco2FullInput {
    pub co2_ppm: f64,
    pub h2o_percent: f64,
    pub ch4_ppm: f64,
    pub d13co2_permil: Option<f64>,
    pub lab_temp_c: f64,
    pub lab_pressure_atm: f64,
    pub vol_sa_ml: f64,
    pub vol_water_ml: f64,
    pub water_temp_c: f64,
    pub field_pressure_hpa: f64,
}

/// All outputs from the full pCO2 pipeline, matching legacy CNET naming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pco2FullResult {
    /// CO2 headspace in µmol/L.
    pub co2_hs_umol: f64,
    /// pCO2 simple (µatm).
    pub pco2_uatm: f64,
    /// pCO2 P1 (µatm).
    pub pco2_p1_uatm: f64,
    /// pCO2 P2 (µatm).
    pub pco2_p2_uatm: f64,
    /// CH4 dry (ppm).
    pub ch4_dry_ppm: f64,
    /// Dissolved CH4 (µmol/L).
    pub ch4_dissolved_umol: f64,
    /// δ13C-CO2 pass-through (‰).
    pub d13co2_permil: Option<f64>,
}

/// Run the full pCO2 pipeline from raw Picarro data.
///
/// 1. CO2 headspace from raw ppm via `co2_headspace()`
/// 2. pCO2 simple, P1, P2 from headspace CO2aq
/// 3. CH4 dry correction
/// 4. Dissolved CH4
/// 5. δ13C-CO2 pass-through
#[must_use]
pub fn pco2_full_pipeline(input: &Pco2FullInput, constants: &GasConstants) -> Pco2FullResult {
    let co2_hs_umol = super::co2_air::co2_headspace(
        input.co2_ppm,
        input.lab_temp_c,
        input.lab_pressure_atm,
        input.vol_sa_ml,
        input.vol_water_ml,
        constants,
    );

    let pco2_uatm = pco2_from_co2aq(co2_hs_umol, input.water_temp_c, constants);
    let pco2_p1_uatm = pco2_p1(
        co2_hs_umol,
        input.water_temp_c,
        input.field_pressure_hpa,
        constants,
    );
    let pco2_p2_uatm = pco2_p2(
        co2_hs_umol,
        input.water_temp_c,
        input.field_pressure_hpa,
        constants,
    );

    let ch4_dry_ppm = ch4_dry(input.ch4_ppm, input.h2o_percent);

    let ch4_dissolved_umol = dissolved_ch4(
        ch4_dry_ppm,
        input.water_temp_c,
        input.field_pressure_hpa,
        input.lab_temp_c,
        constants,
    );

    Pco2FullResult {
        co2_hs_umol,
        pco2_uatm,
        pco2_p1_uatm,
        pco2_p2_uatm,
        ch4_dry_ppm,
        ch4_dissolved_umol,
        d13co2_permil: input.d13co2_permil,
    }
}

// ============================================================================
// Replicate averaging
// ============================================================================

/// Averaged results from two replicates (A and B) of the full pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pco2ReplicateResult {
    pub a: Pco2FullResult,
    pub b: Pco2FullResult,
    // Averages
    pub co2_hs_umol_avg: f64,
    pub pco2_uatm_avg: f64,
    pub pco2_p1_uatm_avg: f64,
    pub pco2_p2_uatm_avg: f64,
    pub ch4_dry_ppm_avg: f64,
    pub ch4_dissolved_umol_avg: f64,
    pub d13co2_permil_avg: Option<f64>,
    // Sample standard deviations
    pub co2_hs_umol_sd: f64,
    pub pco2_uatm_sd: f64,
    pub pco2_p1_uatm_sd: f64,
    pub pco2_p2_uatm_sd: f64,
    pub ch4_dry_ppm_sd: f64,
    pub ch4_dissolved_umol_sd: f64,
    pub d13co2_permil_sd: Option<f64>,
}

/// Run the full pipeline on two replicates and return averages + SDs.
///
/// Averages and SDs drop non-finite replicates like R's `mean(na.rm = TRUE)` /
/// `sd(na.rm = TRUE)`: one NaN replicate yields the other value (SD NaN),
/// both NaN yields NaN.
#[must_use]
pub fn pco2_replicates(
    input_a: &Pco2FullInput,
    input_b: &Pco2FullInput,
    constants: &GasConstants,
) -> Pco2ReplicateResult {
    use super::common::{mean, std_dev};

    let a = pco2_full_pipeline(input_a, constants);
    let b = pco2_full_pipeline(input_b, constants);

    let d13_avg = match (a.d13co2_permil, b.d13co2_permil) {
        (Some(da), Some(db)) => Some(mean(&[da, db])),
        (Some(v), None) | (None, Some(v)) => Some(v),
        (None, None) => None,
    };
    let d13_sd = match (a.d13co2_permil, b.d13co2_permil) {
        (Some(da), Some(db)) => Some(std_dev(&[da, db])),
        _ => None,
    };

    Pco2ReplicateResult {
        co2_hs_umol_avg: mean(&[a.co2_hs_umol, b.co2_hs_umol]),
        pco2_uatm_avg: mean(&[a.pco2_uatm, b.pco2_uatm]),
        pco2_p1_uatm_avg: mean(&[a.pco2_p1_uatm, b.pco2_p1_uatm]),
        pco2_p2_uatm_avg: mean(&[a.pco2_p2_uatm, b.pco2_p2_uatm]),
        ch4_dry_ppm_avg: mean(&[a.ch4_dry_ppm, b.ch4_dry_ppm]),
        ch4_dissolved_umol_avg: mean(&[a.ch4_dissolved_umol, b.ch4_dissolved_umol]),
        d13co2_permil_avg: d13_avg,
        co2_hs_umol_sd: std_dev(&[a.co2_hs_umol, b.co2_hs_umol]),
        pco2_uatm_sd: std_dev(&[a.pco2_uatm, b.pco2_uatm]),
        pco2_p1_uatm_sd: std_dev(&[a.pco2_p1_uatm, b.pco2_p1_uatm]),
        pco2_p2_uatm_sd: std_dev(&[a.pco2_p2_uatm, b.pco2_p2_uatm]),
        ch4_dry_ppm_sd: std_dev(&[a.ch4_dry_ppm, b.ch4_dry_ppm]),
        ch4_dissolved_umol_sd: std_dev(&[a.ch4_dissolved_umol, b.ch4_dissolved_umol]),
        d13co2_permil_sd: d13_sd,
        a,
        b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: f64 = 0.01;

    #[test]
    fn test_ch4_dry() {
        let result = ch4_dry(2000.0, 1.5);
        let expected = (1.5 * 1.2347 - 0.0016) * 2000.0 / 100.0 + 2000.0;
        assert!(
            (result - expected).abs() < TOL,
            "expected {expected}, got {result}"
        );
    }

    #[test]
    fn test_resolve_lab_conditions_all_defaults() {
        let lc = resolve_lab_conditions(&LabEntry::default(), &LabConstants::default()).unwrap();
        assert!((lc.lab_temp_c - 22.5).abs() < 1e-12);
        assert!((lc.lab_pressure_atm - 0.957237).abs() < 1e-12);
        assert!((lc.vol_sa - 0.03).abs() < 1e-12);
        assert!((lc.vol_water - 0.03).abs() < 1e-12);
    }

    #[test]
    fn test_resolve_lab_conditions_hpa_entry_converted() {
        let entry = LabEntry {
            lab_pressure_hpa: Some(970.0),
            ..Default::default()
        };
        let lc = resolve_lab_conditions(&entry, &LabConstants::default()).unwrap();
        // 970 / 1013.25
        assert!((lc.lab_pressure_atm - 970.0 / 1013.25).abs() < 1e-12);
    }

    #[test]
    fn test_resolve_lab_conditions_rejects_out_of_band_pressure() {
        for hpa in [0.957, 96.0, 699.0, 1051.0] {
            let entry = LabEntry {
                lab_pressure_hpa: Some(hpa),
                ..Default::default()
            };
            assert!(resolve_lab_conditions(&entry, &LabConstants::default()).is_err());
        }
    }

    #[test]
    fn test_pco2_from_co2aq() {
        let constants = GasConstants::default();
        let result = pco2_from_co2aq(50.0, 15.0, &constants);
        assert!(
            result > 0.0 && result.is_finite(),
            "expected positive pCO2, got {result}"
        );
    }

    #[test]
    fn test_pco2_p1_vs_p2_reciprocal() {
        let constants = GasConstants::default();
        let co2 = 50.0;
        let temp = 15.0;
        let bp = 900.0;
        let p1 = pco2_p1(co2, temp, bp, &constants);
        let p2 = pco2_p2(co2, temp, bp, &constants);
        let ratio = p1 / p2;
        let expected_ratio = (bp / 1013.25).powi(2);
        assert!(
            (ratio - expected_ratio).abs() < 0.001,
            "P1/P2 ratio {ratio} != expected {expected_ratio}"
        );
    }

    fn make_test_input(co2_ppm: f64, ch4_ppm: f64, d13: Option<f64>) -> Pco2FullInput {
        Pco2FullInput {
            co2_ppm,
            h2o_percent: 1.5,
            ch4_ppm,
            d13co2_permil: d13,
            lab_temp_c: 22.0,
            lab_pressure_atm: 0.95,
            vol_sa_ml: 60.0,
            vol_water_ml: 40.0,
            water_temp_c: 12.0,
            field_pressure_hpa: 960.0,
        }
    }

    #[test]
    fn test_full_pipeline_results_finite_and_positive() {
        let constants = GasConstants::default();
        let input = make_test_input(3000.0, 5.0, Some(-12.5));
        let result = pco2_full_pipeline(&input, &constants);

        assert!(
            result.co2_hs_umol > 0.0 && result.co2_hs_umol.is_finite(),
            "co2_hs_umol should be positive and finite, got {}",
            result.co2_hs_umol
        );
        assert!(
            result.pco2_uatm > 0.0 && result.pco2_uatm.is_finite(),
            "pco2_uatm should be positive and finite, got {}",
            result.pco2_uatm
        );
        assert!(
            result.pco2_p1_uatm > 0.0 && result.pco2_p1_uatm.is_finite(),
            "pco2_p1_uatm should be positive and finite, got {}",
            result.pco2_p1_uatm
        );
        assert!(
            result.pco2_p2_uatm > 0.0 && result.pco2_p2_uatm.is_finite(),
            "pco2_p2_uatm should be positive and finite, got {}",
            result.pco2_p2_uatm
        );
        assert!(
            result.ch4_dry_ppm > 0.0 && result.ch4_dry_ppm.is_finite(),
            "ch4_dry_ppm should be positive and finite, got {}",
            result.ch4_dry_ppm
        );
        assert!(
            result.ch4_dissolved_umol.is_finite(),
            "ch4_dissolved_umol should be finite, got {}",
            result.ch4_dissolved_umol
        );
        assert_eq!(result.d13co2_permil, Some(-12.5));
    }

    #[test]
    fn test_full_pipeline_co2hs_feeds_pco2() {
        let constants = GasConstants::default();
        let input = make_test_input(3000.0, 5.0, None);
        let result = pco2_full_pipeline(&input, &constants);

        let expected_pco2 = pco2_from_co2aq(result.co2_hs_umol, input.water_temp_c, &constants);
        assert!(
            (result.pco2_uatm - expected_pco2).abs() < 1e-10,
            "pipeline pco2 {} != direct pco2 {}",
            result.pco2_uatm,
            expected_pco2
        );
    }

    #[test]
    fn test_replicates_averages_and_sds_finite() {
        let constants = GasConstants::default();
        let a = make_test_input(3000.0, 5.0, Some(-12.0));
        let b = make_test_input(3200.0, 5.5, Some(-13.0));
        let rep = pco2_replicates(&a, &b, &constants);

        assert!(rep.co2_hs_umol_avg.is_finite());
        assert!(rep.pco2_uatm_avg.is_finite());
        assert!(rep.pco2_p1_uatm_avg.is_finite());
        assert!(rep.pco2_p2_uatm_avg.is_finite());
        assert!(rep.ch4_dry_ppm_avg.is_finite());
        assert!(rep.ch4_dissolved_umol_avg.is_finite());

        assert!(rep.co2_hs_umol_sd >= 0.0 && rep.co2_hs_umol_sd.is_finite());
        assert!(rep.pco2_uatm_sd >= 0.0 && rep.pco2_uatm_sd.is_finite());
        assert!(rep.pco2_p1_uatm_sd >= 0.0 && rep.pco2_p1_uatm_sd.is_finite());
        assert!(rep.pco2_p2_uatm_sd >= 0.0 && rep.pco2_p2_uatm_sd.is_finite());
        assert!(rep.ch4_dry_ppm_sd >= 0.0 && rep.ch4_dry_ppm_sd.is_finite());
        assert!(rep.ch4_dissolved_umol_sd >= 0.0 && rep.ch4_dissolved_umol_sd.is_finite());

        assert!(rep.d13co2_permil_avg.is_some());
        assert!(rep.d13co2_permil_sd.is_some());
    }

    #[test]
    fn test_replicates_sd_matches_two_value_sample_sd() {
        let constants = GasConstants::default();
        let a = make_test_input(3000.0, 5.0, None);
        let b = make_test_input(3200.0, 5.5, None);
        let rep = pco2_replicates(&a, &b, &constants);

        let expected_sd = (rep.a.co2_hs_umol - rep.b.co2_hs_umol).abs() / 2.0_f64.sqrt();
        assert!(
            (rep.co2_hs_umol_sd - expected_sd).abs() < 1e-10,
            "SD {} != expected {}",
            rep.co2_hs_umol_sd,
            expected_sd
        );
    }

    #[test]
    fn test_replicates_nan_replicate_dropped() {
        // Scenario: replicate B has zero water volume, so its headspace CO2 is NaN.
        // Expected behaviour: averages fall back to replicate A alone (mean na.rm = TRUE),
        // SDs are NaN (single value).
        let constants = GasConstants::default();
        let a = make_test_input(3000.0, 5.0, None);
        let mut b = make_test_input(3200.0, 5.5, None);
        b.vol_water_ml = 0.0;
        let rep = pco2_replicates(&a, &b, &constants);

        assert!(rep.b.co2_hs_umol.is_nan());
        assert!(
            (rep.co2_hs_umol_avg - rep.a.co2_hs_umol).abs() < 1e-10,
            "avg {} != replicate A {}",
            rep.co2_hs_umol_avg,
            rep.a.co2_hs_umol
        );
        assert!(rep.co2_hs_umol_sd.is_nan());
    }

    #[test]
    fn test_replicates_identical_inputs_zero_sd() {
        let constants = GasConstants::default();
        let input = make_test_input(3000.0, 5.0, Some(-12.0));
        let rep = pco2_replicates(&input, &input, &constants);

        assert!(
            (rep.co2_hs_umol_sd).abs() < 1e-10,
            "identical inputs should give SD=0"
        );
        assert!(
            (rep.pco2_uatm_sd).abs() < 1e-10,
            "identical inputs should give SD=0"
        );
        assert_eq!(rep.d13co2_permil_sd, Some(0.0));
    }
}
