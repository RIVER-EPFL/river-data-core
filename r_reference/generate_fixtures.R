#!/usr/bin/env Rscript
#
# Golden value fixture generator for river-data-core toolbox tests.
#
# Produces tests/fixtures/golden_values.json with expected outputs for each
# function, computed from standalone R implementations matching the portal
# originals (cnet-data-portal/app/utils/calculation_functions.R).
#
# Requirements: R >= 4.0, jsonlite
# Run: Rscript r_reference/generate_fixtures.R
#
# The verbatim portal code lives in r_reference/functions/ for traceability.
# This script contains standalone versions (no DB, no dplyr) that produce
# identical numeric results for the same inputs.

library(jsonlite)

# =============================================================================
# Standalone pure-math implementations
# =============================================================================

# --- Common ---

calc_mean <- function(values) {
  v <- values[!is.na(values)]
  if (length(v) == 0) return(NA_real_)
  mean(v)
}

calc_sd <- function(values) {
  v <- values[!is.na(values)]
  if (length(v) < 2) return(NA_real_)
  sd(v)
}

calc_minus <- function(a, b) {
  if (is.na(a) || is.na(b)) return(NA_real_)
  a - b
}

calc_equals <- function(primary, fallback) {
  if (is.na(primary)) return(fallback)
  primary
}

calc_ratio <- function(dividend, divisor) {
  if (is.na(dividend) || is.na(divisor) || divisor == 0) return(NA_real_)
  dividend / divisor
}

# --- TSS / AFDM ---

calc_tss <- function(wgt_dried, wgt_prefilt, vol_filtered) {
  if (any(is.na(c(wgt_dried, wgt_prefilt, vol_filtered)))) return(NA_real_)
  if (vol_filtered == 0) return(NA_real_)
  1000000 * (wgt_dried - wgt_prefilt) / vol_filtered
}

calc_afdm <- function(wgt_dried, wgt_ashed, vol_filtered) {
  if (any(is.na(c(wgt_dried, wgt_ashed, vol_filtered)))) return(NA_real_)
  if (vol_filtered == 0) return(NA_real_)
  1000000 * (wgt_dried - wgt_ashed) / vol_filtered
}

# --- DOM ---

calc_suva <- function(a254, doc_avg_ppb) {
  if (any(is.na(c(a254, doc_avg_ppb))) || doc_avg_ppb == 0) return(NA_real_)
  a254 * 1000 / doc_avg_ppb
}

# --- DOC ---

calc_doc_average <- function(replicates, slope = NA, intercept = NA) {
  if (!is.na(slope) && !is.na(intercept)) {
    replicates <- replicates * slope + intercept
  }
  calc_mean(replicates)
}

calc_doc_sd <- function(replicates, slope = NA, intercept = NA) {
  if (!is.na(slope) && !is.na(intercept)) {
    replicates <- replicates * slope + intercept
  }
  calc_sd(replicates)
}

# --- Chlorophyll ---

calc_chla_acid <- function(fluor_before, fluor_after, slope, intercept) {
  if (any(is.na(c(fluor_before, fluor_after, slope, intercept)))) return(NA_real_)
  (fluor_before - fluor_after) * slope + intercept
}

calc_chla_no_acid <- function(fluor, slope, intercept) {
  if (any(is.na(c(fluor, slope, intercept)))) return(NA_real_)
  fluor * slope + intercept
}

# --- Benthic ---

calc_rock_surface_area <- function(dims_cm) {
  # Thomsen approximation for ellipsoid surface area
  # area = 2 * pi * mean(combn((d/100)^1.6075, 2, prod))^(1/1.6075)
  d_m <- (dims_cm / 100)^1.6075
  pairs <- combn(d_m, 2, prod)
  2 * pi * mean(pairs)^(1/1.6075)
}

calc_per_m2 <- function(sample_value, dims_cm, vol_filtrated, vol_total) {
  area <- calc_rock_surface_area(dims_cm)
  if (area == 0 || vol_filtrated == 0) return(NA_real_)
  sample_value * vol_total / (vol_filtrated * area)
}

calc_benthic_afdm <- function(afdm_g, dims_cm, vol_filtrated, vol_total) {
  calc_per_m2(afdm_g, dims_cm, vol_filtrated, vol_total)
}

calc_chla_per_m2 <- function(chla_ugl, dims_cm, vol_filtrated, vol_total) {
  calc_per_m2(chla_ugl * 0.005, dims_cm, vol_filtrated, vol_total)
}

# --- Field data ---

calc_barometric_pressure <- function(elevation_m, temp_c) {
  # Barometric formula matching bigleaf::pressure.from.elevation
  # P = 1013.25 * (1 - 0.0065 * elev / (T + 273.15 + 0.0065 * elev))^5.2561
  # Returns hPa * 10 (matching R portal which rounds to integer)
  if (any(is.na(c(elevation_m, temp_c)))) return(NA_real_)
  p_kpa <- 101.325 * (1 - 0.0065 * elevation_m / (temp_c + 273.15 + 0.0065 * elevation_m))^5.2561
  p_kpa * 10  # kPa to hPa
}

calc_co2_correction <- function(raw_co2, temp_c, pressure_hpa,
                                 std_slope = NA, std_intercept = NA) {
  if (any(is.na(c(raw_co2, temp_c, pressure_hpa)))) return(NA_real_)
  co2 <- raw_co2
  if (!is.na(std_slope) && !is.na(std_intercept)) {
    co2 <- co2 * std_slope + std_intercept
  }
  co2 * pressure_hpa * 298 / (1013 * (273 + temp_c))
}

# --- CH4 dry ---

calc_ch4_dry <- function(ch4_wet, h2o_percent) {
  if (any(is.na(c(ch4_wet, h2o_percent)))) return(NA_real_)
  (h2o_percent * 1.2347 - 0.0016) * ch4_wet / 100 + ch4_wet
}

# --- pCO2 variants ---

calc_pco2 <- function(co2_aq_umol, water_temp_c, c_const = 2392.86) {
  if (any(is.na(c(co2_aq_umol, water_temp_c)))) return(NA_real_)
  water_temp_k <- water_temp_c + 273.15
  divisor <- 0.034 * exp(c_const * (1/water_temp_k - 1/298.15))
  if (divisor == 0) return(NA_real_)
  co2_aq_umol / divisor
}

calc_pco2_p1 <- function(co2_aq_umol, water_temp_c, bp_hpa, c_const = 2392.86) {
  if (any(is.na(c(co2_aq_umol, water_temp_c, bp_hpa)))) return(NA_real_)
  water_temp_k <- water_temp_c + 273.15
  dividend <- co2_aq_umol * bp_hpa
  divisor <- 0.034 * exp(c_const * (1/water_temp_k - 1/298.15)) * 1013.25
  if (divisor == 0) return(NA_real_)
  dividend / divisor
}

calc_pco2_p2 <- function(co2_aq_umol, water_temp_c, bp_hpa, c_const = 2392.86) {
  if (any(is.na(c(co2_aq_umol, water_temp_c, bp_hpa)))) return(NA_real_)
  water_temp_k <- water_temp_c + 273.15
  dividend <- co2_aq_umol * 1013.25
  divisor <- 0.034 * exp(c_const * (1/water_temp_k - 1/298.15)) * bp_hpa
  if (divisor == 0) return(NA_real_)
  dividend / divisor
}

# --- CO2 headspace (calcCO2) ---

calc_co2_headspace <- function(co2_ppm, lab_temp_c, lab_pressure_atm,
                                vol_sa_ml, vol_water_ml,
                                kh_co2 = 0.034, c_const = 2392.86,
                                gas_const_r_atm = 0.08206) {
  if (any(is.na(c(co2_ppm, lab_temp_c, lab_pressure_atm, vol_sa_ml, vol_water_ml)))) return(NA_real_)
  t_lab_k <- lab_temp_c + 273.15
  exponent <- exp(c_const * (1/t_lab_k - 1/298.15))
  dividend <- co2_ppm * lab_pressure_atm * (vol_sa_ml + kh_co2 * exponent * vol_water_ml * gas_const_r_atm * t_lab_k)
  divisor <- gas_const_r_atm * vol_water_ml * t_lab_k
  if (divisor == 0) return(NA_real_)
  dividend / divisor
}

# --- DIC ---

calc_dic <- function(acid_sample_wght, acid_wght, vol_overpressure,
                      sa_added, co2_dry, air_temp_c,
                      h_co2_29815k = 3.3e-4, gas_const_r_mol = 8.314,
                      vial_volume = 12.0, h3po4_added = 0.1) {
  if (any(is.na(c(acid_sample_wght, acid_wght, vol_overpressure,
                   sa_added, co2_dry, air_temp_c)))) return(NA_real_)

  lab_temp_k <- air_temp_c + 273.15
  sample_v <- acid_sample_wght - acid_wght
  hs_v <- vial_volume + vol_overpressure - (sample_v + h3po4_added)
  co2_acid <- co2_dry * (sa_added + hs_v)
  gas_temp <- gas_const_r_mol * lab_temp_k
  exponent <- exp(2392.86 * (1/lab_temp_k - 1/298.15))

  dividend <- co2_acid * (h_co2_29815k * exponent * sample_v * gas_temp + 101.325 * hs_v)
  divisor <- 10^3 * gas_temp * hs_v * sample_v
  if (is.na(divisor) || divisor == 0) return(NA_real_)
  dividend / divisor
}

# --- d13C-DIC ---

calc_d13c_dic <- function(acid_sample_wght, acid_wght, vol_overpressure,
                           delta_13co2, air_temp_c,
                           h_co2_29815k = 3.3e-4, gas_const_r_mol = 8.314,
                           vial_volume = 12.0, h3po4_added = 0.1) {
  if (any(is.na(c(acid_sample_wght, acid_wght, vol_overpressure,
                   delta_13co2, air_temp_c)))) return(NA_real_)

  lab_temp_k <- air_temp_c + 273.15
  sample_v <- acid_sample_wght - acid_wght
  hs_v <- vial_volume + vol_overpressure - (sample_v + h3po4_added)
  exponent <- exp(2392.86 * (1/lab_temp_k - 1/298.15))
  H <- h_co2_29815k * exponent * sample_v * gas_const_r_mol

  dividend <- delta_13co2 * 101.325 * hs_v + (lab_temp_k * (delta_13co2 + 0.19) - 373) * H
  divisor <- 101.325 * hs_v + H * lab_temp_k
  if (is.na(divisor) || divisor == 0) return(NA_real_)
  dividend / divisor
}

# --- Dissolved CH4 (calcCH4) ---

calc_dissolved_ch4 <- function(ch4_dry_ppb, water_temp_c, bp_hpa,
                                lab_temp_c, lab_pressure_atm,
                                kh_ch4 = 0.0014, ch4_temp_const = 1750.0,
                                ch4_in_sa = 1.9, gas_const_r_mol = 8.314) {
  if (any(is.na(c(ch4_dry_ppb, water_temp_c, bp_hpa, lab_temp_c, lab_pressure_atm)))) return(NA_real_)

  lab_temp_k <- lab_temp_c + 273.15
  water_temp_k <- water_temp_c + 273.15

  h_ch4_t_eq <- kh_ch4 * exp(ch4_temp_const * (1/lab_temp_k - 1/298.15))
  A <- ch4_dry_ppb * (lab_pressure_atm * 1013.25) * 101.325 * water_temp_k - bp_hpa * (ch4_in_sa * lab_temp_k * 10^3)
  B <- h_ch4_t_eq * gas_const_r_mol * 10 * water_temp_k + bp_hpa

  dividend <- A * B
  divisor <- lab_temp_k * bp_hpa * gas_const_r_mol * water_temp_k
  if (divisor == 0) return(NA_real_)
  dividend / divisor
}


# =============================================================================
# Test case definitions
# =============================================================================

# Helper: wrap a test case
tc <- function(name, inputs, expected, tolerance = 1e-10) {
  list(name = name, inputs = inputs, expected = expected, tolerance = tolerance)
}

cases <- list()

# --- common::mean ---
cases$common$mean <- list(
  tc("three_values", list(values = c(1.0, 2.0, 3.0)), 2.0),
  tc("single_value", list(values = c(42.0)), 42.0),
  tc("with_na", list(values = c(1.0, NA, 3.0)), 2.0),
  tc("all_na", list(values = c(NA, NA)), NA_real_),
  tc("negative_values", list(values = c(-5.0, 10.0, -3.0)), 2.0/3.0),
  tc("large_spread", list(values = c(0.001, 1000000.0)), 500000.0005)
)

# --- common::std_dev ---
cases$common$std_dev <- list(
  tc("basic", list(values = c(2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0)), sd(c(2,4,4,4,5,5,7,9))),
  tc("two_values", list(values = c(1.0, 3.0)), sd(c(1,3))),
  tc("single_value", list(values = c(5.0)), NA_real_),
  tc("with_na", list(values = c(1.0, NA, 5.0)), sd(c(1,5))),
  tc("all_na", list(values = c(NA, NA)), NA_real_),
  tc("identical_values", list(values = c(3.0, 3.0, 3.0)), 0.0)
)

# --- common::minus ---
cases$common$minus <- list(
  tc("positive", list(a = 10.0, b = 3.0), 7.0),
  tc("negative_result", list(a = 3.0, b = 10.0), -7.0),
  tc("zero", list(a = 5.0, b = 5.0), 0.0),
  tc("a_na", list(a = NA, b = 3.0), NA_real_),
  tc("b_na", list(a = 10.0, b = NA), NA_real_)
)

# --- common::equals ---
cases$common$equals <- list(
  tc("primary_valid", list(primary = 42.0, fallback = 99.0), 42.0),
  tc("primary_na", list(primary = NA, fallback = 99.0), 99.0),
  tc("both_na", list(primary = NA, fallback = NA), NA_real_)
)

# --- common::ratio ---
cases$common$ratio <- list(
  tc("normal", list(dividend = 10.0, divisor = 3.0), 10.0/3.0),
  tc("zero_divisor", list(dividend = 10.0, divisor = 0.0), NA_real_),
  tc("dividend_na", list(dividend = NA, divisor = 3.0), NA_real_),
  tc("negative", list(dividend = -6.0, divisor = 2.0), -3.0)
)

# --- tss_afdm::tss ---
cases$tss_afdm$tss_mg_l <- list(
  tc("normal", list(wgt_dried = 0.1025, wgt_prefilt = 0.1000, vol_filtered = 500.0),
     calc_tss(0.1025, 0.1000, 500.0)),
  tc("turbid_sample", list(wgt_dried = 0.1150, wgt_prefilt = 0.1000, vol_filtered = 250.0),
     calc_tss(0.1150, 0.1000, 250.0)),
  tc("zero_volume", list(wgt_dried = 0.1025, wgt_prefilt = 0.1000, vol_filtered = 0.0), NA_real_),
  tc("negative_tss", list(wgt_dried = 0.0990, wgt_prefilt = 0.1000, vol_filtered = 500.0),
     calc_tss(0.0990, 0.1000, 500.0)),
  tc("na_input", list(wgt_dried = NA, wgt_prefilt = 0.1000, vol_filtered = 500.0), NA_real_)
)

# --- tss_afdm::afdm ---
cases$tss_afdm$afdm_mg_l <- list(
  tc("normal", list(wgt_dried = 0.1025, wgt_ashed = 0.1005, vol_filtered = 500.0),
     calc_afdm(0.1025, 0.1005, 500.0)),
  tc("zero_volume", list(wgt_dried = 0.1025, wgt_ashed = 0.1005, vol_filtered = 0.0), NA_real_),
  tc("na_input", list(wgt_dried = 0.1025, wgt_ashed = NA, vol_filtered = 500.0), NA_real_)
)

# --- dom::suva ---
cases$dom$suva <- list(
  tc("normal", list(a254 = 0.15, doc_avg_ppb = 2500.0), calc_suva(0.15, 2500.0)),
  tc("high_doc", list(a254 = 0.05, doc_avg_ppb = 8000.0), calc_suva(0.05, 8000.0)),
  tc("zero_doc", list(a254 = 0.15, doc_avg_ppb = 0.0), NA_real_),
  tc("na_input", list(a254 = NA, doc_avg_ppb = 2500.0), NA_real_)
)

# --- doc ---
cases$doc$doc_average <- list(
  tc("no_curve", list(replicates = c(120.0, 125.0, 118.0), slope = NA, intercept = NA),
     calc_doc_average(c(120, 125, 118))),
  tc("with_curve", list(replicates = c(120.0, 125.0, 118.0), slope = 1.05, intercept = -2.0),
     calc_doc_average(c(120, 125, 118), 1.05, -2.0)),
  tc("single_rep", list(replicates = c(120.0), slope = NA, intercept = NA), 120.0),
  tc("all_na", list(replicates = c(NA, NA, NA), slope = NA, intercept = NA), NA_real_)
)

cases$doc$doc_std_dev <- list(
  tc("no_curve", list(replicates = c(120.0, 125.0, 118.0), slope = NA, intercept = NA),
     calc_doc_sd(c(120, 125, 118))),
  tc("with_curve", list(replicates = c(120.0, 125.0, 118.0), slope = 1.05, intercept = -2.0),
     calc_doc_sd(c(120, 125, 118), 1.05, -2.0)),
  tc("single_rep", list(replicates = c(120.0), slope = NA, intercept = NA), NA_real_)
)

# --- chlorophyll ---
cases$chlorophyll$chla_acid <- list(
  tc("normal", list(fluor_before = 150.0, fluor_after = 80.0, slope = 0.25, intercept = -1.5),
     calc_chla_acid(150.0, 80.0, 0.25, -1.5)),
  tc("zero_diff", list(fluor_before = 100.0, fluor_after = 100.0, slope = 0.25, intercept = -1.5),
     calc_chla_acid(100.0, 100.0, 0.25, -1.5)),
  tc("na_fluor", list(fluor_before = NA, fluor_after = 80.0, slope = 0.25, intercept = -1.5), NA_real_)
)

cases$chlorophyll$chla_no_acid <- list(
  tc("normal", list(fluor = 150.0, slope = 0.30, intercept = -2.0),
     calc_chla_no_acid(150.0, 0.30, -2.0)),
  tc("na_slope", list(fluor = 150.0, slope = NA, intercept = -2.0), NA_real_)
)

# --- benthic ---
cases$benthic$rock_surface_area_m2 <- list(
  tc("typical_rock", list(dims_cm = c(10.0, 8.0, 5.0)),
     calc_rock_surface_area(c(10.0, 8.0, 5.0))),
  tc("sphere", list(dims_cm = c(10.0, 10.0, 10.0)),
     calc_rock_surface_area(c(10.0, 10.0, 10.0)))
)

cases$benthic$per_m2 <- list(
  tc("normal", list(sample_value = 0.005, dims_cm = c(10.0, 8.0, 5.0), vol_filtrated = 50.0, vol_total = 200.0),
     calc_per_m2(0.005, c(10.0, 8.0, 5.0), 50.0, 200.0))
)

cases$benthic$benthic_afdm_per_m2 <- list(
  tc("normal", list(afdm_g = 0.003, dims_cm = c(12.0, 9.0, 6.0), vol_filtrated = 50.0, vol_total = 250.0),
     calc_benthic_afdm(0.003, c(12.0, 9.0, 6.0), 50.0, 250.0))
)

# --- field_data ---
cases$field_data$barometric_pressure_from_altitude <- list(
  tc("sea_level", list(elevation_m = 0.0, temp_c = 15.0),
     calc_barometric_pressure(0.0, 15.0), tolerance = 0.5),
  tc("martigny_470m", list(elevation_m = 470.0, temp_c = 15.0),
     calc_barometric_pressure(470.0, 15.0), tolerance = 0.5),
  tc("high_altitude_2000m", list(elevation_m = 2000.0, temp_c = 5.0),
     calc_barometric_pressure(2000.0, 5.0), tolerance = 0.5),
  tc("zero_temp", list(elevation_m = 1000.0, temp_c = 0.0),
     calc_barometric_pressure(1000.0, 0.0), tolerance = 0.5),
  tc("na_elevation", list(elevation_m = NA, temp_c = 15.0), NA_real_)
)

cases$field_data$co2_correction <- list(
  tc("no_curve", list(raw_co2 = 500.0, temp_c = 10.0, pressure_hpa = 850.0, std_slope = NA, std_intercept = NA),
     calc_co2_correction(500.0, 10.0, 850.0)),
  tc("with_curve", list(raw_co2 = 500.0, temp_c = 10.0, pressure_hpa = 850.0, std_slope = 1.1, std_intercept = -5.0),
     calc_co2_correction(500.0, 10.0, 850.0, 1.1, -5.0)),
  tc("ref_conditions", list(raw_co2 = 1013.0, temp_c = 25.0, pressure_hpa = 1013.0, std_slope = NA, std_intercept = NA),
     calc_co2_correction(1013.0, 25.0, 1013.0)),
  tc("na_temp", list(raw_co2 = 500.0, temp_c = NA, pressure_hpa = 850.0, std_slope = NA, std_intercept = NA), NA_real_)
)

# --- CH4 dry ---
cases$pco2$ch4_dry <- list(
  tc("normal", list(ch4_wet = 2000.0, h2o_percent = 1.5),
     calc_ch4_dry(2000.0, 1.5)),
  tc("zero_h2o", list(ch4_wet = 2000.0, h2o_percent = 0.0),
     calc_ch4_dry(2000.0, 0.0)),
  tc("high_h2o", list(ch4_wet = 2000.0, h2o_percent = 3.0),
     calc_ch4_dry(2000.0, 3.0)),
  tc("na_input", list(ch4_wet = NA, h2o_percent = 1.5), NA_real_)
)

# --- pCO2 ---
cases$pco2$pco2_from_co2aq <- list(
  tc("alpine_10c", list(co2_aq = 50.0, water_temp_c = 10.0, c_const = 2392.86),
     calc_pco2(50.0, 10.0, 2392.86), tolerance = 1e-6),
  tc("ref_25c", list(co2_aq = 50.0, water_temp_c = 25.0, c_const = 2392.86),
     calc_pco2(50.0, 25.0, 2392.86), tolerance = 1e-6),
  tc("cold_2c", list(co2_aq = 100.0, water_temp_c = 2.0, c_const = 2392.86),
     calc_pco2(100.0, 2.0, 2392.86), tolerance = 1e-6),
  tc("warm_30c", list(co2_aq = 30.0, water_temp_c = 30.0, c_const = 2392.86),
     calc_pco2(30.0, 30.0, 2392.86), tolerance = 1e-6),
  tc("na_temp", list(co2_aq = 50.0, water_temp_c = NA, c_const = 2392.86), NA_real_)
)

cases$pco2$pco2_p1 <- list(
  tc("alpine_850hpa", list(co2_aq = 50.0, water_temp_c = 10.0, bp_hpa = 850.0, c_const = 2392.86),
     calc_pco2_p1(50.0, 10.0, 850.0, 2392.86), tolerance = 1e-6),
  tc("sea_level", list(co2_aq = 50.0, water_temp_c = 15.0, bp_hpa = 1013.25, c_const = 2392.86),
     calc_pco2_p1(50.0, 15.0, 1013.25, 2392.86), tolerance = 1e-6),
  tc("na_bp", list(co2_aq = 50.0, water_temp_c = 10.0, bp_hpa = NA, c_const = 2392.86), NA_real_)
)

cases$pco2$pco2_p2 <- list(
  tc("alpine_850hpa", list(co2_aq = 50.0, water_temp_c = 10.0, bp_hpa = 850.0, c_const = 2392.86),
     calc_pco2_p2(50.0, 10.0, 850.0, 2392.86), tolerance = 1e-6),
  tc("sea_level", list(co2_aq = 50.0, water_temp_c = 15.0, bp_hpa = 1013.25, c_const = 2392.86),
     calc_pco2_p2(50.0, 15.0, 1013.25, 2392.86), tolerance = 1e-6)
)

# --- CO2 headspace ---
cases$co2_air$co2_headspace <- list(
  tc("standard_lab", list(co2_ppm = 400.0, lab_temp_c = 22.0, lab_pressure_atm = 0.95,
                          vol_sa_ml = 60.0, vol_water_ml = 40.0,
                          kh_co2 = 0.034, c_const = 2392.86, gas_const_r_atm = 0.08206),
     calc_co2_headspace(400.0, 22.0, 0.95, 60.0, 40.0), tolerance = 1e-6),
  tc("high_co2", list(co2_ppm = 5000.0, lab_temp_c = 20.0, lab_pressure_atm = 1.0,
                      vol_sa_ml = 60.0, vol_water_ml = 40.0,
                      kh_co2 = 0.034, c_const = 2392.86, gas_const_r_atm = 0.08206),
     calc_co2_headspace(5000.0, 20.0, 1.0, 60.0, 40.0), tolerance = 1e-6),
  tc("ref_25c", list(co2_ppm = 400.0, lab_temp_c = 25.0, lab_pressure_atm = 1.0,
                     vol_sa_ml = 60.0, vol_water_ml = 40.0,
                     kh_co2 = 0.034, c_const = 2392.86, gas_const_r_atm = 0.08206),
     calc_co2_headspace(400.0, 25.0, 1.0, 60.0, 40.0), tolerance = 1e-6)
)

# --- DIC ---
cases$dic$dic_concentration <- list(
  tc("normal",
     list(acid_sample_wght = 12.5, acid_wght = 10.0, vol_overpressure = 0.5,
          sa_added = 0.3, co2_dry = 2000.0, air_temp_c = 22.0,
          h_co2_29815k = 3.3e-4, gas_const_r_mol = 8.314,
          vial_volume = 12.0, h3po4_added = 0.1),
     calc_dic(12.5, 10.0, 0.5, 0.3, 2000.0, 22.0), tolerance = 1e-6),
  tc("cold_lab",
     list(acid_sample_wght = 12.5, acid_wght = 10.0, vol_overpressure = 0.5,
          sa_added = 0.3, co2_dry = 2000.0, air_temp_c = 15.0,
          h_co2_29815k = 3.3e-4, gas_const_r_mol = 8.314,
          vial_volume = 12.0, h3po4_added = 0.1),
     calc_dic(12.5, 10.0, 0.5, 0.3, 2000.0, 15.0), tolerance = 1e-6),
  tc("high_co2",
     list(acid_sample_wght = 12.5, acid_wght = 10.0, vol_overpressure = 0.5,
          sa_added = 0.3, co2_dry = 10000.0, air_temp_c = 22.0,
          h_co2_29815k = 3.3e-4, gas_const_r_mol = 8.314,
          vial_volume = 12.0, h3po4_added = 0.1),
     calc_dic(12.5, 10.0, 0.5, 0.3, 10000.0, 22.0), tolerance = 1e-6),
  tc("na_input",
     list(acid_sample_wght = NA, acid_wght = 10.0, vol_overpressure = 0.5,
          sa_added = 0.3, co2_dry = 2000.0, air_temp_c = 22.0,
          h_co2_29815k = 3.3e-4, gas_const_r_mol = 8.314,
          vial_volume = 12.0, h3po4_added = 0.1),
     NA_real_)
)

# --- d13C-DIC ---
cases$dic$d13c_dic <- list(
  tc("normal",
     list(acid_sample_wght = 12.5, acid_wght = 10.0, vol_overpressure = 0.5,
          delta_13co2 = -12.0, air_temp_c = 22.0,
          h_co2_29815k = 3.3e-4, gas_const_r_mol = 8.314,
          vial_volume = 12.0, h3po4_added = 0.1),
     calc_d13c_dic(12.5, 10.0, 0.5, -12.0, 22.0), tolerance = 1e-6),
  tc("positive_delta",
     list(acid_sample_wght = 12.5, acid_wght = 10.0, vol_overpressure = 0.5,
          delta_13co2 = 2.0, air_temp_c = 22.0,
          h_co2_29815k = 3.3e-4, gas_const_r_mol = 8.314,
          vial_volume = 12.0, h3po4_added = 0.1),
     calc_d13c_dic(12.5, 10.0, 0.5, 2.0, 22.0), tolerance = 1e-6),
  tc("na_delta",
     list(acid_sample_wght = 12.5, acid_wght = 10.0, vol_overpressure = 0.5,
          delta_13co2 = NA, air_temp_c = 22.0,
          h_co2_29815k = 3.3e-4, gas_const_r_mol = 8.314,
          vial_volume = 12.0, h3po4_added = 0.1),
     NA_real_)
)

# --- Dissolved CH4 ---
cases$pco2$dissolved_ch4 <- list(
  tc("alpine_stream",
     list(ch4_dry = 5000.0, water_temp_c = 10.0, bp_hpa = 850.0,
          lab_temp_c = 22.0, lab_pressure_atm = 0.95,
          kh_ch4 = 0.0014, ch4_temp_const = 1750.0,
          ch4_in_sa = 1.9, gas_const_r_mol = 8.314),
     calc_dissolved_ch4(5000.0, 10.0, 850.0, 22.0, 0.95), tolerance = 1e-4),
  tc("warm_lowland",
     list(ch4_dry = 3000.0, water_temp_c = 20.0, bp_hpa = 1013.0,
          lab_temp_c = 22.0, lab_pressure_atm = 1.0,
          kh_ch4 = 0.0014, ch4_temp_const = 1750.0,
          ch4_in_sa = 1.9, gas_const_r_mol = 8.314),
     calc_dissolved_ch4(3000.0, 20.0, 1013.0, 22.0, 1.0), tolerance = 1e-4),
  tc("na_input",
     list(ch4_dry = NA, water_temp_c = 10.0, bp_hpa = 850.0,
          lab_temp_c = 22.0, lab_pressure_atm = 0.95,
          kh_ch4 = 0.0014, ch4_temp_const = 1750.0,
          ch4_in_sa = 1.9, gas_const_r_mol = 8.314),
     NA_real_)
)


# =============================================================================
# Generate JSON fixture
# =============================================================================

output <- list(
  metadata = list(
    generator = "r_reference/generate_fixtures.R",
    r_version = paste(R.version$major, R.version$minor, sep = "."),
    generated_at = format(Sys.time(), "%Y-%m-%dT%H:%M:%SZ", tz = "UTC")
  ),
  modules = cases
)

# Write to tests/fixtures/
args <- commandArgs(trailingOnly = FALSE)
script_path <- sub("--file=", "", args[grep("--file=", args)])
if (length(script_path) == 0) script_path <- "r_reference/generate_fixtures.R"
script_dir <- dirname(script_path)
output_path <- file.path(script_dir, "..", "tests", "fixtures", "golden_values.json")
dir.create(dirname(output_path), showWarnings = FALSE, recursive = TRUE)

json_text <- toJSON(output, auto_unbox = TRUE, pretty = TRUE, na = "null", digits = 17)
writeLines(json_text, output_path)

cat("Generated", output_path, "\n")
cat("Modules:", paste(names(cases), collapse = ", "), "\n")
total_cases <- sum(sapply(cases, function(m) sum(sapply(m, length))))
cat("Total test cases:", total_cases, "\n")
